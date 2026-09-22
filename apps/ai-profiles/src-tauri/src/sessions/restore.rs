//! Putting an archived session back.
//!
//! An archive (see [`super::archive`]) is
//! `<config>/session-transfer-backups/<id>/<time>-archived/`, holding the
//! transcript at `projects/<project>/<id>.jsonl` and any desktop records at
//! `desktop-records/<account>/<org>/local_<uuid>.json`, each at the path it
//! came from. Restoring moves them back and removes the emptied archive.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::archive::{move_into, BACKUPS_DIR};
use super::desktop;
use super::scan::{self, is_safe_name};
use super::transfer::single_transcript;
use super::{apps_blocker, home, parse_process_list, quit_apps, AppToQuit, Home};
use crate::error::{AppError, AppResult};
use crate::launch::process_list;

const ARCHIVED_SUFFIX: &str = "-archived";
const RECORDS_DIR: &str = "desktop-records";

/// One archived session, as the Archived list shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedSession {
    pub id: String,
    /// The archive folder's name, `<time>-archived`: which archive of the
    /// session this is, since one can be archived more than once.
    pub archive: String,
    /// When it was archived, RFC 3339, read from the folder's name.
    pub archived_at: Option<String>,
    pub title: Option<String>,
    pub cwd: Option<String>,
    /// The archive holds the desktop app's record, so restoring lists the
    /// session in the app again.
    pub in_desktop: bool,
}

/// What stands between an archived session and being restored.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreCheck {
    /// A reason only the user can clear, if there is one.
    pub blocker: Option<String>,
    /// The profile's desktop app, if it has to quit so the session can go
    /// back into its list.
    pub app_to_quit: Option<AppToQuit>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    /// Where the transcript went back to.
    pub transcript: String,
}

/// What one archive holds.
struct Archive {
    root: PathBuf,
    project: String,
    transcript: PathBuf,
    /// (archived record, where it goes back to).
    records: Vec<(PathBuf, PathBuf)>,
}

/// The archived sessions of `home`, newest first.
pub fn list_archived(home: &Home) -> Vec<ArchivedSession> {
    let Ok(sessions) = fs::read_dir(home.config_dir.join(BACKUPS_DIR)) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for session in sessions.flatten() {
        let id = session.file_name().to_string_lossy().into_owned();
        let Ok(archives) = fs::read_dir(session.path()) else {
            continue;
        };
        for archive in archives.flatten() {
            let name = archive.file_name().to_string_lossy().into_owned();
            if !name.ends_with(ARCHIVED_SUFFIX) {
                continue;
            }
            let Ok(contents) = read_archive(home, &id, &name) else {
                continue;
            };
            let info = scan::read_transcript(&contents.transcript).unwrap_or_default();
            let record_title = contents.records.iter().find_map(|(path, _)| {
                let text = fs::read_to_string(path).ok()?;
                let body: Value = serde_json::from_str(&text).ok()?;
                body.get("title")?.as_str().map(str::to_string)
            });
            found.push(ArchivedSession {
                id: id.clone(),
                archived_at: archived_at(&name),
                title: record_title.or_else(|| info.name().map(|(name, _)| name)),
                cwd: info.cwd,
                in_desktop: !contents.records.is_empty(),
                archive: name,
            });
        }
    }
    // The folder names start with the time, so newest first is a name sort.
    found.sort_by(|a, b| b.archive.cmp(&a.archive).then(a.id.cmp(&b.id)));
    found
}

/// `20260922-130832-archived` as RFC 3339, in local time.
fn archived_at(name: &str) -> Option<String> {
    let stamp = name.strip_suffix(ARCHIVED_SUFFIX)?;
    let naive = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
    let local = naive.and_local_timezone(chrono::Local).earliest()?;
    Some(local.to_rfc3339())
}

/// Read archive `archive` of session `id` in `home`.
fn read_archive(home: &Home, id: &str, archive: &str) -> AppResult<Archive> {
    if !is_safe_name(id) || !is_safe_name(archive) || !archive.ends_with(ARCHIVED_SUFFIX) {
        return Err(AppError::Validation(format!(
            "invalid archive {id}/{archive}"
        )));
    }
    let root = home.config_dir.join(BACKUPS_DIR).join(id).join(archive);
    let transcript_name = format!("{id}.jsonl");
    let mut transcripts: Vec<(String, PathBuf)> = fs::read_dir(root.join("projects"))
        .map_err(|_| AppError::NotFound(format!("archive {archive} of session {id} not found")))?
        .flatten()
        .map(|project| {
            (
                project.file_name().to_string_lossy().into_owned(),
                project.path().join(&transcript_name),
            )
        })
        .filter(|(_, path)| path.is_file())
        .collect();
    if transcripts.len() != 1 {
        return Err(AppError::Validation(format!(
            "archive {archive} of session {id} holds {} transcripts",
            transcripts.len()
        )));
    }
    let (project, transcript) = transcripts.pop().expect("one transcript");

    let records_root = root.join(RECORDS_DIR);
    let live_root = home.gui_data_dir.join("claude-code-sessions");
    let mut records = Vec::new();
    for account in fs::read_dir(&records_root).into_iter().flatten().flatten() {
        for org in fs::read_dir(account.path()).into_iter().flatten().flatten() {
            for record in fs::read_dir(org.path()).into_iter().flatten().flatten() {
                let path = record.path();
                let Ok(rel) = path.strip_prefix(&records_root) else {
                    continue;
                };
                if path.extension().is_some_and(|ext| ext == "json") {
                    records.push((path.clone(), live_root.join(rel)));
                }
            }
        }
    }
    Ok(Archive {
        root,
        project,
        transcript,
        records,
    })
}

struct Prepared {
    home: Home,
    archive: Archive,
    check: RestoreCheck,
}

fn prepare(profile_id: &str, session_id: &str, archive: &str) -> AppResult<Prepared> {
    let home = home(profile_id)?;
    let contents = read_archive(&home, session_id, archive)?;

    let mut blocker = None;
    if single_transcript(&home, session_id)?.is_some() {
        blocker = Some(format!(
            "{} already has this session. Archive or move that copy first.",
            home.label
        ));
    } else if contents.records.iter().any(|(_, to)| to.exists()) {
        blocker = Some(format!(
            "Claude ({}) already lists a session under the same record.",
            home.label
        ));
    }
    let processes = parse_process_list(&process_list()?);
    let app_to_quit = (!contents.records.is_empty() && desktop::app_running(&home, &processes))
        .then(|| AppToQuit::of(&home));
    Ok(Prepared {
        home,
        archive: contents,
        check: RestoreCheck {
            blocker,
            app_to_quit,
        },
    })
}

/// What restoring the session would need, without doing it.
pub fn check_restore(profile_id: &str, session_id: &str, archive: &str) -> AppResult<RestoreCheck> {
    Ok(prepare(profile_id, session_id, archive)?.check)
}

/// Restore archive `archive` of session `session_id` in profile `profile_id`.
/// Refuses while the profile has a live copy of the session. The profile's
/// desktop app, when a record goes back into its list, is quit first if
/// `quit_app` is set and refused otherwise.
pub fn restore(
    profile_id: &str,
    session_id: &str,
    archive: &str,
    quit_app: bool,
) -> AppResult<RestoreReport> {
    let mut prepared = prepare(profile_id, session_id, archive)?;
    if let Some(blocker) = prepared.check.blocker {
        return Err(AppError::Validation(blocker));
    }
    if let Some(app) = prepared.check.app_to_quit.clone() {
        if !quit_app {
            return Err(apps_blocker(&[app]));
        }
        quit_apps(&[app])?;
        prepared = prepare(profile_id, session_id, archive)?;
        if let Some(app) = prepared.check.app_to_quit {
            return Err(apps_blocker(&[app]));
        }
    }
    restore_files(&prepared.home, session_id, &prepared.archive)
}

fn restore_files(home: &Home, id: &str, archive: &Archive) -> AppResult<RestoreReport> {
    let transcript = home
        .config_dir
        .join("projects")
        .join(&archive.project)
        .join(format!("{id}.jsonl"));
    for (from, to) in &archive.records {
        move_into(from, to)?;
    }
    move_into(&archive.transcript, &transcript)?;
    remove_empty_dirs(&archive.root);
    if let Some(session_dir) = archive.root.parent() {
        let _ = fs::remove_dir(session_dir);
    }
    Ok(RestoreReport {
        transcript: transcript.display().to_string(),
    })
}

/// Remove `dir` and the folders under it, if no file is left in any of them.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::archive::archive_files;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

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

    fn seed(home: &Home) {
        write(
            &home.config_dir.join("projects/-w/s.jsonl"),
            concat!(
                r#"{"type":"user","cwd":"/w"}"#,
                "\n",
                r#"{"type":"last-prompt","lastPrompt":"do it"}"#,
                "\n"
            ),
        );
        write(
            &home.config_dir.join("projects/-w/s/subagents/a.jsonl"),
            "a",
        );
        write(
            &home
                .gui_data_dir
                .join("claude-code-sessions/acct/org/local_1.json"),
            r#"{"cliSessionId":"s","title":"Named"}"#,
        );
    }

    #[test]
    fn lists_and_restores_what_archive_took_out() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        seed(&home);
        let before_transcript = fs::read(home.config_dir.join("projects/-w/s.jsonl")).unwrap();
        let records = desktop::records(&home.gui_data_dir);
        archive_files(&home, "s", "-w", &records, "20260101-120000").unwrap();

        let archived = list_archived(&home);
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, "s");
        assert_eq!(archived[0].archive, "20260101-120000-archived");
        assert_eq!(archived[0].title.as_deref(), Some("Named"));
        assert_eq!(archived[0].cwd.as_deref(), Some("/w"));
        assert!(archived[0].in_desktop);
        assert!(archived[0]
            .archived_at
            .as_deref()
            .unwrap()
            .starts_with("2026-01-01T12:00:00"));

        let contents = read_archive(&home, "s", "20260101-120000-archived").unwrap();
        restore_files(&home, "s", &contents).unwrap();

        assert_eq!(
            fs::read(home.config_dir.join("projects/-w/s.jsonl")).unwrap(),
            before_transcript
        );
        assert_eq!(desktop::records(&home.gui_data_dir).len(), 1);
        assert!(!home.config_dir.join(BACKUPS_DIR).join("s").exists());
        assert!(list_archived(&home).is_empty());
    }

    #[test]
    fn keeps_other_archives_of_the_same_session() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        seed(&home);
        archive_files(&home, "s", "-w", &[], "20260101-120000").unwrap();
        write(&home.config_dir.join("projects/-w/s.jsonl"), "{}\n");
        archive_files(&home, "s", "-w", &[], "20260102-120000").unwrap();

        let archived = list_archived(&home);
        assert_eq!(
            archived
                .iter()
                .map(|a| a.archive.as_str())
                .collect::<Vec<_>>(),
            vec!["20260102-120000-archived", "20260101-120000-archived"]
        );
        let contents = read_archive(&home, "s", "20260102-120000-archived").unwrap();
        restore_files(&home, "s", &contents).unwrap();
        assert_eq!(list_archived(&home).len(), 1);
    }

    #[test]
    fn refuses_names_that_leave_the_backups_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        assert!(read_archive(&home, "..", "x-archived").is_err());
        assert!(read_archive(&home, "s", "../../x-archived").is_err());
        assert!(read_archive(&home, "s", "20260101-120000").is_err());
    }
}
