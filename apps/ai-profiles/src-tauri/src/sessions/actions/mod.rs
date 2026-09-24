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
use super::claude::repair::{self, RepairReport};
use super::claude::transfer::{self, Prepared};
use super::codex;
use super::home::{home_for, homes_of};
use super::instance::{desktop_label, quit_desktop, QUIT_TIMEOUT};
use super::move_plan::{MovePlan, MoveReport};
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
        // Joined as the move dialog joins them.
        let blocker = if !plan.blockers.is_empty() {
            Some(plan.blockers.join(". "))
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
            let session_id = session_id.to_string();
            let checked = blocking(move || {
                claude_archive::check(&home, &homes, &session_id, action, &process_list()?)
            })
            .await?;
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
/// desktop app `quit_timeout` to quit. The check, which reads every home's
/// transcripts and runs `ps`, and the write are plain synchronous work, so
/// each runs on a blocking thread.
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
        || {
            let (home, homes, session_id) = (home.clone(), homes.to_vec(), session_id.to_string());
            blocking(move || {
                claude_archive::check(&home, &homes, &session_id, action, &process_list()?)
            })
        },
        |_| quit_blocking(home.clone(), quit_timeout),
        |target| {
            let home = home.clone();
            blocking(move || claude_archive::apply(&home, target, action))
        },
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
    if source.app == AppKind::Codex {
        let prepared = codex::transfer::plan(&source, &destination, session_id).await?;
        return Ok(prepared.plan);
    }
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
/// every home of the app.
fn move_homes(profile_id: &str, destination_id: &str) -> AppResult<(Home, Home, Vec<Home>)> {
    let source = home_for(profile_id)?;
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
    if request.source.app == AppKind::Codex {
        return run_codex_move(request, quit_apps, quit_timeout).await;
    }
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

/// [`run_move`] for a Codex session. The plan keeps both homes' app-servers
/// up, so the move goes through the ones the last plan checked with.
async fn run_codex_move(
    request: MoveRequest,
    quit_apps: bool,
    quit_timeout: Duration,
) -> AppResult<MoveReport> {
    let homes = request.homes.clone();
    run_checked(
        quit_apps,
        || async {
            let prepared =
                codex::transfer::plan(&request.source, &request.destination, &request.session_id)
                    .await?;
            let check = MoveGate::of(&prepared.plan, &request.destination, request.replace_newer);
            Ok(Checked {
                check,
                target: prepared,
            })
        },
        |apps| quit_all(apps, homes, quit_timeout),
        codex::transfer::execute,
    )
    .await
}

/// What stands between the sessions of profile `profile_id` (or
/// `default:<app>`) that need repair and their repair: the profile's desktop
/// app, when it runs. Only Claude sessions ever need repair.
pub async fn check_repair(profile_id: &str) -> AppResult<ActionCheck> {
    let home = home_for(profile_id)?;
    if home.app != AppKind::Claude {
        return Ok(ActionCheck::default());
    }
    let homes = homes_of(AppKind::Claude)?;
    blocking(move || Ok(repair::check(&home, &homes, &process_list()?)?.check)).await
}

/// Repair the sessions of profile `profile_id` (or `default:<app>`) that need
/// it, quitting its desktop app first if `quit_app`.
pub async fn repair_sessions(profile_id: &str, quit_app: bool) -> AppResult<RepairReport> {
    let home = home_for(profile_id)?;
    if home.app != AppKind::Claude {
        return Ok(RepairReport::default());
    }
    let homes = homes_of(AppKind::Claude)?;
    run_repair(&home, &homes, quit_app, QUIT_TIMEOUT).await
}

/// [`repair_sessions`] for `home`, one of `homes`, giving its desktop app
/// `quit_timeout` to quit. No other home's app is quit: only `home`'s config
/// dir is written.
async fn run_repair(
    home: &Home,
    homes: &[Home],
    quit_app: bool,
    quit_timeout: Duration,
) -> AppResult<RepairReport> {
    run_checked(
        quit_app,
        || {
            let (home, homes) = (home.clone(), homes.to_vec());
            blocking(move || repair::check(&home, &homes, &process_list()?))
        },
        |_| quit_blocking(home.clone(), quit_timeout),
        |prepared| blocking(move || repair::apply(prepared, Utc::now())),
    )
    .await
}

/// Run `work`, plain synchronous filesystem work, on a blocking thread.
pub(super) async fn blocking<R: Send + 'static>(
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
/// again, refusing if any still runs, or saying they were quit when something
/// else stands in the way now; then `apply` the action to what the last check
/// found.
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
    let first = check().await?;
    refuse_blocked(&first.check)?;
    let apps = first.check.apps_to_quit();
    if apps.is_empty() {
        return apply(first.target).await;
    }
    // Checked again once the apps quit, so what this check found is let go
    // now: a Codex move plan holds an app-server of each home.
    drop(first);
    if !quit_apps {
        return Err(AppError::Validation(format!(
            "Quit {} first",
            labels(&apps)
        )));
    }
    let quit_labels = labels(&apps);
    quit(apps).await?;
    let checked = check().await?;
    if let Some(blocker) = checked.check.blocker() {
        return Err(AppError::Validation(format!(
            "Quit {quit_labels}, but it still can't be done: {blocker}"
        )));
    }
    let running = checked.check.apps_to_quit();
    if !running.is_empty() {
        let verb = if running.len() == 1 { "is" } else { "are" };
        return Err(AppError::Validation(format!(
            "{} {verb} still running",
            labels(&running)
        )));
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
mod tests;
