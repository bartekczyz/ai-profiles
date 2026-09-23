//! The Claude desktop app's records of its Code tab sessions.
//!
//! Each record is `<gui-data>/claude-code-sessions/<account>/<org>/local_<uuid>.json`
//! and names the transcript it continues by `cliSessionId`. The account and
//! org a record belongs to are told by its folder only. Beside the records,
//! `archived-sessions.idx` lists the archived ones and `deleted_<uuid>` marks
//! one the app deleted.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::non_blank;
use crate::sessions::Home;

/// The folder under `<gui-data>` holding the records, by account and org.
const RECORDS_DIR: &str = "claude-code-sessions";

/// The file in each `<account>/<org>` folder listing its archived records.
const ARCHIVED_INDEX: &str = "archived-sessions.idx";

/// What the Sessions list needs to know about one desktop record.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopRecord {
    /// The record: `<gui-data>/claude-code-sessions/<account>/<org>/local_<uuid>.json`.
    pub path: PathBuf,
    /// The record's own id, its file name without `.json`: `local_<uuid>`.
    pub local_id: String,
    /// The transcript the session continues: its `cliSessionId`. A session
    /// the app hasn't started Claude Code for yet has none.
    pub cli_session_id: Option<String>,
    /// The transcripts the session continued before its current one: its
    /// `priorCliSessionIds`, as the app starts a new transcript when it
    /// can't resume the last.
    pub prior_cli_session_ids: Vec<String>,
    /// The title the app shows for the session.
    pub title: Option<String>,
    /// The folder the session works in.
    pub cwd: Option<String>,
    /// When the session was last active: its `lastActivityAt`, in ms since
    /// the epoch.
    pub last_activity_at: Option<DateTime<Utc>>,
    /// The session is archived: flagged `isArchived`, or listed in its
    /// folder's `archived-sessions.idx`.
    pub archived: bool,
}

/// The fields of a `local_<uuid>.json` record read here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordFile {
    /// The transcript the session continues.
    cli_session_id: Option<String>,
    /// The transcripts the session continued before, read leniently: entries
    /// that aren't a string are skipped rather than failing the record.
    prior_cli_session_ids: Option<Vec<Value>>,
    /// The title the app shows.
    title: Option<String>,
    /// The folder the session works in.
    cwd: Option<String>,
    /// When the session was last active, in ms since the epoch.
    last_activity_at: Option<f64>,
    /// Archived from within the app.
    #[serde(default)]
    is_archived: bool,
}

/// The `archived-sessions.idx` file: `{"v":1,"archived":["local_<uuid>",…]}`.
#[derive(Deserialize)]
struct ArchivedIndex {
    /// The ids of the archived records.
    #[serde(default)]
    archived: Vec<String>,
}

/// Every record under `gui_data_dir`, of every `<account>/<org>` folder, as
/// the user may have switched accounts. Records the app deleted and files
/// that aren't a readable record are skipped.
pub fn read_records(gui_data_dir: &Path) -> Vec<DesktopRecord> {
    let mut records = Vec::new();
    for org_dir in account_org_dirs(gui_data_dir) {
        let Ok(files) = fs::read_dir(&org_dir) else {
            continue;
        };
        let archived = archived_ids(&org_dir);
        for file in files.flatten() {
            let path = file.path();
            let Some(local_id) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".json"))
                .filter(|stem| stem.starts_with("local_"))
                .map(str::to_string)
            else {
                continue;
            };
            let tombstone = format!("deleted_{}", &local_id["local_".len()..]);
            if org_dir.join(tombstone).exists() {
                continue;
            }
            let Some(record) = fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<RecordFile>(&text).ok())
            else {
                continue;
            };
            records.push(DesktopRecord {
                archived: record.is_archived || archived.contains(&local_id),
                path,
                local_id,
                cli_session_id: non_blank(record.cli_session_id),
                prior_cli_session_ids: record
                    .prior_cli_session_ids
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|id| non_blank(id.as_str().map(str::to_string)))
                    .collect(),
                title: non_blank(record.title),
                cwd: non_blank(record.cwd),
                last_activity_at: record
                    .last_activity_at
                    .and_then(|millis| DateTime::from_timestamp_millis(millis as i64)),
            });
        }
    }
    records
}

/// The folder `home`'s desktop app keeps its records in for the account it is
/// signed in to, or `None` when that can't be told, as when it has never been
/// signed in.
///
/// The app names the account in `<gui-data>/config.json`. The org is the one
/// the CLI's `.claude.json` gives when it describes the same account, else the
/// only org folder the app has made for the account.
#[allow(dead_code)] // Consumed by moving sessions between profiles.
pub fn current_account_dir(home: &Home) -> Option<PathBuf> {
    account_dir(&home.gui_data_dir, &claude_json_candidates(home))
}

/// The `.claude.json` files that can describe the account `home`'s CLI is
/// signed in to: the one in its config dir, and for the stock install also
/// `~/.claude.json`, where the stock CLI keeps it.
fn claude_json_candidates(home: &Home) -> Vec<PathBuf> {
    let mut candidates = vec![home.config_dir.join(".claude.json")];
    if home.stock {
        if let Some(user_home) = dirs::home_dir() {
            candidates.push(user_home.join(".claude.json"));
        }
    }
    candidates
}

/// [`current_account_dir`] for the app keeping its data in `gui_data_dir`,
/// reading the `.claude.json` files at `claude_jsons` in order.
fn account_dir(gui_data_dir: &Path, claude_jsons: &[PathBuf]) -> Option<PathBuf> {
    let config = read_json(&gui_data_dir.join("config.json"))?;
    let account = config.get("lastKnownAccountUuid")?.as_str()?;
    if !is_folder_name(account) {
        return None;
    }
    let account_dir = gui_data_dir.join(RECORDS_DIR).join(account);
    let org = claude_jsons
        .iter()
        .filter_map(|path| read_json(path))
        .find_map(|claude_json| {
            let oauth = claude_json.get("oauthAccount")?;
            if oauth.get("accountUuid")?.as_str()? != account {
                return None;
            }
            oauth.get("organizationUuid")?.as_str().map(str::to_string)
        })
        .or_else(|| sole_subdir(&account_dir))?;
    if !is_folder_name(&org) {
        return None;
    }
    Some(account_dir.join(org))
}

/// The name of the only folder in `dir`, if it holds exactly one.
fn sole_subdir(dir: &Path) -> Option<String> {
    let mut names = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned());
    let name = names.next()?;
    if names.next().is_some() {
        return None;
    }
    Some(name)
}

/// `name` can be joined onto a path as one folder, without leaving it.
fn is_folder_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/')
}

/// The `claude-code-sessions/<account>/<org>` folders under `gui_data_dir`.
fn account_org_dirs(gui_data_dir: &Path) -> Vec<PathBuf> {
    let Ok(accounts) = fs::read_dir(gui_data_dir.join(RECORDS_DIR)) else {
        return Vec::new();
    };
    accounts
        .flatten()
        .filter(|account| account.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|account| fs::read_dir(account.path()).ok())
        .flat_map(|orgs| orgs.flatten())
        .filter(|org| org.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|org| org.path())
        .collect()
}

/// The record ids `org_dir`'s `archived-sessions.idx` lists; none when it is
/// missing or unreadable.
fn archived_ids(org_dir: &Path) -> HashSet<String> {
    fs::read_to_string(org_dir.join(ARCHIVED_INDEX))
        .ok()
        .and_then(|text| serde_json::from_str::<ArchivedIndex>(&text).ok())
        .map(|index| index.archived.into_iter().collect())
        .unwrap_or_default()
}

/// The JSON file at `path`, if it can be read.
fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::app_kind::AppKind;

    const ACCOUNT: &str = "1a19a582-d7b1-4f72-acef-cbe78c1a68e4";
    const ORG: &str = "18d53058-434e-4c78-9624-e290f7a80ccb";
    const OTHER_ACCOUNT: &str = "a99c6b36-dd42-44d7-b3ae-9496265549fd";
    const OTHER_ORG: &str = "527aadd2-01c3-49a6-a770-e65e047242c3";

    fn org_dir(gui_data_dir: &Path, account: &str, org: &str) -> PathBuf {
        let dir = gui_data_dir.join(RECORDS_DIR).join(account).join(org);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_json(path: &Path, value: &Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value.to_string()).unwrap();
    }

    fn write_record(org_dir: &Path, uuid: &str, fields: Value) -> PathBuf {
        let path = org_dir.join(format!("local_{uuid}.json"));
        write_json(&path, &fields);
        path
    }

    fn sorted(mut records: Vec<DesktopRecord>) -> Vec<DesktopRecord> {
        records.sort_by(|left, right| left.local_id.cmp(&right.local_id));
        records
    }

    #[test]
    fn records_are_read_from_every_account_and_org() {
        let root = tempdir().unwrap();
        let current = org_dir(root.path(), ACCOUNT, ORG);
        let earlier = org_dir(root.path(), OTHER_ACCOUNT, OTHER_ORG);
        let first = write_record(
            &current,
            "aaa",
            json!({
                "sessionId": "local_aaa",
                "cliSessionId": "cli-a",
                "title": "Fix the login bug",
                "cwd": "/work/app",
                "lastActivityAt": 1_790_113_004_345_i64,
                "priorCliSessionIds": ["cli-before", 7, ""],
                "isArchived": false,
                "model": "claude-opus-5-5",
            }),
        );
        let second = write_record(
            &earlier,
            "bbb",
            json!({
                "sessionId": "local_bbb",
                "title": "  ",
                "cwd": "/work/other",
                "priorCliSessionIds": null,
            }),
        );

        let records = sorted(read_records(root.path()));

        assert_eq!(
            records,
            [
                DesktopRecord {
                    path: first,
                    local_id: "local_aaa".to_string(),
                    cli_session_id: Some("cli-a".to_string()),
                    prior_cli_session_ids: vec!["cli-before".to_string()],
                    title: Some("Fix the login bug".to_string()),
                    cwd: Some("/work/app".to_string()),
                    last_activity_at: DateTime::from_timestamp_millis(1_790_113_004_345),
                    archived: false,
                },
                DesktopRecord {
                    path: second,
                    local_id: "local_bbb".to_string(),
                    cli_session_id: None,
                    prior_cli_session_ids: Vec::new(),
                    title: None,
                    cwd: Some("/work/other".to_string()),
                    last_activity_at: None,
                    archived: false,
                },
            ]
        );
    }

    #[test]
    fn a_record_is_archived_by_its_flag_or_the_index() {
        let root = tempdir().unwrap();
        let dir = org_dir(root.path(), ACCOUNT, ORG);
        write_record(&dir, "flagged", json!({ "isArchived": true }));
        write_record(&dir, "indexed", json!({ "isArchived": false }));
        write_record(&dir, "active", json!({}));
        write_json(
            &dir.join(ARCHIVED_INDEX),
            &json!({ "v": 1, "archived": ["local_indexed", "local_gone"] }),
        );

        let archived: Vec<(String, bool)> = sorted(read_records(root.path()))
            .into_iter()
            .map(|record| (record.local_id, record.archived))
            .collect();

        assert_eq!(
            archived,
            [
                ("local_active".to_string(), false),
                ("local_flagged".to_string(), true),
                ("local_indexed".to_string(), true),
            ]
        );
    }

    #[test]
    fn deleted_and_unreadable_records_are_skipped() {
        let root = tempdir().unwrap();
        let dir = org_dir(root.path(), ACCOUNT, ORG);
        write_record(&dir, "kept", json!({ "cliSessionId": "cli-kept" }));
        write_record(&dir, "deleted", json!({ "cliSessionId": "cli-deleted" }));
        fs::write(dir.join("deleted_deleted"), "1786614202445").unwrap();
        fs::write(dir.join("deleted_other"), "1786614202445").unwrap();
        fs::write(dir.join("local_broken.json"), "{\"cliSessionId\":").unwrap();
        write_json(&dir.join("scheduled-tasks.json"), &json!({ "tasks": [] }));
        fs::write(dir.join(ARCHIVED_INDEX), "not json").unwrap();

        let ids: Vec<String> = read_records(root.path())
            .into_iter()
            .map(|record| record.local_id)
            .collect();

        assert_eq!(ids, ["local_kept"]);
    }

    #[test]
    fn an_app_without_records_has_none() {
        let root = tempdir().unwrap();

        assert_eq!(read_records(root.path()), []);
    }

    fn write_config(gui_data_dir: &Path, account: &str) {
        write_json(
            &gui_data_dir.join("config.json"),
            &json!({ "lastKnownAccountUuid": account }),
        );
    }

    fn write_claude_json(path: &Path, account: &str, org: &str) {
        write_json(
            path,
            &json!({ "oauthAccount": { "accountUuid": account, "organizationUuid": org } }),
        );
    }

    #[test]
    fn the_org_is_the_one_the_cli_gives_for_the_same_account() {
        let root = tempdir().unwrap();
        let gui = root.path().join("gui-data");
        write_config(&gui, ACCOUNT);
        org_dir(&gui, ACCOUNT, ORG);
        org_dir(&gui, ACCOUNT, OTHER_ORG);
        let elsewhere = root.path().join("elsewhere.json");
        let config = root.path().join("cli-config").join(".claude.json");
        write_claude_json(&elsewhere, OTHER_ACCOUNT, ORG);
        write_claude_json(&config, ACCOUNT, OTHER_ORG);

        assert_eq!(
            account_dir(&gui, &[elsewhere, config]),
            Some(gui.join(RECORDS_DIR).join(ACCOUNT).join(OTHER_ORG))
        );
    }

    #[test]
    fn the_org_is_the_only_one_the_app_made_when_the_cli_is_on_another_account() {
        let root = tempdir().unwrap();
        let gui = root.path().join("gui-data");
        write_config(&gui, ACCOUNT);
        org_dir(&gui, ACCOUNT, ORG);
        let config = root.path().join(".claude.json");
        write_claude_json(&config, OTHER_ACCOUNT, OTHER_ORG);

        assert_eq!(
            account_dir(&gui, std::slice::from_ref(&config)),
            Some(gui.join(RECORDS_DIR).join(ACCOUNT).join(ORG))
        );

        org_dir(&gui, ACCOUNT, OTHER_ORG);

        assert_eq!(account_dir(&gui, &[config]), None);
    }

    #[test]
    fn an_app_never_signed_in_has_no_account_dir() {
        let root = tempdir().unwrap();
        let gui = root.path().join("gui-data");
        write_json(&gui.join("config.json"), &json!({ "locale": "en" }));

        assert_eq!(account_dir(&gui, &[]), None);

        write_config(&gui, "../escape");
        fs::create_dir_all(gui.join("escape").join(ORG)).unwrap();

        assert_eq!(account_dir(&gui, &[]), None);
    }

    #[test]
    fn a_profiles_account_dir_is_told_by_its_own_config_dirs() {
        let root = tempdir().unwrap();
        let home = Home {
            id: "id".to_string(),
            app: AppKind::Claude,
            label: "Label".to_string(),
            config_dir: root.path().join("cli-config"),
            gui_data_dir: root.path().join("gui-data"),
            stock: false,
        };
        write_config(&home.gui_data_dir, ACCOUNT);
        org_dir(&home.gui_data_dir, ACCOUNT, ORG);
        org_dir(&home.gui_data_dir, ACCOUNT, OTHER_ORG);
        write_claude_json(&home.config_dir.join(".claude.json"), ACCOUNT, ORG);

        assert_eq!(
            current_account_dir(&home),
            Some(home.gui_data_dir.join(RECORDS_DIR).join(ACCOUNT).join(ORG))
        );
    }

    #[test]
    fn the_stock_cli_is_also_read_from_the_users_home() {
        let root = tempdir().unwrap();
        let home = |stock| Home {
            id: "id".to_string(),
            app: AppKind::Claude,
            label: "Label".to_string(),
            config_dir: root.path().join("cli-config"),
            gui_data_dir: root.path().join("gui-data"),
            stock,
        };

        assert_eq!(
            claude_json_candidates(&home(false)),
            [root.path().join("cli-config").join(".claude.json")]
        );
        assert_eq!(
            claude_json_candidates(&home(true)),
            [
                root.path().join("cli-config").join(".claude.json"),
                dirs::home_dir().unwrap().join(".claude.json"),
            ]
        );
    }
}
