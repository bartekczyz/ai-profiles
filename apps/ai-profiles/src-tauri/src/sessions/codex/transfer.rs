//! Moving a Codex session to another profile of the app.
//!
//! A move goes through both homes' app-servers and touches no Codex state db
//! directly: the thread's rollout is copied, as it is, into the destination's
//! `archived_sessions/` under its own name; `thread/unarchive` there makes
//! the destination take it in, putting it back among its sessions; and
//! `thread/archive` at the source archives it there, so Restore undoes the
//! move. A copy the destination wouldn't take is set aside, never removed.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::actions::{
    apply_with, check_thread, held_lock, is_archived, ps_output, read_thread, resolve_error, Target,
};
use crate::app_kind::AppKind;
use crate::codex_rpc::{CodexRpc, CodexRpcError, CodexTransport};
use crate::error::{AppError, AppResult};
use crate::sessions::actions::{AppToQuit, SessionAction};
use crate::sessions::claude::archive_store::occupied;
use crate::sessions::claude::copy::{move_new, place_new, ItemAction};
use crate::sessions::claude::transfer::{DesktopAction, MovePlan, MoveReport, PlannedItem};
use crate::sessions::instance::{desktop_pid, running_again};
use crate::sessions::list::OPEN_IN_TERMINAL;
use crate::sessions::Home;

/// Where, in a destination's `archived_sessions/`, a copy it wouldn't take
/// is set aside.
const FAILED_DIR: &str = ".ai-profiles-failed";

/// What a set-aside copy's name ends with. App-server finds a thread by a
/// `.jsonl` file named after it anywhere under `archived_sessions/`, so a copy
/// set aside under its own name would still count as the destination's.
const FAILED_SUFFIX: &str = "failed";

/// Why a thread without a rollout file can't move.
const NO_ROLLOUT: &str = "Codex has no file of it to move";

/// A planned move, with the app-servers of both homes it goes through.
pub struct Prepared<S = CodexRpc, D = CodexRpc> {
    /// What the move does, as the user is shown it.
    pub plan: MovePlan,
    /// The source's app-server.
    from: S,
    /// The destination's app-server.
    to: D,
    /// Where the session is.
    source: Home,
    /// Where it goes.
    destination: Home,
    /// The thread's id.
    session_id: String,
    /// The thread's rollout file at the source, if it has one.
    rollout: Option<PathBuf>,
}

/// What moving Codex session `session_id` of `source` to `destination` would
/// do, through app-servers started on both homes. They stay up in what is
/// returned, for [`execute`] to use.
pub async fn plan(source: &Home, destination: &Home, session_id: &str) -> AppResult<Prepared> {
    refuse_other(source, destination)?;
    let from = CodexRpc::start(&source.config_dir)
        .await
        .map_err(|error| start_error(&error))?;
    let to = CodexRpc::start(&destination.config_dir)
        .await
        .map_err(|error| start_error(&error))?;
    let ps_output = ps_output().await?;
    plan_with(from, to, source, destination, session_id, &ps_output).await
}

/// [`plan`], through `from` at the source and `to` at the destination, given
/// the output of `ps -ax -o pid=,command=`.
///
/// The move is blocked while archiving the session at the source is (it is
/// open in a terminal, or archived already), while the thread has no rollout
/// to copy, while the destination has the thread already, and while a file by
/// the rollout's name sits in the destination's `archived_sessions/`. Each
/// home's desktop app that runs has to quit first: it keeps its own thread
/// catalog.
pub(super) async fn plan_with<S: CodexTransport, D: CodexTransport>(
    mut from: S,
    mut to: D,
    source: &Home,
    destination: &Home,
    session_id: &str,
    ps_output: &str,
) -> AppResult<Prepared<S, D>> {
    refuse_other(source, destination)?;
    let (archive, thread) = check_thread(
        &mut from,
        source,
        session_id,
        SessionAction::Archive,
        ps_output,
    )
    .await?;
    let rollout = thread
        .path
        .filter(|path| fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()));
    // An archived source is blocked below anyway; its file lives elsewhere.
    if let Some(path) = rollout.as_deref() {
        if !is_archived(&source.config_dir, Some(path)) {
            confine(path, source, session_id)?;
        }
    }
    let mut blockers: Vec<String> = archive.check.blocker.into_iter().collect();
    match read_thread(&mut to, session_id).await {
        Ok(there) if is_archived(&destination.config_dir, there.path.as_deref()) => {
            blockers.push(format!(
                "{} has it archived — restore it there instead",
                destination.label
            ));
        }
        Ok(_) => blockers.push(format!("{} has this session already", destination.label)),
        Err(error) => match resolve_error(&error, session_id, destination) {
            AppError::NotFound(_) => {}
            other => return Err(other),
        },
    }
    let items = match &rollout {
        None => {
            blockers.push(NO_ROLLOUT.to_string());
            Vec::new()
        }
        Some(path) => {
            let name = file_name(path)?;
            if occupied(&copy_path(destination, &name)) {
                blockers.push(format!(
                    "{} already has a file named {name} in archived_sessions",
                    destination.label
                ));
            }
            vec![PlannedItem {
                path: planned_path(path, source, &name),
                action: ItemAction::Copy,
            }]
        }
    };
    let mut apps_to_quit: Vec<AppToQuit> = archive.check.app_to_quit.into_iter().collect();
    if desktop_pid(destination, ps_output).is_some() {
        apps_to_quit.push(AppToQuit::of(destination));
    }
    let plan = MovePlan {
        summary: format!(
            "Moves 1 file from {} to {}",
            source.label, destination.label
        ),
        items,
        destination_newer: false,
        desktop: DesktopAction::NoDesktop,
        blockers,
        apps_to_quit,
        notes: Vec::new(),
    };
    Ok(Prepared {
        plan,
        from,
        to,
        source: source.clone(),
        destination: destination.clone(),
        session_id: session_id.to_string(),
        rollout,
    })
}

/// Carry out `prepared`, a move [`plan`] let through.
pub async fn execute(prepared: Prepared) -> AppResult<MoveReport> {
    execute_with(prepared, ps_output).await
}

/// [`execute`], reading the process list with `processes` each time it looks
/// for a desktop app.
///
/// Right before anything is written, the rollout must still be one of the
/// source's session files, the session still closed at the source (its writer
/// lock, by `lsof`) and neither home's desktop app running. Then the rollout
/// is copied into the destination's `archived_sessions/`, refusing to replace
/// anything there, and unarchived at the destination.
///
/// A failed unarchive doesn't say whether the destination took the session
/// (a timeout, or app-server exiting, may come after it did), so the
/// destination is asked again. Taken, the move goes on. Not taken, the copy,
/// if it is still where it was put, is set aside in
/// `archived_sessions/.ai-profiles-failed/`, and the source left as it was.
/// When that can't be told, nothing more is done and the error says so.
///
/// Last, the session is archived at the source, re-checking it there as
/// archiving always does; if that fails, the destination has it already, so
/// the error says both, and how to finish.
pub(super) async fn execute_with<S, D, P, F>(
    prepared: Prepared<S, D>,
    mut processes: P,
) -> AppResult<MoveReport>
where
    S: CodexTransport,
    D: CodexTransport,
    P: FnMut() -> F,
    F: Future<Output = AppResult<String>>,
{
    let Prepared {
        mut from,
        mut to,
        source,
        destination,
        session_id,
        rollout,
        ..
    } = prepared;
    let rollout = rollout.ok_or_else(|| AppError::Validation(NO_ROLLOUT.to_string()))?;
    confine(&rollout, &source, &session_id)?;
    if held_lock(&source.config_dir, &session_id).await? {
        return Err(AppError::Validation(OPEN_IN_TERMINAL.to_string()));
    }
    let ps_output = processes().await?;
    for home in [&source, &destination] {
        if desktop_pid(home, &ps_output).is_some() {
            return Err(running_again(home));
        }
    }
    let copy = copy_path(&destination, &file_name(&rollout)?);
    let target = copy.clone();
    tokio::task::spawn_blocking(move || place_new(&rollout, &target))
        .await
        .map_err(|error| AppError::Io(std::io::Error::other(error)))??;
    if let Err(error) = to
        .request("thread/unarchive", json!({ "threadId": session_id }))
        .await
    {
        match taken(&mut to, &destination, &session_id).await {
            Some(true) => {}
            Some(false) => return Err(not_taken(&copy, &destination, &error)),
            None => return Err(undecided(&copy, &source, &destination, &error)),
        }
    }
    let ps_output = processes().await?;
    let target = Target { id: session_id };
    apply_with(
        &mut from,
        &source,
        &target,
        SessionAction::Archive,
        &ps_output,
    )
    .await
    .map_err(|error| {
        AppError::Validation(format!(
            "Moved to {}, but couldn't archive it in {} ({}). Archive it in {} to finish.",
            destination.label,
            source.label,
            error.message(),
            source.label
        ))
    })?;
    Ok(MoveReport::default())
}

/// Refuse a move from `source` to `destination` unless both are Codex homes,
/// and different ones.
fn refuse_other(source: &Home, destination: &Home) -> AppResult<()> {
    if destination.app != source.app || source.app != AppKind::Codex {
        return Err(AppError::Validation(format!(
            "{} isn't a Codex profile",
            destination.label
        )));
    }
    if destination.id == source.id || destination.config_dir == source.config_dir {
        return Err(AppError::Validation(format!(
            "It's already in {}",
            source.label
        )));
    }
    Ok(())
}

/// The error starting app-server for a move fails with.
fn start_error(error: &CodexRpcError) -> AppError {
    match error {
        CodexRpcError::NotInstalled => AppError::Validation(
            "Codex CLI not found. Install it to move this session.".to_string(),
        ),
        _ => AppError::Validation(format!("Codex: {error}")),
    }
}

/// The name of the rollout at `path`.
fn file_name(path: &Path) -> AppResult<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::Validation(NO_ROLLOUT.to_string()))
}

/// Where a rollout named `name` is copied to in `destination`.
fn copy_path(destination: &Home, name: &str) -> PathBuf {
    destination.config_dir.join("archived_sessions").join(name)
}

/// How the plan names the rollout at `path`, named `name`: relative to the
/// source's config dir, which is also where the destination puts it back when
/// it takes it in; just its name when it lives elsewhere. App-server names
/// paths resolved, so the config dir is resolved too before comparing.
fn planned_path(path: &Path, source: &Home, name: &str) -> String {
    let config_dir =
        fs::canonicalize(&source.config_dir).unwrap_or_else(|_| source.config_dir.clone());
    path.strip_prefix(&config_dir)
        .or_else(|_| path.strip_prefix(&source.config_dir))
        .map_or_else(
            |_| name.to_string(),
            |relative| relative.display().to_string(),
        )
}

/// Refuse `path` as the rollout of thread `session_id` of `source` unless it
/// is one of the source's session files, named for that thread: a file under
/// `<config dir>/sessions/`, resolved, named `rollout-…-<id>.jsonl` or
/// `….jsonl.zst`. App-server names the path; this keeps a move from copying
/// anything else out of the home.
fn confine(path: &Path, source: &Home, session_id: &str) -> AppResult<()> {
    let sessions = fs::canonicalize(source.config_dir.join("sessions"));
    let inside = fs::canonicalize(path)
        .is_ok_and(|resolved| sessions.is_ok_and(|sessions| resolved.starts_with(sessions)));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let named = name.starts_with("rollout-")
        && [".jsonl", ".jsonl.zst"]
            .iter()
            .any(|extension| name.ends_with(&format!("-{session_id}{extension}")));
    if inside && named {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "{} isn't one of {}'s session files",
        path.display(),
        source.label
    )))
}

/// Whether `destination` has thread `session_id` among its sessions, asked
/// through `to`: `None` when that can't be told. A thread it finds only in its
/// `archived_sessions/` — where the copy was put — isn't taken.
async fn taken(to: &mut impl CodexTransport, destination: &Home, session_id: &str) -> Option<bool> {
    match read_thread(to, session_id).await {
        Ok(thread) => Some(!is_archived(
            &destination.config_dir,
            thread.path.as_deref(),
        )),
        Err(error) => match resolve_error(&error, session_id, destination) {
            AppError::NotFound(_) => Some(false),
            _ => None,
        },
    }
}

/// The error for `copy`, which `destination` didn't take in, failing with
/// `error`, after setting it aside with [`set_aside`] if it is still there. A
/// place is only named once the copy is known to be there.
fn not_taken(copy: &Path, destination: &Home, error: &CodexRpcError) -> AppError {
    let refused = format!("{} couldn't take it (Codex: {error})", destination.label);
    if !occupied(copy) {
        return AppError::Validation(refused);
    }
    match set_aside(copy) {
        Ok(aside) => AppError::Validation(format!(
            "{refused}. Its copy is set aside in {}",
            aside.display()
        )),
        Err(_) if !occupied(copy) => AppError::Validation(refused),
        Err(set_aside_error) => AppError::Validation(format!(
            "{refused}. Its copy couldn't be set aside ({set_aside_error}), so it is still in {}",
            copy.display()
        )),
    }
}

/// Move `copy` into [`FAILED_DIR`] beside it, under a name app-server doesn't
/// read as the thread's. Returns where it went. Nothing is replaced: a name
/// taken by an earlier failure gets a number.
fn set_aside(copy: &Path) -> std::io::Result<PathBuf> {
    let (Some(folder), Some(name)) = (copy.parent(), copy.file_name()) else {
        return Err(std::io::Error::other("it has no name"));
    };
    let name = name.to_string_lossy();
    let failed = folder.join(FAILED_DIR);
    fs::create_dir_all(&failed)?;
    for count in 1..=100 {
        let aside = match count {
            1 => failed.join(format!("{name}.{FAILED_SUFFIX}")),
            _ => failed.join(format!("{name}.{count}.{FAILED_SUFFIX}")),
        };
        match move_new(copy, &aside) {
            Ok(()) => return Ok(aside),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("every name for it is taken"))
}

/// The error for a move whose unarchive at `destination` failed with `error`
/// without it being told whether `destination` took the session. Nothing more
/// is done: the session stays in `source`, and `copy` where it is, if it is.
fn undecided(copy: &Path, source: &Home, destination: &Home, error: &CodexRpcError) -> AppError {
    let mut message = format!(
        "Couldn't tell whether {} took it (Codex: {error}). It is still in {}; check {} before \
         moving it again.",
        destination.label, source.label, destination.label
    );
    if occupied(copy) {
        message.push_str(&format!(" Its copy is in {}", copy.display()));
    }
    AppError::Validation(message)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime};

    use async_trait::async_trait;
    use serde_json::{json, Value};
    use tempfile::{tempdir, TempDir};

    use super::*;
    use crate::app_kind::AppKind;
    use crate::codex_rpc::CodexRpcError;
    use crate::sessions::claude::copy::ItemAction;
    use crate::sessions::claude::transfer::{DesktopAction, PlannedItem};
    use crate::sessions::codex::fakes::{read_response, running_desktop, write_rollout};

    /// The moved thread's id.
    const ID: &str = "019e2222-3333-7444-8555-666677778888";

    /// When the rollout was last written, in the tests.
    const WRITTEN: u64 = 1_780_000_000;

    /// The calls both app-servers received, in order: `source thread/read`.
    type Log = Arc<Mutex<Vec<String>>>;

    /// A stand-in app-server of one side of a move: answers each call with
    /// `respond`, logging it under `side` in a log both sides share.
    struct Side<F> {
        /// `source` or `destination`.
        side: &'static str,
        /// The log both sides write to.
        log: Log,
        /// Answers a call to a method with its params.
        respond: F,
    }

    #[async_trait]
    impl<F> CodexTransport for Side<F>
    where
        F: FnMut(&str, &Value) -> Result<Value, CodexRpcError> + Send,
    {
        async fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexRpcError> {
            assert_eq!(params["threadId"], json!(ID));
            self.log
                .lock()
                .unwrap()
                .push(format!("{} {method}", self.side));
            (self.respond)(method, &params)
        }
    }

    /// Two Codex homes, `Work` and `Personal`, with `Work` holding thread
    /// [`ID`]'s rollout, dated [`WRITTEN`].
    struct Setup {
        /// Holds the homes.
        root: TempDir,
        /// Where the thread is.
        work: Home,
        /// Where it goes.
        personal: Home,
        /// The rollout at `work`, as app-server names it: resolved.
        rollout: PathBuf,
        /// The log both app-servers write to.
        log: Log,
    }

    impl Setup {
        /// The two homes, the thread in `Work`.
        fn new() -> Self {
            let root = tempdir().unwrap();
            let work = codex_home(root.path(), "Work");
            let personal = codex_home(root.path(), "Personal");
            let written = write_rollout(&root.path().join("Work"), ID, "codex-tui");
            File::options()
                .write(true)
                .open(&written)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN))
                .unwrap();
            let rollout = fs::canonicalize(&written).unwrap();
            Setup {
                root,
                work,
                personal,
                rollout,
                log: Log::default(),
            }
        }

        /// The source's app-server, answering with `respond`.
        fn source<F>(&self, respond: F) -> Side<F>
        where
            F: FnMut(&str, &Value) -> Result<Value, CodexRpcError>,
        {
            Side {
                side: "source",
                log: self.log.clone(),
                respond,
            }
        }

        /// The destination's app-server, answering with `respond`.
        fn destination<F>(&self, respond: F) -> Side<F>
        where
            F: FnMut(&str, &Value) -> Result<Value, CodexRpcError>,
        {
            Side {
                side: "destination",
                log: self.log.clone(),
                respond,
            }
        }

        /// A source app-server that knows the thread, idle, and archives it.
        fn idle_source(&self) -> Side<impl FnMut(&str, &Value) -> Result<Value, CodexRpcError>> {
            let read = read_response(ID, Some(&self.rollout), Some("notLoaded"));
            self.source(move |method, _| match method {
                "thread/read" => Ok(read.clone()),
                "thread/archive" => Ok(json!({})),
                other => panic!("unexpected call: {other}"),
            })
        }

        /// A destination app-server that doesn't know the thread, and
        /// answers `thread/unarchive` with `unarchived`.
        fn free_destination(
            &self,
            mut unarchived: impl FnMut() -> Result<Value, CodexRpcError> + Send,
        ) -> Side<impl FnMut(&str, &Value) -> Result<Value, CodexRpcError>> {
            self.destination(move |method, _| match method {
                "thread/read" => Err(CodexRpcError::Rpc(format!("thread not loaded: {ID}"))),
                "thread/unarchive" => unarchived(),
                other => panic!("unexpected call: {other}"),
            })
        }

        /// Where the move puts the rollout's copy: the destination's
        /// archived sessions, under the rollout's name.
        fn copy(&self) -> PathBuf {
            self.personal
                .config_dir
                .join("archived_sessions")
                .join(self.rollout.file_name().unwrap())
        }

        /// The calls logged so far.
        fn calls(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    /// A managed Codex home named `name`, under `root`.
    fn codex_home(root: &Path, name: &str) -> Home {
        Home {
            id: name.to_lowercase(),
            app: AppKind::Codex,
            label: name.to_string(),
            config_dir: root.join(name).join("cli-config"),
            gui_data_dir: root.join(name).join("gui-data"),
            stock: false,
        }
    }

    /// A process list with nothing of the homes running.
    async fn nothing_running() -> AppResult<String> {
        Ok(String::new())
    }

    /// The message of a validation error.
    fn message<T>(result: AppResult<T>) -> String {
        match result {
            Err(AppError::Validation(message)) => message,
            Err(other) => panic!("not a validation error: {other:?}"),
            Ok(_) => panic!("it succeeded"),
        }
    }

    #[tokio::test]
    async fn a_move_copies_the_rollout_unarchives_it_at_the_destination_then_archives_it_at_the_source(
    ) {
        let setup = Setup::new();
        let copy = setup.copy();
        let seen = Arc::new(Mutex::new(None));
        let copy_at_unarchive = seen.clone();
        let to = setup.free_destination(move || {
            *copy_at_unarchive.lock().unwrap() = Some(fs::read(&copy).unwrap());
            Ok(json!({ "thread": { "id": ID } }))
        });

        let prepared = plan_with(
            setup.idle_source(),
            to,
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap();

        let name = setup.rollout.file_name().unwrap().to_string_lossy();
        assert_eq!(
            prepared.plan,
            MovePlan {
                summary: "Moves 1 file from Work to Personal".to_string(),
                items: vec![PlannedItem {
                    path: format!("sessions/2026/09/01/{name}"),
                    action: ItemAction::Copy,
                }],
                destination_newer: false,
                desktop: DesktopAction::NoDesktop,
                blockers: vec![],
                apps_to_quit: vec![],
                notes: vec![],
            }
        );

        let report = execute_with(prepared, nothing_running).await.unwrap();

        assert_eq!(report, MoveReport::default());
        assert_eq!(
            setup.calls(),
            [
                "source thread/read",
                "destination thread/read",
                "destination thread/unarchive",
                "source thread/read",
                "source thread/archive",
            ]
        );
        let original = fs::read(&setup.rollout).unwrap();
        assert_eq!(seen.lock().unwrap().as_deref(), Some(original.as_slice()));
        assert_eq!(
            fs::metadata(setup.copy()).unwrap().modified().unwrap(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN)
        );
        assert_eq!(
            fs::read_dir(setup.personal.config_dir.join("archived_sessions"))
                .unwrap()
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn a_failed_unarchive_sets_the_copy_aside_and_leaves_the_source_unarchived() {
        let setup = Setup::new();
        let to = setup.free_destination(|| Err(CodexRpcError::Rpc("boom".to_string())));
        let prepared = plan_with(
            setup.idle_source(),
            to,
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap();

        let moved = message(execute_with(prepared, nothing_running).await);

        let name = setup.rollout.file_name().unwrap().to_string_lossy();
        let aside = setup
            .personal
            .config_dir
            .join("archived_sessions/.ai-profiles-failed")
            .join(format!("{name}.failed"));
        assert_eq!(
            moved,
            format!(
                "Personal couldn't take it (Codex: boom). Its copy is set aside in {}",
                aside.display()
            )
        );
        assert_eq!(
            setup.calls(),
            [
                "source thread/read",
                "destination thread/read",
                "destination thread/unarchive",
                "destination thread/read",
            ]
        );
        assert!(aside.is_file());
        assert!(!setup.copy().exists());
        assert!(setup.rollout.is_file());
    }

    #[tokio::test]
    async fn a_thread_the_destination_has_already_is_a_blocker() {
        let setup = Setup::new();
        let there = read_response(ID, Some(Path::new("/elsewhere/rollout.jsonl")), None);
        let to = setup.destination(move |_, _| Ok(there.clone()));

        let plan = plan_with(
            setup.idle_source(),
            to,
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap()
        .plan;

        assert_eq!(plan.blockers, ["Personal has this session already"]);
    }

    #[tokio::test]
    async fn a_destination_that_cant_say_whether_it_has_the_thread_fails_the_plan() {
        let setup = Setup::new();
        let to = setup.destination(|_, _| Err(CodexRpcError::Rpc("not signed in".to_string())));

        let planned = plan_with(
            setup.idle_source(),
            to,
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await;

        assert_eq!(message(planned), "Codex: not signed in");
    }

    #[tokio::test]
    async fn a_source_open_in_a_terminal_is_a_blocker() {
        let setup = Setup::new();
        let active = read_response(ID, Some(&setup.rollout), Some("active"));
        let from = setup.source(move |_, _| Ok(active.clone()));
        let to = setup.free_destination(|| unreachable!());
        let locks = setup.work.config_dir.join("thread-writer-locks");
        fs::create_dir_all(&locks).unwrap();

        let active_plan = plan_with(from, to, &setup.work, &setup.personal, ID, "")
            .await
            .unwrap()
            .plan;
        // Holding the lock open here is enough for `lsof` to name a holder.
        let held = File::create(locks.join(format!("{ID}.lock"))).unwrap();
        let locked_plan = plan_with(
            setup.idle_source(),
            setup.free_destination(|| unreachable!()),
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap()
        .plan;
        drop(held);

        assert_eq!(active_plan.blockers, ["Close it in the terminal first"]);
        assert_eq!(locked_plan.blockers, ["Close it in the terminal first"]);
    }

    #[tokio::test]
    async fn the_desktop_apps_in_the_way_are_named() {
        let setup = Setup::new();
        let ps_output = format!(
            "{}{}",
            running_desktop(&setup.work),
            running_desktop(&setup.personal).replace("900", "901")
        );

        let plan = plan_with(
            setup.idle_source(),
            setup.free_destination(|| unreachable!()),
            &setup.work,
            &setup.personal,
            ID,
            &ps_output,
        )
        .await
        .unwrap()
        .plan;

        let labels: Vec<&str> = plan
            .apps_to_quit
            .iter()
            .map(|app| app.label.as_str())
            .collect();
        assert_eq!(labels, ["ChatGPT (Work)", "ChatGPT (Personal)"]);
        assert_eq!(plan.blockers, Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_file_by_the_rollouts_name_in_the_destinations_archive_is_never_replaced() {
        let setup = Setup::new();
        let prepared = plan_with(
            setup.idle_source(),
            setup.free_destination(|| unreachable!()),
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap();
        fs::create_dir_all(setup.copy().parent().unwrap()).unwrap();
        fs::write(setup.copy(), "other").unwrap();

        let replanned = plan_with(
            setup.idle_source(),
            setup.free_destination(|| unreachable!()),
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap()
        .plan;
        let moved = execute_with(prepared, nothing_running).await;

        let name = setup.rollout.file_name().unwrap().to_string_lossy();
        assert_eq!(
            replanned.blockers,
            [format!(
                "Personal already has a file named {name} in archived_sessions"
            )]
        );
        assert!(moved.is_err());
        assert_eq!(fs::read_to_string(setup.copy()).unwrap(), "other");
        assert!(!setup
            .calls()
            .contains(&"destination thread/unarchive".to_string()));
        assert!(!setup.calls().contains(&"source thread/archive".to_string()));
    }

    #[tokio::test]
    async fn a_session_moves_only_to_another_codex_profile() {
        let setup = Setup::new();
        let mut claude = codex_home(setup.root.path(), "Claude");
        claude.app = AppKind::Claude;

        let same = plan_with(
            setup.idle_source(),
            setup.free_destination(|| unreachable!()),
            &setup.work,
            &setup.work,
            ID,
            "",
        )
        .await;
        let other_app = plan_with(
            setup.idle_source(),
            setup.free_destination(|| unreachable!()),
            &setup.work,
            &claude,
            ID,
            "",
        )
        .await;

        assert_eq!(message(same), "It's already in Work");
        assert_eq!(message(other_app), "Claude isn't a Codex profile");
        assert_eq!(setup.calls(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn nothing_is_written_when_a_terminal_opened_the_session_since_the_plan() {
        let setup = Setup::new();
        let prepared = plan_with(
            setup.idle_source(),
            setup.free_destination(|| unreachable!()),
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap();
        let locks = setup.work.config_dir.join("thread-writer-locks");
        fs::create_dir_all(&locks).unwrap();
        let held = File::create(locks.join(format!("{ID}.lock"))).unwrap();

        let moved = execute_with(prepared, nothing_running).await;

        drop(held);
        assert_eq!(message(moved), "Close it in the terminal first");
        assert!(!setup.personal.config_dir.exists());
        assert_eq!(
            setup.calls(),
            ["source thread/read", "destination thread/read"]
        );
    }

    #[tokio::test]
    async fn nothing_is_written_when_a_desktop_app_started_again_since_the_plan() {
        for (index, relaunched) in ["Work", "Personal"].into_iter().enumerate() {
            let setup = Setup::new();
            let prepared = plan_with(
                setup.idle_source(),
                setup.free_destination(|| unreachable!()),
                &setup.work,
                &setup.personal,
                ID,
                "",
            )
            .await
            .unwrap();
            let home = [&setup.work, &setup.personal][index].clone();
            let processes = move || {
                let ps_output = running_desktop(&home);
                async move { Ok(ps_output) }
            };

            let moved = execute_with(prepared, processes).await;

            assert_eq!(
                message(moved),
                format!("ChatGPT ({relaunched}) is running again — quit it and try again")
            );
            assert!(!setup.personal.config_dir.exists());
            assert_eq!(setup.calls().len(), 2);
        }
    }

    #[tokio::test]
    async fn a_source_that_cant_be_archived_after_the_destination_took_it_says_so() {
        let setup = Setup::new();
        let read = read_response(ID, Some(&setup.rollout), Some("notLoaded"));
        let from = setup.source(move |method, _| match method {
            "thread/read" => Ok(read.clone()),
            _ => Err(CodexRpcError::Rpc("write conflict".to_string())),
        });
        let to = setup.free_destination(|| Ok(json!({})));
        let prepared = plan_with(from, to, &setup.work, &setup.personal, ID, "")
            .await
            .unwrap();

        let moved = execute_with(prepared, nothing_running).await;

        assert_eq!(
            message(moved),
            "Moved to Personal, but couldn't archive it in Work (Codex: write conflict). \
             Archive it in Work to finish."
        );
        assert!(setup.copy().is_file());
        assert!(setup.rollout.is_file());
    }

    /// A destination app-server whose `thread/unarchive` runs `unarchived`,
    /// and whose `thread/read` knows the thread only once `took` holds.
    fn destination_taking(
        setup: &Setup,
        took: Arc<Mutex<bool>>,
        mut unarchived: impl FnMut() -> Result<Value, CodexRpcError> + Send,
    ) -> Side<impl FnMut(&str, &Value) -> Result<Value, CodexRpcError>> {
        let there = read_response(ID, Some(Path::new("/dest/sessions/rollout.jsonl")), None);
        setup.destination(move |method, _| match method {
            "thread/read" if *took.lock().unwrap() => Ok(there.clone()),
            "thread/read" => Err(CodexRpcError::Rpc(format!("thread not loaded: {ID}"))),
            "thread/unarchive" => unarchived(),
            other => panic!("unexpected call: {other}"),
        })
    }

    #[tokio::test]
    async fn an_unarchive_that_timed_out_after_the_destination_took_it_still_finishes_the_move() {
        let setup = Setup::new();
        let took = Arc::new(Mutex::new(false));
        let taking = took.clone();
        let copy = setup.copy();
        let taken = setup
            .personal
            .config_dir
            .join("sessions/2026/09/01")
            .join(copy.file_name().unwrap());
        let to = destination_taking(&setup, took, move || {
            fs::create_dir_all(taken.parent().unwrap()).unwrap();
            fs::rename(&copy, &taken).unwrap();
            *taking.lock().unwrap() = true;
            Err(CodexRpcError::Timeout)
        });
        let prepared = plan_with(
            setup.idle_source(),
            to,
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap();

        execute_with(prepared, nothing_running).await.unwrap();

        assert_eq!(
            setup.calls(),
            [
                "source thread/read",
                "destination thread/read",
                "destination thread/unarchive",
                "destination thread/read",
                "source thread/read",
                "source thread/archive",
            ]
        );
        assert!(!setup
            .personal
            .config_dir
            .join("archived_sessions/.ai-profiles-failed")
            .exists());
    }

    #[tokio::test]
    async fn an_unarchive_whose_outcome_cant_be_told_leaves_everything_where_it_is() {
        let setup = Setup::new();
        // The app-server is gone after the failed unarchive, so asking it
        // again fails too.
        let mut gone = false;
        let to = setup.destination(move |method, _| match method {
            "thread/read" if !gone => Err(CodexRpcError::Rpc(format!("thread not loaded: {ID}"))),
            _ => {
                gone = true;
                Err(CodexRpcError::Closed)
            }
        });
        let prepared = plan_with(
            setup.idle_source(),
            to,
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap();

        let moved = message(execute_with(prepared, nothing_running).await);

        assert_eq!(
            moved,
            format!(
                "Couldn't tell whether Personal took it (Codex: codex app-server exited before \
                 answering). It is still in Work; check Personal before moving it again. \
                 Its copy is in {}",
                setup.copy().display()
            )
        );
        assert!(setup.copy().is_file());
        assert!(!setup.calls().contains(&"source thread/archive".to_string()));
    }

    #[tokio::test]
    async fn an_unarchive_that_failed_without_a_copy_left_names_no_location() {
        let setup = Setup::new();
        let copy = setup.copy();
        let to = setup.free_destination(move || {
            fs::remove_file(&copy).unwrap();
            Err(CodexRpcError::Rpc("boom".to_string()))
        });
        let prepared = plan_with(
            setup.idle_source(),
            to,
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap();

        let moved = message(execute_with(prepared, nothing_running).await);

        assert_eq!(moved, "Personal couldn't take it (Codex: boom)");
    }

    #[tokio::test]
    async fn moving_back_to_where_it_is_archived_says_to_restore_it_there() {
        let setup = Setup::new();
        let archived = setup
            .personal
            .config_dir
            .join("archived_sessions")
            .join(setup.rollout.file_name().unwrap());
        fs::create_dir_all(archived.parent().unwrap()).unwrap();
        fs::write(&archived, "moved here once").unwrap();
        let there = read_response(ID, Some(&fs::canonicalize(&archived).unwrap()), None);
        let to = setup.destination(move |_, _| Ok(there.clone()));

        let plan = plan_with(
            setup.idle_source(),
            to,
            &setup.work,
            &setup.personal,
            ID,
            "",
        )
        .await
        .unwrap()
        .plan;

        assert_eq!(
            plan.blockers[0],
            "Personal has it archived — restore it there instead"
        );
        assert!(!plan
            .blockers
            .contains(&"Personal has this session already".to_string()));
    }

    #[tokio::test]
    async fn a_rollout_outside_the_sources_sessions_or_named_for_another_thread_is_refused() {
        let setup = Setup::new();
        let outside = setup
            .work
            .config_dir
            .join("elsewhere")
            .join(setup.rollout.file_name().unwrap());
        let misnamed = setup
            .rollout
            .with_file_name("rollout-2026-09-01T10-00-00-other.jsonl");
        for path in [&outside, &misnamed] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::copy(&setup.rollout, path).unwrap();
        }

        for path in [outside, misnamed] {
            let resolved = fs::canonicalize(&path).unwrap();
            let read = read_response(ID, Some(&resolved), Some("notLoaded"));
            let from = setup.source(move |_, _| Ok(read.clone()));

            let planned = plan_with(
                from,
                setup.free_destination(|| unreachable!()),
                &setup.work,
                &setup.personal,
                ID,
                "",
            )
            .await;

            assert_eq!(
                message(planned),
                format!("{} isn't one of Work's session files", resolved.display())
            );
        }
    }
}
