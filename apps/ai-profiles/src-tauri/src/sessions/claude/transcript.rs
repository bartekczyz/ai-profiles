//! Summaries of Claude Code session transcripts.
//!
//! A transcript is a JSONL file Claude Code appends a record to as the session
//! goes: messages (`user`, `assistant`, `attachment`, `system`, most carrying
//! `timestamp`, `cwd` and `isSidechain`) and metadata (`custom-title`,
//! `ai-title`, `last-prompt`, `relocated`) that is re-appended whenever it
//! changes, so the last record of each kind is the current one. Transcripts
//! reach 100 MB and a config dir holds thousands, and the list is re-read on
//! every window focus: what was read is cached until the file changes, a
//! transcript that grew since is read on from where reading stopped, and only
//! lines that can hold a field read here are decoded at all.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::value::RawValue;

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
    /// The plans the session wrote: the `slug` of its records, each the name
    /// of a `<config>/plans/<slug>.md`. Only names that stay in that folder
    /// count.
    pub plan_slugs: BTreeSet<String>,
}

/// How much of the first prompt a summary keeps.
const FIRST_PROMPT_MAX_CHARS: usize = 200;

/// A line is only decoded if it contains one of these: every record read here
/// carries one, and decoding the rest (tool output, file snapshots) is most of
/// the cost of reading a transcript.
const MARKERS: [&str; 7] = [
    "\"timestamp\"",
    "\"slug\"",
    "\"custom-title\"",
    "\"ai-title\"",
    "\"last-prompt\"",
    "\"relocated\"",
    "\"type\":\"user\"",
];

/// Transcripts already read, by path. One that still has the length and
/// modification time it had then is not read again, and one that only grew
/// since is read on from where reading stopped. The title file fallback is
/// applied after the cache, as that file changes on its own.
static SUMMARY_CACHE: LazyLock<Mutex<SummaryCache>> = LazyLock::new(Mutex::default);

/// What reading each transcript found, by its path.
type SummaryCache = HashMap<PathBuf, CachedRead>;

/// What reading a transcript found, kept to go on from.
#[derive(Clone)]
struct CachedRead {
    /// The file's inode when read: a file replaced by another is read afresh.
    inode: u64,
    /// The file's length when read.
    len: u64,
    /// The file's modification time when read.
    modified: SystemTime,
    /// How far it was read: the end of its last whole line.
    offset: u64,
    /// What its whole lines up to `offset` say.
    scan: Scan,
    /// Its summary, `None` for a transcript of subagent records only.
    summary: Option<TranscriptSummary>,
}

/// What a transcript's records say so far, read line by line. Later records
/// win.
#[derive(Debug, Clone, Default)]
struct Scan {
    /// The `cwd` of the last message, or where the session was last
    /// relocated to.
    cwd: Option<String>,
    /// The last `custom-title` record's.
    custom_title: Option<String>,
    /// The last `ai-title` record's.
    ai_title: Option<String>,
    /// The first text the user typed.
    first_prompt: Option<String>,
    /// The last `last-prompt` record's.
    last_prompt: Option<String>,
    /// The last `timestamp`, parsed only once reading is done.
    last_timestamp: Option<String>,
    /// A record of a subagent's conversation was read.
    sidechain_seen: bool,
    /// A record of the session's own conversation was read.
    main_seen: bool,
    /// The plans the session wrote.
    plan_slugs: BTreeSet<String>,
}

/// The fields of a transcript record read here. Everything else is skipped
/// without being built.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record<'a> {
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
    /// The plan the record wrote: `<config>/plans/<slug>.md`.
    slug: Option<String>,
    /// Of a message: the message, as it is written, read only when it may
    /// hold the first prompt.
    #[serde(borrow)]
    message: Option<&'a RawValue>,
}

/// A message of a `user` record, once it is known to be the first one that
/// can hold the first prompt.
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

/// The files and folders that make up `summary`'s session in `config_dir`,
/// those that exist of: the transcript, the `<slug>/<id>/` folder beside it and
/// `file-history/<id>/`.
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
/// from [`SUMMARY_CACHE`] if the file hasn't changed since it was read, read
/// on from where reading stopped if it only grew, else read whole, and
/// cached. The lock isn't held while reading, so scans of other config dirs
/// don't wait on this one.
fn cached_summary(path: &Path) -> Option<TranscriptSummary> {
    let mut file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    let (inode, len, modified) = (metadata.ino(), metadata.len(), metadata.modified().ok()?);
    let cached = lock_cache().get(path).cloned();
    if let Some(cached) = &cached {
        if cached.inode == inode && cached.len == len && cached.modified == modified {
            return cached.summary.clone();
        }
    }
    let (start, mut scan) = match cached {
        Some(cached) if grew(&mut file, &cached, inode, len) => (cached.offset, cached.scan),
        _ => (0, Scan::default()),
    };
    file.seek(SeekFrom::Start(start)).ok()?;
    let (read, tail) = read_lines(BufReader::new(file), &mut scan);
    let summary = finish(&scan, tail.as_deref(), path, modified);
    let read = CachedRead {
        inode,
        len,
        modified,
        offset: start + read,
        scan,
        summary: summary.clone(),
    };
    lock_cache().insert(path.to_path_buf(), read);
    summary
}

/// Whether `file`, read before as `cached` and now of `inode` and `len`, only
/// grew since: it is the same file, no shorter, and still ends a line where
/// reading stopped. Claude Code only ever appends to a transcript.
fn grew(file: &mut File, cached: &CachedRead, inode: u64, len: u64) -> bool {
    if cached.inode != inode || len < cached.len {
        return false;
    }
    if cached.offset == 0 {
        return true;
    }
    let mut last = [0_u8; 1];
    file.seek(SeekFrom::Start(cached.offset - 1)).is_ok()
        && file.read_exact(&mut last).is_ok()
        && last[0] == b'\n'
}

/// Read the lines `reader` holds into `scan`, up to the end of its last whole
/// line. Returns how many bytes those are, with the line after them that has
/// no end yet, which Claude Code may still be writing. Reading stops early,
/// as at the end, when the file can't be read on.
fn read_lines(mut reader: impl BufRead, scan: &mut Scan) -> (u64, Option<Vec<u8>>) {
    let mut read = 0;
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return (read, None),
            Ok(_) if line.last() != Some(&b'\n') => return (read, Some(line)),
            Ok(count) => {
                read += count as u64;
                scan.read(&line);
            }
        }
    }
}

/// The summary of the transcript at `path`, last written at `modified`, whose
/// whole lines say `scan`, and `tail` after them if it has one: `None` when it
/// holds a subagent's records only.
fn finish(
    scan: &Scan,
    tail: Option<&[u8]>,
    path: &Path,
    modified: SystemTime,
) -> Option<TranscriptSummary> {
    let Some(tail) = tail else {
        return scan.summary(path, modified);
    };
    let mut whole = scan.clone();
    whole.read(tail);
    whole.summary(path, modified)
}

impl Scan {
    /// Take in the record on `line`. A line that isn't UTF-8, or isn't a
    /// record, like one Claude Code is still writing, is skipped, as is one
    /// without a field read here.
    fn read(&mut self, line: &[u8]) {
        let Ok(line) = std::str::from_utf8(line) else {
            return;
        };
        if !MARKERS.iter().any(|marker| line.contains(marker)) {
            return;
        }
        let Ok(record) = serde_json::from_str::<Record>(line) else {
            return;
        };
        match record.is_sidechain {
            Some(true) => self.sidechain_seen = true,
            Some(false) => self.main_seen = true,
            None => {}
        }
        if record.timestamp.is_some() {
            self.last_timestamp = record.timestamp;
        }
        if let Some(slug) = record
            .slug
            .filter(|slug| !slug.is_empty() && !slug.starts_with('.') && !slug.contains('/'))
        {
            self.plan_slugs.insert(slug);
        }
        match record.kind.as_deref() {
            Some(kind @ ("user" | "assistant")) => {
                if record.cwd.is_some() {
                    self.cwd = record.cwd;
                }
                let typed = kind == "user" && record.is_sidechain != Some(true) && !record.is_meta;
                if typed && self.first_prompt.is_none() {
                    self.first_prompt = record.message.and_then(first_text);
                }
            }
            Some("custom-title") => {
                self.custom_title = record.custom_title.or(self.custom_title.take());
            }
            Some("ai-title") => {
                self.ai_title = record.ai_title.or(self.ai_title.take());
            }
            Some("last-prompt") => {
                self.last_prompt = record.last_prompt.or(self.last_prompt.take());
            }
            Some("relocated") => {
                self.cwd = record.relocated_cwd.or(self.cwd.take());
            }
            _ => {}
        }
    }

    /// The summary of the transcript at `path`, last written at `modified`,
    /// that says this: `None` when it holds a subagent's records only. Its
    /// last use is its last `timestamp`, or, when that is missing or doesn't
    /// parse, `modified`.
    fn summary(&self, path: &Path, modified: SystemTime) -> Option<TranscriptSummary> {
        if self.sidechain_seen && !self.main_seen {
            return None;
        }
        let last_used_at = self
            .last_timestamp
            .as_deref()
            .and_then(|timestamp| timestamp.parse().ok())
            .unwrap_or_else(|| DateTime::<Utc>::from(modified));
        Some(TranscriptSummary {
            session_id: path.file_stem()?.to_str()?.to_string(),
            path: path.to_path_buf(),
            cwd: self.cwd.clone(),
            custom_title: self.custom_title.clone(),
            ai_title: self.ai_title.clone(),
            first_prompt: self.first_prompt.clone(),
            last_prompt: self.last_prompt.clone(),
            last_used_at,
            plan_slugs: self.plan_slugs.clone(),
        })
    }
}

/// The text of the user's `message`, trimmed and cut to
/// [`FIRST_PROMPT_MAX_CHARS`]: its content if plain text, else its first
/// non-blank text block. `None` for a message with no text, such as a tool
/// result.
fn first_text(message: &RawValue) -> Option<String> {
    let message = serde_json::from_str::<UserMessage>(message.get()).ok()?;
    let text = match message.content {
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

    /// Read the summary of the transcript at `path`, all of it, past the
    /// cache. `None` when it can't be read, or holds a subagent's records
    /// only.
    fn summarize(path: &Path) -> Option<TranscriptSummary> {
        let file = File::open(path).ok()?;
        let modified = file.metadata().ok()?.modified().ok()?;
        let mut scan = Scan::default();
        let (_, tail) = read_lines(BufReader::new(file), &mut scan);
        finish(&scan, tail.as_deref(), path, modified).map(with_title_file)
    }

    /// Writes `lines` to `path`, one JSON record per line, making its folder.
    fn write_lines(path: &Path, lines: &[Value]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
        fs::write(path, body).unwrap();
    }

    /// Where transcript `session` is in `config_dir`.
    fn transcript_path(config_dir: &Path, session: &str) -> PathBuf {
        config_dir
            .join("projects")
            .join("-work-app")
            .join(format!("{session}.jsonl"))
    }

    /// A user record of the tests' session, written at `timestamp` in `cwd`,
    /// saying `content`.
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

    /// An assistant record of the tests' session, written at `timestamp` in
    /// `cwd`.
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

    /// `timestamp`, an RFC 3339 time.
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
    fn the_plans_a_transcript_wrote_are_the_slugs_that_stay_in_the_plans_folder() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        let slug = |slug: &str| {
            let mut record = assistant("2026-09-01T10:01:00Z", "/work");
            record["slug"] = json!(slug);
            record
        };
        write_lines(
            &path,
            &[
                user("2026-09-01T10:00:00Z", "/work", json!("Plan it")),
                slug("bold-plan"),
                slug("../escape"),
                slug(".hidden"),
                slug("bold-plan"),
                slug("second"),
            ],
        );

        let plans: Vec<String> = summarize(&path).unwrap().plan_slugs.into_iter().collect();

        assert_eq!(plans, ["bold-plan", "second"]);
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
    fn a_line_that_isnt_utf8_is_skipped_and_the_rest_read() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(&path, &[user("2026-09-01T10:00:00Z", "/work", json!("Hi"))]);
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"type\":\"custom-title\",\"customTitle\":\"\xff\xfe\"}\n")
            .unwrap();
        let later = json!({ "type": "custom-title", "customTitle": "Kept", "sessionId": SESSION });
        writeln!(file, "{later}").unwrap();

        assert_eq!(
            summarize(&path).unwrap().custom_title.as_deref(),
            Some("Kept")
        );
    }

    /// Date the file at `path` `modified`.
    fn date(path: &Path, modified: SystemTime) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    #[test]
    fn a_transcript_of_sidechain_records_only_is_not_read_again_until_it_changes() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_750_000_000);
        let mut sidechain = user("2026-09-01T10:00:00Z", "/work", json!("Subtasks"));
        sidechain["isSidechain"] = json!(true);
        write_lines(&path, &[sidechain]);
        date(&path, written);
        assert_eq!(scan_projects(root.path()), []);

        // Rewritten to the same length and date, it isn't read again.
        let mut main = user("2026-09-01T10:00:00Z", "/work", json!("Subtask"));
        main["isSidechain"] = json!(false);
        write_lines(&path, &[main]);
        date(&path, written);

        assert_eq!(scan_projects(root.path()), []);
    }

    #[test]
    fn a_transcript_that_grew_reads_the_same_as_read_whole() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(
            &path,
            &[
                user(
                    "2026-09-01T10:00:00Z",
                    "/work/app",
                    json!("Fix the login bug"),
                ),
                json!({ "type": "ai-title", "aiTitle": "Login fix", "sessionId": SESSION }),
            ],
        );
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(br#"{"type":"last-prompt","lastPrompt":"Fix the lo"#)
            .unwrap();
        let first = scan_projects(root.path());
        assert_eq!(first[0].last_prompt, None);

        writeln!(file, r#"gin bug","sessionId":"{SESSION}"}}"#).unwrap();
        let later = [
            assistant("2026-09-01T10:05:00Z", "/work/app/web"),
            json!({ "type": "custom-title", "customTitle": "Login", "sessionId": SESSION }),
            user("2026-09-01T10:07:30Z", "/work/app/web", json!("And logout")),
        ];
        for line in later {
            writeln!(file, "{line}").unwrap();
        }
        drop(file);

        let grown = scan_projects(root.path());

        assert_eq!(grown, [summarize(&path).unwrap()]);
        assert_eq!(grown[0].first_prompt.as_deref(), Some("Fix the login bug"));
        assert_eq!(grown[0].last_prompt.as_deref(), Some("Fix the login bug"));
        assert_eq!(grown[0].custom_title.as_deref(), Some("Login"));
        assert_eq!(grown[0].cwd.as_deref(), Some("/work/app/web"));
        assert_eq!(grown[0].last_used_at, utc("2026-09-01T10:07:30Z"));
    }

    #[test]
    fn a_transcript_that_grew_is_read_on_from_where_it_was_read_to() {
        let root = tempdir().unwrap();
        let path = transcript_path(root.path(), SESSION);
        write_lines(
            &path,
            &[
                user("2026-09-01T10:00:00Z", "/work", json!("Hi")),
                json!({ "type": "ai-title", "aiTitle": "Aaaa", "sessionId": SESSION }),
            ],
        );
        assert_eq!(
            scan_projects(root.path())[0].ai_title.as_deref(),
            Some("Aaaa")
        );

        // Claude Code only ever appends, so what was read isn't read again.
        let text = fs::read_to_string(&path).unwrap().replace("Aaaa", "Bbbb");
        let tail = json!({ "type": "last-prompt", "lastPrompt": "Hi", "sessionId": SESSION });
        fs::write(&path, format!("{text}{tail}\n")).unwrap();

        let grown = scan_projects(root.path());

        assert_eq!(grown[0].ai_title.as_deref(), Some("Aaaa"));
        assert_eq!(grown[0].last_prompt.as_deref(), Some("Hi"));
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
