//! Bringing back the sessions that were running when the machine rebooted.
//!
//! Restarting the server leaves tmux, and every Claude in it, running. A
//! reboot, or tmux itself going away, ends them all. So the server keeps a
//! list of the sessions running in tmux ([`remember`], every half minute and
//! whenever it stops one), and when it starts ([`revive`]) after a reboot, or
//! finds its tmux session gone, it resumes each of them that isn't running,
//! in its folder, with Remote Control, as Resume would.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::accounts;
use crate::routes::{resume_held, ServerState};
use crate::sessions;

/// How often the list of running sessions is written down.
pub const REMEMBER_EVERY: Duration = Duration::from_secs(30);

const FILE: &str = "running.json";

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    /// The kernel's id for this boot, so a later start can tell it rebooted.
    boot_id: Option<String>,
    sessions: Vec<Remembered>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Remembered {
    account: String,
    id: String,
}

fn path(state: &ServerState) -> PathBuf {
    state.state_dir.join(FILE)
}

fn boot_id() -> Option<String> {
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|id| id.trim().to_owned())
}

/// Write down the sessions running in tmux now, in every account.
pub fn remember(state: &ServerState) {
    let mut remembered = Vec::new();
    for account in accounts::discover(&state.config) {
        for (id, entry) in sessions::running(&account, state.processes.as_ref()) {
            if entry.tmux_location().is_some() {
                remembered.push(Remembered {
                    account: account.name.clone(),
                    id,
                });
            }
        }
    }
    remembered.sort_by(|a, b| (&a.account, &a.id).cmp(&(&b.account, &b.id)));
    let snapshot = Snapshot {
        boot_id: boot_id(),
        sessions: remembered,
    };
    let Ok(text) = serde_json::to_string_pretty(&snapshot) else {
        return;
    };
    let tmp = path(state).with_extension("json.tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, path(state));
    }
}

/// Pure: whether what was running before should be brought back: the
/// machine has rebooted since it was written down, or tmux has lost the
/// server's session.
fn gone(previous_boot: Option<&str>, current_boot: Option<&str>, tmux_has_session: bool) -> bool {
    let rebooted =
        matches!((previous_boot, current_boot), (Some(before), Some(now)) if before != now);
    rebooted || !tmux_has_session
}

/// On start: resume what was running before a reboot, or before tmux went
/// away. Returns `(account, session, outcome)` for each one it tried.
pub fn revive(state: &ServerState) -> Vec<(String, String, Result<(), String>)> {
    let Some(snapshot) = fs::read_to_string(path(state))
        .ok()
        .and_then(|text| serde_json::from_str::<Snapshot>(&text).ok())
    else {
        return Vec::new();
    };
    if snapshot.sessions.is_empty()
        || !gone(
            snapshot.boot_id.as_deref(),
            boot_id().as_deref(),
            state.tmux.has_session(),
        )
    {
        return Vec::new();
    }
    let mut outcomes = Vec::new();
    for remembered in snapshot.sessions {
        let Some(account) = accounts::find(&state.config, &remembered.account) else {
            continue;
        };
        if sessions::running(&account, state.processes.as_ref()).contains_key(&remembered.id) {
            continue;
        }
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let outcome = resume_held(state, &account, &remembered.id, true)
            .map(|_| ())
            .map_err(|err| err.message);
        outcomes.push((remembered.account, remembered.id, outcome));
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brings_sessions_back_after_a_reboot_or_when_tmux_lost_them() {
        assert!(gone(Some("a"), Some("b"), true), "rebooted");
        assert!(gone(Some("a"), Some("a"), false), "tmux lost its session");
        assert!(
            !gone(Some("a"), Some("a"), true),
            "only the server restarted"
        );
        assert!(!gone(None, None, true), "no boot id to compare, tmux fine");
    }
}
