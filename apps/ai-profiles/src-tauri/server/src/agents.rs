//! Which sessions are running, as Claude itself says: `claude agents --json`
//! lists an account's live sessions, interactive and background, with the pid
//! each runs as. That is Claude's own view, and supported for scripting, so
//! it decides what counts as running. The registry (`sessions/<pid>.json`)
//! still supplies what the list leaves out: the tmux window, the Remote
//! Control id, whether the user chose the name.
//!
//! Each call starts `claude`, which takes a third of a second, so an
//! account's answer is kept for a moment: a list, a resume and the revive
//! bookkeeping can all ask within the same second. When `claude` can't say
//! (not found, too old for `agents --json`, too slow), there's no answer, and
//! each registry entry is checked on its own, as before.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ai_profiles_core::registry::RegistryEntry;
use serde::Deserialize;

use crate::accounts::AccountDir;
use crate::procs::{ProcessTable, SystemProcesses};
use ai_profiles_core::child::run_within;

/// How long an account's answer is kept.
const FRESH_FOR: Duration = Duration::from_secs(2);
/// How long `claude agents --json` is given.
const TIMEOUT: Duration = Duration::from_secs(10);

/// One entry of `claude agents --json`: only what says which session runs
/// as which process.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Agent {
    session_id: Option<String>,
    pid: Option<i32>,
}

/// Pure: session id → pid, from `claude agents --json`'s output. `None`
/// when it isn't that list.
pub fn parse(json: &str) -> Option<HashMap<String, i32>> {
    let agents: Vec<Agent> = serde_json::from_str(json).ok()?;
    Some(
        agents
            .into_iter()
            .filter_map(|agent| Some((agent.session_id?, agent.pid?)))
            .collect(),
    )
}

/// An account's live sessions, session id → pid; `None` when Claude couldn't say.
type Answer = Option<HashMap<String, i32>>;

/// The machine's processes, with Claude saying which sessions are live.
pub struct ClaudeAgents {
    claude: Option<PathBuf>,
    processes: SystemProcesses,
    /// Each account's last answer, by config dir, with when it was given.
    answers: Mutex<HashMap<PathBuf, (Instant, Answer)>>,
}

impl ClaudeAgents {
    /// `claude` is where the server found it; without one, Claude is never
    /// asked.
    pub fn new(claude: Option<PathBuf>) -> ClaudeAgents {
        ClaudeAgents {
            claude,
            processes: SystemProcesses,
            answers: Mutex::new(HashMap::new()),
        }
    }
}

impl ProcessTable for ClaudeAgents {
    fn is_live_claude(&self, entry: &RegistryEntry) -> bool {
        self.processes.is_live_claude(entry)
    }

    fn signal(&self, entry: &RegistryEntry, force: bool) -> bool {
        self.processes.signal(entry, force)
    }

    fn live_sessions(&self, account: &AccountDir) -> Option<HashMap<String, i32>> {
        let claude = self.claude.as_deref()?;
        let mut answers = self
            .answers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((at, answer)) = answers.get(&account.dir) {
            if at.elapsed() < FRESH_FOR {
                return answer.clone();
            }
        }
        let answer = ask(claude, account);
        answers.insert(account.dir.clone(), (Instant::now(), answer.clone()));
        answer
    }

    fn changed(&self, account: &AccountDir) {
        self.answers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&account.dir);
    }
}

/// Run `claude agents --json` as `account`: on its config dir, or with none
/// set for the stock install, whatever the server itself was started with.
fn ask(claude: &Path, account: &AccountDir) -> Answer {
    let mut command = Command::new(claude);
    command.args(["agents", "--json"]);
    if account.is_default {
        command.env_remove("CLAUDE_CONFIG_DIR");
    } else {
        command.env("CLAUDE_CONFIG_DIR", &account.dir);
    }
    let finished = run_within(&mut command, Vec::new(), TIMEOUT).ok()??;
    if !finished.status.success() {
        return None;
    }
    parse(&String::from_utf8_lossy(&finished.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_which_session_runs_as_which_process() {
        let json = r#"[
            {"cwd":"/w","kind":"interactive","name":"a","pid":101,"sessionId":"s1","startedAt":1,"status":"idle"},
            {"cwd":"/w","kind":"background","id":"ea8df9cb","name":"b","pid":102,"sessionId":"s2","state":"blocked","status":"idle"},
            {"kind":"interactive","name":"no id"}
        ]"#;
        let live = parse(json).unwrap();
        assert_eq!(live.len(), 2);
        assert_eq!(live.get("s1"), Some(&101));
        assert_eq!(live.get("s2"), Some(&102));
        assert_eq!(parse("[]").map(|live| live.len()), Some(0));
        assert!(parse("not json").is_none());
        assert!(parse(r#"{"error":"unknown command"}"#).is_none());
    }

    #[test]
    fn without_claude_there_is_no_answer() {
        let agents = ClaudeAgents::new(None);
        let account = AccountDir {
            name: "work".into(),
            dir: PathBuf::from("/nowhere"),
            is_default: false,
        };
        assert!(agents.live_sessions(&account).is_none());
    }
}
