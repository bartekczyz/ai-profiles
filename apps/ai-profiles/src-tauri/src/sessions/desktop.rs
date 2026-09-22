//! The Claude desktop app's records of its Code tab sessions.
//!
//! Each record is `<gui-data>/claude-code-sessions/<account>/<org>/local_<uuid>.json`
//! and names its transcript by `cliSessionId`. The app keeps them in memory
//! while it runs, so they are only written while it is closed.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use super::{parse_process_list, Home};
use crate::app_kind::CLAUDE;
use crate::error::{AppError, AppResult};
use crate::launch::{find_running_pid, process_list};
use crate::paths::resolve_gui_app;

const RECORDS_DIR: &str = "claude-code-sessions";

/// Fields that describe the account or the moment a session last ran, not the
/// session. The app fills them in again for the account it opens under:
/// connectors, directories approved in that app, and snapshots of the prompt
/// and tools it was started with.
const ACCOUNT_BOUND_FIELDS: &[&str] = &[
    "remoteMcpServersConfig",
    "sessionPermissionUpdates",
    "alwaysAllowedReasons",
    "promptAppendSnapshot",
    "toolSurfaceSnapshot",
    "spawnSeed",
];

/// One record, with the fields this module looks at.
#[derive(Debug, Clone)]
pub(crate) struct DesktopRecord {
    pub path: PathBuf,
    pub cli_session_id: String,
    pub title: Option<String>,
    pub archived: bool,
    pub body: Map<String, Value>,
}

/// Every record under `gui_data_dir`, for any account.
pub(crate) fn records(gui_data_dir: &Path) -> Vec<DesktopRecord> {
    let mut found = Vec::new();
    for org_dir in account_org_dirs(gui_data_dir) {
        let Ok(files) = fs::read_dir(&org_dir) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let is_record = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("local_") && name.ends_with(".json"));
            if !is_record {
                continue;
            }
            let Some(body) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Map<String, Value>>(&text).ok())
            else {
                continue;
            };
            let Some(cli_session_id) = body.get("cliSessionId").and_then(Value::as_str) else {
                continue;
            };
            found.push(DesktopRecord {
                cli_session_id: cli_session_id.to_string(),
                title: body
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                archived: body
                    .get("isArchived")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                path,
                body,
            });
        }
    }
    found
}

/// `claude-code-sessions/<account>/<org>` folders under `gui_data_dir`.
fn account_org_dirs(gui_data_dir: &Path) -> Vec<PathBuf> {
    let Ok(accounts) = fs::read_dir(gui_data_dir.join(RECORDS_DIR)) else {
        return Vec::new();
    };
    accounts
        .flatten()
        .filter(|account| account.file_type().is_ok_and(|kind| kind.is_dir()))
        .flat_map(|account| {
            fs::read_dir(account.path())
                .into_iter()
                .flatten()
                .flatten()
                .filter(|org| org.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|org| org.path())
        })
        .collect()
}

/// The folder `home`'s desktop app keeps its records in for the account it is
/// signed in to, or `None` if that cannot be told (never signed in).
///
/// The desktop app names the account in `config.json`. The org comes from the
/// profile's `.claude.json` when that describes the same account, else from the
/// only org folder the app has made for it.
pub(crate) fn records_dir(home: &Home) -> Option<PathBuf> {
    let config: Value = read_json(&home.gui_data_dir.join("config.json"))?;
    let account = config.get("lastKnownAccountUuid")?.as_str()?.to_string();
    if !super::scan::is_safe_name(&account) {
        return None;
    }
    let account_dir = home.gui_data_dir.join(RECORDS_DIR).join(&account);

    let org = home
        .claude_json_candidates()
        .iter()
        .filter_map(|path| read_json(path))
        .find_map(|claude_json| {
            let oauth = claude_json.get("oauthAccount")?;
            (oauth.get("accountUuid")?.as_str()? == account)
                .then(|| oauth.get("organizationUuid")?.as_str().map(str::to_string))?
        })
        .or_else(|| {
            let orgs: Vec<_> = fs::read_dir(&account_dir)
                .ok()?
                .flatten()
                .filter(|org| org.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|org| org.file_name().to_string_lossy().into_owned())
                .collect();
            (orgs.len() == 1).then(|| orgs[0].clone())
        })?;
    super::scan::is_safe_name(&org).then(|| account_dir.join(org))
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// The pid of `home`'s desktop app, if it is running.
///
/// A profile's app, and the stock app when ai-profiles opened it, carry
/// `--user-data-dir`. The stock app opened from the Dock or Finder carries no
/// arguments at all.
pub(crate) fn app_pid(home: &Home, processes: &HashMap<i32, String>) -> Option<i32> {
    let ps_output: String = processes
        .iter()
        .map(|(pid, command)| format!("{pid} {command}\n"))
        .collect();
    let data_dir = home.gui_data_dir.display().to_string();
    let bound = CLAUDE
        .gui_bundle_candidates
        .iter()
        .find_map(|candidate| find_running_pid(&ps_output, &data_dir, candidate.macos_exec));
    if bound.is_some() || !home.stock {
        return bound;
    }
    let app = resolve_gui_app(&CLAUDE)?;
    let exec = app
        .bundle_path
        .join("Contents/MacOS")
        .join(app.macos_exec)
        .display()
        .to_string();
    processes
        .iter()
        .find(|(_, command)| **command == exec)
        .map(|(pid, _)| *pid)
}

pub(crate) fn app_running(home: &Home, processes: &HashMap<i32, String>) -> bool {
    app_pid(home, processes).is_some()
}

/// How long [`quit_app`] waits for the app to finish quitting.
const QUIT_WAIT: Duration = Duration::from_secs(15);

/// Quit `home`'s desktop app, if it is running, and wait until it has gone.
///
/// Sends SIGTERM to that one process, which Electron treats as a normal quit:
/// windows close and state is saved, as with ⌘Q. Other profiles' apps share
/// the bundle id, so asking the app to quit by name would quit them all.
pub(crate) fn quit_app(home: &Home) -> AppResult<()> {
    let Some(pid) = app_pid(home, &parse_process_list(&process_list()?)) else {
        return Ok(());
    };
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "Claude ({}) could not be quit. Quit it yourself and try again.",
            home.label
        )));
    }
    let deadline = Instant::now() + QUIT_WAIT;
    while Instant::now() < deadline {
        if !parse_process_list(&process_list()?).contains_key(&pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(AppError::Validation(format!(
        "Claude ({}) didn't finish quitting. Quit it yourself and try again.",
        home.label
    )))
}

/// What a moved session's record starts from.
pub(crate) struct NewRecord<'a> {
    pub cli_session_id: &'a str,
    pub cwd: &'a str,
    /// The session's name, for a record that has none: see
    /// [`super::scan::TranscriptInfo::name`].
    pub title: Option<&'a str>,
    /// The title was set by the user rather than generated.
    pub title_from_user: bool,
    pub created_at_ms: i64,
    pub last_activity_ms: i64,
}

/// A record for the destination app: the source's record with its
/// account-bound fields dropped when there is one, else the fields the app
/// needs to list and open the session.
pub(crate) fn build_record(
    source: Option<&Map<String, Value>>,
    new: &NewRecord,
) -> Map<String, Value> {
    let mut record = match source {
        Some(source) => {
            let mut record = source.clone();
            for field in ACCOUNT_BOUND_FIELDS {
                record.remove(*field);
            }
            record
        }
        None => {
            let mut record = Map::new();
            record.insert("cwd".into(), new.cwd.into());
            record.insert("originCwd".into(), new.cwd.into());
            record.insert("createdAt".into(), new.created_at_ms.into());
            record.insert("lastActivityAt".into(), new.last_activity_ms.into());
            record.insert("lastFocusedAt".into(), new.last_activity_ms.into());
            record.insert("permissionMode".into(), "default".into());
            record
        }
    };
    // Name it as the source did. Without a title the app shows a placeholder
    // ("General coding session") that isn't stored anywhere, so a copied
    // record that never got one is named here too.
    let untitled = record
        .get("title")
        .and_then(Value::as_str)
        .is_none_or(|title| title.trim().is_empty());
    if let (true, Some(title)) = (untitled, new.title) {
        record.insert("title".into(), title.into());
        let source = if new.title_from_user { "user" } else { "auto" };
        record.insert("titleSource".into(), source.into());
    }
    record.insert(
        "sessionId".into(),
        format!("local_{}", uuid::Uuid::new_v4()).into(),
    );
    record.insert("cliSessionId".into(), new.cli_session_id.into());
    record.insert("isArchived".into(), false.into());
    record
}

/// Write `record` into `dir` under its own `sessionId`, through a temporary
/// file so the app never reads half of it. Returns the record's path.
pub(crate) fn write_record(dir: &Path, record: &Map<String, Value>) -> std::io::Result<PathBuf> {
    let id = record
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{id}.json"));
    let tmp = dir.join(format!(".{id}.json.tmp"));
    fs::write(&tmp, serde_json::to_vec_pretty(record)?)?;
    fs::rename(&tmp, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn home(root: &Path) -> Home {
        Home {
            id: "p".into(),
            label: "P".into(),
            config_dir: root.join("cli-config"),
            gui_data_dir: root.join("gui-data"),
            stock: false,
            desktop: true,
        }
    }

    fn write(path: &Path, value: Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_string(&value).unwrap()).unwrap();
    }

    #[test]
    fn reads_records_of_every_account() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("gui-data").join(RECORDS_DIR);
        write(
            &root.join("acct/org/local_a.json"),
            json!({"cliSessionId": "s1", "title": "One", "isArchived": true}),
        );
        write(
            &root.join("acct2/org/local_b.json"),
            json!({"cliSessionId": "s2"}),
        );
        write(&root.join("acct/org/scheduled-tasks.json"), json!({}));
        write(&root.join("acct/org/local_c.json"), json!({"noId": true}));
        let mut found = records(&dir.path().join("gui-data"));
        found.sort_by(|a, b| a.cli_session_id.cmp(&b.cli_session_id));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].title.as_deref(), Some("One"));
        assert!(found[0].archived);
        assert!(!found[1].archived);
    }

    #[test]
    fn records_dir_takes_the_org_from_claude_json_for_the_same_account() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        write(
            &home.gui_data_dir.join("config.json"),
            json!({"lastKnownAccountUuid": "acct"}),
        );
        write(
            &home.config_dir.join(".claude.json"),
            json!({"oauthAccount": {"accountUuid": "acct", "organizationUuid": "org"}}),
        );
        assert_eq!(
            records_dir(&home),
            Some(home.gui_data_dir.join(RECORDS_DIR).join("acct/org"))
        );
    }

    #[test]
    fn records_dir_falls_back_to_the_only_org_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        write(
            &home.gui_data_dir.join("config.json"),
            json!({"lastKnownAccountUuid": "acct"}),
        );
        write(
            &home.config_dir.join(".claude.json"),
            json!({"oauthAccount": {"accountUuid": "other", "organizationUuid": "wrong"}}),
        );
        fs::create_dir_all(home.gui_data_dir.join(RECORDS_DIR).join("acct/org1")).unwrap();
        assert_eq!(
            records_dir(&home),
            Some(home.gui_data_dir.join(RECORDS_DIR).join("acct/org1"))
        );
        fs::create_dir_all(home.gui_data_dir.join(RECORDS_DIR).join("acct/org2")).unwrap();
        assert_eq!(records_dir(&home), None);
    }

    #[test]
    fn records_dir_is_none_before_sign_in() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(records_dir(&home(dir.path())), None);
    }

    #[test]
    fn a_copied_record_drops_account_bound_fields_and_gets_a_new_id() {
        let source = json!({
            "sessionId": "local_old",
            "cliSessionId": "s",
            "title": "Audit",
            "model": "claude-opus-5",
            "prs": [{"prNumber": 1}],
            "isArchived": true,
            "remoteMcpServersConfig": [{"uuid": "x"}],
            "sessionPermissionUpdates": [],
            "promptAppendSnapshot": {},
            "toolSurfaceSnapshot": {},
        });
        let new = NewRecord {
            cli_session_id: "s",
            cwd: "/w",
            title: None,
            title_from_user: false,
            created_at_ms: 1,
            last_activity_ms: 2,
        };
        let record = build_record(source.as_object(), &new);
        assert_eq!(record["title"], "Audit");
        assert_eq!(record["model"], "claude-opus-5");
        assert_eq!(record["prs"], json!([{"prNumber": 1}]));
        assert_eq!(record["isArchived"], false);
        assert_ne!(record["sessionId"], "local_old");
        assert!(record["sessionId"].as_str().unwrap().starts_with("local_"));
        for field in ACCOUNT_BOUND_FIELDS {
            assert!(!record.contains_key(*field), "{field} kept");
        }
    }

    #[test]
    fn a_copied_record_keeps_its_title_or_gains_the_sessions_name() {
        let new = NewRecord {
            cli_session_id: "s",
            cwd: "/w",
            title: Some("Reply with just ok"),
            title_from_user: false,
            created_at_ms: 1,
            last_activity_ms: 2,
        };
        let titled =
            json!({"cliSessionId": "s", "title": "Named in the app", "titleSource": "user"});
        let record = build_record(titled.as_object(), &new);
        assert_eq!(record["title"], "Named in the app");
        assert_eq!(record["titleSource"], "user");

        let untitled = json!({"cliSessionId": "s", "completedTurns": 1});
        let record = build_record(untitled.as_object(), &new);
        assert_eq!(record["title"], "Reply with just ok");
        assert_eq!(record["titleSource"], "auto");
    }

    #[test]
    fn a_new_record_carries_what_the_app_lists_a_session_by() {
        let new = NewRecord {
            cli_session_id: "s",
            cwd: "/w",
            title: Some("Named"),
            title_from_user: true,
            created_at_ms: 1,
            last_activity_ms: 2,
        };
        let record = build_record(None, &new);
        assert_eq!(record["cliSessionId"], "s");
        assert_eq!(record["cwd"], "/w");
        assert_eq!(record["originCwd"], "/w");
        assert_eq!(record["title"], "Named");
        assert_eq!(record["titleSource"], "user");
        assert_eq!(record["createdAt"], 1);
        assert_eq!(record["lastActivityAt"], 2);
    }

    #[test]
    fn write_record_names_the_file_after_the_session_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut record = Map::new();
        record.insert("sessionId".into(), "local_x".into());
        let path = write_record(&dir.path().join("a/b"), &record).unwrap();
        assert_eq!(path, dir.path().join("a/b/local_x.json"));
        assert_eq!(fs::read_dir(dir.path().join("a/b")).unwrap().count(), 1);
    }

    #[test]
    fn quit_app_ends_only_this_profiles_app_and_waits_for_it() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        fs::create_dir_all(&home.gui_data_dir).unwrap();
        let other_data = dir.path().join("other-gui-data");
        let mut other =
            crate::test_support::fake_wrapper_process(&dir.path().join("o"), &other_data);
        let mut child = crate::test_support::fake_wrapper_process(dir.path(), &home.gui_data_dir);
        let pid = child.id() as i32;
        // `wait` closes stdin, which the stand-in would take as its cue to exit;
        // keep it open so only the quit ends it.
        let _stdin = child.stdin.take();
        // Reap it the way launchd reaps a real app, so it leaves the process list.
        let reaper = thread::spawn(move || {
            let mut child = child;
            child.wait().unwrap()
        });
        assert_eq!(
            app_pid(&home, &parse_process_list(&process_list().unwrap())),
            Some(pid)
        );

        quit_app(&home).unwrap();

        assert!(!reaper.join().unwrap().success());
        assert!(
            other.try_wait().unwrap().is_none(),
            "another profile's app was quit"
        );
        other.kill().unwrap();
        other.wait().unwrap();
        // Nothing running: a no-op.
        quit_app(&home).unwrap();
    }

    #[test]
    fn app_running_matches_only_this_profiles_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        let data_dir = home.gui_data_dir.display().to_string();
        let running = super::super::parse_process_list(&format!(
            "10 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={data_dir}\n"
        ));
        assert!(app_running(&home, &running));
        let other = super::super::parse_process_list(&format!(
            "10 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={data_dir}-other\n\
             11 /Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=gpu --user-data-dir={data_dir} --x\n"
        ));
        assert!(!app_running(&home, &other));
    }
}
