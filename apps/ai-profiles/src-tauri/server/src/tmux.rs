//! Driving tmux: one session (`ai` unless configured) with a window per
//! Claude session.
//!
//! Every call is an argv, never a shell line: the command a window runs goes
//! after `--` as separate arguments (tmux 3.0 and later run those directly),
//! so a folder or session name can hold any character without being
//! interpreted.

use std::process::{Command, Output};

use ai_profiles_core::api::TmuxWindow;

/// `-F` format for a window just made: what to target it by, and the pid of
/// its pane (the `claude` process itself, since the command is exec'd).
const NEW_WINDOW_FORMAT: &str = "#{session_name}\t#{window_id}\t#{pane_id}\t#{pane_pid}";

#[derive(Debug, Clone)]
pub struct Tmux {
    socket: Option<String>,
    pub session: String,
}

/// A window made for a Claude session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub window: TmuxWindow,
    pub pane_pid: i32,
}

#[derive(Debug)]
pub struct TmuxError(pub String);

impl std::fmt::Display for TmuxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Tmux {
    pub fn new(session: &str, socket: Option<&str>) -> Tmux {
        Tmux {
            socket: socket.map(str::to_owned),
            session: session.to_owned(),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new("tmux");
        if let Some(socket) = &self.socket {
            command.args(["-L", socket]);
        }
        command
    }

    fn run(&self, args: &[&str]) -> Result<Output, TmuxError> {
        self.command()
            .args(args)
            .output()
            .map_err(|err| TmuxError(format!("could not run tmux: {err}")))
    }

    fn run_ok(&self, args: &[&str]) -> Result<String, TmuxError> {
        let output = self.run(args)?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(TmuxError(format!(
                "tmux {}: {}",
                args.first().copied().unwrap_or_default(),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    pub fn has_session(&self) -> bool {
        self.run(&["has-session", "-t", &format!("={}", self.session)])
            .is_ok_and(|output| output.status.success())
    }

    /// Open a window named `name`, working in `cwd`, running `argv`. Makes the
    /// tmux session (with this as its first window, so no stray shell comes
    /// with it) when there isn't one yet.
    pub fn open_window(
        &self,
        name: &str,
        cwd: &str,
        argv: &[String],
    ) -> Result<Launched, TmuxError> {
        // tmux reads a working folder as a format, where `#(…)` runs a
        // shell command: `##` keeps each `#` a `#`.
        let cwd = &literal_semicolon(&cwd.replace('#', "##"));
        let mut args: Vec<&str> = Vec::new();
        let target = format!("={}:", self.session);
        let fresh = !self.has_session();
        if fresh {
            args.extend(["new-session", "-d", "-s", &self.session]);
        } else {
            args.extend(["new-window", "-d", "-t", &target]);
        }
        args.extend(["-n", name, "-c", cwd, "-P", "-F", NEW_WINDOW_FORMAT, "--"]);
        let argv: Vec<String> = argv.iter().map(|arg| literal_semicolon(arg)).collect();
        args.extend(argv.iter().map(String::as_str));
        let output = match self.run_ok(&args) {
            // Someone made the session between the check and now: add a window.
            Err(TmuxError(message)) if fresh && message.contains("duplicate session") => {
                let mut again: Vec<&str> = vec!["new-window", "-d", "-t", &target];
                again.extend(["-n", name, "-c", cwd, "-P", "-F", NEW_WINDOW_FORMAT, "--"]);
                again.extend(argv.iter().map(String::as_str));
                self.run_ok(&again)?
            }
            other => other?,
        };
        parse_launched(&output)
            .ok_or_else(|| TmuxError(format!("tmux answered in an unexpected way: {output:?}")))
    }

    /// Tag a window with a user option (`@aip_session`, …), so it can be found
    /// again before Claude has registered itself.
    pub fn tag(&self, window_id: &str, option: &str, value: &str) -> Result<(), TmuxError> {
        self.run_ok(&["set-option", "-w", "-t", window_id, option, value])
            .map(|_| ())
    }

    /// Windows carrying `option`, with its value: `(window, value)`.
    pub fn tagged(&self, option: &str) -> Vec<(TmuxWindow, String)> {
        let format = format!("#{{session_name}}\t#{{window_id}}\t#{{pane_id}}\t#{{{option}}}");
        let Ok(text) = self.run_ok(&["list-panes", "-a", "-F", &format]) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let window = TmuxWindow {
                    session: fields.next()?.to_owned(),
                    window_id: fields.next()?.to_owned(),
                    pane_id: fields.next()?.to_owned(),
                };
                let value = fields.next()?.to_owned();
                (!value.is_empty()).then_some((window, value))
            })
            .collect()
    }

    /// Close the window `window_id`. For a window the server made to run one
    /// Claude session, which has nothing else in it.
    pub fn kill_window(&self, window_id: &str) -> Result<(), TmuxError> {
        self.run_ok(&["kill-window", "-t", window_id]).map(|_| ())
    }

    /// What a pane shows, or `None` when it's gone (its command exited).
    /// With the last 200 lines of scrollback: a prompt taller than the
    /// window starts above what's visible.
    pub fn capture(&self, pane_id: &str) -> Option<String> {
        self.run_ok(&["capture-pane", "-p", "-J", "-S", "-200", "-t", pane_id])
            .ok()
    }

    /// The visible screen as it is, row by row, with the pane's size.
    pub fn screen(&self, pane_id: &str) -> Option<(String, u16, u16)> {
        let text = self.run_ok(&["capture-pane", "-p", "-t", pane_id]).ok()?;
        let size = self
            .run_ok(&[
                "display-message",
                "-p",
                "-t",
                pane_id,
                "#{pane_width}\t#{pane_height}",
            ])
            .ok()?;
        let (width, height) = size.trim().split_once('\t')?;
        Some((text, width.parse().ok()?, height.parse().ok()?))
    }

    /// A window by id, with the account it was opened for (`@aip_account`,
    /// empty for windows this server didn't open).
    pub fn window(&self, window_id: &str) -> Option<(Launched, String)> {
        let text = self
            .run_ok(&[
                "list-panes",
                "-t",
                window_id,
                "-F",
                "#{session_name}\t#{window_id}\t#{pane_id}\t#{pane_pid}\t#{@aip_account}",
            ])
            .ok()?;
        let mut fields = text.lines().next()?.split('\t');
        let window = TmuxWindow {
            session: fields.next()?.to_owned(),
            window_id: fields.next()?.to_owned(),
            pane_id: fields.next()?.to_owned(),
        };
        let pane_pid = fields.next()?.parse().ok()?;
        (window.window_id == window_id).then(|| {
            (
                Launched { window, pane_pid },
                fields.next().unwrap_or_default().to_owned(),
            )
        })
    }

    /// Type `text` literally (no key names) into a pane.
    pub fn type_text(&self, pane_id: &str, text: &str) -> Result<(), TmuxError> {
        self.run_ok(&["send-keys", "-l", "-t", pane_id, "--", text])
            .map(|_| ())
    }

    /// Press one named key in a pane (`Down`, `Enter`). Only ever used on a
    /// pane this server opened: to answer a prompt it recognised, or for
    /// someone typing into it from the app.
    pub fn press(&self, pane_id: &str, key: &str) -> Result<(), TmuxError> {
        self.run_ok(&["send-keys", "-t", pane_id, key]).map(|_| ())
    }

    /// `tmux attach` for a window, as typed on the host.
    pub fn attach_command(&self, window: &TmuxWindow) -> String {
        let socket = self
            .socket
            .as_deref()
            .map(|socket| format!("-L {socket} "))
            .unwrap_or_default();
        format!(
            "tmux {socket}attach -t {} \\; select-window -t {}",
            window.session, window.window_id
        )
    }
}

fn parse_launched(output: &str) -> Option<Launched> {
    let mut fields = output.trim().split('\t');
    let session = fields.next()?.to_owned();
    let window_id = fields.next()?.to_owned();
    let pane_id = fields.next()?.to_owned();
    let pane_pid = fields.next()?.parse().ok()?;
    Some(Launched {
        window: TmuxWindow {
            session,
            window_id,
            pane_id,
        },
        pane_pid,
    })
}

/// `arg`, with a `;` at its end escaped: tmux takes an argument ending in
/// `;` for the end of its command, and `\;` for a `;`.
fn literal_semicolon(arg: &str) -> String {
    match arg.strip_suffix(';') {
        Some(rest) => format!("{rest}\\;"),
        None => arg.to_owned(),
    }
}

/// A window name from a session's name or folder: printable, one line, and
/// short enough for tmux's status bar. Windows are targeted by id, never by
/// name, so `:` and `.` may stay. Without `#`, which newer tmux reads as a
/// format (`#(…)` runs a command), and without a `;` at the end, which tmux
/// takes for the end of its command.
pub fn window_name(preferred: &str, fallback: &str) -> String {
    let clean = |text: &str| -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        let joined = words.join(" ");
        let printable: String = joined
            .chars()
            .filter(|c| !c.is_control() && *c != '#')
            .collect();
        printable
            .chars()
            .take(40)
            .collect::<String>()
            .trim_end_matches([';', ' '])
            .trim()
            .to_owned()
    };
    let name = clean(preferred);
    if name.is_empty() {
        let fallback = clean(fallback);
        if fallback.is_empty() {
            "claude".into()
        } else {
            fallback
        }
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_new_window_prints() {
        let launched = parse_launched("ai\t@7\t%12\t4242\n").unwrap();
        assert_eq!(launched.window.window_id, "@7");
        assert_eq!(launched.window.pane_id, "%12");
        assert_eq!(launched.pane_pid, 4242);
        assert_eq!(parse_launched("ai\t@7\n"), None);
        assert_eq!(parse_launched("ai\t@7\t%1\tnot-a-pid"), None);
    }

    #[test]
    fn window_names_are_one_short_printable_line() {
        assert_eq!(
            window_name("  Brain   Dev\nServer ", "x"),
            "Brain Dev Server"
        );
        assert_eq!(window_name("a\u{7}b", "x"), "ab");
        assert_eq!(window_name("", "marcus2-brain"), "marcus2-brain");
        assert_eq!(window_name(" ", ""), "claude");
        assert_eq!(window_name("x#(touch y)", ""), "x(touch y)");
        assert_eq!(window_name("PR #50;", ""), "PR 50");
        assert_eq!(window_name(&"x".repeat(80), "").len(), 40);
    }

    #[test]
    fn attach_commands_name_the_socket_when_there_is_one() {
        let window = TmuxWindow {
            session: "ai".into(),
            window_id: "@3".into(),
            pane_id: "%3".into(),
        };
        assert_eq!(
            Tmux::new("ai", None).attach_command(&window),
            "tmux attach -t ai \\; select-window -t @3"
        );
        assert_eq!(
            Tmux::new("ai", Some("test")).attach_command(&window),
            "tmux -L test attach -t ai \\; select-window -t @3"
        );
    }
}
