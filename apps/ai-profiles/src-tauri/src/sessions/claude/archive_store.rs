//! Claude Code sessions ai-profiles archived.
//!
//! A session with no desktop record has nothing to flag it archived, so
//! archiving moves its bundle out of Claude Code's sight, to
//! `<config>/ai-profiles-archive/<id>/<utc-timestamp>/` with paths kept
//! relative to `<config>`, beside a `manifest.json` describing the session.
//! That also keeps Claude Code's cleanup of old transcripts off it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::non_blank;
use crate::error::{AppError, AppResult};

/// The folder under a config dir holding the archived bundles.
const ARCHIVE_DIR: &str = "ai-profiles-archive";

/// The file describing an archived bundle, in its timestamp folder.
const MANIFEST: &str = "manifest.json";

/// How a bundle's timestamp folder is named: UTC, and sortable as text.
const STAMP_FORMAT: &str = "%Y-%m-%dT%H-%M-%SZ";

/// How a move's backup folder is named: UTC to the millisecond, so two moves
/// in one second don't share one, and sortable as text.
const REPLACED_STAMP_FORMAT: &str = "%Y-%m-%dT%H-%M-%S%.3fZ";

/// The folder under [`ARCHIVE_DIR`] holding what moves replaced.
const REPLACED_DIR: &str = ".replaced";

/// Why an archived session can't be restored over files it would replace.
pub const ACTIVE_COPY: &str = "It's already active in this profile";

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

/// What an archived bundle's `manifest.json` holds.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile<'a> {
    /// The session's id.
    session_id: &'a str,
    /// The session's title.
    title: Option<&'a str>,
    /// The folder the session worked in.
    cwd: Option<&'a str>,
    /// The last thing typed into the session.
    last_prompt: Option<&'a str>,
    /// When the session was last used.
    last_used_at: DateTime<Utc>,
    /// When the session was archived.
    archived_at: DateTime<Utc>,
}

/// Archive the session `bundle` describes: move `paths`, its files and
/// folders in `config_dir`, to `<config>/ai-profiles-archive/<id>/<stamp>/`,
/// keeping their paths relative to `config_dir`, beside a `manifest.json` of
/// `bundle`. The moves are renames, so what moves keeps its modification
/// time; one that fails puts back what had moved. What can't be put back
/// stays in the bundle, which keeps its manifest so it still lists and can be
/// restored. Returns the bundle's folder.
pub fn archive_bundle(
    config_dir: &Path,
    bundle: &ArchivedBundle,
    paths: &[PathBuf],
    archived_at: DateTime<Utc>,
) -> AppResult<PathBuf> {
    archive_bundle_with(config_dir, bundle, paths, archived_at, &mut |from, to| {
        fs::rename(from, to)
    })
}

/// [`archive_bundle`], moving each file or folder with `rename`.
fn archive_bundle_with(
    config_dir: &Path,
    bundle: &ArchivedBundle,
    paths: &[PathBuf],
    archived_at: DateTime<Utc>,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> AppResult<PathBuf> {
    let session_id = bundle.session_id.as_str();
    if !is_session_dir_name(session_id) {
        return Err(AppError::Validation(format!(
            "invalid session id {session_id:?}"
        )));
    }
    let relative: Vec<PathBuf> = paths
        .iter()
        .map(|path| {
            path.strip_prefix(config_dir)
                .map(Path::to_path_buf)
                .map_err(|_| {
                    AppError::Validation(format!(
                        "{} isn't in {}",
                        path.display(),
                        config_dir.display()
                    ))
                })
        })
        .collect::<AppResult<_>>()?;
    let session_dir = config_dir.join(ARCHIVE_DIR).join(session_id);
    let bundle_dir = session_dir.join(archived_at.format(STAMP_FORMAT).to_string());
    fs::create_dir_all(&session_dir)?;
    fs::create_dir(&bundle_dir)?;
    let manifest = ManifestFile {
        session_id,
        title: bundle.title.as_deref(),
        cwd: bundle.cwd.as_deref(),
        last_prompt: bundle.last_prompt.as_deref(),
        last_used_at: bundle.last_used_at,
        archived_at,
    };
    let written = fs::write(bundle_dir.join(MANIFEST), serde_json::to_string(&manifest)?);
    let moved = match written {
        Ok(()) => move_all(&relative, config_dir, &bundle_dir, rename),
        Err(error) => Err(MoveFailed {
            error: error.into(),
            stranded: Vec::new(),
        }),
    };
    match moved {
        Ok(()) => Ok(bundle_dir),
        Err(failed) if failed.stranded.is_empty() => {
            let _ = fs::remove_file(bundle_dir.join(MANIFEST));
            remove_empty_dirs(&bundle_dir);
            let _ = fs::remove_dir(&session_dir);
            Err(failed.error)
        }
        Err(failed) => Err(failed.stranded_error(&format!(
            "so they stay archived in {}",
            bundle_dir.display()
        ))),
    }
}

/// Where a move into `config_dir` at `at` keeps what it replaces of session
/// `session_id`: `<config>/ai-profiles-archive/.replaced/<id>/<stamp>/`, laid
/// out like the config dir. Refused for an id that isn't one folder. The folder starts with `.`, so it never lists as
/// an archived session.
pub fn replaced_dir(config_dir: &Path, session_id: &str, at: DateTime<Utc>) -> AppResult<PathBuf> {
    if !is_session_dir_name(session_id) {
        return Err(AppError::Validation(format!(
            "invalid session id {session_id:?}"
        )));
    }
    Ok(config_dir
        .join(ARCHIVE_DIR)
        .join(REPLACED_DIR)
        .join(session_id)
        .join(at.format(REPLACED_STAMP_FORMAT).to_string()))
}

/// Restore session `session_id`'s latest archived bundle in `config_dir`:
/// move its files back to where they were, then remove the emptied bundle
/// folder, and the session's folder with it once that is empty too. Refused
/// when any of them would replace a file already there.
pub fn restore_bundle(config_dir: &Path, session_id: &str) -> AppResult<()> {
    let (bundle_dir, _) = latest_bundle_of(config_dir, session_id)
        .ok_or_else(|| AppError::NotFound(format!("archived session {session_id} not found")))?;
    let items = bundle_items(&bundle_dir);
    if items.iter().any(|item| occupied(&config_dir.join(item))) {
        return Err(AppError::Validation(ACTIVE_COPY.to_string()));
    }
    move_all(&items, &bundle_dir, config_dir, &mut |from, to| {
        fs::rename(from, to)
    })
    .map_err(|failed| {
        if failed.stranded.is_empty() {
            failed.error
        } else {
            failed.stranded_error("so they are restored while the rest stays archived")
        }
    })?;
    // The session is back either way; a manifest left behind only keeps it
    // listed as archived too, until it is removed by hand.
    let _ = fs::remove_file(bundle_dir.join(MANIFEST));
    remove_empty_dirs(&bundle_dir);
    if let Some(session_dir) = bundle_dir.parent() {
        let _ = fs::remove_dir(session_dir);
    }
    Ok(())
}

/// Whether restoring session `session_id`'s latest archived bundle in
/// `config_dir` would replace a file already there. `false` when there is no
/// such bundle.
pub fn has_active_copy(config_dir: &Path, session_id: &str) -> bool {
    latest_bundle_of(config_dir, session_id).is_some_and(|(bundle_dir, _)| {
        bundle_items(&bundle_dir)
            .iter()
            .any(|item| occupied(&config_dir.join(item)))
    })
}

/// The latest bundle of session `session_id` in `config_dir` that has a
/// readable manifest, with its folder.
fn latest_bundle_of(config_dir: &Path, session_id: &str) -> Option<(PathBuf, ArchivedBundle)> {
    if !is_session_dir_name(session_id) {
        return None;
    }
    latest_bundle(
        session_id.to_string(),
        &config_dir.join(ARCHIVE_DIR).join(session_id),
    )
}

/// `session_id` names a session's folder in the archive: one folder, not a
/// hidden one, which hold other things.
fn is_session_dir_name(session_id: &str) -> bool {
    !session_id.is_empty() && !session_id.starts_with('.') && !session_id.contains('/')
}

/// What `bundle_dir` holds of a session, relative to it: the entries of each
/// `projects/<slug>/` (the transcript and the folder beside it) and of
/// `file-history/`, as the bundle is laid out like a config dir.
fn bundle_items(bundle_dir: &Path) -> Vec<PathBuf> {
    let entries = |dir: &Path| -> Vec<PathBuf> {
        fs::read_dir(bundle_dir.join(dir))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| dir.join(entry.file_name()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut items: Vec<PathBuf> = entries(Path::new("projects"))
        .iter()
        .filter(|slug| bundle_dir.join(slug).is_dir())
        .flat_map(|slug| entries(slug))
        .chain(entries(Path::new("file-history")))
        .collect();
    items.sort();
    items
}

/// Why moving a bundle's items stopped part way.
pub(super) struct MoveFailed {
    /// Why the move stopped.
    pub(super) error: AppError,
    /// The items that had moved and could not be put back.
    pub(super) stranded: Vec<PathBuf>,
}

impl MoveFailed {
    /// The error for a move that stopped and left items where they had moved
    /// to, saying what became of them: `outcome`.
    pub(super) fn stranded_error(&self, outcome: &str) -> AppError {
        let items: Vec<String> = self
            .stranded
            .iter()
            .map(|item| item.display().to_string())
            .collect();
        AppError::Validation(format!(
            "{}. {} couldn't be put back, {outcome}",
            self.error,
            items.join(", ")
        ))
    }
}

/// Move each of `items`, relative paths, from under `from` to the same path
/// under `to` with `rename`, making the folders on the way. Refuses to
/// replace anything, a dangling link included. A move that fails puts back
/// the ones before it, and says which of those couldn't be.
pub(super) fn move_all(
    items: &[PathBuf],
    from: &Path,
    to: &Path,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), MoveFailed> {
    for (done, item) in items.iter().enumerate() {
        let target = to.join(item);
        let moved = if occupied(&target) {
            Err(AppError::Validation(format!(
                "{} already exists",
                target.display()
            )))
        } else {
            target
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| rename(&from.join(item), &target))
                .map_err(AppError::from)
        };
        if let Err(error) = moved {
            let stranded = items[..done]
                .iter()
                .filter(|back| rename(&to.join(back), &from.join(back)).is_err())
                .cloned()
                .collect();
            return Err(MoveFailed { error, stranded });
        }
    }
    Ok(())
}

/// Something is at `path`: a file, a folder or a link, even a dangling one.
pub(super) fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Remove `dir` and the folders in it, deepest first, as far as they are
/// empty. Nothing but empty folders is removed.
fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                remove_empty_dirs(&entry.path());
            }
        }
    }
    let _ = fs::remove_dir(dir);
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
            latest_bundle(session_id, &entry.path()).map(|(_, bundle)| bundle)
        })
        .collect()
}

/// The latest bundle of session `session_id` in `session_dir` that has a
/// readable manifest, with its folder. The folders are UTC timestamps, so the
/// latest sorts last.
fn latest_bundle(session_id: String, session_dir: &Path) -> Option<(PathBuf, ArchivedBundle)> {
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
        let bundle = ArchivedBundle {
            session_id: session_id.clone(),
            title: non_blank(manifest.title),
            cwd: non_blank(manifest.cwd),
            last_prompt: non_blank(manifest.last_prompt),
            last_used_at,
        };
        Some((dir, bundle))
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

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

    /// Write `contents` to `path`, making its folder, and date it `modified`.
    fn write_dated(path: &Path, contents: &str, modified: SystemTime) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    /// When the file at `path` was last written.
    fn modified(path: &Path) -> SystemTime {
        fs::metadata(path).unwrap().modified().unwrap()
    }

    /// A config dir holding session `s`: its transcript, a subagent's
    /// transcript in the folder beside it and a file history, each dated
    /// `written`. Returns the paths of the three, as `bundle_paths` would.
    fn session_files(config_dir: &Path, written: SystemTime) -> Vec<PathBuf> {
        let transcript = config_dir.join("projects/-work-app/s.jsonl");
        write_dated(&transcript, "{\"type\":\"user\"}\n", written);
        write_dated(
            &config_dir.join("projects/-work-app/s/subagents/agent-1.jsonl"),
            "{}\n",
            written,
        );
        write_dated(&config_dir.join("file-history/s/abc@v1"), "old", written);
        vec![
            transcript,
            config_dir.join("projects/-work-app/s"),
            config_dir.join("file-history/s"),
        ]
    }

    /// What the Sessions list shows for session `s`.
    fn described() -> ArchivedBundle {
        ArchivedBundle {
            session_id: "s".to_string(),
            title: Some("Fix the login bug".to_string()),
            cwd: Some("/work/app".to_string()),
            last_prompt: Some("And logout".to_string()),
            last_used_at: "2026-09-01T10:07:30Z".parse().unwrap(),
        }
    }

    const ARCHIVED_AT: &str = "2026-09-23T08:15:00Z";

    const SESSION_FILES: [&str; 3] = [
        "projects/-work-app/s.jsonl",
        "projects/-work-app/s/subagents/agent-1.jsonl",
        "file-history/s/abc@v1",
    ];

    #[test]
    fn an_archived_bundle_keeps_its_paths_and_dates_and_lists_as_described() {
        let root = tempdir().unwrap();
        let config = root.path();
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        let paths = session_files(config, written);

        let bundle_dir =
            archive_bundle(config, &described(), &paths, ARCHIVED_AT.parse().unwrap()).unwrap();

        assert_eq!(
            bundle_dir,
            config
                .join(ARCHIVE_DIR)
                .join("s")
                .join("2026-09-23T08-15-00Z")
        );
        for file in SESSION_FILES {
            assert!(!config.join(file).exists(), "{file} was left behind");
            assert_eq!(modified(&bundle_dir.join(file)), written, "{file}");
        }
        assert!(config.join("projects/-work-app").is_dir());
        assert_eq!(archived_bundles(config), [described()]);
    }

    #[test]
    fn a_restored_bundle_is_back_where_it_was_and_its_folders_are_gone() {
        let root = tempdir().unwrap();
        let config = root.path();
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        let paths = session_files(config, written);
        archive_bundle(config, &described(), &paths, ARCHIVED_AT.parse().unwrap()).unwrap();
        let other = write_manifest(config, "other", "2026-09-01T10-00-00Z", &json!({}));

        restore_bundle(config, "s").unwrap();

        for file in SESSION_FILES {
            assert_eq!(modified(&config.join(file)), written, "{file}");
        }
        assert_eq!(
            fs::read_to_string(config.join("file-history/s/abc@v1")).unwrap(),
            "old"
        );
        assert!(!config.join(ARCHIVE_DIR).join("s").exists());
        assert!(other.join(MANIFEST).exists());
        assert_eq!(archived_bundles(config).len(), 1);
    }

    #[test]
    fn a_bundle_is_not_restored_over_an_active_copy() {
        let root = tempdir().unwrap();
        let config = root.path();
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        let paths = session_files(config, written);
        let bundle_dir =
            archive_bundle(config, &described(), &paths, ARCHIVED_AT.parse().unwrap()).unwrap();
        assert!(!has_active_copy(config, "s"));
        fs::create_dir_all(config.join("file-history/s")).unwrap();

        let restored = restore_bundle(config, "s");

        assert!(
            matches!(&restored, Err(AppError::Validation(message)) if message == ACTIVE_COPY),
            "{restored:?}"
        );
        assert!(has_active_copy(config, "s"));
        for file in SESSION_FILES {
            assert!(bundle_dir.join(file).exists(), "{file} left the archive");
        }
        assert!(!config.join("projects/-work-app/s.jsonl").exists());
    }

    #[test]
    fn a_dangling_link_counts_as_an_active_copy() {
        let root = tempdir().unwrap();
        let config = root.path();
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        let paths = session_files(config, written);
        archive_bundle(config, &described(), &paths, ARCHIVED_AT.parse().unwrap()).unwrap();
        let link = config.join("projects/-work-app/s.jsonl");
        std::os::unix::fs::symlink(config.join("nowhere"), &link).unwrap();

        let restored = restore_bundle(config, "s");

        assert!(has_active_copy(config, "s"));
        assert!(
            matches!(&restored, Err(AppError::Validation(message)) if message == ACTIVE_COPY),
            "{restored:?}"
        );
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn an_archive_that_cant_be_put_back_keeps_its_manifest_and_can_be_restored() {
        let root = tempdir().unwrap();
        let config = root.path();
        let written = SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000);
        let paths = session_files(config, written);
        // The file history won't move, and then neither will the transcript
        // go back.
        let mut calls = 0;
        let archived = archive_bundle_with(
            config,
            &described(),
            &paths,
            ARCHIVED_AT.parse().unwrap(),
            &mut |from, to| {
                calls += 1;
                if calls == 3 || calls == 4 {
                    return Err(io::Error::from(io::ErrorKind::PermissionDenied));
                }
                fs::rename(from, to)
            },
        );

        let bundle_dir = config
            .join(ARCHIVE_DIR)
            .join("s")
            .join("2026-09-23T08-15-00Z");
        let message = match &archived {
            Err(AppError::Validation(message)) => message.clone(),
            other => panic!("{other:?}"),
        };
        assert!(message.contains("projects/-work-app/s.jsonl"), "{message}");
        assert!(bundle_dir.join(MANIFEST).exists());
        assert!(bundle_dir.join("projects/-work-app/s.jsonl").exists());
        assert!(config.join("projects/-work-app/s").is_dir());
        assert!(config.join("file-history/s/abc@v1").exists());
        assert_eq!(archived_bundles(config), [described()]);

        restore_bundle(config, "s").unwrap();

        for file in SESSION_FILES {
            assert_eq!(modified(&config.join(file)), written, "{file}");
        }
        assert!(!config.join(ARCHIVE_DIR).join("s").exists());
    }

    #[test]
    fn a_session_with_no_archived_bundle_cant_be_restored() {
        let root = tempdir().unwrap();

        assert!(matches!(
            restore_bundle(root.path(), "s"),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            restore_bundle(root.path(), ".."),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn a_file_outside_the_config_dir_is_not_archived() {
        let root = tempdir().unwrap();
        let config = root.path().join("config");
        let outside = root.path().join("elsewhere.jsonl");
        fs::write(&outside, "{}").unwrap();

        let archived = archive_bundle(
            &config,
            &described(),
            std::slice::from_ref(&outside),
            ARCHIVED_AT.parse().unwrap(),
        );

        assert!(matches!(archived, Err(AppError::Validation(_))));
        assert!(outside.exists());
        assert!(!config.join(ARCHIVE_DIR).exists());
    }

    #[test]
    fn what_a_move_replaces_is_kept_by_session_and_millisecond() {
        let root = tempdir().unwrap();
        let at = "2026-09-23T08:15:00.123Z".parse().unwrap();

        let dir = replaced_dir(root.path(), "s", at).unwrap();

        assert_eq!(
            dir,
            root.path()
                .join(ARCHIVE_DIR)
                .join(".replaced/s/2026-09-23T08-15-00.123Z")
        );
        assert!(matches!(
            replaced_dir(root.path(), "../s", at),
            Err(AppError::Validation(_))
        ));
    }
}
