//! Archiving and restoring a session: checked, confirmed, then done.
//!
//! An action is first checked, which tells the user what stands in its way
//! before they confirm: a reason only they can clear (the session is open in
//! a terminal), or the desktop app that has to quit first, which the app can
//! do for them. Doing the action checks again, as things may have changed
//! since, quits the app if the user agreed to, and checks once more that it
//! is gone before anything is written.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::claude::archive as claude_archive;
use super::home::{home_for, homes_of};
use super::instance::{desktop_label, quit_desktop, QUIT_TIMEOUT};
use super::Home;
use crate::app_kind::AppKind;
use crate::error::{AppError, AppResult};
use crate::launch::process_list;

/// What can be done to a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionAction {
    /// Put it away in the Archived tab.
    Archive,
    /// Bring it back from the Archived tab.
    Restore,
}

/// A desktop app instance that has to quit before an action can be done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppToQuit {
    /// The home whose instance it is.
    pub home_id: String,
    /// How the instance is named to the user: `Claude (Work)`.
    pub label: String,
}

impl AppToQuit {
    /// `home`'s desktop app instance.
    pub fn of(home: &Home) -> Self {
        AppToQuit {
            home_id: home.id.clone(),
            label: desktop_label(home),
        }
    }
}

/// What stands between a session and an action.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionCheck {
    /// Why the action can't be done, when only the user can change that.
    pub blocker: Option<String>,
    /// The desktop app that has to quit first, when it holds files the
    /// action writes and is running.
    pub app_to_quit: Option<AppToQuit>,
}

/// A check, with what the action would be done to.
#[derive(Debug)]
pub struct Checked<T> {
    /// What stands in the action's way.
    pub check: ActionCheck,
    /// What the action would be done to.
    pub target: T,
}

/// What stands between session `session_id` of profile `profile_id` (or
/// `default:<app>`) and `action`.
pub fn check(profile_id: &str, session_id: &str, action: SessionAction) -> AppResult<ActionCheck> {
    let home = home_for(profile_id)?;
    match home.app {
        AppKind::Claude => {
            let homes = homes_of(AppKind::Claude)?;
            let checked =
                claude_archive::check(&home, &homes, session_id, action, &process_list()?)?;
            Ok(checked.check)
        }
        AppKind::Codex => Err(codex_unsupported()),
    }
}

/// Archive session `session_id` of profile `profile_id` (or `default:<app>`),
/// quitting the desktop app in the way if `quit_app`.
pub fn archive(profile_id: &str, session_id: &str, quit_app: bool) -> AppResult<()> {
    run(profile_id, session_id, SessionAction::Archive, quit_app)
}

/// Restore archived session `session_id` of profile `profile_id` (or
/// `default:<app>`), quitting the desktop app in the way if `quit_app`.
pub fn restore(profile_id: &str, session_id: &str, quit_app: bool) -> AppResult<()> {
    run(profile_id, session_id, SessionAction::Restore, quit_app)
}

/// Do `action` to session `session_id` of profile `profile_id` (or
/// `default:<app>`), quitting the desktop app in its way if `quit_app`, and
/// refusing if it is in the way otherwise.
fn run(profile_id: &str, session_id: &str, action: SessionAction, quit_app: bool) -> AppResult<()> {
    let home = home_for(profile_id)?;
    match home.app {
        AppKind::Claude => {
            let homes = homes_of(AppKind::Claude)?;
            run_claude(&home, &homes, session_id, action, quit_app, QUIT_TIMEOUT)
        }
        AppKind::Codex => Err(codex_unsupported()),
    }
}

/// The error for an action on a Codex session, which isn't offered yet.
fn codex_unsupported() -> AppError {
    AppError::Validation("Codex sessions can't be archived or restored yet".to_string())
}

/// [`run`] for a Claude session of `home`, one of `homes`, giving its
/// desktop app `quit_timeout` to quit.
fn run_claude(
    home: &Home,
    homes: &[Home],
    session_id: &str,
    action: SessionAction,
    quit_app: bool,
    quit_timeout: Duration,
) -> AppResult<()> {
    run_checked(
        quit_app,
        || claude_archive::check(home, homes, session_id, action, &process_list()?),
        || quit_desktop(home, quit_timeout),
        |target| claude_archive::apply(home, target, action),
    )
}

/// Do an action once `check` allows it: refuse a blocked one; when a desktop
/// app is in the way, refuse unless `quit_app`, else `quit` it and check
/// again, refusing if it still runs; then `apply` the action to what the last
/// check found.
fn run_checked<T>(
    quit_app: bool,
    mut check: impl FnMut() -> AppResult<Checked<T>>,
    quit: impl FnOnce() -> AppResult<()>,
    apply: impl FnOnce(T) -> AppResult<()>,
) -> AppResult<()> {
    let mut checked = check()?;
    refuse_blocked(&checked.check)?;
    if let Some(app) = &checked.check.app_to_quit {
        if !quit_app {
            return Err(AppError::Validation(format!("Quit {} first", app.label)));
        }
        quit()?;
        checked = check()?;
        refuse_blocked(&checked.check)?;
        if let Some(app) = &checked.check.app_to_quit {
            return Err(AppError::Validation(format!(
                "{} is still running",
                app.label
            )));
        }
    }
    apply(checked.target)
}

/// The blocker `check` names, as an error.
fn refuse_blocked(check: &ActionCheck) -> AppResult<()> {
    match &check.blocker {
        Some(blocker) => Err(AppError::Validation(blocker.clone())),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::path::Path;
    use std::process::{Child, ChildStdin};
    use std::thread::{self, JoinHandle};

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;
    use crate::sessions::instance::running_desktop_pid;
    use crate::test_support::{fake_wrapper_process, stubborn_wrapper_process};

    /// A check that finds `target`, blocked by `blocker`, with `app` in the
    /// way.
    fn checked(blocker: Option<&str>, app: Option<&str>, target: u8) -> Checked<u8> {
        Checked {
            check: ActionCheck {
                blocker: blocker.map(str::to_string),
                app_to_quit: app.map(|label| AppToQuit {
                    home_id: "work".to_string(),
                    label: label.to_string(),
                }),
            },
            target,
        }
    }

    /// Runs an action whose checks return `checks` in turn, logging each
    /// step taken.
    fn run_logged(quit_app: bool, checks: Vec<Checked<u8>>) -> (AppResult<()>, Vec<String>) {
        let log = RefCell::new(Vec::new());
        let mut checks = checks.into_iter();
        let result = run_checked(
            quit_app,
            || {
                log.borrow_mut().push("check".to_string());
                Ok(checks.next().unwrap())
            },
            || {
                log.borrow_mut().push("quit".to_string());
                Ok(())
            },
            |target| {
                log.borrow_mut().push(format!("apply {target}"));
                Ok(())
            },
        );
        (result, log.into_inner())
    }

    #[test]
    fn an_action_nothing_stands_in_the_way_of_is_applied_to_what_was_checked() {
        let (result, log) = run_logged(false, vec![checked(None, None, 1)]);

        result.unwrap();
        assert_eq!(log, ["check", "apply 1"]);
    }

    #[test]
    fn a_blocked_action_is_refused_with_its_reason() {
        let (result, log) = run_logged(
            true,
            vec![checked(
                Some("Close it in the terminal first"),
                Some("Claude (Work)"),
                1,
            )],
        );

        assert!(
            matches!(&result, Err(AppError::Validation(message)) if message == "Close it in the terminal first")
        );
        assert_eq!(log, ["check"]);
    }

    #[test]
    fn a_desktop_app_in_the_way_is_only_quit_when_the_user_agreed() {
        let (result, log) = run_logged(false, vec![checked(None, Some("Claude (Work)"), 1)]);

        assert!(
            matches!(&result, Err(AppError::Validation(message)) if message == "Quit Claude (Work) first")
        );
        assert_eq!(log, ["check"]);

        let (result, log) = run_logged(
            true,
            vec![
                checked(None, Some("Claude (Work)"), 1),
                checked(None, None, 2),
            ],
        );

        result.unwrap();
        assert_eq!(log, ["check", "quit", "check", "apply 2"]);
    }

    #[test]
    fn nothing_is_applied_while_the_desktop_app_still_runs_after_quitting() {
        let (result, log) = run_logged(
            true,
            vec![
                checked(None, Some("Claude (Work)"), 1),
                checked(None, Some("Claude (Work)"), 1),
            ],
        );

        assert!(
            matches!(&result, Err(AppError::Validation(message)) if message == "Claude (Work) is still running")
        );
        assert_eq!(log, ["check", "quit", "check"]);
    }

    #[test]
    fn checks_and_actions_cross_the_bridge_in_camel_case() {
        let action: SessionAction = serde_json::from_value(json!("restore")).unwrap();
        let check = serde_json::to_value(ActionCheck {
            blocker: None,
            app_to_quit: Some(AppToQuit {
                home_id: "work".to_string(),
                label: "Claude (Work)".to_string(),
            }),
        })
        .unwrap();

        assert_eq!(action, SessionAction::Restore);
        assert_eq!(
            check,
            json!({ "blocker": null, "appToQuit": { "homeId": "work", "label": "Claude (Work)" } })
        );
    }

    /// A managed Claude home named `name`, under `root`.
    fn claude_home(root: &Path, name: &str) -> Home {
        Home {
            id: name.to_string(),
            app: AppKind::Claude,
            label: name.to_string(),
            config_dir: root.join(name).join("cli-config"),
            gui_data_dir: root.join(name).join("gui-data"),
            stock: false,
        }
    }

    /// Gives `home` a desktop session `s`: a transcript and a record of it.
    /// Returns the record's path.
    fn desktop_session(home: &Home) -> std::path::PathBuf {
        let projects = home.config_dir.join("projects").join("-work-app");
        fs::create_dir_all(&projects).unwrap();
        let line = json!({
            "type": "user",
            "sessionId": "s",
            "timestamp": "2026-09-01T10:00:00Z",
            "cwd": "/work/app",
            "message": { "role": "user", "content": "Fix the bug" },
        });
        fs::write(projects.join("s.jsonl"), format!("{line}\n")).unwrap();
        let org = home
            .gui_data_dir
            .join("claude-code-sessions")
            .join("account")
            .join("org");
        fs::create_dir_all(&org).unwrap();
        let record = org.join("local_r1.json");
        fs::write(
            &record,
            json!({ "cliSessionId": "s", "model": "m" }).to_string(),
        )
        .unwrap();
        record
    }

    /// The record at `path`, as JSON.
    fn read_value(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    /// Reap `child` on another thread once it exits, as launchd reaps a real
    /// app, keeping its stdin open, as the stand-in exits when that closes.
    fn reaped(mut child: Child) -> (JoinHandle<bool>, Option<ChildStdin>) {
        let stdin = child.stdin.take();
        (thread::spawn(move || child.wait().is_ok()), stdin)
    }

    #[test]
    fn archiving_a_desktop_session_quits_its_desktop_app_first() {
        let root = tempdir().unwrap();
        let work = claude_home(root.path(), "Work");
        let record = desktop_session(&work);
        let homes = [work.clone()];
        let (reaper, stdin) = reaped(fake_wrapper_process(
            &root.path().join("app"),
            &work.gui_data_dir,
        ));

        let refused = run_claude(
            &work,
            &homes,
            "s",
            SessionAction::Archive,
            false,
            QUIT_TIMEOUT,
        );
        let untouched = read_value(&record);
        let archived = run_claude(
            &work,
            &homes,
            "s",
            SessionAction::Archive,
            true,
            QUIT_TIMEOUT,
        );
        let quit = running_desktop_pid(&work).unwrap().is_none();
        // Closing its stdin ends the stand-in whatever happened, so the test
        // never waits on it.
        drop(stdin);
        reaper.join().unwrap();

        archived.unwrap();
        assert!(quit, "the desktop app still runs");
        assert!(
            matches!(&refused, Err(AppError::Validation(message)) if message == "Quit Claude (Work) first")
        );
        assert_eq!(untouched, json!({ "cliSessionId": "s", "model": "m" }));
        assert_eq!(
            read_value(&record),
            json!({ "cliSessionId": "s", "model": "m", "isArchived": true })
        );
        assert!(work.config_dir.join("projects/-work-app/s.jsonl").exists());
    }

    #[test]
    fn nothing_is_written_when_the_desktop_app_wont_quit() {
        let root = tempdir().unwrap();
        let work = claude_home(root.path(), "Work");
        let record = desktop_session(&work);
        let mut stubborn = stubborn_wrapper_process(&root.path().join("app"), &work.gui_data_dir);

        let archived = run_claude(
            &work,
            std::slice::from_ref(&work),
            "s",
            SessionAction::Archive,
            true,
            Duration::from_millis(600),
        );

        stubborn.kill().unwrap();
        stubborn.wait().unwrap();
        assert!(
            matches!(&archived, Err(AppError::Validation(message)) if message == "Claude (Work) didn't quit"),
            "{archived:?}"
        );
        assert_eq!(
            read_value(&record),
            json!({ "cliSessionId": "s", "model": "m" })
        );
        assert!(!record.with_file_name("archived-sessions.idx").exists());
    }
}
