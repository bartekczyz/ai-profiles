//! Claude Code sessions ai-profiles archived.
//!
//! A session with no desktop record has nothing to flag it archived, so
//! archiving moves its bundle out of Claude Code's sight, to
//! `<config>/ai-profiles-archive/<id>/<utc-timestamp>/` with paths kept
//! relative to `<config>`, beside a `manifest.json` describing the session.
//! That also keeps Claude Code's cleanup of old transcripts off it.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::non_blank;

/// The folder under a config dir holding the archived bundles.
const ARCHIVE_DIR: &str = "ai-profiles-archive";

/// The file describing an archived bundle, in its timestamp folder.
const MANIFEST: &str = "manifest.json";

/// One archived session: its latest bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivedBundle {
    /// The session's id.
    pub session_id: String,
    /// The session's title when it was archived.
    pub title: Option<String>,
    /// The folder the session worked in.
    pub cwd: Option<String>,
    /// The last thing typed into the session.
    pub last_prompt: Option<String>,
    /// When the session was last used, else when it was archived.
    pub last_used_at: DateTime<Utc>,
}

/// The fields of a bundle's `manifest.json` read here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    /// The session's title.
    title: Option<String>,
    /// The folder the session worked in.
    cwd: Option<String>,
    /// The last thing typed into the session.
    last_prompt: Option<String>,
    /// When the session was last used.
    last_used_at: Option<DateTime<Utc>>,
}

/// The sessions archived in `config_dir`, each by its latest bundle that has
/// a readable manifest. Folders starting with `.` hold other things, such as
/// backups of files a move replaced, and are skipped.
pub fn archived_bundles(config_dir: &Path) -> Vec<ArchivedBundle> {
    let Ok(sessions) = fs::read_dir(config_dir.join(ARCHIVE_DIR)) else {
        return Vec::new();
    };
    sessions
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let session_id = entry.file_name().to_str()?.to_string();
            if session_id.starts_with('.') {
                return None;
            }
            latest_bundle(session_id, &entry.path())
        })
        .collect()
}

/// The latest bundle of session `session_id` in `session_dir` that has a
/// readable manifest. The folders are UTC timestamps, so the latest sorts
/// last.
fn latest_bundle(session_id: String, session_dir: &Path) -> Option<ArchivedBundle> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(session_dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    dirs.sort();
    dirs.into_iter().rev().find_map(|dir| {
        let path = dir.join(MANIFEST);
        let text = fs::read_to_string(&path).ok()?;
        let manifest = serde_json::from_str::<Manifest>(&text).ok()?;
        let last_used_at = match manifest.last_used_at {
            Some(last_used_at) => last_used_at,
            None => DateTime::from(fs::metadata(&path).ok()?.modified().ok()?),
        };
        Some(ArchivedBundle {
            session_id: session_id.clone(),
            title: non_blank(manifest.title),
            cwd: non_blank(manifest.cwd),
            last_prompt: non_blank(manifest.last_prompt),
            last_used_at,
        })
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;

    fn write_manifest(config_dir: &Path, session: &str, stamp: &str, manifest: &Value) -> PathBuf {
        let dir = config_dir.join(ARCHIVE_DIR).join(session).join(stamp);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST), manifest.to_string()).unwrap();
        dir
    }

    #[test]
    fn each_archived_session_is_read_from_its_latest_manifest() {
        let root = tempdir().unwrap();
        write_manifest(
            root.path(),
            "s1",
            "2026-09-01T10-00-00Z",
            &json!({ "title": "Old", "cwd": "/old" }),
        );
        write_manifest(
            root.path(),
            "s1",
            "2026-09-02T10-00-00Z",
            &json!({
                "sessionId": "s1",
                "title": "Fix the login bug",
                "cwd": "/work/app",
                "lastPrompt": "And logout",
                "lastUsedAt": "2026-09-01T10:07:30Z",
            }),
        );
        let broken = root
            .path()
            .join(ARCHIVE_DIR)
            .join("s1")
            .join("2026-09-03T10-00-00Z");
        fs::create_dir_all(&broken).unwrap();
        fs::write(broken.join(MANIFEST), "{\"title\":").unwrap();

        assert_eq!(
            archived_bundles(root.path()),
            [ArchivedBundle {
                session_id: "s1".to_string(),
                title: Some("Fix the login bug".to_string()),
                cwd: Some("/work/app".to_string()),
                last_prompt: Some("And logout".to_string()),
                last_used_at: "2026-09-01T10:07:30Z".parse().unwrap(),
            }]
        );
    }

    #[test]
    fn a_manifest_without_a_last_use_was_last_used_when_written() {
        let root = tempdir().unwrap();
        let dir = write_manifest(root.path(), "s1", "2026-09-01T10-00-00Z", &json!({}));
        let written = fs::metadata(dir.join(MANIFEST))
            .unwrap()
            .modified()
            .unwrap();

        let bundles = archived_bundles(root.path());

        assert_eq!(bundles[0].last_used_at, DateTime::<Utc>::from(written));
    }

    #[test]
    fn blank_manifest_fields_are_absent() {
        let root = tempdir().unwrap();
        write_manifest(
            root.path(),
            "s1",
            "2026-09-01T10-00-00Z",
            &json!({ "title": " ", "cwd": "", "lastPrompt": "\n" }),
        );

        let bundle = &archived_bundles(root.path())[0];

        assert_eq!(
            (&bundle.title, &bundle.cwd, &bundle.last_prompt),
            (&None, &None, &None)
        );
    }

    #[test]
    fn backups_and_sessions_without_a_manifest_are_skipped() {
        let root = tempdir().unwrap();
        write_manifest(root.path(), ".replaced", "2026-09-01T10-00-00Z", &json!({}));
        fs::create_dir_all(
            root.path()
                .join(ARCHIVE_DIR)
                .join("s2")
                .join("2026-09-01T10-00-00Z"),
        )
        .unwrap();

        assert_eq!(archived_bundles(root.path()), []);
    }
}
