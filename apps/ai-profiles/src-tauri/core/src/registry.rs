//! Claude Code's registry of running sessions: one `<config>/sessions/<pid>.json`
//! per live `claude` process, written by that process and removed when it
//! exits cleanly. A process that dies leaves its file behind, so an entry says
//! only that a process *was* running; callers check the pid is still Claude.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One `sessions/<pid>.json`. Only `pid` is required; the rest depends on the
/// Claude Code version and on how the process was started.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub pid: i32,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    /// `busy` or `idle`.
    pub status: Option<String>,
    /// `interactive`, …
    pub kind: Option<String>,
    /// `cli`, `claude-desktop`, `sdk-ts`, …
    pub entrypoint: Option<String>,
    /// Where the process runs in tmux, `<session>:@<window>.%<pane>`, when it
    /// runs in tmux at all. See [`TmuxLocation::parse`].
    pub tmux: Option<String>,
    /// The session's name, as Remote Control and `/rename` show it.
    pub name: Option<String>,
    /// `user` when someone chose the name, `derived` when Claude made it up.
    pub name_source: Option<String>,
    /// Set while Remote Control is connected.
    pub bridge_session_id: Option<String>,
    /// The process's start time as the kernel counts it (Linux: field 22 of
    /// `/proc/<pid>/stat`), to tell the process from a later one with its pid.
    pub proc_start: Option<String>,
    /// Which pid namespace `pid` belongs to, e.g. `linux:<machine>:pid:[4026531836]`.
    pub pid_domain: Option<String>,
    /// When the process started, in milliseconds since the epoch.
    pub started_at: Option<u64>,
    /// The Claude Code version the process runs, e.g. `2.1.280`.
    pub version: Option<String>,
}

impl RegistryEntry {
    /// Someone chose this session's name (with `/rename`, `--remote-control
    /// <name>` or the app), rather than Claude deriving one.
    pub fn named_by_user(&self) -> bool {
        self.name_source.as_deref() == Some("user")
    }

    /// The tmux window it runs in, if the entry says.
    pub fn tmux_location(&self) -> Option<TmuxLocation> {
        TmuxLocation::parse(self.tmux.as_deref()?)
    }
}

/// Every parseable entry in `<config_dir>/sessions/`, with the file it came
/// from. Files that aren't JSON entries (a write cut short, the `.key` files
/// beside them) are skipped.
pub fn read_registry(config_dir: &Path) -> Vec<(PathBuf, RegistryEntry)> {
    let Ok(files) = fs::read_dir(config_dir.join("sessions")) else {
        return Vec::new();
    };
    files
        .flatten()
        .map(|file| file.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            let entry = serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
            Some((path, entry))
        })
        .collect()
}

/// A process's place in tmux, as the registry records it:
/// `<session>:@<window id>.%<pane id>`, e.g. `0:@2.%2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxLocation {
    pub session: String,
    /// `@2`: stable for the window's lifetime, unlike its index.
    pub window_id: String,
    /// `%2`
    pub pane_id: String,
}

impl TmuxLocation {
    /// Parse `<session>:@<window>.%<pane>`. The session name may itself contain
    /// `:`, so the split is at the last `:@`.
    pub fn parse(text: &str) -> Option<TmuxLocation> {
        let split = text.rfind(":@")?;
        let (session, rest) = (&text[..split], &text[split + 1..]);
        let (window_id, pane_id) = rest.split_once('.')?;
        let digits = |id: &str, sigil: char| {
            id.strip_prefix(sigil).is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
            })
        };
        (!session.is_empty() && digits(window_id, '@') && digits(pane_id, '%')).then(|| {
            TmuxLocation {
                session: session.to_owned(),
                window_id: window_id.to_owned(),
                pane_id: pane_id.to_owned(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_entry_as_claude_2_1_writes_it() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(
            sessions.join("78136401.json"),
            r#"{"pid":78136401,"sessionId":"5d0f…","cwd":"/home/m/brain","startedAt":1,
                "procStart":"78136401","version":"2.1.280","peerProtocol":1,"peerFeatures":[],
                "kind":"interactive","entrypoint":"cli",
                "pidDomain":"linux:1041:pid:[4026531836]","tmux":"0:@1.%1",
                "messagingSocketPath":"/tmp/x.sock","name":"Brain-Dev-Server (xJOPA)",
                "nameSource":"user","status":"idle","bridgeSessionId":"b"}"#,
        )
        .unwrap();
        fs::write(sessions.join("78136401.abc.key"), "not json").unwrap();
        fs::write(sessions.join("99.json"), "{\"pid\":").unwrap();

        let entries = read_registry(dir.path());
        assert_eq!(entries.len(), 1);
        let entry = &entries[0].1;
        assert_eq!(entry.pid, 78136401);
        assert_eq!(entry.entrypoint.as_deref(), Some("cli"));
        assert!(entry.named_by_user());
        assert!(entry.bridge_session_id.is_some());
        assert_eq!(
            entry.tmux_location(),
            Some(TmuxLocation {
                session: "0".into(),
                window_id: "@1".into(),
                pane_id: "%1".into(),
            })
        );
    }

    #[test]
    fn a_missing_sessions_folder_is_no_entries() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_registry(dir.path()).is_empty());
    }

    #[test]
    fn parses_tmux_locations_and_rejects_malformed_ones() {
        let parsed = TmuxLocation::parse("my:session:@12.%40").unwrap();
        assert_eq!(parsed.session, "my:session");
        assert_eq!(parsed.window_id, "@12");
        assert_eq!(parsed.pane_id, "%40");
        for bad in [
            "", "0", "0:@", "0:@1", "0:@1.%", ":@1.%1", "0:@x.%1", "0:1.%1", "0:@1.1",
        ] {
            assert_eq!(TmuxLocation::parse(bad), None, "{bad}");
        }
    }
}
