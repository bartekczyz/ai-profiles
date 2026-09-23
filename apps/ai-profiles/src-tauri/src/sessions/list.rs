//! The sessions a home owns, as the Sessions list shows them.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::claude::archive_store::{archived_bundles, ArchivedBundle};
use super::claude::desktop::read_records;
use super::claude::live::{live_sessions, LiveHolder};
use super::claude::markup::strip_markup;
use super::claude::ownership::{owned_by, HomeScan, Owned};
use super::claude::transcript::{scan_projects, TranscriptSummary};
use super::codex;
use super::home::homes_of;
use super::Home;
use crate::app_kind::AppKind;
use crate::error::{AppError, AppResult};
use crate::launch::process_list;

/// Why a session whose desktop record outlived its transcript can't move.
const TRANSCRIPT_DELETED: &str = "Transcript deleted";

/// Why a session started in the desktop app without a project can't move.
const IN_SCRATCH_FOLDER: &str = "Lives in the desktop app's scratch folder";

/// Why a session open in a terminal can't move, or be archived.
pub(super) const OPEN_IN_TERMINAL: &str = "Close it in the terminal first";

/// Where a session was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionKind {
    /// The desktop app: Claude's Code tab, or Codex desktop.
    Desktop,
    /// The CLI, or an IDE extension.
    Cli,
}

/// What the session's files are doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    /// Nothing has it open.
    Idle,
    /// A CLI in a terminal has it open.
    OpenInTerminal,
    /// The desktop app has it open.
    OpenInDesktop,
    /// The desktop app has a record of it, but its transcript is gone.
    TranscriptMissing,
}

/// One row of the Sessions list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Claude: the id of the transcript shown, which for a desktop session is
    /// its record's `cliSessionId`, else the last used of its
    /// `priorCliSessionIds`. A desktop session whose transcripts are all gone
    /// keeps its `cliSessionId`, else its record's `local_<uuid>`. Codex: the
    /// thread id.
    pub id: String,
    /// Where the session was started.
    pub kind: SessionKind,
    /// Claude: the desktop record's title, else the name set with `/rename`,
    /// else the title Claude generated, else the first prompt without its
    /// markup. Codex: the thread's name, else its first prompt.
    pub title: Option<String>,
    /// The folder the session works in.
    pub cwd: Option<String>,
    /// Claude: the last thing typed into the session. Codex: the first, as
    /// app-server only lists that.
    pub last_prompt: Option<String>,
    /// When the session was last used: the later of its desktop record's last
    /// activity and its transcript's last record.
    pub last_used_at: DateTime<Utc>,
    /// The session is archived.
    pub archived: bool,
    /// What the session's files are doing right now.
    pub state: SessionState,
    /// One of the session's transcripts sits in another home's config dir,
    /// where the desktop app of an older version of ai-profiles left it.
    pub needs_repair: bool,
    /// Why the session can't be moved to another profile, if it can't.
    pub unmovable_reason: Option<String>,
}

/// The sessions a home owns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    /// Active and archived, most recently used first.
    pub sessions: Vec<Session>,
    /// How many of them need repair.
    pub repair_count: u32,
}

/// The sessions `home` owns. Ownership of a Claude session depends on what
/// every home of the app holds, so all of them are read, on a blocking thread
/// as that walks every transcript. Codex sessions are what the home's
/// `codex app-server` lists.
pub async fn list_sessions(home: Home) -> AppResult<SessionList> {
    match home.app {
        AppKind::Claude => tokio::task::spawn_blocking(move || {
            let homes = homes_of(AppKind::Claude)?;
            // Without a process list nothing shows as open, which is only
            // wrong until the next listing.
            let ps_output = process_list().unwrap_or_default();
            Ok(claude_sessions(&home, &homes, &ps_output))
        })
        .await
        .map_err(|error| AppError::Io(std::io::Error::other(error)))?,
        AppKind::Codex => Ok(SessionList {
            sessions: codex::list(&home).await?,
            repair_count: 0,
        }),
    }
}

/// The Claude sessions `home` owns, of all `homes` of the app, given the
/// output of `ps -ax -o pid=,command=`.
fn claude_sessions(home: &Home, homes: &[Home], ps_output: &str) -> SessionList {
    let scans = home_scans(homes);
    let live = live_anywhere(homes, ps_output);
    let mut sessions: Vec<Session> = owned_by(&home.id, &scans)
        .into_iter()
        .map(|owned| owned_session(home, owned, &live))
        .collect();
    let listed: HashSet<String> = sessions.iter().map(|session| session.id.clone()).collect();
    let bundles = archived_bundles(&home.config_dir)
        .into_iter()
        .filter(|bundle| !listed.contains(&bundle.session_id))
        .map(|bundle| archived_session(home, bundle));
    sessions.extend(bundles);
    sessions.sort_by_key(|session| Reverse(session.last_used_at));
    let repair_count = sessions
        .iter()
        .filter(|session| session.needs_repair)
        .count();
    SessionList {
        sessions,
        repair_count: u32::try_from(repair_count).unwrap_or(u32::MAX),
    }
}

/// What each of `homes` holds: its transcripts and desktop records.
pub(super) fn home_scans(homes: &[Home]) -> Vec<HomeScan> {
    homes
        .iter()
        .map(|each| HomeScan {
            home_id: each.id.clone(),
            transcripts: scan_projects(&each.config_dir),
            records: read_records(&each.gui_data_dir),
        })
        .collect()
}

/// The sessions open in any of `homes`, by session id, given the output of
/// `ps -ax -o pid=,command=`. A process registers the session it has open in
/// the config dir its transcript is in, which for an orphan is another
/// home's. A terminal holds a session also open in a desktop app.
pub(super) fn live_anywhere(homes: &[Home], ps_output: &str) -> HashMap<String, LiveHolder> {
    let mut live = HashMap::new();
    for each in homes {
        for (session_id, holder) in live_sessions(&each.config_dir, ps_output) {
            let held = live.entry(session_id).or_insert(holder);
            if holder == LiveHolder::Terminal {
                *held = holder;
            }
        }
    }
    live
}

/// The title `transcript` gives its session: the name set with `/rename`,
/// else the title Claude generated, else the first prompt without its markup.
pub(super) fn transcript_title(transcript: &TranscriptSummary) -> Option<String> {
    transcript
        .custom_title
        .clone()
        .or_else(|| transcript.ai_title.clone())
        .or_else(|| transcript.first_prompt.as_deref().and_then(strip_markup))
}

/// The row of a session `home` owns, given the sessions open anywhere.
fn owned_session(home: &Home, owned: Owned, live: &HashMap<String, LiveHolder>) -> Session {
    let Owned {
        session_id,
        transcript,
        record,
        claimed_transcripts,
    } = owned;
    let transcript = transcript.as_ref().map(|held| &held.summary);
    let record = record.as_ref();
    let title = record
        .and_then(|record| record.title.clone())
        .or_else(|| transcript.and_then(transcript_title));
    let cwd = record
        .and_then(|record| record.cwd.clone())
        .or_else(|| transcript.and_then(|transcript| transcript.cwd.clone()));
    let last_used_at = [
        record.and_then(|record| record.last_activity_at),
        transcript.map(|transcript| transcript.last_used_at),
    ]
    .into_iter()
    .flatten()
    .max()
    .or_else(|| record.and_then(|record| modified_at(&record.path)))
    .unwrap_or_default();
    let archived = record.is_some_and(|record| record.archived);
    let state = if transcript.is_none() {
        SessionState::TranscriptMissing
    } else if archived {
        SessionState::Idle
    } else {
        match live.get(&session_id) {
            Some(LiveHolder::Terminal) => SessionState::OpenInTerminal,
            Some(LiveHolder::Desktop) => SessionState::OpenInDesktop,
            None => SessionState::Idle,
        }
    };
    let unmovable_reason = unmovable_reason(home, state, cwd.as_deref());
    Session {
        id: session_id,
        kind: if record.is_some() {
            SessionKind::Desktop
        } else {
            SessionKind::Cli
        },
        title,
        last_prompt: transcript.and_then(|transcript| transcript.last_prompt.clone()),
        cwd,
        last_used_at,
        archived,
        state,
        needs_repair: claimed_transcripts
            .iter()
            .any(|held| held.home_id != home.id),
        unmovable_reason,
    }
}

/// The row of a session archived into `home`'s config dir.
fn archived_session(home: &Home, bundle: ArchivedBundle) -> Session {
    let state = SessionState::Idle;
    Session {
        unmovable_reason: unmovable_reason(home, state, bundle.cwd.as_deref()),
        id: bundle.session_id,
        kind: SessionKind::Cli,
        title: bundle.title,
        cwd: bundle.cwd,
        last_prompt: bundle.last_prompt,
        last_used_at: bundle.last_used_at,
        archived: true,
        state,
        needs_repair: false,
    }
}

/// Why a session of `home` in `state`, working in `cwd`, can't be moved, if
/// it can't: lasting reasons before one the user can clear.
pub(super) fn unmovable_reason(
    home: &Home,
    state: SessionState,
    cwd: Option<&str>,
) -> Option<String> {
    let in_scratch = cwd.is_some_and(|cwd| Path::new(cwd).starts_with(&home.gui_data_dir));
    let reason = match state {
        SessionState::TranscriptMissing => TRANSCRIPT_DELETED,
        _ if in_scratch => IN_SCRATCH_FOLDER,
        SessionState::OpenInTerminal => OPEN_IN_TERMINAL,
        SessionState::Idle | SessionState::OpenInDesktop => return None,
    };
    Some(reason.to_string())
}

/// When the file at `path` was last written.
fn modified_at(path: &Path) -> Option<DateTime<Utc>> {
    Some(DateTime::from(fs::metadata(path).ok()?.modified().ok()?))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;

    const ACCOUNT: &str = "1a19a582-d7b1-4f72-acef-cbe78c1a68e4";
    const ORG: &str = "18d53058-434e-4c78-9624-e290f7a80ccb";

    fn home(root: &Path, id: &str, stock: bool) -> Home {
        Home {
            id: id.to_string(),
            app: AppKind::Claude,
            label: id.to_string(),
            config_dir: root.join(id).join("cli-config"),
            gui_data_dir: root.join(id).join("gui-data"),
            stock,
        }
    }

    fn write_transcript(home: &Home, session: &str, lines: &[Value]) {
        let dir = home.config_dir.join("projects").join("-work-app");
        fs::create_dir_all(&dir).unwrap();
        let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
        fs::write(dir.join(format!("{session}.jsonl")), body).unwrap();
    }

    fn user(session: &str, timestamp: &str, cwd: &str, text: &str) -> Value {
        json!({
            "type": "user",
            "sessionId": session,
            "timestamp": timestamp,
            "cwd": cwd,
            "isSidechain": false,
            "message": { "role": "user", "content": text },
        })
    }

    fn write_record(home: &Home, local: &str, fields: Value) {
        let dir = home
            .gui_data_dir
            .join("claude-code-sessions")
            .join(ACCOUNT)
            .join(ORG);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("local_{local}.json")), fields.to_string()).unwrap();
    }

    fn write_registry(home: &Home, pid: i32, session: &str, entrypoint: &str) {
        let dir = home.config_dir.join("sessions");
        fs::create_dir_all(&dir).unwrap();
        let entry = json!({ "pid": pid, "sessionId": session, "entrypoint": entrypoint });
        fs::write(dir.join(format!("{pid}.json")), entry.to_string()).unwrap();
    }

    fn utc(timestamp: &str) -> DateTime<Utc> {
        timestamp.parse().unwrap()
    }

    fn millis(timestamp: &str) -> i64 {
        utc(timestamp).timestamp_millis()
    }

    fn session(id: &str, kind: SessionKind, last_used_at: &str) -> Session {
        Session {
            id: id.to_string(),
            kind,
            title: None,
            cwd: Some("/work/app".to_string()),
            last_prompt: None,
            last_used_at: utc(last_used_at),
            archived: false,
            state: SessionState::Idle,
            needs_repair: false,
            unmovable_reason: None,
        }
    }

    const PS_OUTPUT: &str = "  4100 claude --resume\n  4200 claude\n";

    /// Default holds a CLI session, a live CLI session, an orphaned transcript
    /// of Personal's desktop app, earlier transcripts of two of Personal's
    /// desktop sessions and an archived bundle. Personal's desktop app has
    /// records of the orphan, of a session whose transcript is gone, of an
    /// archived session in its own config dir, of a session continued from an
    /// earlier transcript, of one whose current transcript never got written
    /// and of one it never started Claude Code for.
    fn two_homes(root: &Path) -> (Home, Home) {
        let default = home(root, "default", true);
        let personal = home(root, "personal", false);
        write_transcript(
            &default,
            "cli",
            &[
                user("cli", "2026-09-01T10:00:00Z", "/work/app", "<command-message>review</command-message>\n<command-name>/review</command-name>"),
                json!({ "type": "ai-title", "aiTitle": "Review the PR", "sessionId": "cli" }),
                json!({ "type": "last-prompt", "lastPrompt": "Looks good?", "sessionId": "cli" }),
            ],
        );
        write_transcript(
            &default,
            "busy",
            &[user(
                "busy",
                "2026-09-02T10:00:00Z",
                "/work/app",
                "<command-message>plan</command-message> Plan the release",
            )],
        );
        write_registry(&default, 4100, "busy", "cli");
        write_transcript(
            &default,
            "orphan",
            &[user(
                "orphan",
                "2026-09-03T10:00:00Z",
                "/work/app",
                "Fix the bug",
            )],
        );
        write_record(
            &personal,
            "orphan",
            json!({
                "cliSessionId": "orphan",
                "title": "Fix the login bug",
                "cwd": "/work/app",
                "lastActivityAt": millis("2026-09-03T11:00:00Z"),
            }),
        );
        write_record(
            &personal,
            "gone",
            json!({
                "cliSessionId": "gone",
                "title": "Old chat",
                "cwd": "/work/app",
                "lastActivityAt": millis("2026-08-01T10:00:00Z"),
            }),
        );
        let scratch = personal.gui_data_dir.join("scratch").join("abc");
        write_transcript(
            &personal,
            "put-away",
            &[user(
                "put-away",
                "2026-09-04T10:00:00Z",
                &scratch.display().to_string(),
                "Try this",
            )],
        );
        write_registry(&personal, 4200, "put-away", "claude-desktop");
        write_record(
            &personal,
            "put-away",
            json!({
                "cliSessionId": "put-away",
                "cwd": scratch.display().to_string(),
                "isArchived": true,
                "lastActivityAt": millis("2026-09-04T09:00:00Z"),
            }),
        );
        write_transcript(
            &default,
            "resumed-before",
            &[user(
                "resumed-before",
                "2026-09-06T08:00:00Z",
                "/work/app",
                "Start",
            )],
        );
        write_transcript(
            &personal,
            "resumed",
            &[user(
                "resumed",
                "2026-09-06T10:00:00Z",
                "/work/app",
                "Continue",
            )],
        );
        write_record(
            &personal,
            "resumed",
            json!({
                "cliSessionId": "resumed",
                "priorCliSessionIds": ["resumed-before"],
                "title": "Migration",
                "cwd": "/work/app",
                "lastActivityAt": millis("2026-09-06T09:00:00Z"),
            }),
        );
        write_transcript(
            &default,
            "restarted-before",
            &[user(
                "restarted-before",
                "2026-09-07T10:00:00Z",
                "/work/app",
                "Hi",
            )],
        );
        write_record(
            &personal,
            "restarted",
            json!({
                "priorCliSessionIds": ["restarted-before"],
                "title": "Restarted",
                "cwd": "/work/app",
                "lastActivityAt": millis("2026-09-07T09:00:00Z"),
            }),
        );
        write_record(&personal, "unstarted", json!({ "title": "Draft" }));
        let bundle = default
            .config_dir
            .join("ai-profiles-archive")
            .join("shelved")
            .join("2026-09-05T10-00-00Z");
        fs::create_dir_all(&bundle).unwrap();
        let manifest = json!({
            "title": "Shelved idea",
            "cwd": "/work/app",
            "lastUsedAt": "2026-07-01T10:00:00Z",
        });
        fs::write(bundle.join("manifest.json"), manifest.to_string()).unwrap();
        (default, personal)
    }

    #[test]
    fn a_profile_lists_its_desktop_sessions_wherever_their_transcripts_are() {
        let root = tempdir().unwrap();
        let (default, personal) = two_homes(root.path());
        let homes = [default, personal.clone()];

        let list = claude_sessions(&personal, &homes, PS_OUTPUT);

        let scratch = personal.gui_data_dir.join("scratch").join("abc");
        assert_eq!(
            list,
            SessionList {
                sessions: vec![
                    Session {
                        title: Some("Restarted".to_string()),
                        needs_repair: true,
                        ..session(
                            "restarted-before",
                            SessionKind::Desktop,
                            "2026-09-07T10:00:00Z",
                        )
                    },
                    Session {
                        title: Some("Migration".to_string()),
                        needs_repair: true,
                        ..session("resumed", SessionKind::Desktop, "2026-09-06T10:00:00Z")
                    },
                    Session {
                        title: Some("Try this".to_string()),
                        cwd: Some(scratch.display().to_string()),
                        archived: true,
                        unmovable_reason: Some(IN_SCRATCH_FOLDER.to_string()),
                        ..session("put-away", SessionKind::Desktop, "2026-09-04T10:00:00Z")
                    },
                    Session {
                        title: Some("Fix the login bug".to_string()),
                        needs_repair: true,
                        ..session("orphan", SessionKind::Desktop, "2026-09-03T11:00:00Z")
                    },
                    Session {
                        title: Some("Old chat".to_string()),
                        state: SessionState::TranscriptMissing,
                        unmovable_reason: Some(TRANSCRIPT_DELETED.to_string()),
                        ..session("gone", SessionKind::Desktop, "2026-08-01T10:00:00Z")
                    },
                ],
                repair_count: 3,
            }
        );
    }

    #[test]
    fn the_stock_install_lists_the_sessions_no_profile_claims() {
        let root = tempdir().unwrap();
        let (default, personal) = two_homes(root.path());
        let homes = [default.clone(), personal];

        let list = claude_sessions(&default, &homes, PS_OUTPUT);

        assert_eq!(
            list,
            SessionList {
                sessions: vec![
                    Session {
                        title: Some("Plan the release".to_string()),
                        state: SessionState::OpenInTerminal,
                        unmovable_reason: Some(OPEN_IN_TERMINAL.to_string()),
                        ..session("busy", SessionKind::Cli, "2026-09-02T10:00:00Z")
                    },
                    Session {
                        title: Some("Review the PR".to_string()),
                        last_prompt: Some("Looks good?".to_string()),
                        ..session("cli", SessionKind::Cli, "2026-09-01T10:00:00Z")
                    },
                    Session {
                        title: Some("Shelved idea".to_string()),
                        archived: true,
                        ..session("shelved", SessionKind::Cli, "2026-07-01T10:00:00Z")
                    },
                ],
                repair_count: 0,
            }
        );
    }

    #[test]
    fn a_markup_only_first_prompt_leaves_the_session_untitled() {
        let root = tempdir().unwrap();
        let default = home(root.path(), "default", true);
        write_transcript(
            &default,
            "cmd",
            &[user(
                "cmd",
                "2026-09-01T10:00:00Z",
                "/work/app",
                "<command-name>/clear</command-name>",
            )],
        );

        let list = claude_sessions(&default, std::slice::from_ref(&default), "");

        assert_eq!(list.sessions[0].title, None);
    }

    #[test]
    fn sessions_serialize_in_camel_case() {
        let value = serde_json::to_value(SessionList {
            sessions: vec![Session {
                state: SessionState::OpenInTerminal,
                ..session("s", SessionKind::Desktop, "2026-09-01T10:00:00Z")
            }],
            repair_count: 0,
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "sessions": [{
                    "id": "s",
                    "kind": "desktop",
                    "title": null,
                    "cwd": "/work/app",
                    "lastPrompt": null,
                    "lastUsedAt": "2026-09-01T10:00:00Z",
                    "archived": false,
                    "state": "openInTerminal",
                    "needsRepair": false,
                    "unmovableReason": null,
                }],
                "repairCount": 0,
            })
        );
    }
}
