//! Sessions a running `claude` process has open.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

/// What has a session open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveHolder {
    /// A `claude` run from a terminal, which only the user can close.
    Terminal,
    /// The desktop app's Code tab, which lets go when the app quits.
    Desktop,
}

/// The fields of a `<config>/sessions/<pid>.json` registry entry read here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    /// The `claude` process that wrote the entry.
    pid: i32,
    /// The session it has open.
    session_id: String,
    /// How it was started: `"claude-desktop"` for the desktop app's Code tab.
    entrypoint: Option<String>,
}

/// The sessions open in `config_dir`, by session id, given the output of
/// `ps -ax -o pid=,command=`.
///
/// A running `claude` keeps an entry in `<config>/sessions/<pid>.json`, and an
/// entry whose pid is no longer running is left behind by one that crashed, so
/// only entries whose pid `ps_output` lists count. Files that aren't a readable
/// entry are skipped. A session open in a terminal and the desktop app at once
/// is held by the terminal, the holder that only the user can close.
#[allow(dead_code)] // Consumed by the sessions commands.
pub fn live_sessions(config_dir: &Path, ps_output: &str) -> HashMap<String, LiveHolder> {
    let running = running_pids(ps_output);
    let Ok(files) = fs::read_dir(config_dir.join("sessions")) else {
        return HashMap::new();
    };
    let mut live = HashMap::new();
    for file in files.flatten() {
        let path = file.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Some(entry) = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<RegistryEntry>(&text).ok())
        else {
            continue;
        };
        if !running.contains(&entry.pid) {
            continue;
        }
        let holder = if entry.entrypoint.as_deref() == Some("claude-desktop") {
            LiveHolder::Desktop
        } else {
            LiveHolder::Terminal
        };
        let held = live.entry(entry.session_id).or_insert(holder);
        if holder == LiveHolder::Terminal {
            *held = holder;
        }
    }
    live
}

/// The pids listed in `ps -ax -o pid=,command=` output: the first word of each
/// line.
fn running_pids(ps_output: &str) -> HashSet<i32> {
    ps_output
        .lines()
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    fn write_registry(config_dir: &Path, pid: i32, session: &str, entrypoint: &str) {
        let entry = json!({
            "pid": pid,
            "sessionId": session,
            "cwd": "/work",
            "status": "idle",
            "updatedAt": 1_758_600_000_000_i64,
            "entrypoint": entrypoint,
        });
        let dir = config_dir.join("sessions");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{pid}.json")), entry.to_string()).unwrap();
    }

    const PS_OUTPUT: &str = "    1 /sbin/launchd\n  \
        4100 claude --resume\n  \
        4200 /Users/me/Library/Application Support/Claude/claude-code/2.1.9/claude\n";

    #[test]
    fn a_running_process_holds_its_session_by_entrypoint() {
        let root = tempdir().unwrap();
        write_registry(root.path(), 4100, "in-terminal", "cli");
        write_registry(root.path(), 4200, "in-desktop", "claude-desktop");

        let live = live_sessions(root.path(), PS_OUTPUT);

        assert_eq!(
            live,
            HashMap::from([
                ("in-terminal".to_string(), LiveHolder::Terminal),
                ("in-desktop".to_string(), LiveHolder::Desktop),
            ])
        );
    }

    #[test]
    fn a_session_whose_process_is_gone_is_not_live() {
        let root = tempdir().unwrap();
        write_registry(root.path(), 4300, "closed", "cli");

        assert!(live_sessions(root.path(), PS_OUTPUT).is_empty());
    }

    #[test]
    fn a_registry_file_that_is_not_json_is_skipped() {
        let root = tempdir().unwrap();
        write_registry(root.path(), 4100, "in-terminal", "cli");
        fs::write(
            root.path().join("sessions").join("4200.json"),
            "{\"pid\":42",
        )
        .unwrap();

        let live = live_sessions(root.path(), PS_OUTPUT);

        assert_eq!(
            live,
            HashMap::from([("in-terminal".to_string(), LiveHolder::Terminal)])
        );
    }

    #[test]
    fn a_session_open_in_a_terminal_and_the_desktop_app_is_held_by_the_terminal() {
        let root = tempdir().unwrap();
        write_registry(root.path(), 4100, "both", "cli");
        write_registry(root.path(), 4200, "both", "claude-desktop");

        let live = live_sessions(root.path(), PS_OUTPUT);

        assert_eq!(live.get("both"), Some(&LiveHolder::Terminal));
    }

    #[test]
    fn a_config_dir_without_a_registry_has_no_live_sessions() {
        let root = tempdir().unwrap();

        assert!(live_sessions(root.path(), PS_OUTPUT).is_empty());
    }
}
