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
use crate::sessions::codex::fakes::{
    read_response, running_desktop, write_rollout, ScriptedServer,
};

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

    /// A destination app-server started afresh, answering with `respond`.
    fn fresh_destination<F>(&self, respond: F) -> Side<F>
    where
        F: FnMut(&str, &Value) -> Result<Value, CodexRpcError>,
    {
        Side {
            side: "fresh destination",
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

/// A stand-in app-server that answers nothing.
type NoServer = ScriptedServer<fn(&str, &Value) -> Result<Value, CodexRpcError>>;

/// Starts the destination's app-server again: never, in a test that
/// doesn't expect it to be.
async fn never_restarted(_: Home) -> Result<NoServer, CodexRpcError> {
    panic!("the destination's app-server was started again")
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
async fn a_move_copies_the_rollout_unarchives_it_at_the_destination_then_archives_it_at_the_source()
{
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

    let report = execute_with(prepared, nothing_running, never_restarted)
        .await
        .unwrap();

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

    let moved = message(execute_with(prepared, nothing_running, never_restarted).await);

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

/// Plans the move of [`ID`] from `Work` to `Personal`, whose app-server fails
/// to unarchive it, and carries it out. Returns the error message.
async fn move_personal_refuses(setup: &Setup) -> String {
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
    message(execute_with(prepared, nothing_running, never_restarted).await)
}

#[tokio::test]
async fn a_copy_set_aside_where_an_earlier_one_is_gets_a_number() {
    let setup = Setup::new();
    let failed = setup
        .personal
        .config_dir
        .join("archived_sessions/.ai-profiles-failed");
    let name = setup.rollout.file_name().unwrap().to_string_lossy();
    let earlier = failed.join(format!("{name}.failed"));
    fs::create_dir_all(&failed).unwrap();
    fs::write(&earlier, "earlier").unwrap();

    let moved = move_personal_refuses(&setup).await;

    let aside = failed.join(format!("{name}.2.failed"));
    assert_eq!(
        moved,
        format!(
            "Personal couldn't take it (Codex: boom). Its copy is set aside in {}",
            aside.display()
        )
    );
    assert_eq!(fs::read_to_string(&earlier).unwrap(), "earlier");
    assert_eq!(fs::read(&aside).unwrap(), fs::read(&setup.rollout).unwrap());
    assert!(!setup.copy().exists());
}

#[tokio::test]
async fn a_copy_that_cant_be_set_aside_is_named_where_it_stays() {
    let setup = Setup::new();
    let archived = setup.personal.config_dir.join("archived_sessions");
    fs::create_dir_all(&archived).unwrap();
    // A file where the set-aside folder goes keeps the folder from being made.
    fs::write(archived.join(".ai-profiles-failed"), "in the way").unwrap();

    let moved = move_personal_refuses(&setup).await;

    let prefix = "Personal couldn't take it (Codex: boom). Its copy couldn't be set aside (";
    let suffix = format!("), so it is still in {}", setup.copy().display());
    assert!(
        moved.starts_with(prefix) && moved.ends_with(&suffix),
        "{moved}"
    );
    assert!(setup.copy().is_file());
    assert!(setup.rollout.is_file());
}

#[test]
fn a_compressed_rollout_is_one_of_the_sources_session_files() {
    let setup = Setup::new();
    let compressed = setup
        .rollout
        .with_file_name(format!("rollout-2026-09-01T10-00-00-{ID}.jsonl.zst"));
    fs::write(&compressed, "zstd").unwrap();

    assert!(confine(&compressed, &setup.work, ID).is_ok());
}

#[tokio::test]
async fn a_rollout_replaced_since_the_plan_is_refused_before_anything_is_written() {
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
    // The rollout is swapped for a link to a file outside the sessions.
    let outside = setup.work.config_dir.join("auth.json");
    fs::write(&outside, "secret").unwrap();
    fs::remove_file(&setup.rollout).unwrap();
    std::os::unix::fs::symlink(&outside, &setup.rollout).unwrap();

    let moved = message(execute_with(prepared, nothing_running, never_restarted).await);

    assert_eq!(
        moved,
        format!(
            "{} isn't one of Work's session files",
            setup.rollout.display()
        )
    );
    assert!(!setup.personal.config_dir.exists());
    assert_eq!(
        setup.calls(),
        ["source thread/read", "destination thread/read"]
    );
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

    assert_eq!(active_plan.blockers, [CODEX_HAS_IT_OPEN]);
    assert_eq!(locked_plan.blockers, [CODEX_HAS_IT_OPEN]);
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
    let moved = execute_with(prepared, nothing_running, never_restarted).await;

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

    let moved = execute_with(prepared, nothing_running, never_restarted).await;

    drop(held);
    assert_eq!(message(moved), CODEX_HAS_IT_OPEN);
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

        let moved = execute_with(prepared, processes, never_restarted).await;

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

    let moved = execute_with(prepared, nothing_running, never_restarted).await;

    assert_eq!(
        message(moved),
        "Moved to Personal, but couldn't archive it in Work (Codex: write conflict). \
         Archive it in Work to finish."
    );
    assert!(setup.copy().is_file());
    assert!(setup.rollout.is_file());
}

/// Plans a move whose destination's unarchive takes the thread in, then
/// fails with `failure`, after which its app-server answers any further
/// call with `then`; a fresh one knows the thread. Returns what carrying the
/// move out gave, the calls it made, and whether a copy was set aside.
async fn move_through_lost_unarchive(
    failure: CodexRpcError,
    then: fn() -> Result<Value, CodexRpcError>,
) -> (AppResult<MoveReport>, Vec<String>, bool) {
    let setup = Setup::new();
    let took = Arc::new(Mutex::new(false));
    let taking = took.clone();
    let copy = setup.copy();
    let taken = setup
        .personal
        .config_dir
        .join("sessions/2026/09/01")
        .join(copy.file_name().unwrap());
    let mut failure = Some(failure);
    let mut lost = false;
    let to = setup.destination(move |method, _| match method {
        _ if lost => then(),
        "thread/read" => Err(CodexRpcError::Rpc(format!("thread not loaded: {ID}"))),
        "thread/unarchive" => {
            fs::create_dir_all(taken.parent().unwrap()).unwrap();
            fs::rename(&copy, &taken).unwrap();
            *taking.lock().unwrap() = true;
            lost = true;
            Err(failure.take().unwrap())
        }
        other => panic!("unexpected call: {other}"),
    });
    let there = read_response(ID, Some(Path::new("/dest/sessions/rollout.jsonl")), None);
    let fresh = setup.fresh_destination(move |method, _| match method {
        "thread/read" if *took.lock().unwrap() => Ok(there.clone()),
        "thread/read" => Err(CodexRpcError::Rpc(format!("thread not loaded: {ID}"))),
        other => panic!("unexpected call: {other}"),
    });
    let restart = move |home: Home| {
        assert_eq!(home.id, "personal");
        async move { Ok(fresh) }
    };
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

    let moved = execute_with(prepared, nothing_running, restart).await;

    let set_aside = setup
        .personal
        .config_dir
        .join("archived_sessions/.ai-profiles-failed")
        .exists();
    (moved, setup.calls(), set_aside)
}

#[tokio::test]
async fn an_unarchive_that_timed_out_after_the_destination_took_it_still_finishes_the_move() {
    let (moved, calls, set_aside) =
        move_through_lost_unarchive(CodexRpcError::Timeout, || panic!("asked the stale server"))
            .await;

    moved.unwrap();
    assert_eq!(
        calls,
        [
            "source thread/read",
            "destination thread/read",
            "destination thread/unarchive",
            "fresh destination thread/read",
            "source thread/read",
            "source thread/archive",
        ]
    );
    assert!(!set_aside);
}

#[tokio::test]
async fn a_destination_whose_app_server_exited_is_asked_again_on_a_fresh_one() {
    let (moved, calls, set_aside) =
        move_through_lost_unarchive(CodexRpcError::Closed, || Err(CodexRpcError::Closed)).await;

    moved.unwrap();
    assert!(calls.contains(&"fresh destination thread/read".to_string()));
    assert_eq!(calls.last().unwrap(), "source thread/archive");
    assert!(!set_aside);
}

#[tokio::test]
async fn a_destination_that_fails_after_an_unrelated_error_is_asked_again_on_a_fresh_one() {
    let (moved, calls, _) =
        move_through_lost_unarchive(CodexRpcError::Rpc("busy".to_string()), || {
            Err(CodexRpcError::Closed)
        })
        .await;

    moved.unwrap();
    assert_eq!(
        calls[2..5],
        [
            "destination thread/unarchive",
            "destination thread/read",
            "fresh destination thread/read",
        ]
    );
}

#[tokio::test]
async fn an_unarchive_whose_outcome_cant_be_told_leaves_everything_where_it_is() {
    let setup = Setup::new();
    // The app-server is gone after the failed unarchive, and a fresh one
    // won't start either.
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
    let restart = |_: Home| async { Err::<NoServer, _>(CodexRpcError::Closed) };

    let moved = message(execute_with(prepared, nothing_running, restart).await);

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

    let moved = message(execute_with(prepared, nothing_running, never_restarted).await);

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

#[test]
fn a_missing_cli_explains_itself_and_a_failed_start_says_why() {
    let missing = start_error(&CodexRpcError::NotInstalled);
    let failed = start_error(&CodexRpcError::Closed);

    assert!(matches!(
        missing,
        AppError::NotInstalled(message) if message == "Install the Codex CLI to move this session"
    ));
    assert_eq!(
        failed.message(),
        "Codex: codex app-server exited before answering"
    );
}

#[tokio::test]
async fn a_destination_answer_with_an_unreadable_path_cant_say_whether_it_took_it() {
    let setup = Setup::new();
    let mut to = setup.destination(|_, _| Ok(json!({ "thread": { "id": ID, "path": 7 } })));

    let taken = taken(
        &mut to,
        &CodexRpcError::Rpc("busy".to_string()),
        never_restarted,
        &setup.personal,
        ID,
    )
    .await;

    assert_eq!(taken, None);
}
