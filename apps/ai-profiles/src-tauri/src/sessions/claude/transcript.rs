//! Summaries of Claude Code session transcripts.
//!
//! A transcript is a JSONL file Claude Code appends a record to as the session
//! goes: messages (`user`, `assistant`, `attachment`, `system`, most carrying
//! `timestamp`, `cwd` and `isSidechain`) and metadata (`custom-title`,
//! `ai-title`, `last-prompt`, `relocated`) that is re-appended whenever it
//! changes, so the last record of each kind is the current one. Transcripts
//! reach 100 MB and a config dir holds thousands, and the list is re-read on
//! every window focus: summaries are cached until the file changes, and only
//! lines that can hold a field read here are decoded at all.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// What the Sessions list needs to know about one transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptSummary {
    /// The session's id: the transcript's file name without `.jsonl`.
    pub session_id: String,
    /// The transcript: `<config>/projects/<slug>/<id>.jsonl`.
    pub path: PathBuf,
    /// The folder the session last worked in: the `cwd` of its last message,
    /// or where it was last relocated to.
    pub cwd: Option<String>,
    /// The name set with `/rename`: the last `custom-title` record, else the
    /// one in `<slug>/<id>/custom-title.json`.
    pub custom_title: Option<String>,
    /// The title Claude generated for the session: the last `ai-title` record.
    pub ai_title: Option<String>,
    /// The first text the user typed into the session, cut to
    /// [`FIRST_PROMPT_MAX_CHARS`].
    pub first_prompt: Option<String>,
    /// The last thing typed into the session: the last `last-prompt` record.
    pub last_prompt: Option<String>,
    /// The last record's `timestamp`, else when the file was last written.
    pub last_used_at: DateTime<Utc>,
}

/// How much of the first prompt a summary keeps.
const FIRST_PROMPT_MAX_CHARS: usize = 200;

/// A line is only decoded if it contains one of these: every record read here
/// carries one, and decoding the rest (tool output, file snapshots) is most of
/// the cost of reading a transcript.
const MARKERS: [&str; 6] = [
    "\"timestamp\"",
    "\"custom-title\"",
    "\"ai-title\"",
    "\"last-prompt\"",
    "\"relocated\"",
    "\"type\":\"user\"",
];

/// Summaries already read, by transcript, with the length and modification
/// time the file had when read. A file that still has both is not read again.
/// The title file fallback is applied after the cache, as that file changes on
/// its own.
static SUMMARY_CACHE: LazyLock<Mutex<SummaryCache>> = LazyLock::new(Mutex::default);

/// Cached summaries by transcript, each with the file's length and
/// modification time when read.
type SummaryCache = HashMap<PathBuf, (u64, SystemTime, TranscriptSummary)>;

/// The fields of a transcript record read here. Everything else is skipped
/// without being built.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    /// The record's kind: `user`, `assistant`, `custom-title`, …
    #[serde(rename = "type")]
    kind: Option<String>,
    /// When the record was written, ISO 8601. Metadata records have none.
    timestamp: Option<String>,
    /// The folder the session worked in when the record was written.
    cwd: Option<String>,
    /// The record belongs to a subagent's conversation, not the session's own.
    is_sidechain: Option<bool>,
    /// A message Claude Code added on the user's behalf, not one they typed.
    #[serde(default)]
    is_meta: bool,
    /// Of a `custom-title` record.
    custom_title: Option<String>,
    /// Of an `ai-title` record.
    ai_title: Option<String>,
    /// Of a `last-prompt` record.
    last_prompt: Option<String>,
    /// Of a `relocated` record: the folder the session moved to.
    relocated_cwd: Option<String>,
}

/// A `user` record, read again for its message once it is known to be the
/// first one that can hold the first prompt.
#[derive(Deserialize)]
struct UserRecord {
    /// The message the record carries.
    message: UserMessage,
}

/// The message of a [`UserRecord`].
#[derive(Deserialize)]
struct UserMessage {
    /// What the message says.
    content: Content,
}

/// A message's content: plain text, or a list of blocks (text, images, tool
/// results).
#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    /// Plain text.
    Text(String),
    /// A list of blocks.
    Blocks(Vec<Block>),
}

/// One block of a [`Content::Blocks`] list.
#[derive(Deserialize)]
struct Block {
    /// The block's kind: `text`, `image`, `tool_result`, …
    #[serde(rename = "type")]
    kind: Option<String>,
    /// Of a `text` block.
    text: Option<String>,
}

/// The `<slug>/<id>/custom-title.json` file.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TitleFile {
    /// The session's name.
    custom_title: Option<String>,
}

/// Every session transcript in `config_dir`: the `projects/<slug>/<id>.jsonl`
/// files, one level down only, as subagent transcripts sit deeper, in
/// `<slug>/<id>/subagents/`. Transcripts of subagent records only are left
/// out, as is anything that can't be read.
pub fn scan_projects(config_dir: &Path) -> Vec<TranscriptSummary> {
    let projects = config_dir.join("projects");
    let Ok(slugs) = fs::read_dir(&projects) else {
        return Vec::new();
    };
    let mut summaries = Vec::new();
    let mut scanned = HashSet::new();
    for slug in slugs.flatten() {
        if !slug.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(files) = fs::read_dir(slug.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path
                .extension()
                .is_none_or(|extension| extension != "jsonl")
            {
                continue;
            }
            scanned.insert(path.clone());
            if let Some(summary) = cached_summary(&path) {
                summaries.push(with_title_file(summary));
            }
        }
    }
    // Forget transcripts that have gone (moved, archived), so the cache stays
    // the size of what is on disk.
    lock_cache().retain(|path, _| !path.starts_with(&projects) || scanned.contains(path));
    summaries
}

/// Read the summary of the transcript at `path`. `None` when it can't be read,
/// or holds a subagent's records only.
#[allow(dead_code)] // Consumed by the sessions commands.
pub fn summarize(path: &Path) -> Option<TranscriptSummary> {
    let file = File::open(path).ok()?;
    let modified = file.metadata().ok()?.modified().ok()?;
    read_summary(path, file, modified).map(with_title_file)
}

/// The files and folders that make up `summary`'s session in `config_dir`,
/// those that exist of: the transcript, the `<slug>/<id>/` folder beside it and
/// `file-history/<id>/`.
#[allow(dead_code)] // Consumed by the sessions commands.
pub fn bundle_paths(config_dir: &Path, summary: &TranscriptSummary) -> Vec<PathBuf> {
    let sibling = summary.path.with_file_name(&summary.session_id);
    let history = config_dir.join("file-history").join(&summary.session_id);
    [summary.path.clone(), sibling, history]
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

/// Take [`SUMMARY_CACHE`]. Entries are whole values swapped in and out, so a
/// panic under the lock leaves nothing half-written.
fn lock_cache() -> MutexGuard<'static, SummaryCache> {
    SUMMARY_CACHE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The summary of the transcript at `path`, without the title file fallback:
/// from [`SUMMARY_CACHE`] if the file hasn't changed since it was read, else
/// read now and cached. The lock isn't held while reading, so scans of other
/// config dirs don't wait on this one.
fn cached_summary(path: &Path) -> Option<TranscriptSummary> {
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    let len = metadata.len();
    let modified = metadata.modified().ok()?;
    if let Some((cached_len, cached_modified, summary)) = lock_cache().get(path) {
        if *cached_len == len && *cached_modified == modified {
            return Some(summary.clone());
        }
    }
    let summary = read_summary(path, file, modified)?;
    lock_cache().insert(path.to_path_buf(), (len, modified, summary.clone()));
    Some(summary)
}

/// Read the summary of the transcript `file` at `path`, last written at
/// `modified`, from its records. Later records win. Lines that aren't a
/// record, like the last one while Claude Code is still writing it, are
/// skipped; reading stops at the first line that isn't UTF-8, which only a
/// write cut short leaves. Only the last `timestamp` is kept, and parsed once
/// at the end; one that doesn't parse leaves the file's modification time.
fn read_summary(path: &Path, file: File, modified: SystemTime) -> Option<TranscriptSummary> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let mut summary = TranscriptSummary {
        session_id,
        path: path.to_path_buf(),
        cwd: None,
        custom_title: None,
        ai_title: None,
        first_prompt: None,
        last_prompt: None,
        last_used_at: DateTime::<Utc>::from(modified),
    };
    let mut last_timestamp = None;
    let mut sidechain_seen = false;
    let mut main_seen = false;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if !MARKERS.iter().any(|marker| line.contains(marker)) {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Record>(&line) else {
            continue;
        };
        match record.is_sidechain {
            Some(true) => sidechain_seen = true,
            Some(false) => main_seen = true,
            None => {}
        }
        if record.timestamp.is_some() {
            last_timestamp = record.timestamp;
        }
        match record.kind.as_deref() {
            Some(kind @ ("user" | "assistant")) => {
                if record.cwd.is_some() {
                    summary.cwd = record.cwd;
                }
                let typed = kind == "user" && record.is_sidechain != Some(true) && !record.is_meta;
                if typed && summary.first_prompt.is_none() {
                    summary.first_prompt = first_text(&line);
                }
            }
            Some("custom-title") => {
                summary.custom_title = record.custom_title.or(summary.custom_title);
            }
            Some("ai-title") => {
                summary.ai_title = record.ai_title.or(summary.ai_title);
            }
            Some("last-prompt") => {
                summary.last_prompt = record.last_prompt.or(summary.last_prompt);
            }
            Some("relocated") => {
                summary.cwd = record.relocated_cwd.or(summary.cwd);
            }
            _ => {}
        }
    }
    if sidechain_seen && !main_seen {
        return None;
    }
    if let Some(timestamp) = last_timestamp.and_then(|timestamp| timestamp.parse().ok()) {
        summary.last_used_at = timestamp;
    }
    Some(summary)
}

/// The text of the `user` record on `line`, trimmed and cut to
/// [`FIRST_PROMPT_MAX_CHARS`]: its content if plain text, else its first
/// non-blank text block. `None` for a record with no text, such as a tool
/// result.
fn first_text(line: &str) -> Option<String> {
    let record = serde_json::from_str::<UserRecord>(line).ok()?;
    let text = match record.message.content {
        Content::Text(text) => text,
        Content::Blocks(blocks) => blocks
            .into_iter()
            .filter(|block| block.kind.as_deref() == Some("text"))
            .filter_map(|block| block.text)
            .find(|text| !text.trim().is_empty())?,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(FIRST_PROMPT_MAX_CHARS).collect())
}

/// `summary` with its custom title taken from `<slug>/<id>/custom-title.json`
/// when no record in the transcript sets one.
fn with_title_file(mut summary: TranscriptSummary) -> TranscriptSummary {
    if summary.custom_title.is_none() {
        let title_file = summary
            .path
            .with_file_name(&summary.session_id)
            .join("custom-title.json");
        summary.custom_title = fs::read_to_string(title_file)
            .ok()
            .and_then(|text| serde_json::from_str::<TitleFile>(&text).ok())
            .and_then(|file| file.custom_title);
    }
    summary
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Write;
    use std::time::{Duration, SystemTime};

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;

    const SESSION: &str = "0b7c5a1e-4f7a-4c55-9d1e-3a2b1c0d9e8f";

    fn write_lines(path: &Path, lines: &[Value]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
        fs::write(path, body).unwrap();
    }

    fn transcript_path(config_dir: &Path, session: &str) -> PathBuf {
        config_dir
            .join("projects")
            .join("-work-app")
            .join(format!("{session}.jsonl"))
    }

    fn user(timestamp: &str, cwd: &str, content: Value) -> Value {
        json!({
            "type": "user",
            "sessionId": SESSION,
            "uuid": "u",
            "timestamp": timestamp,
            "cwd": cwd,
            "entrypoint": "cli",
            "isSidechain": false,
            "message": { "role": "user", "content": content },
        })
    }

    fn assistant(timestamp: &str, cwd: &str) -> Value {
        json!({
            "type": "assistant",
            "sessionId": SESSION,
            "uuid": "a",
            "timestamp": timestamp,
            "cwd": cwd,
            "isSidechain": false,
            "message": { "role": "assistant", "content": [{ "type": "text", "text": "Done." }] },
        })
    }

    fn utc(timestamp: &str) -> DateTime<Utc> {
        timestamp.parse().unwrap()
    }

    #[test]
    fn a_transcript_is_summarized_from_its_latest_records() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(
            &path,
            &[
                user(
                    "2026-09-01T10:00:00.000Z",
                    "/work/app",
                    json!("  Fix the login bug  "),
                ),
                json!({ "type": "ai-title", "aiTitle": "Login fix", "sessionId": SESSION }),
                json!({ "type": "custom-title", "customTitle": "First name", "sessionId": SESSION }),
                assistant("2026-09-01T10:05:00.000Z", "/work/app"),
                json!({ "type": "custom-title", "customTitle": "Second name", "sessionId": SESSION }),
                json!({ "type": "ai-title", "aiTitle": "Login and logout fix", "sessionId": SESSION }),
                json!({ "type": "last-prompt", "lastPrompt": "Fix the login bug", "sessionId": SESSION }),
                user(
                    "2026-09-01T10:07:30.000Z",
                    "/work/app/web",
                    json!([{ "type": "text", "text": "And logout" }]),
                ),
                json!({ "type": "last-prompt", "lastPrompt": "And logout", "sessionId": SESSION }),
            ],
        );

        let summary = summarize(&path).unwrap();

        assert_eq!(summary.session_id, SESSION);
        assert_eq!(summary.path, path);
        assert_eq!(summary.cwd.as_deref(), Some("/work/app/web"));
        assert_eq!(summary.custom_title.as_deref(), Some("Second name"));
        assert_eq!(summary.ai_title.as_deref(), Some("Login and logout fix"));
        assert_eq!(summary.first_prompt.as_deref(), Some("Fix the login bug"));
        assert_eq!(summary.last_prompt.as_deref(), Some("And logout"));
        assert_eq!(summary.last_used_at, utc("2026-09-01T10:07:30Z"));
    }

    #[test]
    fn the_first_prompt_is_the_first_text_the_user_typed_cut_to_200_chars() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        let long = "é".repeat(250);
        let mut meta = user(
            "2026-09-01T10:00:00Z",
            "/work",
            json!("<local-command-caveat>"),
        );
        meta["isMeta"] = json!(true);
        write_lines(
            &path,
            &[
                meta,
                user(
                    "2026-09-01T10:00:01Z",
                    "/work",
                    json!([{ "type": "tool_result", "tool_use_id": "t", "content": "output" }]),
                ),
                user(
                    "2026-09-01T10:00:02Z",
                    "/work",
                    json!([{ "type": "image" }, { "type": "text", "text": long }]),
                ),
                user("2026-09-01T10:00:03Z", "/work", json!("Later prompt")),
            ],
        );

        let first_prompt = summarize(&path).unwrap().first_prompt.unwrap();

        assert_eq!(first_prompt, "é".repeat(200));
    }

    #[test]
    fn a_relocated_session_works_in_its_new_folder() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(
            &path,
            &[
                user("2026-09-01T10:00:00Z", "/old/place", json!("Hi")),
                json!({ "type": "relocated", "relocatedCwd": "/new/place", "sessionId": SESSION }),
            ],
        );

        assert_eq!(summarize(&path).unwrap().cwd.as_deref(), Some("/new/place"));
    }

    #[test]
    fn the_custom_title_falls_back_to_the_sessions_title_file() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(&path, &[user("2026-09-01T10:00:00Z", "/work", json!("Hi"))]);
        let title_file = path.with_extension("").join("custom-title.json");
        write_lines(&title_file, &[json!({ "customTitle": "From the file" })]);

        assert_eq!(
            summarize(&path).unwrap().custom_title.as_deref(),
            Some("From the file")
        );
    }

    #[test]
    fn a_line_cut_short_by_a_write_in_progress_is_ignored() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(
            &path,
            &[
                user("2026-09-01T10:00:00Z", "/work", json!("Hi")),
                json!({ "type": "custom-title", "customTitle": "Kept", "sessionId": SESSION }),
            ],
        );
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(br#"{"type":"custom-title","customTitle":"Cut sho"#)
            .unwrap();

        let summary = summarize(&path).unwrap();

        assert_eq!(summary.custom_title.as_deref(), Some("Kept"));
        assert_eq!(summary.last_used_at, utc("2026-09-01T10:00:00Z"));
    }

    #[test]
    fn a_transcript_without_timestamps_was_last_used_when_last_written() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(
            &path,
            &[json!({ "type": "custom-title", "customTitle": "Named", "sessionId": SESSION })],
        );
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_750_000_000);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(written)
            .unwrap();

        let summary = summarize(&path).unwrap();

        assert_eq!(summary.last_used_at, DateTime::<Utc>::from(written));
    }

    #[test]
    fn a_transcript_of_sidechain_records_only_is_no_session() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        let mut record = user("2026-09-01T10:00:00Z", "/work", json!("Subtask"));
        record["isSidechain"] = json!(true);
        write_lines(
            &path,
            &[
                record,
                json!({ "type": "ai-title", "aiTitle": "Subtask", "sessionId": SESSION }),
            ],
        );

        assert_eq!(summarize(&path), None);
    }

    #[test]
    fn scanning_lists_top_level_transcripts_only() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(&path, &[user("2026-09-01T10:00:00Z", "/work", json!("Hi"))]);
        let subagent = path
            .with_extension("")
            .join("subagents")
            .join("agent-1.jsonl");
        write_lines(
            &subagent,
            &[user("2026-09-01T10:00:00Z", "/work", json!("Sub"))],
        );
        write_lines(
            &path.with_file_name("notes.txt"),
            &[user(
                "2026-09-01T10:00:00Z",
                "/work",
                json!("Not a transcript"),
            )],
        );

        let summaries = scan_projects(root.path());

        let ids: Vec<&str> = summaries
            .iter()
            .map(|summary| summary.session_id.as_str())
            .collect();
        assert_eq!(ids, [SESSION]);
    }

    #[test]
    fn scanning_rereads_a_transcript_that_changed() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(&path, &[user("2026-09-01T10:00:00Z", "/work", json!("Hi"))]);
        assert_eq!(scan_projects(root.path())[0].last_prompt, None);

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        let line = json!({ "type": "last-prompt", "lastPrompt": "Hi", "sessionId": SESSION });
        writeln!(file, "{line}").unwrap();

        assert_eq!(
            scan_projects(root.path())[0].last_prompt.as_deref(),
            Some("Hi")
        );
    }

    #[test]
    fn a_config_dir_without_projects_has_no_sessions() {
        let root = tempdir().unwrap();

        assert_eq!(scan_projects(root.path()), []);
    }

    #[test]
    fn the_bundle_is_the_parts_of_the_session_that_exist() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(&path, &[user("2026-09-01T10:00:00Z", "/work", json!("Hi"))]);
        let sibling = path.with_extension("");
        fs::create_dir_all(sibling.join("tool-results")).unwrap();
        let summary = summarize(&path).unwrap();

        assert_eq!(
            bundle_paths(root.path(), &summary),
            [path.clone(), sibling.clone()]
        );

        let history = root.path().join("file-history").join(SESSION);
        fs::create_dir_all(&history).unwrap();

        assert_eq!(
            bundle_paths(root.path(), &summary),
            [path, sibling, history]
        );
    }
}
