//! Detect external dependencies the app relies on: Claude Desktop, the
//! Claude Code CLI, Codex Desktop, the Codex CLI, and whether
//! `~/.local/bin` is on the user's interactive shell PATH.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::app_kind::{spec, AppKind, AppSpec};
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppDependency {
    pub gui_installed: bool,
    pub cli_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Dependencies {
    /// Per-app install status, keyed by the lowercase app token.
    pub apps: HashMap<String, AppDependency>,
    pub local_bin_on_path: bool,
}

pub fn check_dependencies() -> AppResult<Dependencies> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let shell_path = cached_shell_path(&home);
    let mut apps = HashMap::new();
    for kind in [AppKind::Claude, AppKind::Codex] {
        let app_spec = spec(kind);
        apps.insert(
            kind.as_str().to_string(),
            AppDependency {
                gui_installed: gui_app_exists(app_spec),
                cli_installed: find_cli_in_path(app_spec.cli_binary, &shell_path, &home),
            },
        );
    }
    Ok(Dependencies {
        apps,
        local_bin_on_path: is_local_bin_in_path(&shell_path, &home),
    })
}

/// Process-lifetime cache for the resolved shell `PATH`. Spawning the
/// interactive login shell to read `$PATH` costs 0.5–2 seconds on most
/// macOS setups (NVM, brew, etc. all source on `-l`), so we only do it
/// once per app launch.
static SHELL_PATH_CACHE: OnceLock<String> = OnceLock::new();

fn cached_shell_path(home: &Path) -> String {
    SHELL_PATH_CACHE
        .get_or_init(|| resolve_shell_path(home))
        .clone()
}

/// Resolve the shell `PATH`. Fast path: if the process-inherited `PATH`
/// already contains `~/.local/bin`, use that — Tauri tends to inherit a
/// useful PATH on macOS unless the user launched the app from Finder
/// before configuring their shell. Slow path: spawn the interactive
/// login shell.
fn resolve_shell_path(home: &Path) -> String {
    if let Ok(process_path) = std::env::var("PATH") {
        if is_local_bin_in_path(&process_path, home) {
            return process_path;
        }
    }
    get_shell_path().unwrap_or_default()
}

pub fn is_local_bin_in_path(path_string: &str, home: &Path) -> bool {
    let target_owned = home.join(".local").join("bin");
    let target = target_owned.to_string_lossy();
    let target_tilde = "~/.local/bin";
    path_string
        .split(':')
        .map(str::trim)
        .any(|segment| segment == target || segment == target_tilde)
}

pub fn find_cli_in_path(binary: &str, path_string: &str, home: &Path) -> bool {
    find_cli_path_in(binary, path_string, home).is_some()
}

/// Pure: resolve the absolute path of `binary` — first existing
/// `<segment>/<binary>` across the PATH segments, then `~/.local/bin`.
fn find_cli_path_in(binary: &str, path_string: &str, home: &Path) -> Option<PathBuf> {
    for segment in path_string.split(':') {
        let candidate = PathBuf::from(segment.trim()).join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let local = home.join(".local").join("bin").join(binary);
    if local.is_file() {
        return Some(local);
    }
    None
}

/// Resolve the absolute path to a managed CLI `binary` on the user's
/// interactive shell PATH (falling back to `~/.local/bin`). Returns `None`
/// when it isn't found. Needed because a Tauri app launched from Finder does
/// not inherit the shell PATH, so spawning a bare `codex`/`claude` fails — we
/// must locate and exec the absolute path instead.
pub fn resolve_cli_binary_path(binary: &str) -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let shell_path = cached_shell_path(&home);
    find_cli_path_in(binary, &shell_path, &home)
}

/// The resolved interactive shell PATH (cached). Exposed so a spawned child
/// process (e.g. `codex app-server`) inherits a usable PATH even when the app
/// was launched from Finder.
pub fn shell_path() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    cached_shell_path(&home)
}

pub fn gui_app_exists(spec: &AppSpec) -> bool {
    crate::paths::gui_app_bundle(spec).is_dir()
}

fn get_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let output = Command::new(&shell)
        .args(["-lic", "echo $PATH"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn local_bin_detected_when_present() {
        let home = PathBuf::from("/Users/test");
        let path = "/usr/bin:/Users/test/.local/bin:/bin";
        assert!(is_local_bin_in_path(path, &home));
    }

    #[test]
    fn local_bin_detected_via_tilde() {
        let home = PathBuf::from("/Users/test");
        let path = "/usr/bin:~/.local/bin:/bin";
        assert!(is_local_bin_in_path(path, &home));
    }

    #[test]
    fn local_bin_not_detected_when_absent() {
        let home = PathBuf::from("/Users/test");
        let path = "/usr/bin:/bin:/usr/sbin";
        assert!(!is_local_bin_in_path(path, &home));
    }

    #[test]
    fn local_bin_not_fooled_by_partial_match() {
        let home = PathBuf::from("/Users/test");
        let path = "/local/bin";
        assert!(!is_local_bin_in_path(path, &home));
    }

    #[test]
    fn find_cli_finds_named_binary_via_path() {
        let home = tempdir().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("codex"), "#!/bin/bash\n").unwrap();
        let path = format!("{}:/usr/bin", bin.display());
        assert!(find_cli_in_path("codex", &path, home.path()));
        assert!(!find_cli_in_path("claude", &path, home.path()));
    }

    #[test]
    fn find_cli_falls_back_to_local_bin_even_when_not_on_path() {
        let home = tempdir().unwrap();
        let local_bin = home.path().join(".local").join("bin");
        std::fs::create_dir_all(&local_bin).unwrap();
        std::fs::write(local_bin.join("claude"), "#!/bin/bash\n").unwrap();
        assert!(find_cli_in_path("claude", "/usr/bin:/bin", home.path()));
    }

    #[test]
    fn find_cli_returns_false_when_truly_missing() {
        let home = tempdir().unwrap();
        assert!(!find_cli_in_path("claude", "/usr/bin:/bin", home.path()));
    }

    #[test]
    fn find_cli_path_in_returns_the_resolved_absolute_path() {
        let home = tempdir().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("codex"), "#!/bin/bash\n").unwrap();
        let path = format!("/nonexistent:{}:/usr/bin", bin.display());
        assert_eq!(
            find_cli_path_in("codex", &path, home.path()),
            Some(bin.join("codex"))
        );
        assert!(find_cli_path_in("claude", &path, home.path()).is_none());
    }

    #[test]
    fn find_cli_path_in_falls_back_to_local_bin() {
        let home = tempdir().unwrap();
        let local_bin = home.path().join(".local").join("bin");
        std::fs::create_dir_all(&local_bin).unwrap();
        std::fs::write(local_bin.join("codex"), "#!/bin/bash\n").unwrap();
        assert_eq!(
            find_cli_path_in("codex", "/usr/bin:/bin", home.path()),
            Some(local_bin.join("codex"))
        );
    }

    #[test]
    fn gui_app_exists_checks_the_spec_bundle() {
        // Smoke: function takes a spec and probes /Applications/<bundle>.
        let _ = gui_app_exists(&crate::app_kind::CODEX);
    }
}
