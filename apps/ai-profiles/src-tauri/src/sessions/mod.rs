//! Claude Code sessions kept by a profile, and moving one to another profile.
//!
//! A session lives in two places. The CLI half is Claude Code's own: a
//! transcript at `<config>/projects/<encoded cwd>/<id>.jsonl` plus a handful of
//! per-session folders beside it. The desktop half is the Code tab's record of
//! it, `<gui-data>/claude-code-sessions/<account>/<org>/local_<uuid>.json`, which
//! points at the transcript by `cliSessionId`. A record whose transcript the app
//! cannot find is not dropped: the app quietly starts a new, empty CLI session
//! under it. So the two halves are always written together, and only while the
//! app that owns them is closed.
//!
//! Everything here reads Claude Code and Claude desktop internals, which can
//! change between versions.

mod archive;
mod desktop;
mod scan;
mod transfer;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::app_kind::{spec, AppKind};
use crate::error::{AppError, AppResult};
use crate::paths::resolve_gui_app;
use crate::profiles;

pub use archive::{archive, ArchiveReport};
pub use scan::{list, SessionSummary};
pub use transfer::{plan, transfer, TransferPlan, TransferReport, TransferRequest};

/// Where one profile (or the stock install) keeps its sessions.
#[derive(Debug, Clone)]
pub struct Home {
    /// The profile's name, or "Default" for the stock install.
    pub label: String,
    /// `CLAUDE_CONFIG_DIR` of the profile: the CLI half of every session.
    pub config_dir: PathBuf,
    /// The desktop app's data dir: the Code tab's session records.
    pub gui_data_dir: PathBuf,
    /// The stock install rather than a managed profile.
    pub stock: bool,
    /// Whether the profile has a desktop app to add sessions to.
    pub desktop: bool,
}

impl Home {
    /// The `.claude.json` the CLI writes its account to. A stock install keeps it
    /// in `$HOME` rather than in the config dir.
    fn claude_json_candidates(&self) -> Vec<PathBuf> {
        let mut candidates = vec![self.config_dir.join(".claude.json")];
        if self.stock {
            if let Some(home) = dirs::home_dir() {
                candidates.push(home.join(".claude.json"));
            }
        }
        candidates
    }
}

/// Resolve the session home of profile `id` (or `default:claude`). Only Claude
/// profiles have sessions this module understands.
pub fn home(id: &str) -> AppResult<Home> {
    let paths = profiles::paths(id)?;
    let (label, stock, desktop) = match AppKind::from_default_id(id) {
        Some(kind) => {
            ensure_claude(kind)?;
            (
                "Default".to_string(),
                true,
                resolve_gui_app(spec(kind)).is_some(),
            )
        }
        None => {
            let profile = profiles::load()?
                .into_iter()
                .find(|candidate| candidate.id == id)
                .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))?;
            ensure_claude(profile.app)?;
            (profile.name, false, profile.surfaces.gui)
        }
    };
    Ok(Home {
        label,
        config_dir: PathBuf::from(paths.cli_config_dir),
        gui_data_dir: PathBuf::from(paths.gui_data_dir),
        stock,
        desktop,
    })
}

fn ensure_claude(kind: AppKind) -> AppResult<()> {
    if kind == AppKind::Claude {
        Ok(())
    } else {
        Err(AppError::Validation(
            "sessions can only be listed and moved for Claude profiles".to_string(),
        ))
    }
}

/// Running processes, pid to command line, from `ps -ax -o pid=,command=`.
pub(crate) fn parse_process_list(ps_output: &str) -> HashMap<i32, String> {
    ps_output
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.trim_start().split_once(char::is_whitespace)?;
            Some((pid.parse().ok()?, command.trim().to_string()))
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunningEntry {
    pid: i32,
    session_id: Option<String>,
    entrypoint: Option<String>,
}

/// A session a live `claude` process has open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunningSession {
    pub session_id: String,
    /// The process belongs to the desktop app, which keeps one running for
    /// every Code tab session it has opened, idle or not, until it quits.
    pub desktop: bool,
}

/// Sessions that a live `claude` process of `config_dir` has open, from the
/// `sessions/<pid>.json` files it keeps there. A file whose pid is gone, or now
/// belongs to something that is not Claude, is stale and ignored.
pub(crate) fn running_sessions(
    config_dir: &Path,
    processes: &HashMap<i32, String>,
) -> Vec<RunningSession> {
    let Ok(entries) = fs::read_dir(config_dir.join("sessions")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let text = fs::read_to_string(entry.path()).ok()?;
            let running: RunningEntry = serde_json::from_str(&text).ok()?;
            let command = processes.get(&running.pid)?;
            if !command.to_lowercase().contains("claude") {
                return None;
            }
            Some(RunningSession {
                session_id: running.session_id?,
                desktop: running.entrypoint.as_deref() == Some("claude-desktop"),
            })
        })
        .collect()
}

/// Why session `id` can't be moved or archived out of `home` right now because
/// something has it open, if something does.
pub(crate) fn open_blocker(
    home: &Home,
    id: &str,
    processes: &HashMap<i32, String>,
) -> Option<String> {
    let open = running_sessions(&home.config_dir, processes);
    let running = open.iter().find(|running| running.session_id == id)?;
    Some(if running.desktop {
        format!(
            "Claude ({}) has the session open. Quit it first.",
            home.label
        )
    } else {
        format!(
            "The session is open in a terminal under {}. Close it first.",
            home.label
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ps_output_into_pid_and_command() {
        let processes = parse_process_list("  12 /bin/zsh -l\n  345 claude --resume x\n garbage\n");
        assert_eq!(processes.get(&12).map(String::as_str), Some("/bin/zsh -l"));
        assert_eq!(
            processes.get(&345).map(String::as_str),
            Some("claude --resume x")
        );
        assert_eq!(processes.len(), 2);
    }

    #[test]
    fn running_sessions_ignore_stale_and_foreign_pids() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(sessions.join("10.json"), r#"{"pid":10,"sessionId":"live"}"#).unwrap();
        fs::write(
            sessions.join("13.json"),
            r#"{"pid":13,"sessionId":"in-app","entrypoint":"claude-desktop"}"#,
        )
        .unwrap();
        fs::write(
            sessions.join("11.json"),
            r#"{"pid":11,"sessionId":"reused-pid"}"#,
        )
        .unwrap();
        fs::write(sessions.join("12.json"), r#"{"pid":12,"sessionId":"gone"}"#).unwrap();
        fs::write(sessions.join("10.abc.key"), "not json").unwrap();
        let processes = parse_process_list(
            "10 /Users/x/.local/bin/claude\n11 /bin/zsh\n13 /p/gui-data/claude-code/2.1/claude.app/Contents/MacOS/claude\n",
        );
        let mut running = running_sessions(dir.path(), &processes);
        running.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        assert_eq!(
            running,
            vec![
                RunningSession {
                    session_id: "in-app".to_string(),
                    desktop: true
                },
                RunningSession {
                    session_id: "live".to_string(),
                    desktop: false
                },
            ]
        );
    }
}
