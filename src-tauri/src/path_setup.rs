//! Detect the user's shell and (optionally) add `~/.local/bin` to their PATH
//! by editing the appropriate shell rc file. Backs up the rc file first;
//! refuses to add a duplicate line if our marker comment is already present.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const MARKER: &str = "# Added by claude-profiles";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    #[allow(dead_code)]
    pub fn detect_from_env() -> Self {
        let shell = std::env::var("SHELL").unwrap_or_default();
        Self::from_path(&shell)
    }

    pub fn from_path(path: &str) -> Self {
        if path.ends_with("/fish") || path.ends_with("fish") {
            Shell::Fish
        } else if path.ends_with("/bash") || path.ends_with("bash") {
            Shell::Bash
        } else {
            Shell::Zsh
        }
    }

    pub fn rc_path(self, home: &Path) -> PathBuf {
        match self {
            Shell::Zsh => home.join(".zshrc"),
            Shell::Bash => home.join(".bashrc"),
            Shell::Fish => home.join(".config").join("fish").join("config.fish"),
        }
    }

    pub fn hook_line(self) -> &'static str {
        match self {
            Shell::Zsh | Shell::Bash => "export PATH=\"$HOME/.local/bin:$PATH\"",
            Shell::Fish => "set -gx PATH $HOME/.local/bin $PATH",
        }
    }

    #[allow(dead_code)]
    pub fn rc_display_name(self) -> &'static str {
        match self {
            Shell::Zsh => "~/.zshrc",
            Shell::Bash => "~/.bashrc",
            Shell::Fish => "~/.config/fish/config.fish",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum PathHookOutcome {
    #[serde(rename_all = "camelCase")]
    AlreadyInstalled { rc_path: String },
    #[serde(rename_all = "camelCase")]
    Installed {
        rc_path: String,
        backup_path: String,
    },
}

pub fn install_path_hook(shell: Shell, home: &Path) -> AppResult<PathHookOutcome> {
    let rc_path = shell.rc_path(home);
    let existing = match fs::read_to_string(&rc_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(AppError::Io(err)),
    };

    if existing.contains(MARKER) {
        return Ok(PathHookOutcome::AlreadyInstalled {
            rc_path: rc_path.display().to_string(),
        });
    }

    let backup_path = if existing.is_empty() {
        rc_path.with_file_name(format!(
            "{}.claude-profiles-backup-{}",
            rc_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "rc".to_string()),
            Utc::now().timestamp_millis()
        ))
    } else {
        let backup = rc_path.with_file_name(format!(
            "{}.claude-profiles-backup-{}",
            rc_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "rc".to_string()),
            Utc::now().timestamp_millis()
        ));
        fs::write(&backup, &existing)?;
        backup
    };

    let mut new_contents = existing;
    if !new_contents.is_empty() && !new_contents.ends_with('\n') {
        new_contents.push('\n');
    }
    new_contents.push('\n');
    new_contents.push_str(MARKER);
    new_contents.push('\n');
    new_contents.push_str(shell.hook_line());
    new_contents.push('\n');

    if let Some(parent) = rc_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&rc_path, new_contents)?;

    Ok(PathHookOutcome::Installed {
        rc_path: rc_path.display().to_string(),
        backup_path: backup_path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detect_zsh_from_path() {
        assert_eq!(Shell::from_path("/bin/zsh"), Shell::Zsh);
        assert_eq!(Shell::from_path("/usr/bin/zsh"), Shell::Zsh);
    }

    #[test]
    fn detect_bash_from_path() {
        assert_eq!(Shell::from_path("/bin/bash"), Shell::Bash);
        assert_eq!(Shell::from_path("/opt/homebrew/bin/bash"), Shell::Bash);
    }

    #[test]
    fn detect_fish_from_path() {
        assert_eq!(Shell::from_path("/opt/homebrew/bin/fish"), Shell::Fish);
        assert_eq!(Shell::from_path("/usr/local/bin/fish"), Shell::Fish);
    }

    #[test]
    fn detect_defaults_to_zsh_for_unknown() {
        assert_eq!(Shell::from_path(""), Shell::Zsh);
        assert_eq!(Shell::from_path("/bin/sh"), Shell::Zsh);
    }

    #[test]
    fn zsh_rc_path_is_home_dot_zshrc() {
        let home = PathBuf::from("/Users/test");
        assert_eq!(
            Shell::Zsh.rc_path(&home),
            PathBuf::from("/Users/test/.zshrc")
        );
    }

    #[test]
    fn fish_rc_path_lives_under_config() {
        let home = PathBuf::from("/Users/test");
        assert_eq!(
            Shell::Fish.rc_path(&home),
            PathBuf::from("/Users/test/.config/fish/config.fish")
        );
    }

    #[test]
    fn install_creates_rc_file_if_missing() {
        let home = tempdir().unwrap();
        let outcome = install_path_hook(Shell::Zsh, home.path()).unwrap();
        match outcome {
            PathHookOutcome::Installed { rc_path, .. } => {
                let body = fs::read_to_string(&rc_path).unwrap();
                assert!(body.contains(MARKER));
                assert!(body.contains("export PATH"));
            }
            other => panic!("expected Installed, got {other:?}"),
        }
    }

    #[test]
    fn install_backs_up_existing_rc_before_appending() {
        let home = tempdir().unwrap();
        let rc = home.path().join(".zshrc");
        fs::write(&rc, "# user content\nalias ll='ls -la'\n").unwrap();

        let outcome = install_path_hook(Shell::Zsh, home.path()).unwrap();
        match outcome {
            PathHookOutcome::Installed { backup_path, .. } => {
                let backup_body = fs::read_to_string(&backup_path).unwrap();
                assert_eq!(backup_body, "# user content\nalias ll='ls -la'\n");
            }
            other => panic!("expected Installed, got {other:?}"),
        }
        let new_body = fs::read_to_string(&rc).unwrap();
        assert!(new_body.starts_with("# user content"));
        assert!(new_body.contains(MARKER));
        assert!(new_body.contains("export PATH"));
    }

    #[test]
    fn install_is_idempotent_when_marker_present() {
        let home = tempdir().unwrap();
        install_path_hook(Shell::Zsh, home.path()).unwrap();
        let outcome = install_path_hook(Shell::Zsh, home.path()).unwrap();
        match outcome {
            PathHookOutcome::AlreadyInstalled { .. } => {}
            other => panic!("expected AlreadyInstalled, got {other:?}"),
        }
    }

    #[test]
    fn install_for_fish_creates_config_dir() {
        let home = tempdir().unwrap();
        let outcome = install_path_hook(Shell::Fish, home.path()).unwrap();
        match outcome {
            PathHookOutcome::Installed { rc_path, .. } => {
                let body = fs::read_to_string(&rc_path).unwrap();
                assert!(body.contains("set -gx PATH"));
            }
            other => panic!("expected Installed, got {other:?}"),
        }
        assert!(home.path().join(".config/fish").is_dir());
    }
}
