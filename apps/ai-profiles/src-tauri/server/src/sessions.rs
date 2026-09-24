//! An account's sessions: its transcripts, joined with the registry of
//! running `claude` processes to say which are open, where in tmux, and
//! whether Remote Control is connected.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use ai_profiles_core::api::{RemoteSession, TmuxWindow};
use ai_profiles_core::registry::{read_registry, RegistryEntry};
use ai_profiles_core::transcript::{read_transcript, transcripts, TranscriptInfo};

use crate::accounts::AccountDir;
use crate::procs::ProcessTable;

/// Parsed transcripts, keyed by path and valid while the file's mtime and
/// size are unchanged: listing reads every transcript, and most don't change
/// between two lists.
#[derive(Default)]
pub struct TranscriptCache(Mutex<HashMap<PathBuf, (SystemTime, u64, TranscriptInfo)>>);

impl TranscriptCache {
    /// What the transcript at `path` says, read again only once it changed.
    pub fn info(&self, path: &std::path::Path) -> Option<TranscriptInfo> {
        let metadata = std::fs::metadata(path).ok()?;
        self.read(
            &path.to_path_buf(),
            metadata.modified().ok()?,
            metadata.len(),
        )
    }

    fn read(&self, path: &PathBuf, modified: SystemTime, size: u64) -> Option<TranscriptInfo> {
        if let Some((cached_at, cached_size, info)) = self.0.lock().ok()?.get(path) {
            if *cached_at == modified && *cached_size == size {
                return Some(info.clone());
            }
        }
        let info = read_transcript(path).ok()?;
        if let Ok(mut cache) = self.0.lock() {
            cache.insert(path.clone(), (modified, size, info.clone()));
        }
        Some(info)
    }
}

/// The live `claude` processes of an account, by session id: its registry
/// entries whose session Claude lists as live under that same pid (and whose
/// process is still there: Claude's answer is kept a moment, and a session
/// stopped since would still be in it), or, when Claude can't say, whose
/// process checks out on its own.
pub fn running(
    account: &AccountDir,
    processes: &dyn ProcessTable,
) -> HashMap<String, RegistryEntry> {
    let live = processes.live_sessions(account);
    read_registry(&account.dir)
        .into_iter()
        .filter(|(_, entry)| match &live {
            Some(live) => entry
                .session_id
                .as_ref()
                .and_then(|id| live.get(id))
                .is_some_and(|pid| *pid == entry.pid && processes.is_live_claude(entry)),
            None => processes.is_live_claude(entry),
        })
        .filter_map(|(_, entry)| Some((entry.session_id.clone()?, entry)))
        .collect()
}

/// How long a session is given to end once asked, before it is made to.
/// Short: a restart waits for this and for the new process together, within
/// the app's request timeout, and Claude ends at once on SIGTERM.
const STOP_GRACE: Duration = Duration::from_secs(4);
/// How long a process that was made to end is given to be gone.
const KILL_GRACE: Duration = Duration::from_secs(2);

/// End the running session `id` of `account`, the way closing its terminal
/// would: SIGTERM, then SIGKILL if it's still there after [`STOP_GRACE`].
/// Only the `claude` process is signalled, never the tmux pane around it,
/// which may be someone's shell. Returns the registry entry it had, or
/// `None` when it wasn't running.
pub fn stop(
    account: &AccountDir,
    id: &str,
    processes: &dyn ProcessTable,
) -> Result<Option<RegistryEntry>, String> {
    let Some(entry) = running(account, processes).remove(id) else {
        return Ok(None);
    };
    for (force, grace) in [(false, STOP_GRACE), (true, KILL_GRACE)] {
        processes.signal(&entry, force);
        let deadline = Instant::now() + grace;
        loop {
            if !processes.is_live_claude(&entry) {
                return Ok(Some(entry));
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    Err(format!("Claude (process {}) didn't stop.", entry.pid))
}

/// The account's sessions, newest first. Sessions nothing happened in are
/// left out, unless they're running (a session just started is empty until
/// its first message).
pub fn list(
    account: &AccountDir,
    processes: &dyn ProcessTable,
    cache: &TranscriptCache,
) -> Vec<RemoteSession> {
    let running = running(account, processes);
    let mut sessions: Vec<(SystemTime, RemoteSession)> = Vec::new();
    let mut listed = std::collections::HashSet::new();
    for (_, id, path) in transcripts(&account.dir) {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let Some(info) = cache.read(&path, modified, metadata.len()) else {
            continue;
        };
        let live = running.get(&id);
        if info.is_empty() && live.is_none() {
            continue;
        }
        listed.insert(id.clone());
        sessions.push((modified, summary(id, &info, live, modified, metadata.len())));
    }
    // Claude writes a session's transcript with its first message; one
    // that's running but hasn't had one yet is listed from the registry.
    for (id, entry) in &running {
        if listed.contains(id) {
            continue;
        }
        let registered = fs::metadata(
            account
                .dir
                .join("sessions")
                .join(format!("{}.json", entry.pid)),
        )
        .and_then(|metadata| metadata.modified())
        .unwrap_or_else(|_| SystemTime::now());
        sessions.push((
            registered,
            summary(
                id.clone(),
                &TranscriptInfo::default(),
                Some(entry),
                registered,
                0,
            ),
        ));
    }
    sessions.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    sessions.into_iter().map(|(_, session)| session).collect()
}

fn summary(
    id: String,
    info: &TranscriptInfo,
    live: Option<&RegistryEntry>,
    modified: SystemTime,
    size: u64,
) -> RemoteSession {
    let user_named_live = live.filter(|entry| entry.named_by_user());
    RemoteSession {
        title: info
            .custom_title
            .clone()
            .or_else(|| user_named_live.and_then(|entry| entry.name.clone()))
            .or_else(|| info.ai_title.clone())
            // What Remote Control calls it (after its folder, unless named).
            .or_else(|| live.and_then(|entry| entry.name.clone())),
        named: info.custom_title.is_some() || user_named_live.is_some(),
        cwd: live
            .and_then(|entry| entry.cwd.clone())
            .or_else(|| info.cwd.clone()),
        last_prompt: info.last_prompt.clone(),
        updated_at: chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339(),
        size_bytes: size,
        running: live.is_some(),
        window: live
            .and_then(|entry| entry.tmux_location())
            .map(|location| TmuxWindow {
                session: location.session,
                window_id: location.window_id,
                pane_id: location.pane_id,
            }),
        remote_control: live.is_some_and(|entry| entry.bridge_session_id.is_some()),
        remote_control_connecting: false,
        bridge_session_id: live.and_then(|entry| entry.bridge_session_id.clone()),
        waiting: false,
        empty: info.is_empty(),
        claude_version: live.and_then(|entry| entry.version.clone()),
        update_pending: false,
        id,
    }
}

/// Pure: whether version `installed` is newer than `running`, comparing
/// their dotted numbers (`2.1.281` > `2.1.280`, `2.10.0` > `2.9.9`). Either
/// one unreadable is not newer.
pub fn newer_version(installed: &str, running: &str) -> bool {
    fn numbers(version: &str) -> Option<Vec<u64>> {
        version
            .trim()
            .split(['.', '-', ' '])
            .take_while(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .map(|part| part.parse().ok())
            .collect::<Option<Vec<u64>>>()
            .filter(|parts| !parts.is_empty())
    }
    match (numbers(installed), numbers(running)) {
        (Some(installed), Some(running)) => installed > running,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_installed_version_is_one_with_higher_numbers() {
        assert!(newer_version("2.1.281", "2.1.280"));
        assert!(newer_version("2.10.0", "2.9.9"));
        assert!(newer_version("2.1.281 (Claude Code)", "2.1.280"));
        assert!(!newer_version("2.1.280", "2.1.280"));
        assert!(!newer_version("2.1.279", "2.1.280"));
        assert!(!newer_version("", "2.1.280"));
        assert!(!newer_version("2.1.281", "unknown"));
    }
    use crate::procs::testing::FakeProcesses;
    use std::collections::HashSet;
    use std::path::Path;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    const NAMED: &str = "11111111-1111-1111-1111-111111111111";
    const OPEN: &str = "22222222-2222-2222-2222-222222222222";
    const EMPTY: &str = "33333333-3333-3333-3333-333333333333";

    fn account(root: &Path) -> AccountDir {
        let dir = root.join("work");
        let project = dir.join("projects/-home-m-code");
        write(
            &project.join(format!("{NAMED}.jsonl")),
            concat!(
                r#"{"type":"user","cwd":"/home/m/code"}"#,
                "\n",
                r#"{"type":"assistant"}"#,
                "\n",
                r#"{"type":"custom-title","customTitle":"Billing fix"}"#,
                "\n",
                r#"{"type":"last-prompt","lastPrompt":"ship it"}"#,
                "\n"
            ),
        );
        write(
            &project.join(format!("{OPEN}.jsonl")),
            concat!(
                r#"{"type":"user","cwd":"/home/m/code"}"#,
                "\n",
                r#"{"type":"assistant"}"#,
                "\n"
            ),
        );
        write(
            &project.join(format!("{EMPTY}.jsonl")),
            "{\"type\":\"user\"}\n",
        );
        write(
            &dir.join("sessions/4242.json"),
            &format!(
                r#"{{"pid":4242,"sessionId":"{OPEN}","cwd":"/home/m/other","tmux":"ai:@3.%5",
                    "name":"Deploy","nameSource":"user","bridgeSessionId":"b"}}"#
            ),
        );
        write(
            &dir.join("sessions/9999.json"),
            &format!(r#"{{"pid":9999,"sessionId":"{NAMED}"}}"#),
        );
        AccountDir {
            name: "work".into(),
            dir,
            is_default: false,
        }
    }

    #[test]
    fn joins_transcripts_with_the_live_registry() {
        let root = tempfile::tempdir().unwrap();
        let account = account(root.path());
        // 9999's file is stale: that process is gone.
        let processes = FakeProcesses(HashSet::from([4242]));
        let sessions = list(&account, &processes, &TranscriptCache::default());

        assert_eq!(sessions.len(), 2, "the empty session is hidden");
        let named = sessions.iter().find(|s| s.id == NAMED).unwrap();
        assert_eq!(named.title.as_deref(), Some("Billing fix"));
        assert!(named.named);
        assert!(!named.running);
        assert_eq!(named.window, None);
        assert_eq!(named.last_prompt.as_deref(), Some("ship it"));

        let open = sessions.iter().find(|s| s.id == OPEN).unwrap();
        assert!(open.running);
        assert!(open.named, "named in the app, so Remote Control uses it");
        assert_eq!(open.title.as_deref(), Some("Deploy"));
        assert_eq!(open.cwd.as_deref(), Some("/home/m/other"));
        assert!(open.remote_control);
        assert_eq!(
            open.window,
            Some(TmuxWindow {
                session: "ai".into(),
                window_id: "@3".into(),
                pane_id: "%5".into()
            })
        );
    }

    #[test]
    fn lists_a_running_session_before_its_first_message() {
        let root = tempfile::tempdir().unwrap();
        let account = account(root.path());
        const FRESH: &str = "44444444-4444-4444-4444-444444444444";
        write(
            &account.dir.join("sessions/5151.json"),
            &format!(
                r#"{{"pid":5151,"sessionId":"{FRESH}","cwd":"/home/m/david","name":"david","nameSource":"derived","tmux":"ai:@10.%12"}}"#
            ),
        );
        // Registered after every transcript was last written.
        fs::File::options()
            .write(true)
            .open(account.dir.join("sessions/5151.json"))
            .unwrap()
            .set_modified(SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
        let processes = FakeProcesses(HashSet::from([5151]));
        let sessions = list(&account, &processes, &TranscriptCache::default());
        let fresh = sessions.iter().find(|s| s.id == FRESH).expect("listed");
        assert!(fresh.running);
        assert!(fresh.empty, "gone once stopped, so the app can say so");
        assert!(
            sessions.iter().filter(|s| s.id != FRESH).all(|s| !s.empty),
            "the rest have something to resume"
        );
        assert_eq!(
            fresh.title.as_deref(),
            Some("david"),
            "what Remote Control calls it"
        );
        assert!(!fresh.named);
        assert_eq!(fresh.cwd.as_deref(), Some("/home/m/david"));
        assert_eq!(
            fresh.window.as_ref().map(|w| w.window_id.as_str()),
            Some("@10")
        );
        assert_eq!(sessions[0].id, FRESH, "newest first");
    }

    #[test]
    fn the_cache_notices_a_transcript_that_changed() {
        let root = tempfile::tempdir().unwrap();
        let account = account(root.path());
        let processes = FakeProcesses(HashSet::new());
        let cache = TranscriptCache::default();
        assert_eq!(list(&account, &processes, &cache).len(), 2);
        write(
            &account
                .dir
                .join(format!("projects/-home-m-code/{EMPTY}.jsonl")),
            "{\"type\":\"user\"}\n{\"type\":\"assistant\"}\n",
        );
        assert_eq!(list(&account, &processes, &cache).len(), 3);
    }

    /// Claude listing an account's live sessions itself.
    struct ClaudeSays(HashMap<String, i32>);

    impl ProcessTable for ClaudeSays {
        fn is_live_claude(&self, entry: &RegistryEntry) -> bool {
            // Only asked of what Claude lists: 102 has ended since.
            entry.pid != 102
        }

        fn signal(&self, _entry: &RegistryEntry, _force: bool) -> bool {
            false
        }

        fn live_sessions(&self, _account: &AccountDir) -> Option<HashMap<String, i32>> {
            Some(self.0.clone())
        }
    }

    #[test]
    fn claude_says_which_registered_sessions_are_running() {
        let dir = tempfile::tempdir().unwrap();
        let account = AccountDir {
            name: "work".into(),
            dir: dir.path().to_path_buf(),
            is_default: false,
        };
        for (pid, id) in [(101, NAMED), (102, OPEN), (103, EMPTY), (104, NAMED)] {
            write(
                &dir.path().join(format!("sessions/{pid}.json")),
                &format!(r#"{{"pid":{pid},"sessionId":"{id}"}}"#),
            );
        }
        // EMPTY isn't listed. OPEN is listed under its pid, but was stopped
        // after Claude said so: its process is gone.
        let claude = ClaudeSays(HashMap::from([
            (NAMED.to_owned(), 101),
            (OPEN.to_owned(), 102),
        ]));
        let running = running(&account, &claude);
        assert_eq!(running.len(), 1);
        assert_eq!(running.get(NAMED).map(|entry| entry.pid), Some(101));
    }
}
