//! What of its own environment ai-profiles never passes on to an app or CLI it
//! starts.
//!
//! What ai-profiles starts is handed ai-profiles' own environment, and that can
//! carry things meant for something else:
//!
//! - an app's config-home variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`), when
//!   ai-profiles was started from a shell running under some profile. The app
//!   would then run on that profile's config: the stock one, which is given
//!   none of its own, always, and a launcher that sets none of its own too.
//! - a whole Claude Code session, when ai-profiles was started from inside one
//!   (`pnpm tauri dev` from a Claude Code shell, say): its session id, the
//!   markers that make `claude` behave as that session's child, the host's
//!   socket and settings. A Dock click would give the app none of these.
//!
//! Outside a Claude Code session only the config homes are held back, so a
//! variable the user set for their apps on purpose (with `launchctl setenv`,
//! say) still reaches them.

use std::ffi::{OsStr, OsString};

use crate::app_kind::AppKind;

/// Set in every process a Claude Code session starts, and only there.
const CLAUDE_CODE_SESSION_MARKER: &str = "CLAUDECODE";

/// The prefixes of the variables a Claude Code session hands its processes.
const CLAUDE_CODE_SESSION_PREFIXES: [&str; 2] = ["CLAUDE", "ANTHROPIC_"];

/// Pure: of the variable names `keys` (an environment's), the ones not to pass
/// on. Every app's config home always, whether `keys` has it or not; and, when
/// `keys` is a Claude Code session's, every `CLAUDE*` and `ANTHROPIC_*` in it.
pub fn not_passed_on<K: AsRef<OsStr>>(keys: impl IntoIterator<Item = K>) -> Vec<OsString> {
    let keys: Vec<OsString> = keys
        .into_iter()
        .map(|key| key.as_ref().to_owned())
        .collect();
    let mut dropped: Vec<OsString> = [AppKind::Claude, AppKind::Codex]
        .into_iter()
        .map(|kind| OsString::from(kind.spec().cli_config_env))
        .collect();
    if !keys.iter().any(|key| key == CLAUDE_CODE_SESSION_MARKER) {
        return dropped;
    }
    for key in keys {
        let from_session = key.to_str().is_some_and(|name| {
            CLAUDE_CODE_SESSION_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
        });
        if from_session && !dropped.contains(&key) {
            dropped.push(key);
        }
    }
    dropped
}

/// [`not_passed_on`] for ai-profiles' own environment.
pub fn current() -> Vec<OsString> {
    not_passed_on(std::env::vars_os().map(|(key, _)| key))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `names` as the [`OsString`]s [`not_passed_on`] returns.
    fn names(names: &[&str]) -> Vec<OsString> {
        names.iter().map(OsString::from).collect()
    }

    #[test]
    fn outside_a_session_only_the_config_homes_are_held_back() {
        let dropped = not_passed_on([
            "PATH",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_USE_BEDROCK",
            "ANTHROPIC_BASE_URL",
        ]);

        assert_eq!(dropped, names(&["CLAUDE_CONFIG_DIR", "CODEX_HOME"]));
    }

    #[test]
    fn the_config_homes_are_held_back_even_when_not_set() {
        assert_eq!(
            not_passed_on(Vec::<&str>::new()),
            names(&["CLAUDE_CONFIG_DIR", "CODEX_HOME"])
        );
    }

    #[test]
    fn inside_a_session_everything_claude_and_anthropic_is_held_back() {
        let dropped = not_passed_on([
            "PATH",
            "HOME",
            "AI_AGENT",
            "CLAUDECODE",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_PID",
            "ANTHROPIC_BASE_URL",
        ]);

        assert_eq!(
            dropped,
            names(&[
                "CLAUDE_CONFIG_DIR",
                "CODEX_HOME",
                "CLAUDECODE",
                "CLAUDE_CODE_SESSION_ID",
                "CLAUDE_PID",
                "ANTHROPIC_BASE_URL",
            ])
        );
    }

    #[test]
    fn a_config_home_the_session_set_is_held_back_once() {
        let dropped = not_passed_on(["CLAUDECODE", "CLAUDE_CONFIG_DIR"]);

        assert_eq!(
            dropped,
            names(&["CLAUDE_CONFIG_DIR", "CODEX_HOME", "CLAUDECODE"])
        );
    }
}
