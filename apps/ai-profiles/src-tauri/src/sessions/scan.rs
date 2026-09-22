//! Reading a profile's transcripts into a list of sessions.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::desktop::{self, DesktopRecord};
use super::{parse_process_list, running_session_ids, Home};
use crate::error::AppResult;
use crate::launch::process_list;

/// One session as the Sessions list shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    /// The folder the session last worked in.
    pub cwd: Option<String>,
    /// The name the desktop app shows, else the one set with `/rename`, else
    /// Claude's generated one.
    pub title: Option<String>,
    /// The last thing typed into the session.
    pub last_prompt: Option<String>,
    /// When the transcript was last written, RFC 3339.
    pub updated_at: String,
    pub size_bytes: u64,
    /// A `claude` process has the session open right now.
    pub running: bool,
    /// The profile's desktop app lists the session.
    pub in_desktop: bool,
    /// Why the session cannot be moved, if it cannot.
    pub unmovable_reason: Option<String>,
}

/// What a transcript says about its session.
#[derive(Debug, Default, Clone)]
pub(crate) struct TranscriptInfo {
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

/// Read what the Sessions list needs from the transcript at `path`. Lines that
/// are not JSON (a write cut short) are skipped. Later lines win: the working
/// folder moves when a session is relocated, and titles are re-appended when
/// they change.
pub(crate) fn read_transcript(path: &Path) -> AppResult<TranscriptInfo> {
    let reader = BufReader::new(fs::File::open(path)?);
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

/// A file name that stays inside the folder it is joined to.
pub(crate) fn is_safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

/// Every transcript in `config_dir`: `projects/<project>/<id>.jsonl`, one level
/// down only, since subagent transcripts sit deeper. Returns (project, id, path).
pub(crate) fn transcripts(config_dir: &Path) -> Vec<(String, String, PathBuf)> {
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

/// Why a session in `home` with working folder `cwd` cannot be moved, if it
/// cannot. A scratch-workspace session works in a folder inside the source
/// profile's desktop data, which the destination has no copy of.
pub(crate) fn unmovable_reason(home: &Home, cwd: Option<&str>) -> Option<String> {
    let scratch = home.gui_data_dir.join("scratch-workspaces");
    match cwd {
        Some(cwd) if Path::new(cwd).starts_with(&scratch) => Some(
            "It works in a scratch folder of this profile's desktop app, which can't be moved yet."
                .to_string(),
        ),
        _ => None,
    }
}

/// The sessions of `home`, newest first. Sessions nothing happened in are left
/// out.
pub fn list(home: &Home) -> AppResult<Vec<SessionSummary>> {
    let processes = parse_process_list(&process_list()?);
    let running = running_session_ids(&home.config_dir, &processes);
    // A session can have an archived record and a live one; the live one wins.
    let mut records: HashMap<String, DesktopRecord> = HashMap::new();
    for record in desktop::records(&home.gui_data_dir) {
        let replaces = records
            .get(&record.cli_session_id)
            .is_none_or(|kept| kept.archived && !record.archived);
        if replaces {
            records.insert(record.cli_session_id.clone(), record);
        }
    }

    let mut sessions: Vec<(SystemTime, SessionSummary)> = Vec::new();
    for (_, id, path) in transcripts(&home.config_dir) {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let Ok(info) = read_transcript(&path) else {
            continue;
        };
        if info.is_empty() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let record = records.get(&id);
        sessions.push((
            modified,
            SessionSummary {
                title: record
                    .and_then(|record| record.title.clone())
                    .or_else(|| info.title()),
                unmovable_reason: unmovable_reason(home, info.cwd.as_deref()),
                running: running.contains(&id),
                in_desktop: record.is_some_and(|record| !record.archived),
                cwd: info.cwd,
                last_prompt: info.last_prompt,
                updated_at: chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339(),
                size_bytes: metadata.len(),
                id,
            },
        ));
    }
    sessions.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    Ok(sessions.into_iter().map(|(_, session)| session).collect())
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

    #[test]
    fn scratch_workspace_sessions_are_unmovable() {
        let home = Home {
            label: "P".into(),
            config_dir: "/p/cli-config".into(),
            gui_data_dir: "/p/gui-data".into(),
            stock: false,
            desktop: true,
        };
        assert!(unmovable_reason(&home, Some("/p/gui-data/scratch-workspaces/a/b/s")).is_some());
        assert!(unmovable_reason(&home, Some("/Users/x/code")).is_none());
        assert!(unmovable_reason(&home, None).is_none());
    }
}
