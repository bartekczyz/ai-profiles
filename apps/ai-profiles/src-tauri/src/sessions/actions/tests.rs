use std::cell::RefCell;
use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::*;
use crate::sessions::instance::running_desktop_pid;
use crate::test_support::{
    claude_home, fake_wrapper_process, read_value, reaped, stubborn_wrapper_process,
};

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
async fn run_logged(quit_app: bool, checks: Vec<Checked<u8>>) -> (AppResult<()>, Vec<String>) {
    let log = RefCell::new(Vec::new());
    let mut checks = checks.into_iter();
    let result = run_checked(
        quit_app,
        || {
            log.borrow_mut().push("check".to_string());
            let next = checks.next().unwrap();
            async move { Ok(next) }
        },
        |_| {
            log.borrow_mut().push("quit".to_string());
            async { Ok(()) }
        },
        |target| {
            log.borrow_mut().push(format!("apply {target}"));
            async { Ok(()) }
        },
    )
    .await;
    (result, log.into_inner())
}

#[tokio::test]
async fn an_action_nothing_stands_in_the_way_of_is_applied_to_what_was_checked() {
    let (result, log) = run_logged(false, vec![checked(None, None, 1)]).await;

    result.unwrap();
    assert_eq!(log, ["check", "apply 1"]);
}

#[tokio::test]
async fn a_blocked_action_is_refused_with_its_reason() {
    let (result, log) = run_logged(
        true,
        vec![checked(
            Some("Close it in the terminal first"),
            Some("Claude (Work)"),
            1,
        )],
    )
    .await;

    assert!(
        matches!(&result, Err(AppError::Validation(message)) if message == "Close it in the terminal first")
    );
    assert_eq!(log, ["check"]);
}

#[tokio::test]
async fn a_desktop_app_in_the_way_is_only_quit_when_the_user_agreed() {
    let (result, log) = run_logged(false, vec![checked(None, Some("Claude (Work)"), 1)]).await;

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
    )
    .await;

    result.unwrap();
    assert_eq!(log, ["check", "quit", "check", "apply 2"]);
}

/// A check's target that logs when it is let go.
struct Held<'a> {
    /// Which check found it.
    name: &'static str,
    /// The log it writes to.
    log: &'a RefCell<Vec<String>>,
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("drop {}", self.name));
    }
}

#[tokio::test]
async fn what_the_first_check_found_is_let_go_before_the_apps_quit() {
    let log = RefCell::new(Vec::new());
    let mut names = ["first", "second"].into_iter();

    run_checked(
        true,
        || {
            let name = names.next().unwrap();
            log.borrow_mut().push(format!("check {name}"));
            let app = (name == "first").then_some("Claude (Work)");
            let checked = Checked {
                check: checked(None, app, 0).check,
                target: Held { name, log: &log },
            };
            async move { Ok(checked) }
        },
        |_| {
            log.borrow_mut().push("quit".to_string());
            async { Ok(()) }
        },
        |target| {
            log.borrow_mut().push(format!("apply {}", target.name));
            async { Ok(()) }
        },
    )
    .await
    .unwrap();

    assert_eq!(
        log.into_inner(),
        [
            "check first",
            "drop first",
            "quit",
            "check second",
            "apply second",
            "drop second"
        ]
    );
}

#[tokio::test]
async fn nothing_is_applied_while_the_desktop_app_still_runs_after_quitting() {
    let (result, log) = run_logged(
        true,
        vec![
            checked(None, Some("Claude (Work)"), 1),
            checked(None, Some("Claude (Work)"), 1),
        ],
    )
    .await;

    assert!(
        matches!(&result, Err(AppError::Validation(message)) if message == "Claude (Work) is still running")
    );
    assert_eq!(log, ["check", "quit", "check"]);
}

#[tokio::test]
async fn a_blocker_found_once_the_app_quit_says_it_quit() {
    let (result, log) = run_logged(
        true,
        vec![
            checked(None, Some("Claude (Work)"), 1),
            checked(Some("Close it in the terminal first"), None, 1),
        ],
    )
    .await;

    assert!(
        matches!(&result, Err(AppError::Validation(message)) if message == "Quit Claude (Work), but it still can't be done: Close it in the terminal first"),
        "{result:?}"
    );
    assert_eq!(log, ["check", "quit", "check"]);
}

#[test]
fn a_moves_blockers_read_as_the_dialog_shows_them() {
    let root = tempdir().unwrap();
    let personal = claude_home(root.path(), "Personal");
    let plan = MovePlan {
        summary: String::new(),
        items: Vec::new(),
        destination_newer: false,
        desktop: crate::sessions::move_plan::DesktopAction::NoDesktop,
        blockers: vec![
            "Lives in the desktop app's scratch folder".to_string(),
            "Close it in the terminal first".to_string(),
        ],
        apps_to_quit: Vec::new(),
        notes: Vec::new(),
    };

    let gate = MoveGate::of(&plan, &personal, false);

    assert_eq!(
        gate.blocker.as_deref(),
        Some("Lives in the desktop app's scratch folder. Close it in the terminal first")
    );
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

#[tokio::test]
async fn archiving_a_desktop_session_quits_its_desktop_app_first() {
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
    )
    .await;
    let untouched = read_value(&record);
    let archived = run_claude(
        &work,
        &homes,
        "s",
        SessionAction::Archive,
        true,
        QUIT_TIMEOUT,
    )
    .await;
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

#[tokio::test]
async fn nothing_is_written_when_the_desktop_app_wont_quit() {
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
    )
    .await;

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

#[tokio::test]
async fn every_desktop_app_in_the_way_is_named_and_quit() {
    let two = |target| Checked {
        check: MoveGate {
            blocker: None,
            apps_to_quit: ["Claude (Work)", "Claude (Personal)"]
                .iter()
                .map(|label| AppToQuit {
                    home_id: label.to_string(),
                    label: label.to_string(),
                })
                .collect(),
        },
        target,
    };
    let quit = RefCell::new(Vec::new());

    let refused = run_checked(
        false,
        || async { Ok(two(1)) },
        |_| async { Ok(()) },
        |_| async { Ok(()) },
    )
    .await;
    let mut checks = vec![two(1), two(2)].into_iter();
    let still = run_checked(
        true,
        || {
            let next = checks.next().unwrap();
            async move { Ok(next) }
        },
        |apps: Vec<AppToQuit>| {
            quit.borrow_mut()
                .extend(apps.into_iter().map(|app| app.label));
            async { Ok(()) }
        },
        |_| async { Ok(()) },
    )
    .await;

    assert!(
        matches!(&refused, Err(AppError::Validation(message)) if message == "Quit Claude (Work) and Claude (Personal) first")
    );
    assert!(
        matches!(&still, Err(AppError::Validation(message)) if message == "Claude (Work) and Claude (Personal) are still running")
    );
    assert_eq!(quit.into_inner(), ["Claude (Work)", "Claude (Personal)"]);
}

/// Gives `home` transcript `s`, last used at `timestamp`.
fn cli_session(home: &Home, timestamp: &str) {
    let projects = home.config_dir.join("projects").join("-work-app");
    fs::create_dir_all(&projects).unwrap();
    let line = json!({
        "type": "user",
        "sessionId": "s",
        "timestamp": timestamp,
        "cwd": "/work/app",
        "message": { "role": "user", "content": "Fix the bug" },
    });
    fs::write(projects.join("s.jsonl"), format!("{line}\n")).unwrap();
}

/// A move of session `s` from `source` to `destination`.
fn move_request(source: &Home, destination: &Home, replace_newer: bool) -> MoveRequest {
    MoveRequest {
        source: source.clone(),
        destination: destination.clone(),
        homes: vec![source.clone(), destination.clone()],
        session_id: "s".to_string(),
        replace_newer,
    }
}

#[tokio::test]
async fn a_newer_copy_at_the_destination_is_replaced_only_when_the_user_agreed() {
    let root = tempdir().unwrap();
    let work = claude_home(root.path(), "Work");
    let personal = claude_home(root.path(), "Personal");
    cli_session(&work, "2026-09-01T10:00:00Z");
    cli_session(&personal, "2026-09-02T10:00:00Z");
    let copy = personal.config_dir.join("projects/-work-app/s.jsonl");
    let newer = fs::read_to_string(&copy).unwrap();

    let refused = run_move(move_request(&work, &personal, false), false, QUIT_TIMEOUT).await;

    assert!(
        matches!(&refused, Err(AppError::Validation(message)) if message == "Personal has a newer copy of this session"),
        "{refused:?}"
    );
    assert_eq!(fs::read_to_string(&copy).unwrap(), newer);

    run_move(move_request(&work, &personal, true), false, QUIT_TIMEOUT)
        .await
        .unwrap();

    assert!(fs::read_to_string(&copy).unwrap().contains("2026-09-01"));
    assert!(!work.config_dir.join("projects/-work-app/s.jsonl").exists());
}

#[tokio::test]
async fn moving_to_a_running_desktop_app_quits_it_first() {
    let root = tempdir().unwrap();
    let work = claude_home(root.path(), "Work");
    let personal = claude_home(root.path(), "Personal");
    cli_session(&work, "2026-09-01T10:00:00Z");
    let org = personal
        .gui_data_dir
        .join("claude-code-sessions")
        .join("account")
        .join("org");
    fs::create_dir_all(&org).unwrap();
    fs::write(
        personal.gui_data_dir.join("config.json"),
        json!({ "lastKnownAccountUuid": "account" }).to_string(),
    )
    .unwrap();
    let (reaper, stdin) = reaped(fake_wrapper_process(
        &root.path().join("app"),
        &personal.gui_data_dir,
    ));

    let refused = run_move(move_request(&work, &personal, false), false, QUIT_TIMEOUT).await;
    let untouched = fs::read_dir(&org).unwrap().count();
    let moved = run_move(move_request(&work, &personal, false), true, QUIT_TIMEOUT).await;
    let quit = running_desktop_pid(&personal).unwrap().is_none();
    drop(stdin);
    reaper.join().unwrap();

    assert!(
        matches!(&refused, Err(AppError::Validation(message)) if message == "Quit Claude (Personal) first"),
        "{refused:?}"
    );
    assert_eq!(untouched, 0);
    moved.unwrap();
    assert!(quit, "the desktop app still runs");
    assert_eq!(fs::read_dir(&org).unwrap().count(), 1);
}

#[test]
fn a_move_plan_crosses_the_bridge_in_camel_case() {
    let root = tempdir().unwrap();
    let work = claude_home(root.path(), "Work");
    let personal = claude_home(root.path(), "Personal");
    cli_session(&work, "2026-09-01T10:00:00Z");
    let homes = [work.clone(), personal.clone()];

    let plan = transfer::plan(&work, &personal, &homes, "s", "")
        .unwrap()
        .plan;

    assert_eq!(
        serde_json::to_value(&plan).unwrap(),
        json!({
            "summary": "Moves 1 file from Work to Personal",
            "items": [{ "path": "projects/-work-app/s.jsonl", "action": "copy" }],
            "destinationNewer": false,
            "desktop": "noDesktop",
            "blockers": [],
            "appsToQuit": [],
            "notes": [],
        })
    );
}

#[tokio::test]
async fn repairing_quits_only_the_profiles_desktop_app_first() {
    let root = tempdir().unwrap();
    let default = claude_home(root.path(), "Default");
    let personal = claude_home(root.path(), "Personal");
    cli_session(&default, "2026-09-01T10:00:00Z");
    let org = personal
        .gui_data_dir
        .join("claude-code-sessions")
        .join("account")
        .join("org");
    fs::create_dir_all(&org).unwrap();
    fs::write(
        org.join("local_r1.json"),
        json!({ "cliSessionId": "s" }).to_string(),
    )
    .unwrap();
    let homes = vec![default.clone(), personal.clone()];
    let mut stock = fake_wrapper_process(&root.path().join("stock-app"), &default.gui_data_dir);
    let (reaper, stdin) = reaped(fake_wrapper_process(
        &root.path().join("app"),
        &personal.gui_data_dir,
    ));

    let refused = run_repair(&personal, &homes, false, QUIT_TIMEOUT).await;
    let untouched = default
        .config_dir
        .join("projects/-work-app/s.jsonl")
        .exists();
    let repaired = run_repair(&personal, &homes, true, QUIT_TIMEOUT).await;
    let quit = running_desktop_pid(&personal).unwrap().is_none();
    let stock_running = stock.try_wait().unwrap().is_none();
    drop(stdin);
    reaper.join().unwrap();
    stock.kill().unwrap();
    stock.wait().unwrap();

    assert!(
        matches!(&refused, Err(AppError::Validation(message)) if message == "Quit Claude (Personal) first"),
        "{refused:?}"
    );
    assert!(untouched);
    assert_eq!(repaired.unwrap().repaired, 1);
    assert!(quit, "the desktop app still runs");
    assert!(stock_running, "the Default desktop app was quit");
    assert!(personal
        .config_dir
        .join("projects/-work-app/s.jsonl")
        .exists());
    assert!(!default
        .config_dir
        .join("projects/-work-app/s.jsonl")
        .exists());
}

#[tokio::test]
async fn a_running_destination_that_isnt_signed_in_is_not_written_to_without_quitting() {
    let root = tempdir().unwrap();
    let work = claude_home(root.path(), "Work");
    let personal = claude_home(root.path(), "Personal");
    cli_session(&work, "2026-09-01T10:00:00Z");
    fs::create_dir_all(&personal.gui_data_dir).unwrap();
    let mut running = fake_wrapper_process(&root.path().join("app"), &personal.gui_data_dir);

    let refused = run_move(move_request(&work, &personal, false), false, QUIT_TIMEOUT).await;
    let still_running = running.try_wait().unwrap().is_none();

    running.kill().unwrap();
    running.wait().unwrap();
    assert!(
        matches!(&refused, Err(AppError::Validation(message)) if message == "Quit Claude (Personal) first"),
        "{refused:?}"
    );
    assert!(still_running);
    assert!(!personal.config_dir.exists());
    assert!(work.config_dir.join("projects/-work-app/s.jsonl").exists());
}
