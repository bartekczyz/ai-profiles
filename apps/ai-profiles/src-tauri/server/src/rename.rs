//! Renaming a session so that everything calling it something agrees: its
//! transcript's title (what the app lists), the registry's name, and Remote
//! Control's title in the Claude app.
//!
//! A running session is renamed by Claude itself, the way typing `/rename`
//! in its window does: Claude writes the title, pushes it to Remote Control
//! and updates the registry, and settles a clash with another live session's
//! name. So the server types that into the window, when Claude is idle at an
//! empty prompt, and only in a window it opened. A stopped session gets the
//! lines Claude would write, and Remote Control takes the name when it is
//! resumed (with `--remote-control <name>`).
//!
//! The other way needs nothing here: a rename in the Claude app reaches the
//! running Claude over Remote Control, which writes it to the transcript.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use ai_profiles_core::registry::RegistryEntry;

use crate::error::ApiError;
use crate::tmux::Tmux;

/// The longest name taken, in characters.
pub const MAX_NAME: usize = 100;

/// How long Claude is given to take a new name.
const RENAME_TIMEOUT: Duration = Duration::from_secs(6);
const POLL: Duration = Duration::from_millis(200);

/// `name` trimmed, if it can be a session's name: not empty, not too long,
/// one line of printable text, and not starting with `-`, as a new
/// session's name can't.
pub fn valid_name(name: &str) -> Option<String> {
    let name = name.trim();
    let fine = !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && !name.chars().any(char::is_control)
        && !name.starts_with('-');
    fine.then(|| name.to_owned())
}

/// Name the stopped session `id`, whose transcript is `transcript`: the
/// title and agent name Claude writes when a session is renamed.
pub fn rename_stopped(transcript: &Path, id: &str, name: &str) -> io::Result<()> {
    let ends_in_newline = fs::read(transcript)
        .map(|bytes| bytes.last().is_none_or(|last| *last == b'\n'))
        .unwrap_or(true);
    let mut text = String::new();
    if !ends_in_newline {
        text.push('\n');
    }
    for line in [
        serde_json::json!({"type": "custom-title", "customTitle": name, "sessionId": id}),
        serde_json::json!({"type": "agent-name", "agentName": name, "sessionId": id}),
    ] {
        text.push_str(&line.to_string());
        text.push('\n');
    }
    OpenOptions::new()
        .append(true)
        .open(transcript)?
        .write_all(text.as_bytes())
}

/// Pure: whether Claude's screen shows its prompt empty and waiting: the
/// line between the two rules around the input box is only its `❯`. A menu
/// or a question has something after its `❯`, and a half-typed message is
/// someone's.
pub fn prompt_is_empty(screen: &str) -> bool {
    let lines: Vec<&str> = screen.lines().collect();
    let rule = |line: &str| {
        let line = line.trim();
        line.chars().count() >= 2 && line.chars().all(|c| c == '─')
    };
    lines
        .iter()
        .rposition(|line| line.trim_start().starts_with('❯'))
        .is_some_and(|at| {
            lines[at].trim_start()['❯'.len_utf8()..].trim().is_empty()
                && at > 0
                && rule(lines[at - 1])
                && lines.get(at + 1).is_some_and(|line| rule(line))
        })
}

/// Have the running Claude in `pane` rename its session to `name`, and wait
/// for its registry entry at `registry` to say so. Returns the name Claude
/// settled on, which has a suffix when another live session holds `name`.
pub fn rename_live(
    tmux: &Tmux,
    pane: &str,
    registry: &Path,
    entry: &RegistryEntry,
    name: &str,
) -> Result<String, ApiError> {
    if entry.status.as_deref() == Some("busy") {
        return Err(ApiError::conflict(
            "session_busy",
            "Claude is working in it. Rename it once it's done, or in the Claude app.",
        ));
    }
    let screen = tmux.capture(pane).unwrap_or_default();
    if !prompt_is_empty(&screen) {
        return Err(ApiError::conflict(
            "prompt_not_empty",
            "Claude isn't at an empty prompt: it's asking something, or something is typed there. Rename it once that's done, or in the Claude app.",
        ));
    }
    let before = entry.name.clone();
    tmux.type_text(pane, &format!("/rename {name}"))
        .map_err(ApiError::internal)?;
    // Let the command menu catch up with what was typed before sending it.
    thread::sleep(Duration::from_millis(250));
    tmux.press(pane, "Enter").map_err(ApiError::internal)?;
    let deadline = Instant::now() + RENAME_TIMEOUT;
    while Instant::now() < deadline {
        thread::sleep(POLL);
        let now = fs::read_to_string(registry)
            .ok()
            .and_then(|text| serde_json::from_str::<RegistryEntry>(&text).ok())
            .and_then(|entry| entry.name);
        match now {
            Some(now) if now == name || Some(&now) != before.as_ref() => return Ok(now),
            _ => {}
        }
    }
    Err(ApiError::conflict(
        "rename_not_taken",
        "Claude didn't take the new name. Look at its window: something may be in the way.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: &str = "────────────────────────────────";

    #[test]
    fn takes_one_line_of_text_as_a_name() {
        assert_eq!(valid_name("  Brain dev  ").as_deref(), Some("Brain dev"));
        assert_eq!(valid_name(""), None);
        assert_eq!(valid_name("   "), None);
        assert_eq!(valid_name("two\nlines"), None);
        assert_eq!(valid_name("--dangerously-skip-permissions"), None);
        assert_eq!(
            valid_name(&"x".repeat(MAX_NAME)).map(|n| n.len()),
            Some(100)
        );
        assert_eq!(valid_name(&"x".repeat(MAX_NAME + 1)), None);
    }

    #[test]
    fn sees_an_empty_prompt_and_nothing_else() {
        let idle = format!("  Done.\n{RULE}\n❯\u{a0}\n{RULE}\n  ⏵⏵ auto mode on\n");
        assert!(prompt_is_empty(&idle));
        let typed = format!("{RULE}\n❯ half a message\n{RULE}\n");
        assert!(!prompt_is_empty(&typed));
        let menu = "Is this a project you trust?\n ❯ No, exit\n   Yes, I trust this folder\n";
        assert!(!prompt_is_empty(menu));
        assert!(!prompt_is_empty("❯ \n"), "no input box around it");
    }

    #[test]
    fn a_stopped_session_gets_the_lines_claude_writes() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("s.jsonl");
        fs::write(&transcript, "{\"type\":\"user\"}").unwrap();
        rename_stopped(&transcript, "s", "Brain \"dev\"").unwrap();
        let text = fs::read_to_string(&transcript).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 3, "the unfinished last line was ended first");
        assert_eq!(lines[1]["type"], "custom-title");
        assert_eq!(lines[1]["customTitle"], "Brain \"dev\"");
        assert_eq!(lines[2]["type"], "agent-name");
        assert_eq!(lines[2]["agentName"], "Brain \"dev\"");
        assert_eq!(lines[2]["sessionId"], "s");
    }
}
