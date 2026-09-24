//! Is a registry entry's process still that Claude?
//!
//! A `claude` that dies leaves its `sessions/<pid>.json` behind, and the pid
//! may since belong to something else. On Linux the entry's `procStart` is
//! the process's start time as `/proc/<pid>/stat` counts it, and its
//! `pidDomain` names the pid namespace, so both can be checked exactly, the
//! way claudemulti does. Elsewhere (a Mac running the tests) `ps` has to do.

use std::collections::HashMap;

use ai_profiles_core::registry::RegistryEntry;

use crate::accounts::AccountDir;

pub trait ProcessTable: Send + Sync {
    fn is_live_claude(&self, entry: &RegistryEntry) -> bool;

    /// `account`'s live sessions, session id → pid, when something can say
    /// for the whole account at once (Claude itself: see
    /// [`crate::agents`]). `None` has each registry entry checked on its own
    /// with [`ProcessTable::is_live_claude`].
    fn live_sessions(&self, _account: &AccountDir) -> Option<HashMap<String, i32>> {
        None
    }

    /// Something was just started in `account`: an answer kept from before
    /// no longer holds.
    fn changed(&self, _account: &AccountDir) {}

    /// Ask `entry`'s process to end (SIGTERM), or make it (SIGKILL) when
    /// `force`. Only while it is still that Claude, so a pid handed on to
    /// something else since is never signalled. `false` if nothing was sent.
    fn signal(&self, entry: &RegistryEntry, force: bool) -> bool;
}

/// The machine's real processes.
pub struct SystemProcesses;

impl ProcessTable for SystemProcesses {
    #[cfg(target_os = "linux")]
    fn is_live_claude(&self, entry: &RegistryEntry) -> bool {
        use std::fs;
        let Ok(stat) = fs::read_to_string(format!("/proc/{}/stat", entry.pid)) else {
            return false;
        };
        if let Some(domain) = &entry.pid_domain {
            if let Ok(namespace) = fs::read_link("/proc/self/ns/pid") {
                if !domain.ends_with(&*namespace.to_string_lossy()) {
                    return false;
                }
            }
        }
        match &entry.proc_start {
            Some(started) => start_time(&stat) == Some(started.as_str()),
            None => fs::read(format!("/proc/{}/cmdline", entry.pid))
                .map(|cmdline| String::from_utf8_lossy(&cmdline).contains("claude"))
                .unwrap_or(false),
        }
    }

    fn signal(&self, entry: &RegistryEntry, force: bool) -> bool {
        self.is_live_claude(entry) && SystemProcesses::send(entry, force)
    }

    #[cfg(not(target_os = "linux"))]
    fn is_live_claude(&self, entry: &RegistryEntry) -> bool {
        std::process::Command::new("ps")
            .args(["-p", &entry.pid.to_string(), "-o", "command="])
            .output()
            .map(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .to_lowercase()
                        .contains("claude")
            })
            .unwrap_or(false)
    }
}

impl SystemProcesses {
    fn send(entry: &RegistryEntry, force: bool) -> bool {
        std::process::Command::new("kill")
            .args([
                if force { "-KILL" } else { "-TERM" },
                &entry.pid.to_string(),
            ])
            .output()
            .is_ok_and(|output| output.status.success())
    }
}

/// Field 22 of a `/proc/<pid>/stat` line, the start time. The command name
/// (field 2) is in parentheses and may itself contain spaces and `)`, so
/// counting starts after the last `)`.
pub fn start_time(stat: &str) -> Option<&str> {
    let after_name = &stat[stat.rfind(')')? + 1..];
    // Fields 3, 4, … follow; field 22 is the 20th of them.
    after_name.split_whitespace().nth(19)
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::collections::HashSet;

    /// Processes that are "live" because a test says so.
    pub struct FakeProcesses(pub HashSet<i32>);

    impl ProcessTable for FakeProcesses {
        fn is_live_claude(&self, entry: &RegistryEntry) -> bool {
            self.0.contains(&entry.pid)
        }

        fn signal(&self, entry: &RegistryEntry, _force: bool) -> bool {
            self.is_live_claude(entry)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_start_time_even_when_the_name_has_parentheses() {
        let fields = (3..=22)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let stat = format!("1234 (claude (v2) x) {fields} 23 24");
        assert_eq!(start_time(&stat), Some("22"));
        assert_eq!(start_time("1234 (short) S 1"), None);
        assert_eq!(start_time("no parentheses"), None);
    }

    #[test]
    fn this_process_is_not_a_claude() {
        let me = RegistryEntry {
            pid: std::process::id() as i32,
            ..RegistryEntry::default()
        };
        assert!(!SystemProcesses.is_live_claude(&me));
    }
}
