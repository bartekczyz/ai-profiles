//! The Claude desktop app's records of its Code tab sessions.
//!
//! Each record is `<gui-data>/claude-code-sessions/<account>/<org>/local_<uuid>.json`
//! and names the transcript it continues by `cliSessionId`. The account and
//! org a record belongs to are told by its folder only. Beside the records,
//! `archived-sessions.idx` lists the archived ones and `deleted_<uuid>` marks
//! one the app deleted.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::archive_store::occupied;
use super::copy::place_new;
use super::non_blank;
use super::transcript::TranscriptSummary;
use crate::error::{AppError, AppResult};
use crate::sessions::list::transcript_title;
use crate::sessions::Home;

/// The folder under `<gui-data>` holding the records, by account and org.
const RECORDS_DIR: &str = "claude-code-sessions";

/// The file in each `<account>/<org>` folder listing its archived records.
pub(super) const ARCHIVED_INDEX: &str = "archived-sessions.idx";

/// Record fields that belong to the account a session last ran under, not to
/// the session: its connectors, the folders and tools approved in that app,
/// and snapshots of the prompt and tools it started with. The app fills them
/// in again for the account it opens the session under.
const ACCOUNT_BOUND_FIELDS: [&str; 6] = [
    "remoteMcpServersConfig",
    "sessionPermissionUpdates",
    "alwaysAllowedReasons",
    "promptAppendSnapshot",
    "toolSurfaceSnapshot",
    "spawnSeed",
];

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
    /// When the session was started: its `createdAt`, in ms since the epoch.
    pub created_at: Option<DateTime<Utc>>,
    /// When the session was last active: its `lastActivityAt`, in ms since
    /// the epoch.
    pub last_activity_at: Option<DateTime<Utc>>,
    /// The session is archived: flagged `isArchived`, or listed in its
    /// folder's `archived-sessions.idx`.
    pub archived: bool,
}

/// Records already read, by path, each with the length and modification
/// time the file had then, and `None` for one that wasn't a readable record.
/// A file that still has both is not read again. Whether a record is archived
/// or deleted is told by other files, read every time.
static RECORD_CACHE: LazyLock<Mutex<RecordCache>> = LazyLock::new(Mutex::default);

/// What each record file held when read, by its path.
type RecordCache = HashMap<PathBuf, (u64, SystemTime, Option<RecordFile>)>;

/// The fields of a `local_<uuid>.json` record read here.
#[derive(Clone, Deserialize)]
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
    /// When the session was started, in ms since the epoch.
    created_at: Option<f64>,
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
/// that aren't a readable record are skipped. A record file that hasn't
/// changed since it was last read comes from [`RECORD_CACHE`].
pub fn read_records(gui_data_dir: &Path) -> Vec<DesktopRecord> {
    let mut records = Vec::new();
    let mut seen = HashSet::new();
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
            seen.insert(path.clone());
            let Some(record) = cached_record(&path) else {
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
                created_at: record
                    .created_at
                    .and_then(|millis| DateTime::from_timestamp_millis(millis as i64)),
                last_activity_at: record
                    .last_activity_at
                    .and_then(|millis| DateTime::from_timestamp_millis(millis as i64)),
            });
        }
    }
    // Forget records that have gone, so the cache stays the size of what is
    // on disk.
    let records_dir = gui_data_dir.join(RECORDS_DIR);
    lock_records().retain(|path, _| !path.starts_with(&records_dir) || seen.contains(path));
    records
}

/// Take [`RECORD_CACHE`]. Entries are whole values swapped in and out, so a
/// panic under the lock leaves nothing half-written.
fn lock_records() -> MutexGuard<'static, RecordCache> {
    RECORD_CACHE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The record file at `path`: from [`RECORD_CACHE`] if it hasn't changed
/// since it was read, else read now and cached. `None` when it isn't a
/// readable record.
fn cached_record(path: &Path) -> Option<RecordFile> {
    let metadata = fs::metadata(path).ok()?;
    let (len, modified) = (metadata.len(), metadata.modified().ok()?);
    if let Some((cached_len, cached_modified, record)) = lock_records().get(path) {
        if *cached_len == len && *cached_modified == modified {
            return record.clone();
        }
    }
    let record = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<RecordFile>(&text).ok());
    lock_records().insert(path.to_path_buf(), (len, modified, record.clone()));
    record
}

/// Archive `record`, or restore it, the way the desktop app does: its
/// `isArchived` flag is set, and its id added to, or taken out of, its
/// folder's `archived-sessions.idx`, made if missing. Both files are edited
/// as JSON, so fields read nowhere here are kept, and each is replaced whole
/// (written beside it, then renamed over it). Both are read before either is
/// written, so one that can't be read leaves both as they were.
///
/// The desktop app keeps its records in memory and writes them back, so it
/// must not be running.
pub fn set_archived(record: &DesktopRecord, archived: bool) -> AppResult<()> {
    let mut fields: Value = serde_json::from_str(&fs::read_to_string(&record.path)?)?;
    fields
        .as_object_mut()
        .ok_or_else(|| unexpected(&record.path))?
        .insert("isArchived".to_string(), Value::Bool(archived));
    let index_path = record
        .path
        .parent()
        .ok_or_else(|| unexpected(&record.path))?
        .join(ARCHIVED_INDEX);
    let index = index_update(&index_path, &record.local_id, archived)?;
    replace_file(&record.path, &fields.to_string())?;
    match index {
        Some(index) => write_index(&index_path, &index, None),
        None => Ok(()),
    }
}

/// The archived index at `index_path` changed to list record `local_id` as
/// archived or not, as `archived` says, or `None` when it does already. A
/// missing index is made only to list one; one that can't be read, or isn't
/// shaped as expected, is an error.
fn index_update(index_path: &Path, local_id: &str, archived: bool) -> AppResult<Option<Value>> {
    let mut index = match fs::read_to_string(index_path) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && archived => {
            json!({ "v": 1, "archived": [] })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let ids = index
        .as_object_mut()
        .ok_or_else(|| unexpected(index_path))?
        .entry("archived")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| unexpected(index_path))?;
    let id = Value::String(local_id.to_string());
    if ids.contains(&id) == archived {
        return Ok(None);
    }
    if archived {
        ids.push(id);
    } else {
        ids.retain(|each| *each != id);
    }
    Ok(Some(index))
}

/// Replace the archived index at `index_path` with `index`, an
/// [`index_update`] of it. The one there is copied into `backup` first, when
/// given, under the same name.
fn write_index(index_path: &Path, index: &Value, backup: Option<&Path>) -> AppResult<()> {
    if let Some(backup) = backup.filter(|_| occupied(index_path)) {
        place_new(index_path, &backup.join(ARCHIVED_INDEX))?;
    }
    replace_file(index_path, &index.to_string())
}

/// The record another home's desktop app gets of a session moved to it,
/// continuing transcript `displayed` after the earlier ones `priors`: the
/// `source` record without the fields bound to its account, else, for a
/// session the CLI started, the fields the app needs to list and open it.
/// Either way it keeps its `local_<uuid>`, is active, and is titled as the
/// Sessions list titles it when it has no title of its own.
pub fn build_destination_record(
    source: Option<&DesktopRecord>,
    displayed: &TranscriptSummary,
    priors: &[String],
) -> AppResult<Value> {
    let mut fields = match source {
        Some(source) => {
            let mut fields: Map<String, Value> =
                serde_json::from_str(&fs::read_to_string(&source.path)?)
                    .map_err(|_| unexpected(&source.path))?;
            for field in ACCOUNT_BOUND_FIELDS {
                fields.remove(field);
            }
            fields.insert("sessionId".to_string(), json!(source.local_id));
            fields
        }
        None => {
            let cwd = displayed.cwd.as_deref().unwrap_or_default();
            let used_at = displayed.last_used_at.timestamp_millis();
            let mut fields = Map::new();
            fields.insert(
                "sessionId".to_string(),
                json!(format!("local_{}", uuid::Uuid::new_v4())),
            );
            fields.insert("cwd".to_string(), json!(cwd));
            fields.insert("originCwd".to_string(), json!(cwd));
            fields.insert("createdAt".to_string(), json!(used_at));
            fields.insert("lastActivityAt".to_string(), json!(used_at));
            fields.insert("lastFocusedAt".to_string(), json!(used_at));
            fields.insert("permissionMode".to_string(), json!("default"));
            fields
        }
    };
    let untitled = fields
        .get("title")
        .and_then(Value::as_str)
        .is_none_or(|title| title.trim().is_empty());
    if let (true, Some(title)) = (untitled, transcript_title(displayed)) {
        let title_source = if displayed.custom_title.is_some() {
            "user"
        } else {
            "auto"
        };
        fields.insert("title".to_string(), json!(title));
        fields.insert("titleSource".to_string(), json!(title_source));
    }
    fields.insert("cliSessionId".to_string(), json!(displayed.session_id));
    fields.insert("priorCliSessionIds".to_string(), json!(priors));
    fields.insert("isArchived".to_string(), Value::Bool(false));
    Ok(Value::Object(fields))
}

/// Write `record`, built by [`build_destination_record`], into `account_dir`
/// as `<sessionId>.json`, replacing it whole. Returns its path. Take its id
/// out of the folder's archived index with [`list_as_active`] for it to show
/// as active.
///
/// The desktop app keeps its records in memory and writes them back, so it
/// must not be running.
pub fn write_destination_record(account_dir: &Path, record: &Value) -> AppResult<PathBuf> {
    let local_id = record
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|id| id.starts_with("local_") && is_folder_name(id))
        .ok_or_else(|| AppError::Validation("the record has no valid sessionId".to_string()))?;
    fs::create_dir_all(account_dir)?;
    let path = account_dir.join(format!("{local_id}.json"));
    replace_file(&path, &record.to_string())?;
    Ok(path)
}

/// Take record `local_id` out of `account_dir`'s archived index if it lists
/// it, so the record shows as active, copying the index into `backup` before
/// it is rewritten. An index that can't be read is left as it is, as the app
/// would read it no better. Returns whether the index was rewritten.
///
/// The desktop app keeps its records in memory and writes them back, so it
/// must not be running.
pub fn list_as_active(account_dir: &Path, local_id: &str, backup: &Path) -> AppResult<bool> {
    let index_path = account_dir.join(ARCHIVED_INDEX);
    let Ok(Some(index)) = index_update(&index_path, local_id, false) else {
        return Ok(false);
    };
    write_index(&index_path, &index, Some(backup))?;
    Ok(true)
}

/// The account a record at `path` belongs to: its `<account>` folder.
pub fn record_account(path: &Path) -> Option<&str> {
    path.parent()?.parent()?.file_name()?.to_str()
}

/// Whether `account_dir` marks record `local_id` deleted, so the desktop app
/// would never show a record written under that id there.
pub fn deleted_in(account_dir: &Path, local_id: &str) -> bool {
    local_id
        .strip_prefix("local_")
        .is_some_and(|uuid| account_dir.join(format!("deleted_{uuid}")).exists())
}

/// The error for a file at `path` that isn't shaped as expected.
fn unexpected(path: &Path) -> AppError {
    AppError::Validation(format!("{} isn't in the expected format", path.display()))
}

/// Replace the file at `path` with `contents` in one step: they are written
/// to a hidden file beside it, which is then renamed over it, so the app
/// never reads a half-written file.
fn replace_file(path: &Path, contents: &str) -> AppResult<()> {
    let name = path.file_name().ok_or_else(|| unexpected(path))?;
    let temp = path.with_file_name(format!(".{}.ai-profiles-tmp", name.to_string_lossy()));
    let written = fs::write(&temp, contents).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    Ok(written?)
}

/// The folder `home`'s desktop app keeps its records in for the account it is
/// signed in to, or `None` when that can't be told, as when it has never been
/// signed in.
///
/// The app names the account in `<gui-data>/config.json`. The org is the one
/// the CLI's `.claude.json` gives when it describes the same account, else the
/// only org folder the app has made for the account.
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

    /// Makes the folder of `account`'s records in `org` under `gui_data_dir`,
    /// and returns it.
    fn org_dir(gui_data_dir: &Path, account: &str, org: &str) -> PathBuf {
        let dir = gui_data_dir.join(RECORDS_DIR).join(account).join(org);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes `value` to `path`, making its folder.
    fn write_json(path: &Path, value: &Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, value.to_string()).unwrap();
    }

    /// Writes desktop record `local_<uuid>` of `fields` into `org_dir`.
    /// Returns its path.
    fn write_record(org_dir: &Path, uuid: &str, fields: Value) -> PathBuf {
        let path = org_dir.join(format!("local_{uuid}.json"));
        write_json(&path, &fields);
        path
    }

    /// `records`, by their local id.
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
                    created_at: None,
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
                    created_at: None,
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
    fn a_record_that_hasnt_changed_isnt_read_again_but_the_index_is() {
        let root = tempdir().unwrap();
        let dir = org_dir(root.path(), ACCOUNT, ORG);
        let path = write_record(&dir, "aaa", json!({ "cliSessionId": "s", "title": "Aaaa" }));
        let written = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(read_records(root.path())[0].title.as_deref(), Some("Aaaa"));

        write_record(&dir, "aaa", json!({ "cliSessionId": "s", "title": "Bbbb" }));
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(written)
            .unwrap();
        write_json(
            &dir.join(ARCHIVED_INDEX),
            &json!({ "v": 1, "archived": ["local_aaa"] }),
        );

        let records = read_records(root.path());

        assert_eq!(records[0].title.as_deref(), Some("Aaaa"));
        assert!(records[0].archived);
    }

    #[test]
    fn an_app_without_records_has_none() {
        let root = tempdir().unwrap();

        assert_eq!(read_records(root.path()), []);
    }

    /// Writes the desktop app's config under `gui_data_dir`, naming `account`
    /// the one it last signed in to.
    fn write_config(gui_data_dir: &Path, account: &str) {
        write_json(
            &gui_data_dir.join("config.json"),
            &json!({ "lastKnownAccountUuid": account }),
        );
    }

    /// Writes the CLI's `.claude.json` at `path`, signed in to `account` in
    /// `org`.
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

    /// The record at `path`, as JSON.
    fn read_value(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The names of the files in `dir`, sorted.
    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn archiving_and_restoring_a_record_keeps_what_isnt_read_here() {
        let root = tempdir().unwrap();
        let dir = org_dir(root.path(), ACCOUNT, ORG);
        let original = json!({
            "sessionId": "local_aaa",
            "cliSessionId": "cli-a",
            "isArchived": false,
            "model": "claude-opus-5-5",
            "remoteMcpServersConfig": { "servers": [1, 2] },
        });
        let path = write_record(&dir, "aaa", original.clone());
        write_json(
            &dir.join(ARCHIVED_INDEX),
            &json!({ "v": 1, "archived": ["local_other"], "next": 7 }),
        );
        let record = read_records(root.path()).remove(0);

        set_archived(&record, true).unwrap();

        let mut archived = original.clone();
        archived["isArchived"] = json!(true);
        assert_eq!(read_value(&path), archived);
        assert_eq!(
            read_value(&dir.join(ARCHIVED_INDEX)),
            json!({ "v": 1, "archived": ["local_other", "local_aaa"], "next": 7 })
        );
        assert!(read_records(root.path())[0].archived);

        set_archived(&record, false).unwrap();

        assert_eq!(read_value(&path), original);
        assert_eq!(
            read_value(&dir.join(ARCHIVED_INDEX)),
            json!({ "v": 1, "archived": ["local_other"], "next": 7 })
        );
        assert!(!read_records(root.path())[0].archived);
        assert_eq!(
            file_names(&dir),
            [ARCHIVED_INDEX.to_string(), "local_aaa.json".to_string()]
        );
    }

    #[test]
    fn archiving_a_record_makes_the_index_when_there_is_none() {
        let root = tempdir().unwrap();
        let dir = org_dir(root.path(), ACCOUNT, ORG);
        write_record(&dir, "aaa", json!({ "cliSessionId": "cli-a" }));
        let record = read_records(root.path()).remove(0);

        set_archived(&record, true).unwrap();
        set_archived(&record, true).unwrap();

        assert_eq!(
            read_value(&dir.join(ARCHIVED_INDEX)),
            json!({ "v": 1, "archived": ["local_aaa"] })
        );
    }

    #[test]
    fn an_index_that_cant_be_read_leaves_the_record_as_it_was() {
        let root = tempdir().unwrap();
        let dir = org_dir(root.path(), ACCOUNT, ORG);
        let path = write_record(&dir, "aaa", json!({ "cliSessionId": "cli-a" }));
        fs::write(dir.join(ARCHIVED_INDEX), "not json").unwrap();
        let record = read_records(root.path()).remove(0);

        assert!(set_archived(&record, true).is_err());

        assert_eq!(read_value(&path), json!({ "cliSessionId": "cli-a" }));
        assert_eq!(
            fs::read_to_string(dir.join(ARCHIVED_INDEX)).unwrap(),
            "not json"
        );
    }

    /// A transcript of session `id`, last used at `used_at` (ms), in `cwd`.
    fn transcript(id: &str, cwd: Option<&str>) -> TranscriptSummary {
        TranscriptSummary {
            session_id: id.to_string(),
            path: PathBuf::from(format!("/config/projects/-work-app/{id}.jsonl")),
            cwd: cwd.map(str::to_string),
            custom_title: None,
            ai_title: Some("Fix the login bug".to_string()),
            first_prompt: Some("fix it".to_string()),
            last_prompt: None,
            last_used_at: DateTime::from_timestamp_millis(1_790_113_004_345).unwrap(),
            plan_slugs: Default::default(),
        }
    }

    #[test]
    fn a_moved_record_drops_what_its_account_bound_and_continues_the_moved_transcript() {
        let root = tempdir().unwrap();
        let dir = org_dir(root.path(), ACCOUNT, ORG);
        write_record(
            &dir,
            "aaa",
            json!({
                "sessionId": "local_aaa",
                "cliSessionId": "gone",
                "priorCliSessionIds": ["earlier", "lost"],
                "title": "Audit the API",
                "model": "claude-opus-5-5",
                "isArchived": true,
                "remoteMcpServersConfig": [{ "uuid": "x" }],
                "sessionPermissionUpdates": [],
                "alwaysAllowedReasons": {},
                "promptAppendSnapshot": {},
                "toolSurfaceSnapshot": {},
                "spawnSeed": 7,
            }),
        );
        let source = read_records(root.path()).remove(0);

        let record = build_destination_record(
            Some(&source),
            &transcript("current", Some("/work/app")),
            &["earlier".to_string()],
        )
        .unwrap();

        assert_eq!(
            record,
            json!({
                "sessionId": "local_aaa",
                "cliSessionId": "current",
                "priorCliSessionIds": ["earlier"],
                "title": "Audit the API",
                "model": "claude-opus-5-5",
                "isArchived": false,
            })
        );
    }

    #[test]
    fn a_cli_session_gets_a_record_the_app_can_list_and_open() {
        let record =
            build_destination_record(None, &transcript("s", Some("/work/app")), &[]).unwrap();

        let local_id = record["sessionId"].as_str().unwrap().to_string();
        assert!(local_id.starts_with("local_"), "{local_id}");
        assert_eq!(
            record,
            json!({
                "sessionId": local_id,
                "cliSessionId": "s",
                "priorCliSessionIds": [],
                "cwd": "/work/app",
                "originCwd": "/work/app",
                "createdAt": 1_790_113_004_345_i64,
                "lastActivityAt": 1_790_113_004_345_i64,
                "lastFocusedAt": 1_790_113_004_345_i64,
                "permissionMode": "default",
                "title": "Fix the login bug",
                "titleSource": "auto",
                "isArchived": false,
            })
        );
    }

    #[test]
    fn a_written_record_is_listed_as_active() {
        let root = tempdir().unwrap();
        let dir = org_dir(root.path(), ACCOUNT, ORG);
        write_json(
            &dir.join(ARCHIVED_INDEX),
            &json!({ "v": 1, "archived": ["local_other", "local_aaa"] }),
        );
        let record = json!({ "sessionId": "local_aaa", "cliSessionId": "s", "isArchived": false });

        let path = write_destination_record(&dir, &record).unwrap();
        let rewritten = list_as_active(&dir, "local_aaa", &root.path().join("backup")).unwrap();

        assert_eq!(path, dir.join("local_aaa.json"));
        assert!(rewritten);
        assert_eq!(
            read_value(&root.path().join("backup").join(ARCHIVED_INDEX)),
            json!({ "v": 1, "archived": ["local_other", "local_aaa"] })
        );
        assert_eq!(read_value(&path), record);
        assert_eq!(
            read_value(&dir.join(ARCHIVED_INDEX)),
            json!({ "v": 1, "archived": ["local_other"] })
        );
        let records = read_records(root.path());
        assert_eq!(records.len(), 1);
        assert!(!records[0].archived);
        assert_eq!(
            file_names(&dir),
            [ARCHIVED_INDEX.to_string(), "local_aaa.json".to_string()]
        );
    }
}
