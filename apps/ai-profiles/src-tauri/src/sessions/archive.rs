//! Taking a session out of a profile without deleting it.
//!
//! The transcript, and the desktop app's record of the session if it has one,
//! move to `<config>/session-transfer-backups/<id>/<time>-archived/`, keeping
//! their paths, so putting them back is a move the other way. The session's
//! other folders stay where they are: the transcript names files in them by
//! absolute path, and they are what makes it whole again if it is restored.
//! A move archives its source the same way.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::desktop::{self, DesktopRecord};
use super::transfer::single_transcript;
use super::{home, open_blocker, parse_process_list, Home};
use crate::error::{AppError, AppResult};
use crate::launch::process_list;

pub(super) const BACKUPS_DIR: &str = "session-transfer-backups";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveReport {
    /// The folder the transcript (and desktop record) went to.
    pub archived_to: String,
}

/// Archive session `session_id` of profile `profile_id` (or `default:claude`).
/// Refuses while anything has the session open, and while the desktop app that
/// lists it is running, since that app rewrites its session list from memory.
pub fn archive(profile_id: &str, session_id: &str) -> AppResult<ArchiveReport> {
    if !super::scan::is_safe_name(session_id) {
        return Err(AppError::Validation(format!(
            "invalid session id {session_id:?}"
        )));
    }
    let home = home(profile_id)?;
    let (project, _) = single_transcript(&home, session_id)?.ok_or_else(|| {
        AppError::NotFound(format!("session {session_id} not found in {}", home.label))
    })?;
    let records: Vec<DesktopRecord> = desktop::records(&home.gui_data_dir)
        .into_iter()
        .filter(|record| record.cli_session_id == session_id)
        .collect();

    let processes = parse_process_list(&process_list()?);
    if let Some(blocker) = open_blocker(&home, session_id, &processes) {
        return Err(AppError::Validation(blocker));
    }
    if !records.is_empty() && desktop::app_running(&home, &processes) {
        return Err(AppError::Validation(format!(
            "Quit Claude ({}) first, so the session can be taken out of its list.",
            home.label
        )));
    }

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let root = archive_files(&home, session_id, &project, &records, &stamp)?;
    Ok(ArchiveReport {
        archived_to: root.display().to_string(),
    })
}

/// Move session `id`'s transcript (in `projects/<project>/`) and `records` into
/// `<config>/session-transfer-backups/<id>/<stamp>-archived/`. Returns that
/// folder.
pub(super) fn archive_files(
    home: &Home,
    id: &str,
    project: &str,
    records: &[DesktopRecord],
    stamp: &str,
) -> AppResult<PathBuf> {
    let root = home
        .config_dir
        .join(BACKUPS_DIR)
        .join(id)
        .join(format!("{stamp}-archived"));
    let transcript_rel = Path::new("projects")
        .join(project)
        .join(format!("{id}.jsonl"));
    move_into(
        &home.config_dir.join(&transcript_rel),
        &root.join(&transcript_rel),
    )?;
    let records_root = home.gui_data_dir.join("claude-code-sessions");
    for record in records {
        let rel = record
            .path
            .strip_prefix(&records_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| PathBuf::from(record.path.file_name().unwrap_or_default()));
        move_into(&record.path, &root.join("desktop-records").join(rel))?;
    }
    Ok(root)
}

/// Move `from` to `to`, making `to`'s folder. Both are under the same app data,
/// so this is a rename.
pub(super) fn move_into(from: &Path, to: &Path) -> AppResult<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(from, to)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn archives_the_transcript_and_records_but_keeps_the_session_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home {
            label: "P".into(),
            config_dir: dir.path().join("cli-config"),
            gui_data_dir: dir.path().join("gui-data"),
            stock: false,
            desktop: true,
        };
        write(&home.config_dir.join("projects/-w/s.jsonl"), "t");
        write(
            &home.config_dir.join("projects/-w/s/subagents/a.jsonl"),
            "a",
        );
        write(
            &home
                .gui_data_dir
                .join("claude-code-sessions/acct/org/local_1.json"),
            r#"{"cliSessionId":"s"}"#,
        );
        let records = desktop::records(&home.gui_data_dir);

        let root = archive_files(&home, "s", "-w", &records, "20260101-000000").unwrap();

        assert_eq!(
            root,
            home.config_dir
                .join("session-transfer-backups/s/20260101-000000-archived")
        );
        assert!(!home.config_dir.join("projects/-w/s.jsonl").exists());
        assert!(root.join("projects/-w/s.jsonl").is_file());
        assert!(root.join("desktop-records/acct/org/local_1.json").is_file());
        assert!(desktop::records(&home.gui_data_dir).is_empty());
        assert!(home
            .config_dir
            .join("projects/-w/s/subagents/a.jsonl")
            .is_file());
    }
}
