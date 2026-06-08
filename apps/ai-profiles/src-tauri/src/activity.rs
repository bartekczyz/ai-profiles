//! Per-profile JSONL activity log.
//!
//! Each profile has its own append-only `<profile_dir>/activity.jsonl`
//! file. One JSON object per line. The schema is intentionally narrow —
//! kind, timestamp, optional metadata — so the React side just maps over
//! it and renders a sentence + glyph.
//!
//! Retention: a soft cap of 500 entries per profile. To avoid rewriting
//! the file on every append, we let it grow up to 600 lines (a 20%
//! buffer) before rotating; the rotation keeps the most recent 500.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const RETENTION_KEEP: usize = 500;
const RETENTION_TRIGGER: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Created,
    Renamed,
    ColorChanged,
    SurfaceToggled,
    LaunchedGui,
    CopiedCli,
    Imported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub kind: ActivityKind,
    /// RFC 3339 timestamp.
    pub at: String,
    /// Optional structured metadata — typed as a JSON value so each kind
    /// can carry whatever fits ({surface: 'gui', enabled: false}, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl Activity {
    pub fn now(kind: ActivityKind, metadata: Option<serde_json::Value>) -> Self {
        Self {
            kind,
            at: Utc::now().to_rfc3339(),
            metadata,
        }
    }
}

/// Append a single activity entry to `path`. The parent directory is
/// created if needed. After each append, the file is rotated when its
/// line count crosses RETENTION_TRIGGER (keeps the last RETENTION_KEEP).
pub fn append(path: &Path, activity: &Activity) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(activity)?;
    line.push('\n');
    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(line.as_bytes())?;
    }
    if line_count(path).unwrap_or(0) > RETENTION_TRIGGER {
        rotate(path)?;
    }
    Ok(())
}

/// Read the last `limit` entries from the log, newest first. Returns an
/// empty vector if the file doesn't exist yet (a profile that hasn't
/// generated any events). Malformed lines are skipped — the log is
/// best-effort, not a contract.
pub fn read_last_n(path: &Path, limit: usize) -> AppResult<Vec<Activity>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut all: Vec<Activity> = reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();
    all.reverse();
    all.truncate(limit);
    Ok(all)
}

/// Idempotently delete the activity log file. Missing-file is not an
/// error. Currently unused in production (profile deletion removes the
/// whole profile dir, taking activity.jsonl with it), but useful for a
/// future "clear history" affordance — kept exported for that case.
#[allow(dead_code)]
pub fn delete(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path)?;
    Ok(())
}

fn line_count(path: &Path) -> std::io::Result<usize> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut count = 0usize;
    for line in reader.lines() {
        let _ = line?;
        count += 1;
    }
    Ok(count)
}

/// Atomic rotation: read the whole file, keep the last RETENTION_KEEP
/// lines, write to a sibling `.tmp` and rename over the original. On any
/// error mid-rotation, the original file stays intact.
fn rotate(path: &Path) -> AppResult<()> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut all: Vec<String> = reader.lines().map_while(Result::ok).collect();
    if all.len() <= RETENTION_KEEP {
        return Ok(());
    }
    let start = all.len() - RETENTION_KEEP;
    let kept = all.split_off(start);
    let parent = path
        .parent()
        .ok_or_else(|| AppError::NotFound(format!("path {} has no parent", path.display())))?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    {
        let mut out = fs::File::create(&tmp)?;
        for line in kept {
            out.write_all(line.as_bytes())?;
            out.write_all(b"\n")?;
        }
        out.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn scratch_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let scratch = tempdir().unwrap();
        let path = scratch.path().join("activity.jsonl");
        (scratch, path)
    }

    #[test]
    fn read_last_n_returns_empty_for_missing_file() {
        let (_scratch, path) = scratch_path();
        let entries = read_last_n(&path, 10).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn append_then_read_returns_newest_first() {
        let (_scratch, path) = scratch_path();
        for kind in [
            ActivityKind::Created,
            ActivityKind::LaunchedGui,
            ActivityKind::CopiedCli,
        ] {
            append(&path, &Activity::now(kind, None)).unwrap();
        }
        let entries = read_last_n(&path, 10).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].kind, ActivityKind::CopiedCli);
        assert_eq!(entries[1].kind, ActivityKind::LaunchedGui);
        assert_eq!(entries[2].kind, ActivityKind::Created);
    }

    #[test]
    fn read_last_n_respects_limit() {
        let (_scratch, path) = scratch_path();
        for _ in 0..50 {
            append(&path, &Activity::now(ActivityKind::LaunchedGui, None)).unwrap();
        }
        let entries = read_last_n(&path, 5).unwrap();
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn rotation_caps_at_retention_trigger_with_buffer() {
        let (_scratch, path) = scratch_path();
        for _ in 0..1000 {
            append(&path, &Activity::now(ActivityKind::LaunchedGui, None)).unwrap();
        }
        // Rotation runs when count crosses RETENTION_TRIGGER (600), bringing
        // it back to RETENTION_KEEP (500). After enough appends the file
        // ends up somewhere in [500, 600] — never larger.
        let count = line_count(&path).unwrap();
        assert!(
            count <= RETENTION_TRIGGER,
            "expected ≤{RETENTION_TRIGGER}, got {count}"
        );
        assert!(
            count >= RETENTION_KEEP,
            "expected ≥{RETENTION_KEEP}, got {count}"
        );
    }

    #[test]
    fn rotation_keeps_the_most_recent_entries() {
        let (_scratch, path) = scratch_path();
        // Use metadata to tag each entry with its sequence number so we
        // can verify the kept entries are the latest.
        for index in 0..1000usize {
            append(
                &path,
                &Activity::now(
                    ActivityKind::LaunchedGui,
                    Some(serde_json::json!({ "seq": index })),
                ),
            )
            .unwrap();
        }
        let entries = read_last_n(&path, RETENTION_KEEP).unwrap();
        // Newest entry should be seq=999 (the very last append).
        let first = entries.first().unwrap();
        assert_eq!(first.metadata.as_ref().unwrap()["seq"], 999);
    }

    #[test]
    fn delete_is_idempotent_for_missing_files() {
        let (_scratch, path) = scratch_path();
        delete(&path).unwrap();
    }

    #[test]
    fn delete_removes_an_existing_file() {
        let (_scratch, path) = scratch_path();
        append(&path, &Activity::now(ActivityKind::Created, None)).unwrap();
        assert!(path.exists());
        delete(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn malformed_lines_are_skipped_on_read() {
        let (_scratch, path) = scratch_path();
        append(&path, &Activity::now(ActivityKind::Created, None)).unwrap();
        // Append a garbage line manually.
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"this is not json\n").unwrap();
        append(&path, &Activity::now(ActivityKind::LaunchedGui, None)).unwrap();
        let entries = read_last_n(&path, 10).unwrap();
        assert_eq!(entries.len(), 2);
    }
}
