//! Archiving, restoring and moving a session: checked, confirmed, then done.
//!
//! An action is first checked, which tells the user what stands in its way
//! before they confirm: a reason only they can clear (the session is open in
//! a terminal), or the desktop apps that have to quit first, which the app
//! can do for them. Doing the action checks again, as things may have changed
//! since, quits the apps if the user agreed to, and checks once more that
//! they are gone before anything is written.

use std::future::Future;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::claude::archive as claude_archive;
use super::claude::transfer::{self, MovePlan, MoveReport, Prepared};
use super::codex;
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

/// What a check says stands in an action's way, however it is shaped.
pub trait Gate {
    /// Why the action can't be done, when only the user can change that.
    fn blocker(&self) -> Option<String>;

    /// The desktop apps that have to quit first.
    fn apps_to_quit(&self) -> Vec<AppToQuit>;
}

impl Gate for ActionCheck {
    fn blocker(&self) -> Option<String> {
        self.blocker.clone()
    }

    fn apps_to_quit(&self) -> Vec<AppToQuit> {
        self.app_to_quit.iter().cloned().collect()
    }
}

/// What stands in a move's way: its plan's blockers, and a newer copy at the
/// destination the user didn't agree to replace.
#[derive(Debug)]
struct MoveGate {
    /// Why the move can't be done.
    blocker: Option<String>,
    /// The desktop apps that have to quit first.
    apps_to_quit: Vec<AppToQuit>,
}

impl MoveGate {
    /// The gate of `plan` of a move to `destination`, the user having agreed
    /// to replace a newer copy there if `replace_newer`.
    fn of(plan: &MovePlan, destination: &Home, replace_newer: bool) -> Self {
        let blocker = if !plan.blockers.is_empty() {
            Some(plan.blockers.join("; "))
        } else if plan.destination_newer && !replace_newer {
            Some(format!(
                "{} has a newer copy of this session",
                destination.label
            ))
        } else {
            None
        };
        MoveGate {
            blocker,
            apps_to_quit: plan.apps_to_quit.clone(),
        }
    }
}

impl Gate for MoveGate {
    fn blocker(&self) -> Option<String> {
        self.blocker.clone()
    }

    fn apps_to_quit(&self) -> Vec<AppToQuit> {
        self.apps_to_quit.clone()
    }
}

/// A check, with what the action would be done to.
#[derive(Debug)]
pub struct Checked<T, G = ActionCheck> {
    /// What stands in the action's way.
    pub check: G,
    /// What the action would be done to.
    pub target: T,
}

/// What stands between session `session_id` of profile `profile_id` (or
/// `default:<app>`) and `action`.
pub async fn check(
    profile_id: &str,
    session_id: &str,
    action: SessionAction,
) -> AppResult<ActionCheck> {
    let home = home_for(profile_id)?;
    match home.app {
        AppKind::Claude => {
            let homes = homes_of(AppKind::Claude)?;
            let checked =
                claude_archive::check(&home, &homes, session_id, action, &process_list()?)?;
            Ok(checked.check)
        }
        AppKind::Codex => Ok(codex::check(&home, session_id, action).await?.check),
    }
}

/// Archive session `session_id` of profile `profile_id` (or `default:<app>`),
/// quitting the desktop app in the way if `quit_app`.
pub async fn archive(profile_id: &str, session_id: &str, quit_app: bool) -> AppResult<()> {
    run(profile_id, session_id, SessionAction::Archive, quit_app).await
}

/// Restore archived session `session_id` of profile `profile_id` (or
/// `default:<app>`), quitting the desktop app in the way if `quit_app`.
pub async fn restore(profile_id: &str, session_id: &str, quit_app: bool) -> AppResult<()> {
    run(profile_id, session_id, SessionAction::Restore, quit_app).await
}

/// Do `action` to session `session_id` of profile `profile_id` (or
/// `default:<app>`), quitting the desktop app in its way if `quit_app`, and
/// refusing if it is in the way otherwise.
async fn run(
    profile_id: &str,
    session_id: &str,
    action: SessionAction,
    quit_app: bool,
) -> AppResult<()> {
    let home = home_for(profile_id)?;
    match home.app {
        AppKind::Claude => {
            let homes = homes_of(AppKind::Claude)?;
            run_claude(&home, &homes, session_id, action, quit_app, QUIT_TIMEOUT).await
        }
        AppKind::Codex => run_codex(&home, session_id, action, quit_app, QUIT_TIMEOUT).await,
    }
}

/// [`run`] for a Claude session of `home`, one of `homes`, giving its
/// desktop app `quit_timeout` to quit. The check and the write are plain
/// synchronous filesystem work; wrapping them in `async` blocks is only so
/// [`run_checked`] can drive both apps' actions through the same sequence.
async fn run_claude(
    home: &Home,
    homes: &[Home],
    session_id: &str,
    action: SessionAction,
    quit_app: bool,
    quit_timeout: Duration,
) -> AppResult<()> {
    run_checked(
        quit_app,
        || async { claude_archive::check(home, homes, session_id, action, &process_list()?) },
        |_| quit_blocking(home.clone(), quit_timeout),
        |target| async move { claude_archive::apply(home, target, action) },
    )
    .await
}

/// [`run`] for a Codex session of `home`, giving its desktop app
/// `quit_timeout` to quit.
async fn run_codex(
    home: &Home,
    session_id: &str,
    action: SessionAction,
    quit_app: bool,
    quit_timeout: Duration,
) -> AppResult<()> {
    run_checked(
        quit_app,
        || codex::check(home, session_id, action),
        |_| quit_blocking(home.clone(), quit_timeout),
        |target| codex::apply(home, target, action),
    )
    .await
}

/// What moving session `session_id` of profile `profile_id` (or
/// `default:<app>`) to profile `destination_id` would do.
pub async fn plan_move(
    profile_id: &str,
    session_id: &str,
    destination_id: &str,
) -> AppResult<MovePlan> {
    let (source, destination, homes) = move_homes(profile_id, destination_id)?;
    let session_id = session_id.to_string();
    let prepared = blocking(move || {
        transfer::plan(&source, &destination, &homes, &session_id, &process_list()?)
    })
    .await?;
    Ok(prepared.plan)
}

/// Move session `session_id` of profile `profile_id` (or `default:<app>`) to
/// profile `destination_id`, replacing a newer copy there only if
/// `replace_newer`, and quitting the desktop apps in the way if `quit_apps`.
pub async fn move_session(
    profile_id: &str,
    session_id: &str,
    destination_id: &str,
    replace_newer: bool,
    quit_apps: bool,
) -> AppResult<MoveReport> {
    let (source, destination, homes) = move_homes(profile_id, destination_id)?;
    let request = MoveRequest {
        source,
        destination,
        homes,
        session_id: session_id.to_string(),
        replace_newer,
    };
    run_move(request, quit_apps, QUIT_TIMEOUT).await
}

/// A move, as asked for.
#[derive(Debug, Clone)]
struct MoveRequest {
    /// Where the session is.
    source: Home,
    /// Where it goes.
    destination: Home,
    /// Every home of the app.
    homes: Vec<Home>,
    /// The session's id.
    session_id: String,
    /// Replace a newer copy at the destination.
    replace_newer: bool,
}

/// The homes of a move from profile `profile_id` to `destination_id`, with
/// every home of the app. Only Claude sessions move so far.
fn move_homes(profile_id: &str, destination_id: &str) -> AppResult<(Home, Home, Vec<Home>)> {
    let source = home_for(profile_id)?;
    if source.app != AppKind::Claude {
        return Err(AppError::Validation(
            "Only Claude sessions can be moved".to_string(),
        ));
    }
    let destination = home_for(destination_id)?;
    let homes = homes_of(source.app)?;
    Ok((source, destination, homes))
}

/// Carry out `request`, planning it again first, giving each desktop app in
/// the way `quit_timeout` to quit if `quit_apps`.
async fn run_move(
    request: MoveRequest,
    quit_apps: bool,
    quit_timeout: Duration,
) -> AppResult<MoveReport> {
    let homes = request.homes.clone();
    run_checked(
        quit_apps,
        || {
            let request = request.clone();
            blocking(move || {
                let prepared = transfer::plan(
                    &request.source,
                    &request.destination,
                    &request.homes,
                    &request.session_id,
                    &process_list()?,
                )?;
                let check =
                    MoveGate::of(&prepared.plan, &request.destination, request.replace_newer);
                Ok(Checked {
                    check,
                    target: prepared,
                })
            })
        },
        |apps| quit_all(apps, homes, quit_timeout),
        |prepared: Prepared| blocking(move || transfer::execute(prepared, Utc::now())),
    )
    .await
}

/// Run `work`, plain synchronous filesystem work, on a blocking thread.
async fn blocking<R: Send + 'static>(
    work: impl FnOnce() -> AppResult<R> + Send + 'static,
) -> AppResult<R> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| AppError::Io(std::io::Error::other(error)))?
}

/// Quit `home`'s desktop app, giving it `timeout`, on a blocking thread, as
/// waiting for it sleeps.
async fn quit_blocking(home: Home, timeout: Duration) -> AppResult<()> {
    blocking(move || quit_desktop(&home, timeout)).await
}

/// Quit each of `apps`, the desktop apps of some of `homes`, in turn, giving
/// each `timeout`.
async fn quit_all(apps: Vec<AppToQuit>, homes: Vec<Home>, timeout: Duration) -> AppResult<()> {
    for app in apps {
        let home = homes
            .iter()
            .find(|home| home.id == app.home_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("profile {} not found", app.home_id)))?;
        quit_blocking(home, timeout).await?;
    }
    Ok(())
}

/// Do an action once `check` allows it: refuse a blocked one; when desktop
/// apps are in the way, refuse unless `quit_apps`, else `quit` them and check
/// again, refusing if any still runs; then `apply` the action to what the
/// last check found.
async fn run_checked<T, G, R, CheckFut, QuitFut, ApplyFut>(
    quit_apps: bool,
    mut check: impl FnMut() -> CheckFut,
    quit: impl FnOnce(Vec<AppToQuit>) -> QuitFut,
    apply: impl FnOnce(T) -> ApplyFut,
) -> AppResult<R>
where
    G: Gate,
    CheckFut: Future<Output = AppResult<Checked<T, G>>>,
    QuitFut: Future<Output = AppResult<()>>,
    ApplyFut: Future<Output = AppResult<R>>,
{
    let mut checked = check().await?;
    refuse_blocked(&checked.check)?;
    let apps = checked.check.apps_to_quit();
    if !apps.is_empty() {
        if !quit_apps {
            return Err(AppError::Validation(format!(
                "Quit {} first",
                labels(&apps)
            )));
        }
        quit(apps).await?;
        checked = check().await?;
        refuse_blocked(&checked.check)?;
        let running = checked.check.apps_to_quit();
        if !running.is_empty() {
            let verb = if running.len() == 1 { "is" } else { "are" };
            return Err(AppError::Validation(format!(
                "{} {verb} still running",
                labels(&running)
            )));
        }
    }
    apply(checked.target).await
}

/// The labels of `apps`, joined: `Claude (Work) and Claude (Personal)`.
fn labels(apps: &[AppToQuit]) -> String {
    apps.iter()
        .map(|app| app.label.as_str())
        .collect::<Vec<_>>()
        .join(" and ")
}

/// The blocker `check` names, as an error.
fn refuse_blocked(check: &impl Gate) -> AppResult<()> {
    match check.blocker() {
        Some(blocker) => Err(AppError::Validation(blocker)),
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
}
