//! Archiving and restoring Claude sessions.
//!
//! A session the desktop app has a record of is archived as the app does it,
//! by flagging the record, so the app's own Archived view agrees; its
//! transcripts stay where they are. One without a record is archived by
//! moving its bundle into the config dir's archive (see [`super::archive_store`]).
//! The session is looked up afresh from disk for every check, so nothing the
//! frontend holds decides what is written.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;

use super::archive_store::{
    archive_bundle, archived_bundles, has_active_copy, restore_bundle, ArchivedBundle, ACTIVE_COPY,
};
use super::desktop::{set_archived, DesktopRecord};
use super::live::LiveHolder;
use super::ownership::{owned_by, HomeScan};
use super::transcript::bundle_paths;
use crate::error::{AppError, AppResult};
use crate::sessions::actions::{ActionCheck, AppToQuit, Checked, SessionAction};
use crate::sessions::instance::{desktop_pid, running_again, running_desktop_pid};
use crate::sessions::list::{home_scans, live_anywhere, transcript_title, OPEN_IN_TERMINAL};
use crate::sessions::Home;

/// Why a session can't be archived again.
const ALREADY_ARCHIVED: &str = "It's already archived";

/// Why a session that isn't archived can't be restored.
const NOT_ARCHIVED: &str = "It isn't archived";

/// What an action on a Claude session is done to.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// The desktop app's record of the session.
    Record(DesktopRecord),
    /// A session without a record, active: its bundle, to be archived.
    Bundle {
        /// What the archive's manifest says of the session.
        described: ArchivedBundle,
        /// The session's files and folders in the home's config dir.
        paths: Vec<PathBuf>,
    },
    /// A session without a record that ai-profiles archived.
    Archived {
        /// The session's id.
        session_id: String,
    },
}

/// What stands between session `session_id` of `home`, one of `homes`, and
/// `action`, given the output of `ps -ax -o pid=,command=`, with what the
/// action would be done to.
///
/// The session is blocked while a terminal has any of its transcripts open.
/// Its desktop app has to quit first when the action writes the session's
/// record and the app runs.
pub fn check(
    home: &Home,
    homes: &[Home],
    session_id: &str,
    action: SessionAction,
    ps_output: &str,
) -> AppResult<Checked<Target>> {
    let live = live_anywhere(homes, ps_output);
    check_scanned(
        home,
        &home_scans(homes),
        &live,
        session_id,
        action,
        ps_output,
    )
}

/// [`check`], given what every home of the app holds, `scans`, and the
/// sessions `live` in any of them, as a move that archives the session read
/// them already.
pub(super) fn check_scanned(
    home: &Home,
    scans: &[HomeScan],
    live: &HashMap<String, LiveHolder>,
    session_id: &str,
    action: SessionAction,
    ps_output: &str,
) -> AppResult<Checked<Target>> {
    let owned = owned_by(&home.id, scans)
        .into_iter()
        .find(|owned| owned.session_id == session_id);
    let (target, transcript_ids) = match owned {
        Some(owned) => {
            let ids: Vec<String> = std::iter::once(owned.session_id.clone())
                .chain(
                    owned
                        .claimed_transcripts
                        .iter()
                        .map(|held| held.summary.session_id.clone()),
                )
                .collect();
            let target = match (owned.record, owned.transcript) {
                (Some(record), _) => Target::Record(record),
                (None, Some(held)) => Target::Bundle {
                    described: ArchivedBundle {
                        session_id: owned.session_id.clone(),
                        title: transcript_title(&held.summary),
                        cwd: held.summary.cwd.clone(),
                        last_prompt: held.summary.last_prompt.clone(),
                        last_used_at: held.summary.last_used_at,
                    },
                    paths: bundle_paths(&home.config_dir, &held.summary),
                },
                (None, None) => return Err(not_found(session_id, home)),
            };
            (target, ids)
        }
        None if archived_bundles(&home.config_dir)
            .iter()
            .any(|bundle| bundle.session_id == session_id) =>
        {
            let target = Target::Archived {
                session_id: session_id.to_string(),
            };
            (target, Vec::new())
        }
        None => return Err(not_found(session_id, home)),
    };
    let open_in_terminal = transcript_ids
        .iter()
        .any(|id| live.get(id) == Some(&LiveHolder::Terminal));
    let blocker = match (action, &target) {
        _ if open_in_terminal => Some(OPEN_IN_TERMINAL),
        (SessionAction::Archive, Target::Record(record)) if record.archived => {
            Some(ALREADY_ARCHIVED)
        }
        (SessionAction::Archive, Target::Archived { .. }) => Some(ALREADY_ARCHIVED),
        (SessionAction::Restore, Target::Record(record)) if !record.archived => Some(NOT_ARCHIVED),
        (SessionAction::Restore, Target::Bundle { .. }) => Some(ACTIVE_COPY),
        (SessionAction::Restore, Target::Archived { session_id })
            if has_active_copy(&home.config_dir, session_id) =>
        {
            Some(ACTIVE_COPY)
        }
        _ => None,
    };
    let writes_record = matches!(target, Target::Record(_));
    let app_to_quit =
        (writes_record && desktop_pid(home, ps_output).is_some()).then(|| AppToQuit::of(home));
    Ok(Checked {
        check: ActionCheck {
            blocker: blocker.map(str::to_string),
            app_to_quit,
        },
        target,
    })
}

/// Do `action` to `target`, a session of `home` that [`check`] let through.
///
/// The desktop app writes its records back from memory, so one that started
/// again since the check is looked for right before a record is written, and
/// the action refused if it runs.
pub fn apply(home: &Home, target: Target, action: SessionAction) -> AppResult<()> {
    match target {
        Target::Record(record) => {
            if running_desktop_pid(home)?.is_some() {
                return Err(running_again(home));
            }
            set_archived(&record, action == SessionAction::Archive)
        }
        Target::Bundle { described, paths } => {
            archive_bundle(&home.config_dir, &described, &paths, Utc::now()).map(drop)
        }
        Target::Archived { session_id } => restore_bundle(&home.config_dir, &session_id),
    }
}

/// The error for a session `home` doesn't own.
fn not_found(session_id: &str, home: &Home) -> AppError {
    AppError::NotFound(format!("session {session_id} not found in {}", home.label))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;
    use crate::app_kind::AppKind;
    use crate::test_support::fake_wrapper_process;

    /// A managed Claude home named `name`, under `root`.
    fn home(root: &Path, name: &str) -> Home {
        Home {
            id: name.to_string(),
            app: AppKind::Claude,
            label: name.to_string(),
            config_dir: root.join(name).join("cli-config"),
            gui_data_dir: root.join(name).join("gui-data"),
            stock: false,
        }
    }

    /// Writes transcript `session` into `home`'s config dir, with a file
    /// history.
    fn write_transcript(home: &Home, session: &str) {
        let dir = home.config_dir.join("projects").join("-work-app");
        fs::create_dir_all(&dir).unwrap();
        let line = json!({
            "type": "user",
            "sessionId": session,
            "timestamp": "2026-09-01T10:00:00Z",
            "cwd": "/work/app",
            "message": { "role": "user", "content": "Fix the login bug" },
        });
        fs::write(dir.join(format!("{session}.jsonl")), format!("{line}\n")).unwrap();
        let history = home.config_dir.join("file-history").join(session);
        fs::create_dir_all(&history).unwrap();
        fs::write(history.join("abc@v1"), "old").unwrap();
    }

    /// Writes `home`'s desktop record `local` of `fields`. Returns its path.
    fn write_record(home: &Home, local: &str, fields: &Value) -> PathBuf {
        let dir = home
            .gui_data_dir
            .join("claude-code-sessions")
            .join("account")
            .join("org");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("local_{local}.json"));
        fs::write(&path, fields.to_string()).unwrap();
        path
    }

    /// Registers session `session` as open in `home` by process `pid`.
    fn write_registry(home: &Home, pid: i32, session: &str, entrypoint: &str) {
        let dir = home.config_dir.join("sessions");
        fs::create_dir_all(&dir).unwrap();
        let entry = json!({ "pid": pid, "sessionId": session, "entrypoint": entrypoint });
        fs::write(dir.join(format!("{pid}.json")), entry.to_string()).unwrap();
    }

    /// `ps` output with `home`'s desktop app running.
    fn desktop_running(home: &Home) -> String {
        format!(
            "  900 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
            home.gui_data_dir.display()
        )
    }

    /// Checks, then applies, `action` on session `session_id` of `home`.
    fn act(home: &Home, homes: &[Home], session_id: &str, action: SessionAction) {
        let checked = check(home, homes, session_id, action, "").unwrap();
        assert_eq!(checked.check, ActionCheck::default());
        apply(home, checked.target, action).unwrap();
    }

    /// The record at `path`, as JSON.
    fn read_value(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn a_desktop_session_is_archived_by_its_record_and_its_transcript_stays() {
        let root = tempdir().unwrap();
        let default = home(root.path(), "Default");
        let work = home(root.path(), "Work");
        write_transcript(&default, "s");
        let record = write_record(&work, "r1", &json!({ "cliSessionId": "s" }));
        let homes = [default.clone(), work.clone()];

        let checked = check(
            &work,
            &homes,
            "s",
            SessionAction::Archive,
            &desktop_running(&work),
        )
        .unwrap();

        assert_eq!(
            checked.check,
            ActionCheck {
                blocker: None,
                app_to_quit: Some(AppToQuit {
                    home_id: "Work".to_string(),
                    label: "Claude (Work)".to_string(),
                }),
            }
        );

        act(&work, &homes, "s", SessionAction::Archive);

        assert_eq!(read_value(&record)["isArchived"], json!(true));
        assert!(default
            .config_dir
            .join("projects/-work-app/s.jsonl")
            .exists());

        act(&work, &homes, "s", SessionAction::Restore);

        assert_eq!(read_value(&record)["isArchived"], json!(false));
    }

    #[test]
    fn a_record_is_not_written_while_the_desktop_app_that_started_again_runs() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        write_transcript(&work, "s");
        let record = write_record(&work, "r1", &json!({ "cliSessionId": "s" }));
        let checked = check(
            &work,
            std::slice::from_ref(&work),
            "s",
            SessionAction::Archive,
            "",
        )
        .unwrap();
        assert_eq!(checked.check, ActionCheck::default());
        let mut relaunched = fake_wrapper_process(&root.path().join("app"), &work.gui_data_dir);

        let applied = apply(&work, checked.target, SessionAction::Archive);

        relaunched.kill().unwrap();
        relaunched.wait().unwrap();
        assert!(
            matches!(&applied, Err(AppError::Validation(message)) if message == "Claude (Work) is running again — quit it and try again"),
            "{applied:?}"
        );
        assert_eq!(read_value(&record), json!({ "cliSessionId": "s" }));
        assert!(!record.with_file_name("archived-sessions.idx").exists());
    }

    #[test]
    fn a_desktop_session_whose_transcript_is_gone_can_be_archived() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        let record = write_record(&work, "r1", &json!({ "cliSessionId": "gone" }));

        act(
            &work,
            std::slice::from_ref(&work),
            "gone",
            SessionAction::Archive,
        );

        assert_eq!(read_value(&record)["isArchived"], json!(true));
    }

    #[test]
    fn a_cli_session_is_archived_and_restored_as_a_bundle_without_the_desktop_app_quitting() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        write_transcript(&work, "s");
        let homes = [work.clone()];

        let checked = check(
            &work,
            &homes,
            "s",
            SessionAction::Archive,
            &desktop_running(&work),
        )
        .unwrap();

        assert_eq!(checked.check, ActionCheck::default());

        act(&work, &homes, "s", SessionAction::Archive);

        assert!(!work.config_dir.join("projects/-work-app/s.jsonl").exists());
        let listed = archived_bundles(&work.config_dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title.as_deref(), Some("Fix the login bug"));

        act(&work, &homes, "s", SessionAction::Restore);

        assert!(work.config_dir.join("projects/-work-app/s.jsonl").exists());
        assert!(work.config_dir.join("file-history/s/abc@v1").exists());
        assert_eq!(archived_bundles(&work.config_dir), []);
    }

    #[test]
    fn a_session_open_in_a_terminal_is_blocked() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        write_transcript(&work, "cli");
        write_transcript(&work, "earlier");
        write_record(
            &work,
            "r1",
            &json!({ "cliSessionId": "now", "priorCliSessionIds": ["earlier"] }),
        );
        write_registry(&work, 4100, "cli", "cli");
        write_registry(&work, 4200, "earlier", "cli");
        let homes = [work.clone()];
        let ps_output = "  4100 claude\n  4200 claude --resume\n";

        for session_id in ["cli", "earlier"] {
            let checked =
                check(&work, &homes, session_id, SessionAction::Archive, ps_output).unwrap();

            assert_eq!(checked.check.blocker.as_deref(), Some(OPEN_IN_TERMINAL));
        }
    }

    #[test]
    fn a_session_is_not_restored_while_an_active_copy_is_here() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        write_transcript(&work, "s");
        let homes = [work.clone()];
        act(&work, &homes, "s", SessionAction::Archive);
        write_transcript(&work, "s");

        let checked = check(&work, &homes, "s", SessionAction::Restore, "").unwrap();

        assert_eq!(checked.check.blocker.as_deref(), Some(ACTIVE_COPY));

        fs::remove_file(work.config_dir.join("projects/-work-app/s.jsonl")).unwrap();
        let checked = check(&work, &homes, "s", SessionAction::Restore, "").unwrap();

        assert_eq!(checked.check.blocker.as_deref(), Some(ACTIVE_COPY));
    }

    #[test]
    fn an_action_that_is_already_done_is_blocked() {
        let root = tempdir().unwrap();
        let work = home(root.path(), "Work");
        write_transcript(&work, "cli");
        write_record(
            &work,
            "r1",
            &json!({ "cliSessionId": "put-away", "isArchived": true }),
        );
        write_record(&work, "r2", &json!({ "cliSessionId": "active" }));
        let homes = [work.clone()];
        let blocker = |session_id, action| {
            check(&work, &homes, session_id, action, "")
                .unwrap()
                .check
                .blocker
        };

        assert_eq!(
            blocker("put-away", SessionAction::Archive).as_deref(),
            Some(ALREADY_ARCHIVED)
        );
        assert_eq!(
            blocker("active", SessionAction::Restore).as_deref(),
            Some(NOT_ARCHIVED)
        );
        assert_eq!(
            blocker("cli", SessionAction::Restore).as_deref(),
            Some(ACTIVE_COPY)
        );
    }

    #[test]
    fn a_session_the_home_doesnt_own_is_not_found() {
        let root = tempdir().unwrap();
        let default = home(root.path(), "Default");
        let work = home(root.path(), "Work");
        write_transcript(&default, "theirs");
        let homes = [default, work.clone()];

        assert!(matches!(
            check(&work, &homes, "theirs", SessionAction::Archive, ""),
            Err(AppError::NotFound(_))
        ));
    }
}
