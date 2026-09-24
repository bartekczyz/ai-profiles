//! Reading Claude Code transcripts: `<config>/projects/<project>/<id>.jsonl`.
//!
//! Shared by the ai-profiles app and ai-profiles-server, so it depends on
//! nothing but std and serde. Everything here reads Claude Code internals,
//! which can change between versions.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// What a transcript says about its session.
#[derive(Debug, Default, Clone)]
pub struct TranscriptInfo {
    pub cwd: Option<String>,
    pub custom_title: Option<String>,
    pub ai_title: Option<String>,
    pub last_prompt: Option<String>,
    pub first_timestamp: Option<String>,
    /// Claude replied at least once.
    pub has_reply: bool,
    /// Plan slugs the session wrote, `plans/<slug>.md`.
    pub slugs: BTreeSet<String>,
}

impl TranscriptInfo {
    /// Opened and closed without anything happening, like Claude's own
    /// `/resume` hides.
    pub fn is_empty(&self) -> bool {
        !self.has_reply && self.last_prompt.is_none()
    }

    pub fn title(&self) -> Option<String> {
        self.custom_title.clone().or_else(|| self.ai_title.clone())
    }

    /// What to call the session where nothing has named it better: its
    /// `/rename` name (the bool is true: the user chose it), else Claude's
    /// generated title, else its last prompt cut to a short line.
    pub fn name(&self) -> Option<(String, bool)> {
        if let Some(title) = &self.custom_title {
            return Some((title.clone(), true));
        }
        self.ai_title
            .clone()
            .or_else(|| self.last_prompt.as_deref().and_then(short_line))
            .map(|name| (name, false))
    }
}

/// The few fields of a transcript line that matter here. Everything else is
/// skipped without being built.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    cwd: Option<String>,
    custom_title: Option<String>,
    ai_title: Option<String>,
    last_prompt: Option<String>,
    slug: Option<String>,
    timestamp: Option<String>,
}

/// Read what the Sessions list needs from the transcript at `path`, gzipped
/// when it ends in `.gz`, as an archived one does. Lines that
/// are not JSON (a write cut short) are skipped. Later lines win: the working
/// folder moves when a session is relocated, and titles are re-appended when
/// they change.
pub fn read_transcript(path: &Path) -> io::Result<TranscriptInfo> {
    let file = fs::File::open(path)?;
    // An archived transcript is kept gzipped.
    let reader: Box<dyn BufRead> = if path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(BufReader::new(flate2::read::GzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let mut info = TranscriptInfo::default();
    for line in reader.lines() {
        let line = line?;
        let Ok(parsed) = serde_json::from_str::<Line>(&line) else {
            continue;
        };
        match parsed.kind.as_deref() {
            Some("assistant") => info.has_reply = true,
            Some("custom-title") => info.custom_title = parsed.custom_title.or(info.custom_title),
            Some("ai-title") => info.ai_title = parsed.ai_title.or(info.ai_title),
            Some("last-prompt") => info.last_prompt = parsed.last_prompt.or(info.last_prompt),
            _ => {}
        }
        if parsed.cwd.is_some() {
            info.cwd = parsed.cwd;
        }
        if let Some(slug) = parsed.slug.filter(|slug| is_safe_name(slug)) {
            info.slugs.insert(slug);
        }
        if info.first_timestamp.is_none() {
            info.first_timestamp = parsed.timestamp;
        }
    }
    Ok(info)
}

/// The first line of `text`, trimmed, cut to a title's length on a character
/// boundary. `None` when nothing is left.
pub fn short_line(text: &str) -> Option<String> {
    const MAX_CHARS: usize = 60;
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    if line.chars().count() <= MAX_CHARS {
        return Some(line.to_string());
    }
    let cut: String = line.chars().take(MAX_CHARS - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// A file name that stays inside the folder it is joined to.
pub fn is_safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

/// Every transcript in `config_dir`: `projects/<project>/<id>.jsonl`, one level
/// down only, since subagent transcripts sit deeper. Returns (project, id, path).
pub fn transcripts(config_dir: &Path) -> Vec<(String, String, PathBuf)> {
    let Ok(projects) = fs::read_dir(config_dir.join("projects")) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for project in projects.flatten() {
        if !project.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let project_name = project.file_name().to_string_lossy().into_owned();
        let Ok(files) = fs::read_dir(project.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            if !file.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
            else {
                continue;
            };
            found.push((project_name.clone(), id, path));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn reads_the_latest_cwd_titles_and_plan_slugs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        write(
            &path,
            concat!(
                r#"{"type":"user","cwd":"/scratch","timestamp":"2026-01-01T00:00:00Z","message":{"content":"{\"cwd\":\"/nested\"}"}}"#,
                "\n",
                r#"{"type":"assistant","cwd":"/scratch","slug":"bold-plan"}"#,
                "\n",
                "{not json\n",
                r#"{"type":"ai-title","aiTitle":"Generated"}"#,
                "\n",
                r#"{"type":"relocated"}"#,
                "\n",
                r#"{"type":"user","cwd":"/work","timestamp":"2026-01-02T00:00:00Z"}"#,
                "\n",
                r#"{"type":"custom-title","customTitle":"Old name"}"#,
                "\n",
                r#"{"type":"custom-title","customTitle":"New name"}"#,
                "\n",
                r#"{"type":"last-prompt","lastPrompt":"do it"}"#,
                "\n",
                r#"{"type":"assistant","slug":"../escape"}"#,
                "\n",
            ),
        );
        let info = read_transcript(&path).unwrap();
        assert_eq!(info.cwd.as_deref(), Some("/work"));
        assert_eq!(info.title().as_deref(), Some("New name"));
        assert_eq!(info.last_prompt.as_deref(), Some("do it"));
        assert_eq!(
            info.first_timestamp.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert!(info.has_reply);
        assert_eq!(
            info.slugs.into_iter().collect::<Vec<_>>(),
            vec!["bold-plan".to_string()]
        );
    }

    #[test]
    fn name_prefers_the_users_name_then_claudes_then_the_last_prompt() {
        let mut info = TranscriptInfo {
            last_prompt: Some("\n  Reply with just: ok  \nand more".into()),
            ..TranscriptInfo::default()
        };
        assert_eq!(info.name(), Some(("Reply with just: ok".into(), false)));
        info.ai_title = Some("Generated".into());
        assert_eq!(info.name(), Some(("Generated".into(), false)));
        info.custom_title = Some("Mine".into());
        assert_eq!(info.name(), Some(("Mine".into(), true)));
        assert_eq!(TranscriptInfo::default().name(), None);
    }

    #[test]
    fn short_line_cuts_long_prompts_on_a_character_boundary() {
        let long = "é".repeat(100);
        let cut = short_line(&long).unwrap();
        assert_eq!(cut.chars().count(), 60);
        assert!(cut.ends_with('…'));
        assert_eq!(short_line("   \n  "), None);
    }

    #[test]
    fn a_session_nothing_happened_in_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        write(&path, "{\"type\":\"user\",\"cwd\":\"/w\"}\n");
        assert!(read_transcript(&path).unwrap().is_empty());
    }

    #[test]
    fn transcripts_skips_subagent_transcripts_and_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("projects/-work");
        write(&project.join("a.jsonl"), "");
        write(&project.join("a/subagents/agent-x.jsonl"), "");
        write(&project.join("memory/MEMORY.md"), "");
        write(&dir.path().join("projects/stray.jsonl"), "");
        let found = transcripts(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "-work");
        assert_eq!(found[0].1, "a");
    }
}
