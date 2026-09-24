//! `install-service`: run the server under systemd as the user.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const UNIT_NAME: &str = "ai-profiles-server.service";

/// The unit file. `PATH` is the one `install-service` ran with, because a
/// user service otherwise gets systemd's bare PATH, which lacks the places
/// `claude` and `tmux` are usually installed.
///
/// `KillMode=process` stops only the server on restart: a tmux server it
/// started (and every Claude session in it) is a child in its cgroup, and
/// would otherwise be killed too. No `PrivateTmp`, because tmux keeps its
/// socket in /tmp and the server has to reach the user's.
pub fn unit_text(executable: &Path, path_env: &str) -> String {
    format!(
        "[Unit]\n\
         Description=ai-profiles server: Claude sessions for ai-profiles on this machine\n\
         After=network-online.target\n\
         \n\
         [Service]\n\
         ExecStart={} serve\n\
         Environment=PATH={path_env}\n\
         Restart=on-failure\n\
         RestartSec=3\n\
         KillMode=process\n\
         NoNewPrivileges=yes\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        executable.display()
    )
}

pub fn unit_path(home: &Path) -> PathBuf {
    home.join(".config/systemd/user").join(UNIT_NAME)
}

/// Write the unit, reload systemd, and (unless told not to) start it now and
/// at every login.
pub fn install(home: &Path, start: bool) -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let path_env = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let unit = unit_path(home);
    fs::create_dir_all(unit.parent().expect("a unit path has a folder"))?;
    fs::write(&unit, unit_text(&executable, &path_env))?;
    systemctl(&["daemon-reload"])?;
    if start {
        systemctl(&["enable", "--now", UNIT_NAME])?;
        // Picks up a rebuilt binary when the service was already running.
        systemctl(&["restart", UNIT_NAME])?;
    }
    Ok(unit)
}

fn systemctl(args: &[&str]) -> io::Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "systemctl --user {} failed ({status})",
            args.join(" ")
        )))
    }
}

/// Whether the user's services keep running after they log out.
pub fn lingering() -> Option<bool> {
    let user = std::env::var("USER").ok()?;
    let output = Command::new("loginctl")
        .args(["show-user", &user, "-p", "Linger", "--value"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim() == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_keeps_sessions_alive_across_restarts() {
        let text = unit_text(
            Path::new("/home/m/bin/ai-profiles-server"),
            "/home/m/.local/bin:/usr/bin",
        );
        assert!(text.contains("ExecStart=/home/m/bin/ai-profiles-server serve\n"));
        assert!(text.contains("Environment=PATH=/home/m/.local/bin:/usr/bin\n"));
        assert!(text.contains("KillMode=process\n"));
        assert!(!text.contains("PrivateTmp"));
        assert!(text.contains("WantedBy=default.target"));
    }
}
