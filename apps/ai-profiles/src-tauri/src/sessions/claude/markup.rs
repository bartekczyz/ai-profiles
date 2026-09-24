//! Claude Code's markup in what the user typed.
//!
//! A slash command, a scheduled task or a `!` shell command reaches the
//! transcript as a user message wrapped in tags, such as
//! `<command-name>/review</command-name>` or
//! `<scheduled-task name="daily-log">…</scheduled-task>`, which say nothing a
//! title can use.

/// The tags Claude Code wraps what it sends on the user's behalf in. Only
/// these count as markup: users write hyphenated placeholders like
/// `<short-slug>` in their own prompts.
const MARKUP_TAGS: [&str; 14] = [
    "command-name",
    "command-message",
    "command-args",
    "local-command-stdout",
    "local-command-stderr",
    "local-command-caveat",
    "system-reminder",
    "bash-input",
    "bash-stdout",
    "bash-stderr",
    "user-prompt-submit-hook",
    "task-notification",
    "agent-message",
    "scheduled-task",
];

/// `prompt` without Claude Code's markup, to title a session with: every
/// element of one of the [`MARKUP_TAGS`] is dropped, content and all, and
/// whitespace runs collapse to one space. An element the prompt was cut short
/// inside of runs to its end. `None` when nothing is left.
pub fn strip_markup(prompt: &str) -> Option<String> {
    let mut text = String::new();
    let mut rest = prompt;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let tagged = &rest[start..];
        let Some(tag) = markup_tag(tagged) else {
            text.push('<');
            rest = &tagged[1..];
            continue;
        };
        let after = &tagged[tag.len..];
        if tag.closing {
            rest = after;
            continue;
        }
        let close = format!("</{}>", tag.name);
        rest = match after.find(&close) {
            Some(end) => &after[end + close.len()..],
            None => "",
        };
    }
    text.push_str(rest);
    let title = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return None;
    }
    Some(title)
}

/// A markup tag found at the start of some text.
struct Tag<'a> {
    /// The tag's name, such as `command-name`.
    name: &'a str,
    /// It ends an element (`</name>`) rather than starting one.
    closing: bool,
    /// How many bytes the tag takes, up to and including its `>`, or the rest
    /// of the text when that was cut off before the `>`.
    len: usize,
}

/// The markup tag `text` starts with: `<name …>` or `</name>` with a name of
/// the [`MARKUP_TAGS`], or the start of one where the text was cut off. `None`
/// for anything else, like the `<` of `a < b` or a tag the user wrote.
fn markup_tag(text: &str) -> Option<Tag<'_>> {
    let body = text.strip_prefix('<')?;
    let (closing, body) = match body.strip_prefix('/') {
        Some(body) => (true, body),
        None => (false, body),
    };
    let name_len = body
        .find(|character: char| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
        })
        .unwrap_or(body.len());
    let name = &body[..name_len];
    let tail = &body[name_len..];
    let known = if tail.is_empty() {
        !name.is_empty() && MARKUP_TAGS.iter().any(|tag| tag.starts_with(name))
    } else {
        MARKUP_TAGS.contains(&name)
    };
    if !known {
        return None;
    }
    if !(tail.is_empty() || tail.starts_with('>') || tail.starts_with(char::is_whitespace)) {
        return None;
    }
    let prefix = text.len() - body.len();
    let len = match tail.find('>') {
        Some(end) => prefix + name_len + end + 1,
        None => text.len(),
    };
    Some(Tag { name, closing, len })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_prompt_is_its_own_title() {
        assert_eq!(
            strip_markup("  Fix the\n\nlogin bug ").as_deref(),
            Some("Fix the login bug")
        );
    }

    #[test]
    fn a_slash_command_leaves_no_title() {
        let prompt = "<command-message>review</command-message>\n\
            <command-name>/review</command-name>\n\
            <command-args>123</command-args>";

        assert_eq!(strip_markup(prompt), None);
    }

    #[test]
    fn text_typed_around_markup_is_kept() {
        let prompt = "<command-message>plan</command-message>\n\
            <command-name>/plan</command-name>\nPlan the </local-command-stdout>login page";

        assert_eq!(strip_markup(prompt).as_deref(), Some("Plan the login page"));
    }

    #[test]
    fn an_element_cut_short_runs_to_the_end() {
        let prompt =
            "<scheduled-task name=\"daily-log\" file=\"/x/SKILL.md\">\nThis is an automated run";

        assert_eq!(strip_markup(prompt), None);
        assert_eq!(strip_markup("Hi <system-remin"), Some("Hi".to_string()));
    }

    #[test]
    fn angle_brackets_that_are_not_markup_are_kept() {
        let prompt = "Why does <div> collapse when a < b and <-x>?";

        assert_eq!(strip_markup(prompt).as_deref(), Some(prompt));
    }

    #[test]
    fn hyphenated_tags_the_user_wrote_are_kept() {
        let prompt = "Why doesn't <my-app> render?";

        assert_eq!(strip_markup(prompt).as_deref(), Some(prompt));
        assert_eq!(
            strip_markup("Write it to <output-dir>/<short-slug>.md").as_deref(),
            Some("Write it to <output-dir>/<short-slug>.md")
        );
    }

    #[test]
    fn notifications_claude_code_sends_on_the_users_behalf_leave_no_title() {
        let prompt = "<task-notification>\n<task-id>a4d4</task-id>\n\
            <output-file>/tmp/a.output</output-file>\n</task-notification>\n\
            <agent-message from=\"a4d4\">Report</agent-message>";

        assert_eq!(strip_markup(prompt), None);
    }
}
