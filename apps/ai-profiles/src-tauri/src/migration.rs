//! First-run migration: detect an existing Desktop / CLI installation
//! (Claude or ChatGPT) and import it as a named profile.

use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::app_kind::{spec, AppKind};
use crate::error::{AppError, AppResult};
use crate::profiles::{Profile, Surfaces};
use crate::slug::slugify;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExistingInstall {
    pub gui_path: Option<String>,
    pub cli_path: Option<String>,
    /// Bytes occupied by the desktop install dir (best-effort, walks the tree).
    /// `None` when the corresponding path doesn't exist; permission-denied
    /// subpaths during the walk are silently skipped.
    pub gui_size_bytes: Option<u64>,
    pub cli_size_bytes: Option<u64>,
}

impl ExistingInstall {
    #[allow(dead_code)]
    pub fn any_detected(&self) -> bool {
        self.gui_path.is_some() || self.cli_path.is_some()
    }
}

/// Pure: check the two well-known paths and report which exist. Sizes
/// are deliberately left `None` — this is the boot-critical-path entry
/// and walking the trees synchronously can take a second or more on a
/// large `~/.claude`. Use [`detect_sizes`] from a lazy IPC once the
/// MigrationDialog opens.
pub fn detect(gui_path: &Path, cli_path: &Path) -> ExistingInstall {
    let desktop_exists = gui_path.exists();
    let cli_exists = cli_path.exists();
    ExistingInstall {
        gui_path: desktop_exists.then(|| gui_path.display().to_string()),
        cli_path: cli_exists.then(|| cli_path.display().to_string()),
        gui_size_bytes: None,
        cli_size_bytes: None,
    }
}

/// Thin wrapper that resolves stock paths for a given app kind and calls
/// the pure [`detect`]. Keeps command handlers free of path resolution.
pub fn detect_for(kind: AppKind) -> AppResult<ExistingInstall> {
    let app = spec(kind);
    let desktop = crate::paths::stock_gui_support_dir(app)?;
    let code = crate::paths::stock_cli_config_dir(app)?;
    Ok(detect(&desktop, &code))
}

/// Sizes-only side-table for [`detect`]. Walks each tree once; returns
/// `None` for paths that don't exist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExistingInstallSizes {
    pub gui_size_bytes: Option<u64>,
    pub cli_size_bytes: Option<u64>,
}

pub fn detect_sizes(gui_path: &Path, cli_path: &Path) -> ExistingInstallSizes {
    ExistingInstallSizes {
        gui_size_bytes: gui_path.exists().then(|| directory_size(gui_path)),
        cli_size_bytes: cli_path.exists().then(|| directory_size(cli_path)),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportParams {
    /// UUID for the new profile. The caller pre-generates this so it can
    /// also pre-compute `profile_dir` consistently.
    pub id: String,
    /// Which managed app this import belongs to.
    pub app: AppKind,
    pub name: String,
    pub color: String,
    pub include_gui: bool,
    pub include_cli: bool,
    /// Absolute path of the existing Desktop dir (None if not detected
    /// or the user unchecked the GUI surface).
    pub gui_source: Option<PathBuf>,
    /// Absolute path of the existing CLI config dir.
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
    crate::profiles::validate_name(trimmed)?;
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
    let app_spec = spec(params.app);
    let mut moved_gui: Option<PathBuf> = None;
    if copied_gui {
        let source = params.gui_source.as_ref().unwrap();
        let dest = params.backup_dir.join(app_spec.gui_support_dir_name);
        if let Err(err) = fs::rename(source, &dest) {
            rollback(&params, &moved_gui, &None);
            return Err(AppError::Io(err));
        }
        moved_gui = Some(dest);
    }
    if copied_cli {
        let source = params.cli_source.as_ref().unwrap();
        let dest = params.backup_dir.join(app_spec.cli_stock_config_dir_name);
        if let Err(err) = fs::rename(source, &dest) {
            rollback(&params, &moved_gui, &None);
            return Err(AppError::Io(err));
        }
    }

    // Step 5: Build the profile struct. (Launcher generation + profiles.json
    // update happen in the caller — keeping them out keeps this unit-testable.)
    let profile = Profile {
        id: params.id.clone(),
        app: params.app,
        name: trimmed.to_string(),
        slug,
        color: params.color.clone(),
        created_at: Utc::now().to_rfc3339(),
        surfaces: Surfaces {
            gui: params.include_gui,
            cli: params.include_cli,
        },
        distinct_dock_icon: false,
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
    let root = normalize_lexically(from);
    copy_tree(from, to, &root)
}

/// `root` is the top of the tree being copied, carried through the recursion so
/// each symlink can be judged against it — see [`rebase_link_target`].
fn copy_tree(from: &Path, to: &Path, root: &Path) -> AppResult<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = to.join(entry.file_name());
        // `file_type` comes from the directory entry, so a symlink to a
        // directory reports `is_symlink`, not `is_dir` — links are never
        // descended into.
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_tree(&source_path, &dest_path, root)?;
        } else if file_type.is_symlink() {
            // For symlinks, copy the link itself rather than resolving.
            let target = fs::read_link(&source_path)?;
            std::os::unix::fs::symlink(
                rebase_link_target(&source_path, &target, root),
                &dest_path,
            )?;
        } else {
            fs::copy(&source_path, &dest_path)?;
        }
    }
    Ok(())
}

/// Decide what a copied symlink should point at.
///
/// A relative target only means anything next to the link it came from, so the
/// question is what it resolved to *before* the copy, and whether that lands
/// inside the tree being copied.
///
/// - **Outside the tree** — pin it absolute at what it resolved to. This is the
///   shape skill managers install (`~/.claude/skills/foo ->
///   ../../.agents/skills/foo`); copied verbatim it would become
///   `<profile>/cli-config/skills/foo -> <profile>/.agents/skills/foo`, so
///   importing a real `~/.claude` used to yield a profile full of dangling
///   skills.
/// - **Inside the tree** — re-express it relative to the copied link, so it
///   follows the *copy*. Keeping the original text is not enough: a target like
///   `../../source/file` climbs out of the tree and back in by name, which still
///   resolves after the copy but points at the original — which migration then
///   moves into the backup dir, leaving the profile dangling anyway.
///
/// Absolute targets already survive the move and are passed through untouched.
///
/// Resolution is lexical, so a target routed through a symlinked *directory*
/// inside the tree is judged by its spelling rather than where it truly lands.
/// Following it would mean resolving links in a tree we are mid-copy of, for a
/// case that does not arise in the configs this imports.
fn rebase_link_target(link: &Path, target: &Path, root: &Path) -> PathBuf {
    if target.is_absolute() {
        return target.to_path_buf();
    }
    let Some(parent) = link.parent() else {
        return target.to_path_buf();
    };
    let resolved = normalize_lexically(&parent.join(target));
    if !resolved.starts_with(root) {
        return resolved;
    }
    let relative = relative_path_between(&normalize_lexically(parent), &resolved);
    if relative.as_os_str().is_empty() {
        return PathBuf::from(".");
    }
    relative
}

/// Express `to` as a path relative to `from`.
///
/// Both sides are normalized and share `root` as a prefix, so the walk-up never
/// climbs past it and the result stays inside the tree.
fn relative_path_between(from: &Path, to: &Path) -> PathBuf {
    let mut from_components = from.components().peekable();
    let mut to_components = to.components().peekable();
    while from_components.peek().is_some() && from_components.peek() == to_components.peek() {
        from_components.next();
        to_components.next();
    }

    let mut relative = PathBuf::new();
    for _ in from_components {
        relative.push("..");
    }
    for component in to_components {
        relative.push(component.as_os_str());
    }
    relative
}

/// Resolve `.` and `..` purely lexically.
///
/// `canonicalize` is unusable here: link targets routinely name paths that do
/// not exist, and it would also collapse intermediate symlinks the user put
/// there deliberately. Lexical resolution is also what makes the containment
/// test meaningful — `Path::starts_with` compares components, so an unresolved
/// `.claude/skills/../../.agents` would look like it sits under `.claude`.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut kept: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match kept.last() {
                // Only a real directory name can be cancelled out. Popping a
                // `..` instead would silently collapse `../../x` into `x`.
                Some(Component::Normal(_)) => {
                    kept.pop();
                }
                // `/..` is `/`, so an absolute path never climbs past the root.
                Some(Component::RootDir) => {}
                // A leading `..` on a relative path has nothing to cancel.
                _ => kept.push(component),
            },
            other => kept.push(other),
        }
    }
    kept.iter().collect()
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
    delete_backup_under(&crate::paths::app_data_dir()?, backup_path)
}

/// Delete `backup_path`, but only if it is a `migration-backup-*` directory
/// sitting directly inside `app_data`. The path comes from the webview, so a
/// matching file name alone isn't enough.
fn delete_backup_under(app_data: &Path, backup_path: &Path) -> AppResult<()> {
    if backup_path.parent() != Some(app_data) {
        return Err(AppError::Validation(format!(
            "{} is not inside {}; refusing to delete",
            backup_path.display(),
            app_data.display()
        )));
    }
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
        assert_eq!(info.gui_path, None);
        assert_eq!(info.cli_path, None);
        assert!(!info.any_detected());
    }

    #[test]
    fn detect_reports_gui_when_only_desktop_exists() {
        let scratch = tempdir().unwrap();
        let desktop = scratch.path().join("Claude");
        fs::create_dir_all(&desktop).unwrap();
        let info = detect(&desktop, &scratch.path().join(".claude-missing"));
        assert!(info.gui_path.is_some());
        assert_eq!(info.cli_path, None);
        assert!(info.any_detected());
    }

    #[test]
    fn detect_reports_cli_when_only_cli_exists() {
        let scratch = tempdir().unwrap();
        let cli = scratch.path().join(".claude");
        fs::create_dir_all(&cli).unwrap();
        let info = detect(&scratch.path().join("Claude-missing"), &cli);
        assert_eq!(info.gui_path, None);
        assert!(info.cli_path.is_some());
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
        assert!(info.gui_path.is_some());
        assert!(info.cli_path.is_some());
    }

    #[test]
    fn detect_leaves_sizes_none_so_boot_path_stays_fast() {
        let scratch = tempdir().unwrap();
        let desktop = scratch.path().join("Claude");
        fs::create_dir_all(&desktop).unwrap();
        fs::write(desktop.join("a.json"), b"0123456789").unwrap();
        let info = detect(&desktop, &scratch.path().join("missing"));
        assert_eq!(info.gui_size_bytes, None);
        assert_eq!(info.cli_size_bytes, None);
    }

    #[test]
    fn detect_sizes_walks_existing_trees() {
        let scratch = tempdir().unwrap();
        let desktop = scratch.path().join("Claude");
        fs::create_dir_all(desktop.join("nested")).unwrap();
        fs::write(desktop.join("a.json"), b"0123456789").unwrap(); // 10 bytes
        fs::write(desktop.join("nested/b.log"), b"abc").unwrap(); // 3 bytes
        let sizes = detect_sizes(&desktop, &scratch.path().join("missing"));
        assert_eq!(sizes.gui_size_bytes, Some(13));
        assert_eq!(sizes.cli_size_bytes, None);
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
            app: crate::app_kind::AppKind::Claude,
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

        // Originals moved to backup (using Claude spec dir names).
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
        assert!(scratch.path().join("backup-12345/Claude/a.json").is_file()); // Claude spec gui_support_dir_name
        assert!(scratch.path().join("profile/gui-data/a.json").is_file());
        assert!(!scratch.path().join("profile/cli-config").exists());
    }

    #[test]
    fn import_rejects_empty_surfaces() {
        let scratch = tempdir().unwrap();
        let params = ImportParams {
            id: "22222222-2222-2222-2222-222222222222".into(),
            app: crate::app_kind::AppKind::Claude,
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
            app: crate::app_kind::AppKind::Claude,
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

    /// The shape skill managers actually install: `~/.claude/skills/<name>` is a
    /// relative link that climbs out of `~/.claude` into a sibling store. Copied
    /// verbatim it would resolve to `<profile>/.agents/...` and dangle.
    #[test]
    fn copy_rewrites_relative_links_that_escape_the_copied_tree() {
        let scratch = tempdir().unwrap();
        let home = scratch.path().join("home");
        let store = home.join(".agents/skills/grilling");
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("SKILL.md"), "# grilling").unwrap();
        let claude = home.join(".claude");
        fs::create_dir_all(claude.join("skills")).unwrap();
        std::os::unix::fs::symlink(
            "../../.agents/skills/grilling",
            claude.join("skills/grilling"),
        )
        .unwrap();

        let dest = scratch.path().join("cli-config");
        copy_dir_recursive(&claude, &dest).unwrap();

        let copied = dest.join("skills/grilling");
        assert!(
            fs::symlink_metadata(&copied).unwrap().is_symlink(),
            "must stay a link, not become a copy"
        );
        assert_eq!(
            fs::read_to_string(copied.join("SKILL.md")).unwrap(),
            "# grilling",
            "imported skill must still resolve"
        );
        assert_eq!(fs::read_link(&copied).unwrap(), store);
    }

    #[test]
    fn copy_keeps_relative_links_that_stay_inside_the_copied_tree() {
        let scratch = tempdir().unwrap();
        let source = scratch.path().join("source");
        fs::create_dir_all(source.join("skills")).unwrap();
        fs::write(source.join("skills/real.md"), "real").unwrap();
        std::os::unix::fs::symlink("real.md", source.join("skills/alias.md")).unwrap();

        let dest = scratch.path().join("dest");
        copy_dir_recursive(&source, &dest).unwrap();

        let copied = dest.join("skills/alias.md");
        assert_eq!(
            fs::read_link(&copied).unwrap(),
            PathBuf::from("real.md"),
            "internal links must stay relative so the copy is self-contained"
        );
        assert_eq!(fs::read_to_string(&copied).unwrap(), "real");
    }

    #[test]
    fn copy_preserves_absolute_link_targets() {
        let scratch = tempdir().unwrap();
        let outside = scratch.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("rule.md"), "rule").unwrap();
        let source = scratch.path().join("source");
        fs::create_dir_all(source.join("rules")).unwrap();
        std::os::unix::fs::symlink(outside.join("rule.md"), source.join("rules/rule.md")).unwrap();

        let dest = scratch.path().join("dest");
        copy_dir_recursive(&source, &dest).unwrap();

        let copied = dest.join("rules/rule.md");
        assert_eq!(fs::read_link(&copied).unwrap(), outside.join("rule.md"));
        assert_eq!(fs::read_to_string(&copied).unwrap(), "rule");
    }

    /// A link that climbs out of the tree and back in by name still resolves if
    /// copied verbatim — but only against the *original*, which import then
    /// moves into the backup dir. It has to be re-pointed at the copy.
    #[test]
    fn copy_repoints_links_that_leave_and_re_enter_at_the_copy() {
        let scratch = tempdir().unwrap();
        let source = scratch.path().join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("target.md"), "target").unwrap();
        std::os::unix::fs::symlink("../../source/target.md", source.join("nested/link.md"))
            .unwrap();

        let dest = scratch.path().join("dest");
        copy_dir_recursive(&source, &dest).unwrap();
        // Import moves the original aside once copied; the copy must stand alone.
        fs::rename(&source, scratch.path().join("backup")).unwrap();

        let copied = dest.join("nested/link.md");
        assert_eq!(
            fs::read_to_string(&copied).unwrap(),
            "target",
            "link must follow the copy, not the original"
        );
    }

    #[test]
    fn relative_path_between_walks_up_then_down() {
        assert_eq!(
            relative_path_between(Path::new("/a/b/nested"), Path::new("/a/b/target.md")),
            PathBuf::from("../target.md")
        );
        assert_eq!(
            relative_path_between(Path::new("/a/b"), Path::new("/a/b/c/d.md")),
            PathBuf::from("c/d.md")
        );
    }

    #[test]
    fn normalize_lexically_resolves_traversal_without_touching_disk() {
        assert_eq!(
            normalize_lexically(Path::new("/home/u/.claude/skills/../../.agents/skills/x")),
            PathBuf::from("/home/u/.agents/skills/x")
        );
        assert_eq!(
            normalize_lexically(Path::new("./a/./b/../c")),
            PathBuf::from("a/c")
        );
        // A relative path may legitimately climb above its own start.
        assert_eq!(
            normalize_lexically(Path::new("../../x")),
            PathBuf::from("../../x")
        );
        // `/..` is `/`, so an absolute path can never climb past the root.
        assert_eq!(normalize_lexically(Path::new("/../x")), PathBuf::from("/x"));
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
        let err = delete_backup_under(scratch.path(), &foreign).unwrap_err();
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
        delete_backup_under(scratch.path(), &target).unwrap();
    }

    #[test]
    fn delete_backup_removes_a_real_backup_dir() {
        let scratch = tempdir().unwrap();
        let target = scratch.path().join("migration-backup-9999");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("a.txt"), b"data").unwrap();
        delete_backup_under(scratch.path(), &target).unwrap();
        assert!(!target.exists());
    }

    #[test]
    fn delete_backup_refuses_backups_outside_app_data() {
        let app_data = tempdir().unwrap();
        let elsewhere = tempdir().unwrap();
        let target = elsewhere.path().join("migration-backup-9999");
        fs::create_dir_all(&target).unwrap();
        let err = delete_backup_under(app_data.path(), &target).unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("refusing to delete")),
            other => panic!("expected Validation, got {other:?}"),
        }
        assert!(target.exists());
    }
}
