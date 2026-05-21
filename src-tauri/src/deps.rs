//! Detect external dependencies the app relies on: Claude Desktop, the
//! Claude Code CLI, and whether `~/.local/bin` is on the user's interactive
//! shell PATH.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Dependencies {
    pub claude_app_installed: bool,
    pub claude_cli_installed: bool,
    pub local_bin_on_path: bool,
}

pub fn check_dependencies() -> AppResult<Dependencies> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let shell_path = get_shell_path().unwrap_or_default();
    Ok(Dependencies {
        claude_app_installed: claude_app_exists(),
        claude_cli_installed: find_claude_in_path(&shell_path, &home),
        local_bin_on_path: is_local_bin_in_path(&shell_path, &home),
    })
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

pub fn find_claude_in_path(path_string: &str, home: &Path) -> bool {
    for segment in path_string.split(':') {
        let candidate = PathBuf::from(segment.trim()).join("claude");
        if candidate.is_file() {
            return true;
        }
    }
    home.join(".local").join("bin").join("claude").is_file()
}

pub fn claude_app_exists() -> bool {
    PathBuf::from("/Applications/Claude.app").is_dir()
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
    fn find_claude_finds_binary_via_path() {
        let home = tempdir().unwrap();
        let bin = home.path().join("custom-bin");
        std::fs::create_dir_all(&bin).unwrap();
        let claude = bin.join("claude");
        std::fs::write(&claude, "#!/bin/bash\n").unwrap();
        let path = format!("{}:{}", bin.display(), "/usr/bin");
        assert!(find_claude_in_path(&path, home.path()));
    }

    #[test]
    fn find_claude_falls_back_to_local_bin_even_when_not_on_path() {
        let home = tempdir().unwrap();
        let local_bin = home.path().join(".local").join("bin");
        std::fs::create_dir_all(&local_bin).unwrap();
        std::fs::write(local_bin.join("claude"), "#!/bin/bash\n").unwrap();
        assert!(find_claude_in_path("/usr/bin:/bin", home.path()));
    }

    #[test]
    fn find_claude_returns_false_when_truly_missing() {
        let home = tempdir().unwrap();
        assert!(!find_claude_in_path("/usr/bin:/bin", home.path()));
    }
}
