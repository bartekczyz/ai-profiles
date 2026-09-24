//! Starting Claude in a tmux window, the way claudemulti does, with Remote
//! Control on:
//!
//! - a session someone named gets `--remote-control <name>`;
//! - any other gets `--settings {"remoteControlAtStartup":true}`, and Remote
//!   Control names it after its folder (an auto-generated title is never
//!   passed as a name);
//! - resuming adds `-r <id>`.
//!
//! Remote Control only connects once the folder is trusted, and Claude asks
//! about that first with "No, exit" selected. So after opening the window
//! this watches it for a few seconds: it accepts the trust prompt when asked
//! to (Down, check "Yes" is selected, Enter), and otherwise waits for Claude
//! to register the session, reporting anything it's left waiting on.
//!
//! When the folder's own settings pre-approve tool permissions, the prompt
//! lists them, and trusting the folder grants them. That is never answered
//! here: it's reported, with the list, for someone to decide in the window.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use ai_profiles_core::api::{Attention, LaunchResult};
use ai_profiles_core::registry::RegistryEntry;

use crate::accounts::AccountDir;
use crate::error::ApiError;
use crate::tmux::{Launched, Tmux};

/// How long a new window is watched for prompts and for Claude registering.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(12);
const POLL: Duration = Duration::from_millis(250);

const TRUST_PROMPT: &str = "Is this a project you created or one you trust";
const TRUST_YES: &str = "Yes, I trust this folder";
const SELECTED: char = '❯';
/// In the trust prompt when the folder's settings pre-approve permissions.
const PREAPPROVED_END: &str = "These will apply without asking";
const PREAPPROVED_START: &str = "pre-approves";

pub struct Launch<'a> {
    pub account: &'a AccountDir,
    pub claude: &'a Path,
    pub cwd: &'a Path,
    pub window_name: String,
    /// Passed as `--remote-control <name>`, without a leading `-`; `None` lets Remote Control name it.
    pub remote_control_name: Option<String>,
    /// `-r <id>`.
    pub resume: Option<String>,
    pub trust_folder: bool,
}

/// The command a window runs. `env` sets the account's config dir (or, for
/// `default`, removes any inherited one, which a tmux server started from a
/// shell with it set would otherwise pass on), then execs claude, so the
/// pane's pid is claude's.
pub fn claude_argv(launch: &Launch) -> Vec<String> {
    let mut argv = vec!["/usr/bin/env".to_owned()];
    if launch.account.is_default {
        argv.extend(["-u".to_owned(), "CLAUDE_CONFIG_DIR".to_owned()]);
    } else {
        argv.push(format!(
            "CLAUDE_CONFIG_DIR={}",
            launch.account.dir.display()
        ));
    }
    argv.push(launch.claude.display().to_string());
    // Without a leading `-`, which would make the name another option. Claude
    // doesn't take `--remote-control=<name>` as the name, so it can't go in one
    // argument.
    let name = launch
        .remote_control_name
        .as_deref()
        .map(|name| name.trim_start_matches(['-', ' ']))
        .filter(|name| !name.is_empty());
    match name {
        Some(name) => argv.extend(["--remote-control".to_owned(), name.to_owned()]),
        None => argv.extend([
            "--settings".to_owned(),
            r#"{"remoteControlAtStartup":true}"#.to_owned(),
        ]),
    }
    if let Some(id) = &launch.resume {
        argv.extend(["-r".to_owned(), id.clone()]);
    }
    argv
}

pub fn start(tmux: &Tmux, launch: &Launch) -> Result<LaunchResult, ApiError> {
    // An account signed in from the app hasn't been through Claude's
    // first-run setup, which would otherwise hold the window up.
    if let Err(err) = launch
        .account
        .finish_setup(|| crate::hostinfo::claude_version(launch.claude))
    {
        eprintln!("could not mark {} set up: {err}", launch.account.name);
    }
    let cwd = launch.cwd.display().to_string();
    let launched = tmux
        .open_window(&launch.window_name, &cwd, &claude_argv(launch))
        .map_err(ApiError::internal)?;
    let _ = tmux.tag(
        &launched.window.window_id,
        "@aip_account",
        &launch.account.name,
    );
    if let Some(id) = &launch.resume {
        let _ = tmux.tag(&launched.window.window_id, "@aip_session", id);
    }
    let settled = settle(tmux, &launched, &launch.account.dir, launch.trust_folder)?;
    Ok(LaunchResult {
        attach_command: tmux.attach_command(&launched.window),
        window: launched.window,
        already_running: false,
        session_id: settled.session_id.or_else(|| launch.resume.clone()),
        remote_control_name: launch.remote_control_name.clone(),
        attention: settled.attention,
    })
}

struct Settled {
    session_id: Option<String>,
    attention: Option<Attention>,
}

/// Watch a new window until Claude registers its session, answering the
/// trust prompt on the way when allowed to.
fn settle(
    tmux: &Tmux,
    launched: &Launched,
    config_dir: &Path,
    trust: bool,
) -> Result<Settled, ApiError> {
    let registry = config_dir
        .join("sessions")
        .join(format!("{}.json", launched.pane_pid));
    let pane = &launched.window.pane_id;
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    let mut pressed_down = false;
    let mut pressed_enter = false;
    let mut last_screen = String::new();
    while Instant::now() < deadline {
        if let Some(entry) = read_entry(&registry) {
            return Ok(Settled {
                session_id: entry.session_id,
                attention: None,
            });
        }
        let Some(screen) = tmux.capture(pane) else {
            return Err(exited(&last_screen));
        };
        // Only a screen that has finished drawing is acted on: its answers
        // are on it, and it hasn't changed since the last look. A capture
        // taken mid-draw can show the question without the permissions
        // warning above its answers.
        let drawn = screen.contains(TRUST_YES) && screen == last_screen;
        if let (Some(prompt), false, true) = (trust_prompt(&screen), pressed_enter, drawn) {
            if !trust {
                return Ok(Settled {
                    session_id: None,
                    attention: Some(Attention {
                        kind: "trustPrompt".into(),
                        text: "Claude is asking whether to trust this folder. Remote Control connects once someone answers it in the window.".into(),
                    }),
                });
            }
            if let Some(permissions) = prompt.preapproved {
                return Ok(Settled {
                    session_id: None,
                    attention: Some(Attention {
                        kind: "trustPermissions".into(),
                        text: permissions,
                    }),
                });
            }
            if prompt.yes_selected {
                tmux.press(pane, "Enter").map_err(ApiError::internal)?;
                pressed_enter = true;
            } else if !pressed_down {
                tmux.press(pane, "Down").map_err(ApiError::internal)?;
                pressed_down = true;
            }
        }
        last_screen = screen;
        thread::sleep(POLL);
    }
    Ok(Settled {
        session_id: None,
        attention: Some(Attention {
            kind: "waiting".into(),
            text: last_lines(&last_screen, 6),
        }),
    })
}

/// What the window at `pane` is waiting for someone to answer, before Claude
/// has registered its session: the trust prompt (saying what trusting would
/// pre-approve, when it would), else whatever its last lines show.
pub fn attention_now(tmux: &Tmux, pane: &str) -> Option<Attention> {
    let screen = tmux.capture(pane)?;
    Some(match trust_prompt(&screen) {
        Some(TrustPrompt {
            preapproved: Some(permissions),
            ..
        }) => Attention {
            kind: "trustPermissions".into(),
            text: permissions,
        },
        Some(_) => Attention {
            kind: "trustPrompt".into(),
            text: "Claude is asking whether to trust this folder. Remote Control connects once someone answers it in the window.".into(),
        },
        None => Attention {
            kind: "waiting".into(),
            text: last_lines(&screen, 6),
        },
    })
}

/// Pure: whether the newest Remote Control line on a Claude screen says it
/// disconnected. The registry keeps a session's Remote Control id after that
/// (when it was ended or archived from the Claude app, say), so only the
/// window tells. A later reconnect prints a newer line, which wins.
pub fn remote_control_disconnected(screen: &str) -> bool {
    screen
        .lines()
        .rev()
        .map(str::to_lowercase)
        .find(|line| line.contains("remote control") || line.contains("remote-control"))
        .is_some_and(|line| line.contains("disconnected"))
}

fn read_entry(path: &Path) -> Option<RegistryEntry> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

struct TrustPrompt {
    /// The cursor is on "Yes, I trust this folder".
    yes_selected: bool,
    /// What it says the folder's settings pre-approve, if anything.
    preapproved: Option<String>,
}

/// The trust prompt, when it's what the window shows now. Claude redraws a
/// prompt taller than the window in full, leaving older copies in the
/// scrollback, so only the text from its last "Is this a project…" on is
/// read.
fn trust_prompt(screen: &str) -> Option<TrustPrompt> {
    let latest = &screen[screen.rfind(TRUST_PROMPT)?..];
    let lines: Vec<&str> = latest
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let yes_selected = lines
        .iter()
        .any(|line| line.contains(SELECTED) && line.contains(TRUST_YES));
    let preapproved = lines
        .iter()
        .position(|line| line.contains(PREAPPROVED_START))
        .map(|start| {
            let end = lines[start..]
                .iter()
                .position(|line| line.contains(PREAPPROVED_END))
                .map_or(lines.len(), |offset| start + offset + 1);
            lines[start..end]
                .join(" ")
                .trim_start_matches(['⚠', ' '])
                .to_owned()
        });
    Some(TrustPrompt {
        yes_selected,
        preapproved,
    })
}

fn last_lines(screen: &str, count: usize) -> String {
    let lines: Vec<&str> = screen
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

fn exited(last_screen: &str) -> ApiError {
    let shown = last_lines(last_screen, 6);
    ApiError::conflict(
        "launch_failed",
        if shown.is_empty() {
            "Claude exited as soon as it started. Try the same in a terminal on the host to see why.".to_owned()
        } else {
            format!("Claude exited as soon as it started. It last showed:\n{shown}")
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_remote_control_disconnect_only_when_it_is_the_newest_word() {
        let ended = "● Remote Control disconnected — this session was ended or archived from\n  another device or app (code 4090)\n────\n❯\u{a0}\n────\n";
        assert!(remote_control_disconnected(ended));
        let active = "  /remote-control is active · Continue here, on your phone, or at\n  https://claude.ai/code/session_01\n";
        assert!(!remote_control_disconnected(active));
        assert!(
            !remote_control_disconnected(&format!("{ended}{active}")),
            "reconnected since"
        );
        assert!(
            !remote_control_disconnected("Done.\n❯ \n"),
            "nothing said: go by the registry"
        );
    }
    use std::path::PathBuf;

    fn account(is_default: bool) -> AccountDir {
        AccountDir {
            name: if is_default { "default" } else { "work" }.into(),
            dir: PathBuf::from("/home/m/.claude-accounts/work"),
            is_default,
        }
    }

    fn launch<'a>(account: &'a AccountDir, name: Option<&str>, resume: Option<&str>) -> Launch<'a> {
        Launch {
            account,
            claude: Path::new("/home/m/.local/bin/claude"),
            cwd: Path::new("/home/m/code"),
            window_name: "w".into(),
            remote_control_name: name.map(str::to_owned),
            resume: resume.map(str::to_owned),
            trust_folder: true,
        }
    }

    #[test]
    fn reads_the_latest_trust_prompt_and_what_it_would_pre_approve() {
        let plain = "Quick safety check: Is this a project you created or one you trust?\n ❯ No, exit\n   Yes, I trust this folder\n";
        let prompt = trust_prompt(plain).unwrap();
        assert!(!prompt.yes_selected);
        assert!(prompt.preapproved.is_none());

        // An older copy in the scrollback with No selected, then the latest.
        let redrawn = format!(
            "{plain}Quick safety check: Is this a project you created or one you trust?\n   No, exit\n ❯ Yes, I trust this folder\n"
        );
        assert!(trust_prompt(&redrawn).unwrap().yes_selected);

        let risky = "Quick safety check: Is this a project you created or one you trust? (Like your\n Claude Code'll be able to read, edit, and execute files here.\n ⚠ This folder pre-approves 10 tool permissions in .claude/settings.local.json:\n   Bash(sudo mkdir:*), Bash(sudo cp:*), and 2 more\n These will apply without asking. Only proceed if you trust this configuration.\n Security guide\n ❯ No, exit\n   Yes, I trust this folder\n";
        assert_eq!(
            trust_prompt(risky).unwrap().preapproved.as_deref(),
            Some("This folder pre-approves 10 tool permissions in .claude/settings.local.json: Bash(sudo mkdir:*), Bash(sudo cp:*), and 2 more These will apply without asking. Only proceed if you trust this configuration.")
        );
        assert!(trust_prompt("Welcome to Claude Code").is_none());
    }

    #[test]
    fn a_named_session_resumes_with_its_remote_control_name() {
        let work = account(false);
        assert_eq!(
            claude_argv(&launch(&work, Some("Brain Dev; $(rm -rf ~)"), Some("abc"))),
            vec![
                "/usr/bin/env",
                "CLAUDE_CONFIG_DIR=/home/m/.claude-accounts/work",
                "/home/m/.local/bin/claude",
                "--remote-control",
                "Brain Dev; $(rm -rf ~)",
                "-r",
                "abc",
            ]
        );
    }

    #[test]
    fn a_name_never_becomes_another_option() {
        let work = account(false);
        let argv = claude_argv(&launch(&work, Some("--dangerously-skip-permissions"), None));
        assert!(!argv.iter().any(|arg| arg.starts_with("--dangerously")));
        assert!(argv.contains(&"dangerously-skip-permissions".to_owned()));
        let argv = claude_argv(&launch(&work, Some(" - "), None));
        assert!(argv.contains(&"--settings".to_owned()), "no name left");
    }

    #[test]
    fn an_unnamed_session_turns_remote_control_on_at_startup() {
        let work = account(false);
        let argv = claude_argv(&launch(&work, None, None));
        assert_eq!(
            &argv[3..],
            ["--settings", r#"{"remoteControlAtStartup":true}"#]
        );
    }

    #[test]
    fn the_default_account_clears_an_inherited_config_dir() {
        let stock = account(true);
        let argv = claude_argv(&launch(&stock, None, None));
        assert_eq!(&argv[..3], ["/usr/bin/env", "-u", "CLAUDE_CONFIG_DIR"]);
    }

    #[test]
    fn keeps_the_last_lines_that_say_something() {
        assert_eq!(last_lines("a\n\nb\n  \nc\n", 2), "b\nc");
        assert_eq!(last_lines("", 3), "");
    }
}
