//! First-run migration: detect an existing Claude Desktop / Claude Code
//! installation and import it as a named profile.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::profiles::{Profile, Surfaces};
use crate::slug::slugify;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExistingInstall {
    pub claude_desktop_path: Option<String>,
    pub claude_code_path: Option<String>,
    /// Bytes occupied by the desktop install dir (best-effort, walks the tree).
    /// `None` when the corresponding path doesn't exist; permission-denied
    /// subpaths during the walk are silently skipped.
    pub claude_desktop_size_bytes: Option<u64>,
    pub claude_code_size_bytes: Option<u64>,
}

impl ExistingInstall {
    #[allow(dead_code)]
    pub fn any_detected(&self) -> bool {
        self.claude_desktop_path.is_some() || self.claude_code_path.is_some()
    }
}

/// Pure: check the two well-known paths and report which exist. Sizes
/// are deliberately left `None` — this is the boot-critical-path entry
/// and walking the trees synchronously can take a second or more on a
/// large `~/.claude`. Use [`detect_sizes`] from a lazy IPC once the
/// MigrationDialog opens.
pub fn detect(claude_desktop_path: &Path, claude_code_path: &Path) -> ExistingInstall {
    let desktop_exists = claude_desktop_path.exists();
    let cli_exists = claude_code_path.exists();
    ExistingInstall {
        claude_desktop_path: desktop_exists.then(|| claude_desktop_path.display().to_string()),
        claude_code_path: cli_exists.then(|| claude_code_path.display().to_string()),
        claude_desktop_size_bytes: None,
        claude_code_size_bytes: None,
    }
}

/// Sizes-only side-table for [`detect`]. Walks each tree once; returns
/// `None` for paths that don't exist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExistingInstallSizes {
    pub claude_desktop_size_bytes: Option<u64>,
    pub claude_code_size_bytes: Option<u64>,
}

pub fn detect_sizes(claude_desktop_path: &Path, claude_code_path: &Path) -> ExistingInstallSizes {
    ExistingInstallSizes {
        claude_desktop_size_bytes: claude_desktop_path
            .exists()
            .then(|| directory_size(claude_desktop_path)),
        claude_code_size_bytes: claude_code_path
            .exists()
            .then(|| directory_size(claude_code_path)),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportParams {
    /// UUID for the new profile. The caller pre-generates this so it can
    /// also pre-compute `profile_dir` consistently.
    pub id: String,
    pub name: String,
    pub color: String,
    pub include_gui: bool,
    pub include_cli: bool,
    /// Absolute path of the existing Claude Desktop dir (None if not detected
    /// or the user unchecked the GUI surface).
    pub gui_source: Option<PathBuf>,
    /// Absolute path of the existing Claude Code dir.
    pub cli_source: Option<PathBuf>,
    /// Where to place the per-profile data dir. In production this is
    /// `<app-data>/profiles/<id>/`; tests pass a tempdir-rooted equivalent.
    pub profile_dir: PathBuf,
    /// Where to place the migration backup. In production this is
    /// `<app-data>/migration-backup-<timestamp>/`; tests pass a tempdir.
    pub backup_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub profile: Profile,
    pub backup_dir: String,
}

/// Run the migration. Order: copy → profile-dir, move → backup, [caller]
/// generates launchers + persists. This function does NOT generate launchers
/// or update profiles.json — that's the caller's job (the IPC handler), so
/// this stays unit-testable without touching /Applications or ~/.local/bin.
///
/// Rolls back its own filesystem effects on failure.
pub fn import(params: ImportParams) -> AppResult<ImportOutcome> {
    if !params.include_gui && !params.include_cli {
        return Err(AppError::Validation(
            "must include at least one surface".to_string(),
        ));
    }

    let trimmed = params.name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("name must not be empty".to_string()));
    }
    let slug = slugify(trimmed);
    if slug.is_empty() {
        return Err(AppError::Validation(
            "name produced an empty slug after sanitisation".to_string(),
        ));
    }

    // Step 1: Create per-profile dirs.
    let gui_data = params.profile_dir.join("gui-data");
    let cli_config = params.profile_dir.join("cli-config");
    if params.include_gui {
        fs::create_dir_all(&gui_data)?;
    }
    if params.include_cli {
        fs::create_dir_all(&cli_config)?;
    }

    // Step 2: Copy source → profile-dir.
    let mut copied_gui = false;
    let mut copied_cli = false;
    if params.include_gui {
        if let Some(source) = params.gui_source.as_ref() {
            if let Err(err) = copy_dir_recursive(source, &gui_data) {
                let _ = fs::remove_dir_all(&params.profile_dir);
                return Err(err);
            }
            copied_gui = true;
        }
    }
    if params.include_cli {
        if let Some(source) = params.cli_source.as_ref() {
            if let Err(err) = copy_dir_recursive(source, &cli_config) {
                let _ = fs::remove_dir_all(&params.profile_dir);
                return Err(err);
            }
            copied_cli = true;
        }
    }

    // Step 3: Make the backup dir.
    if let Err(err) = fs::create_dir_all(&params.backup_dir) {
        let _ = fs::remove_dir_all(&params.profile_dir);
        return Err(AppError::Io(err));
    }

    // Step 4: Move originals into the backup dir. `moved_gui` is tracked so
    // that the CLI block's rollback can undo it; the CLI move has no later
    // step that could fail, so we don't bother tracking moved_cli.
    let mut moved_gui: Option<PathBuf> = None;
    if copied_gui {
        let source = params.gui_source.as_ref().unwrap();
        let dest = params.backup_dir.join("Claude");
        if let Err(err) = fs::rename(source, &dest) {
            rollback(&params, &moved_gui, &None);
            return Err(AppError::Io(err));
        }
        moved_gui = Some(dest);
    }
    if copied_cli {
        let source = params.cli_source.as_ref().unwrap();
        let dest = params.backup_dir.join(".claude");
        if let Err(err) = fs::rename(source, &dest) {
            rollback(&params, &moved_gui, &None);
            return Err(AppError::Io(err));
        }
    }

    // Step 5: Build the profile struct. (Launcher generation + profiles.json
    // update happen in the caller — keeping them out keeps this unit-testable.)
    let profile = Profile {
        id: params.id.clone(),
        name: trimmed.to_string(),
        slug,
        color: params.color.clone(),
        created_at: Utc::now().to_rfc3339(),
        surfaces: Surfaces {
            gui: params.include_gui,
            cli: params.include_cli,
        },
        last_used_at: None,
    };

    Ok(ImportOutcome {
        profile,
        backup_dir: params.backup_dir.display().to_string(),
    })
}

fn rollback(params: &ImportParams, moved_gui: &Option<PathBuf>, moved_cli: &Option<PathBuf>) {
    if let Some(backup_gui) = moved_gui {
        if let Some(source) = params.gui_source.as_ref() {
            let _ = fs::rename(backup_gui, source);
        }
    }
    if let Some(backup_cli) = moved_cli {
        if let Some(source) = params.cli_source.as_ref() {
            let _ = fs::rename(backup_cli, source);
        }
    }
    let _ = fs::remove_dir_all(&params.backup_dir);
    let _ = fs::remove_dir_all(&params.profile_dir);
}

fn copy_dir_recursive(from: &Path, to: &Path) -> AppResult<()> {
    if !from.exists() {
        return Err(AppError::NotFound(format!(
            "source does not exist: {}",
            from.display()
        )));
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &dest_path)?;
        } else if file_type.is_symlink() {
            // For symlinks, copy the link itself rather than resolving.
            let target = fs::read_link(&source_path)?;
            std::os::unix::fs::symlink(&target, &dest_path)?;
        } else {
            fs::copy(&source_path, &dest_path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationBackupInfo {
    pub path: String,
    /// Unix millisecond timestamp extracted from the dir name.
    pub created_at_ms: i64,
    /// Bytes on disk (best-effort; 0 if we couldn't stat).
    pub size_bytes: u64,
    /// True iff the dir is at least 7 days old.
    pub eligible_for_cleanup: bool,
}

const SEVEN_DAYS_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub fn list_backups(app_data_dir: &Path) -> AppResult<Vec<MigrationBackupInfo>> {
    let now_ms = Utc::now().timestamp_millis();
    let mut backups = Vec::new();
    let read = match fs::read_dir(app_data_dir) {
        Ok(read) => read,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(backups),
        Err(err) => return Err(AppError::Io(err)),
    };
    for entry in read {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(suffix) = name.strip_prefix("migration-backup-") else {
            continue;
        };
        let Ok(stamp) = suffix.parse::<i64>() else {
            continue;
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        backups.push(MigrationBackupInfo {
            path: path.display().to_string(),
            created_at_ms: stamp,
            size_bytes: directory_size(&path),
            eligible_for_cleanup: now_ms - stamp >= SEVEN_DAYS_MS,
        });
    }
    backups.sort_by_key(|backup| std::cmp::Reverse(backup.created_at_ms));
    Ok(backups)
}

pub fn delete_backup(backup_path: &Path) -> AppResult<()> {
    let name = backup_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.starts_with("migration-backup-") {
        return Err(AppError::Validation(format!(
            "{} is not a migration backup; refusing to delete",
            backup_path.display()
        )));
    }
    if !backup_path.exists() {
        return Ok(());
    }
    fs::remove_dir_all(backup_path)?;
    Ok(())
}

/// Recursive size walk. Tolerant: any I/O error (typically permission denied
/// on a system-protected subpath) is silently skipped — the caller gets a
/// best-effort sum rather than a hard failure. Symlinks are not followed
/// (their target bytes count once at the link site only).
pub fn directory_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let read = match fs::read_dir(path) {
        Ok(read) => read,
        Err(_) => return 0,
    };
    for entry in read {
        let Ok(entry) = entry else { continue };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() {
            total += metadata.len();
        } else if metadata.is_dir() {
            total += directory_size(&entry.path());
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detect_returns_none_when_neither_path_exists() {
        let scratch = tempdir().unwrap();
        let info = detect(
            &scratch.path().join("does-not-exist-1"),
            &scratch.path().join("does-not-exist-2"),
        );
        assert_eq!(info.claude_desktop_path, None);
        assert_eq!(info.claude_code_path, None);
        assert!(!info.any_detected());
    }

    #[test]
    fn detect_reports_claude_desktop_when_only_desktop_exists() {
        let scratch = tempdir().unwrap();
        let desktop = scratch.path().join("Claude");
        fs::create_dir_all(&desktop).unwrap();
        let info = detect(&desktop, &scratch.path().join(".claude-missing"));
        assert!(info.claude_desktop_path.is_some());
        assert_eq!(info.claude_code_path, None);
        assert!(info.any_detected());
    }

    #[test]
    fn detect_reports_claude_code_when_only_cli_exists() {
        let scratch = tempdir().unwrap();
        let cli = scratch.path().join(".claude");
        fs::create_dir_all(&cli).unwrap();
        let info = detect(&scratch.path().join("Claude-missing"), &cli);
        assert_eq!(info.claude_desktop_path, None);
        assert!(info.claude_code_path.is_some());
        assert!(info.any_detected());
    }

    #[test]
    fn detect_reports_both_when_both_exist() {
        let scratch = tempdir().unwrap();
        let desktop = scratch.path().join("Claude");
        let cli = scratch.path().join(".claude");
        fs::create_dir_all(&desktop).unwrap();
        fs::create_dir_all(&cli).unwrap();
        let info = detect(&desktop, &cli);
        assert!(info.claude_desktop_path.is_some());
        assert!(info.claude_code_path.is_some());
    }

    #[test]
    fn detect_leaves_sizes_none_so_boot_path_stays_fast() {
        let scratch = tempdir().unwrap();
        let desktop = scratch.path().join("Claude");
        fs::create_dir_all(&desktop).unwrap();
        fs::write(desktop.join("a.json"), b"0123456789").unwrap();
        let info = detect(&desktop, &scratch.path().join("missing"));
        assert_eq!(info.claude_desktop_size_bytes, None);
        assert_eq!(info.claude_code_size_bytes, None);
    }

    #[test]
    fn detect_sizes_walks_existing_trees() {
        let scratch = tempdir().unwrap();
        let desktop = scratch.path().join("Claude");
        fs::create_dir_all(desktop.join("nested")).unwrap();
        fs::write(desktop.join("a.json"), b"0123456789").unwrap(); // 10 bytes
        fs::write(desktop.join("nested/b.log"), b"abc").unwrap(); // 3 bytes
        let sizes = detect_sizes(&desktop, &scratch.path().join("missing"));
        assert_eq!(sizes.claude_desktop_size_bytes, Some(13));
        assert_eq!(sizes.claude_code_size_bytes, None);
    }

    #[test]
    fn directory_size_returns_zero_for_missing_paths() {
        let scratch = tempdir().unwrap();
        assert_eq!(directory_size(&scratch.path().join("does-not-exist")), 0);
    }

    #[test]
    fn directory_size_sums_nested_files() {
        let scratch = tempdir().unwrap();
        let root = scratch.path().join("root");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("x"), b"hello").unwrap();
        fs::write(root.join("a/y"), b"world!").unwrap();
        fs::write(root.join("a/b/z"), b"!!").unwrap();
        // 5 + 6 + 2 = 13
        assert_eq!(directory_size(&root), 13);
    }

    fn make_source(scratch: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let path = scratch.join(name);
        fs::create_dir_all(&path).unwrap();
        for (file_name, content) in files {
            fs::write(path.join(file_name), content).unwrap();
        }
        path
    }

    fn fixture_params(scratch: &Path, gui: Option<PathBuf>, cli: Option<PathBuf>) -> ImportParams {
        ImportParams {
            id: "11111111-1111-1111-1111-111111111111".into(),
            name: "Default".into(),
            color: "#7C3AED".into(),
            include_gui: gui.is_some(),
            include_cli: cli.is_some(),
            gui_source: gui,
            cli_source: cli,
            profile_dir: scratch.join("profile"),
            backup_dir: scratch.join("backup-12345"),
        }
    }

    #[test]
    fn import_moves_originals_to_backup_and_copies_to_profile_dir() {
        let scratch = tempdir().unwrap();
        let desktop = make_source(
            scratch.path(),
            "Claude",
            &[("a.json", "{}"), ("b.log", "x")],
        );
        let cli = make_source(scratch.path(), ".claude", &[("settings.json", "{}")]);
        let params = fixture_params(scratch.path(), Some(desktop.clone()), Some(cli.clone()));

        let outcome = import(params).unwrap();

        assert_eq!(outcome.profile.name, "Default");
        assert_eq!(outcome.profile.slug, "default");
        assert!(outcome.profile.surfaces.gui);
        assert!(outcome.profile.surfaces.cli);

        // Originals moved to backup.
        assert!(!desktop.exists());
        assert!(!cli.exists());
        assert!(scratch.path().join("backup-12345/Claude/a.json").is_file());
        assert!(scratch
            .path()
            .join("backup-12345/.claude/settings.json")
            .is_file());

        // Data copied into profile-dir.
        assert!(scratch.path().join("profile/gui-data/a.json").is_file());
        assert!(scratch.path().join("profile/gui-data/b.log").is_file());
        assert!(scratch
            .path()
            .join("profile/cli-config/settings.json")
            .is_file());
    }

    #[test]
    fn import_handles_gui_only() {
        let scratch = tempdir().unwrap();
        let desktop = make_source(scratch.path(), "Claude", &[("a.json", "{}")]);
        let params = fixture_params(scratch.path(), Some(desktop.clone()), None);

        let outcome = import(params).unwrap();
        assert!(outcome.profile.surfaces.gui);
        assert!(!outcome.profile.surfaces.cli);
        assert!(!desktop.exists());
        assert!(scratch.path().join("backup-12345/Claude/a.json").is_file());
        assert!(scratch.path().join("profile/gui-data/a.json").is_file());
        assert!(!scratch.path().join("profile/cli-config").exists());
    }

    #[test]
    fn import_rejects_empty_surfaces() {
        let scratch = tempdir().unwrap();
        let params = ImportParams {
            id: "22222222-2222-2222-2222-222222222222".into(),
            name: "Default".into(),
            color: "#7C3AED".into(),
            include_gui: false,
            include_cli: false,
            gui_source: None,
            cli_source: None,
            profile_dir: scratch.path().join("profile"),
            backup_dir: scratch.path().join("backup"),
        };
        let err = import(params).unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("at least one surface")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn import_rejects_empty_name() {
        let scratch = tempdir().unwrap();
        let desktop = make_source(scratch.path(), "Claude", &[]);
        let mut params = fixture_params(scratch.path(), Some(desktop), None);
        params.name = "   ".into();
        let err = import(params).unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("name must not be empty")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn import_rolls_back_when_a_source_is_missing() {
        let scratch = tempdir().unwrap();
        let cli = make_source(scratch.path(), ".claude", &[("settings.json", "{}")]);
        // include_gui=true but gui_source=Some(non-existent) — should fail step 2 and roll back.
        let params = ImportParams {
            id: "33333333-3333-3333-3333-333333333333".into(),
            name: "Default".into(),
            color: "#7C3AED".into(),
            include_gui: true,
            include_cli: true,
            gui_source: Some(scratch.path().join("nonexistent-Claude")),
            cli_source: Some(cli.clone()),
            profile_dir: scratch.path().join("profile"),
            backup_dir: scratch.path().join("backup"),
        };

        let err = import(params).unwrap_err();
        match err {
            AppError::NotFound(msg) => assert!(msg.contains("nonexistent-Claude")),
            other => panic!("expected NotFound, got {other:?}"),
        }

        // Originals untouched, profile-dir cleaned up.
        assert!(cli.exists());
        assert!(!scratch.path().join("profile").exists());
    }

    #[test]
    fn copy_dir_recursive_preserves_nested_structure() {
        let scratch = tempdir().unwrap();
        let source = scratch.path().join("source");
        fs::create_dir_all(source.join("nested/deep")).unwrap();
        fs::write(source.join("top.txt"), "top").unwrap();
        fs::write(source.join("nested/middle.txt"), "middle").unwrap();
        fs::write(source.join("nested/deep/bottom.txt"), "bottom").unwrap();

        let dest = scratch.path().join("dest");
        copy_dir_recursive(&source, &dest).unwrap();

        assert_eq!(fs::read_to_string(dest.join("top.txt")).unwrap(), "top");
        assert_eq!(
            fs::read_to_string(dest.join("nested/middle.txt")).unwrap(),
            "middle"
        );
        assert_eq!(
            fs::read_to_string(dest.join("nested/deep/bottom.txt")).unwrap(),
            "bottom"
        );
    }

    #[test]
    fn list_backups_returns_empty_when_app_data_missing() {
        let scratch = tempdir().unwrap();
        let backups = list_backups(&scratch.path().join("missing")).unwrap();
        assert!(backups.is_empty());
    }

    #[test]
    fn list_backups_finds_and_sorts_dirs_newest_first() {
        let scratch = tempdir().unwrap();
        let now_ms = Utc::now().timestamp_millis();
        let old = scratch.path().join("migration-backup-1000");
        let newer = scratch
            .path()
            .join(format!("migration-backup-{}", now_ms - 1000));
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&newer).unwrap();
        fs::write(old.join("a.txt"), b"hi").unwrap();
        // Add an unrelated dir to make sure it's ignored.
        fs::create_dir_all(scratch.path().join("not-a-backup")).unwrap();

        let backups = list_backups(scratch.path()).unwrap();
        assert_eq!(backups.len(), 2);
        assert_eq!(backups[0].created_at_ms, now_ms - 1000);
        assert_eq!(backups[1].created_at_ms, 1000);
        assert!(backups[1].eligible_for_cleanup); // 1970 timestamp → very eligible
        assert!(!backups[0].eligible_for_cleanup); // just now
        assert_eq!(backups[1].size_bytes, 2); // "hi"
    }

    #[test]
    fn delete_backup_refuses_paths_without_the_prefix() {
        let scratch = tempdir().unwrap();
        let foreign = scratch.path().join("not-a-backup");
        fs::create_dir_all(&foreign).unwrap();
        let err = delete_backup(&foreign).unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("refusing to delete")),
            other => panic!("expected Validation, got {other:?}"),
        }
        assert!(foreign.exists());
    }

    #[test]
    fn delete_backup_is_idempotent_for_missing_paths() {
        let scratch = tempdir().unwrap();
        let target = scratch.path().join("migration-backup-9999");
        // Doesn't exist yet — should still return Ok.
        delete_backup(&target).unwrap();
    }

    #[test]
    fn delete_backup_removes_a_real_backup_dir() {
        let scratch = tempdir().unwrap();
        let target = scratch.path().join("migration-backup-9999");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("a.txt"), b"data").unwrap();
        delete_backup(&target).unwrap();
        assert!(!target.exists());
    }
}
