use std::collections::BTreeMap;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};
use tempfile::tempdir;

use super::*;
use crate::app_kind::AppKind;
use crate::error::AppError;
use crate::sessions::actions::SessionAction;
use crate::sessions::claude::archive;
use crate::sessions::claude::archive_store::archived_bundles;
use crate::sessions::claude::desktop::read_records;
use crate::sessions::claude::ownership::owned_by;
use crate::sessions::list::home_scans;
use crate::test_support::{fake_wrapper_process, opened_claude_home as home, read_value, tree};

const WORK_ACCOUNT: &str = "1a19a582-d7b1-4f72-acef-cbe78c1a68e4";
const WORK_ORG: &str = "18d53058-434e-4c78-9624-e290f7a80ccb";
const PERSONAL_ACCOUNT: &str = "a99c6b36-dd42-44d7-b3ae-9496265549fd";
const PERSONAL_ORG: &str = "527aadd2-01c3-49a6-a770-e65e047242c3";

/// Write `contents` to `path`, making its folder.
fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// Date the file at `path` `modified`.
fn date(path: &Path, modified: SystemTime) {
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

/// When the file at `path` was last written.
fn modified(path: &Path) -> SystemTime {
    fs::metadata(path).unwrap().modified().unwrap()
}

/// When a transcript's last record was written, in the tests.
const WRITTEN: u64 = 1_780_000_000;

/// Gives `home` transcript `session`, last used at `timestamp`, working in
/// `cwd`, with a subagent transcript beside it and a file history, all
/// dated [`WRITTEN`]. Returns the transcript's path.
fn transcript(home: &Home, session: &str, timestamp: &str, cwd: &str) -> PathBuf {
    let dir = home.config_dir.join("projects/-work-app");
    let line = json!({
        "type": "user",
        "sessionId": session,
        "timestamp": timestamp,
        "cwd": cwd,
        "message": { "role": "user", "content": "Fix the login bug" },
    });
    let path = dir.join(format!("{session}.jsonl"));
    let written = SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN);
    write(&path, &format!("{line}\n"));
    date(&path, written);
    let subagent = dir.join(session).join("subagents/agent-1.jsonl");
    write(&subagent, "{}\n");
    date(&subagent, written);
    let history = home
        .config_dir
        .join("file-history")
        .join(session)
        .join("abc@v1");
    write(&history, "old");
    date(&history, written);
    path
}

/// Signs `home`'s desktop app in to `account`, in `org`.
fn sign_in(home: &Home, account: &str, org: &str) {
    write(
        &home.gui_data_dir.join("config.json"),
        &json!({ "lastKnownAccountUuid": account }).to_string(),
    );
    write(
        &home.config_dir.join(".claude.json"),
        &json!({ "oauthAccount": { "accountUuid": account, "organizationUuid": org } }).to_string(),
    );
    fs::create_dir_all(records_dir(home, account, org)).unwrap();
}

/// Where `home`'s desktop app keeps the records of `account`, in `org`.
fn records_dir(home: &Home, account: &str, org: &str) -> PathBuf {
    home.gui_data_dir
        .join("claude-code-sessions")
        .join(account)
        .join(org)
}

/// Writes `home`'s desktop record `local_<uuid>` of `fields`, under
/// [`WORK_ACCOUNT`]. Returns its path.
fn record(home: &Home, uuid: &str, fields: Value) -> PathBuf {
    let path = records_dir(home, WORK_ACCOUNT, WORK_ORG).join(format!("local_{uuid}.json"));
    write(&path, &fields.to_string());
    path
}

/// A desktop record of session `s`, with fields bound to its account.
fn desktop_fields() -> Value {
    json!({
        "sessionId": "local_r1",
        "cliSessionId": "s",
        "cwd": "/work/app",
        "title": "Audit the API",
        "model": "claude-opus-5-5",
        "isArchived": false,
        "remoteMcpServersConfig": [{ "uuid": "x" }],
        "sessionPermissionUpdates": [],
        "alwaysAllowedReasons": {},
        "spawnSeed": 7,
    })
}

/// Plans moving session `session_id` of `source` to `destination`, then
/// carries the move out.
fn move_session(
    source: &Home,
    destination: &Home,
    homes: &[Home],
    session_id: &str,
) -> AppResult<MoveReport> {
    let prepared = plan(source, destination, homes, session_id, "")?;
    assert_eq!(prepared.plan.blockers, Vec::<String>::new());
    execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap())
}

/// The paths and actions a plan lists.
fn planned(plan: &MovePlan) -> Vec<(&str, ItemAction)> {
    plan.items
        .iter()
        .map(|item| (item.path.as_str(), item.action))
        .collect()
}

#[test]
fn the_plan_copies_what_is_missing_leaves_what_is_the_same_and_replaces_the_rest() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    // A copy of an earlier move, which kept its date.
    let same = personal.config_dir.join("file-history/s/abc@v1");
    write(&same, "old");
    date(&same, SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN));
    write(
        &personal
            .config_dir
            .join("projects/-work-app/s/subagents/agent-1.jsonl"),
        "{\"other\":1}\n",
    );
    let homes = [work.clone(), personal.clone()];

    let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

    assert_eq!(
        planned(&prepared.plan),
        [
            ("file-history/s", ItemAction::Same),
            ("projects/-work-app/s", ItemAction::Replace),
            ("projects/-work-app/s.jsonl", ItemAction::Copy),
        ]
    );
    assert_eq!(prepared.plan.summary, "Moves 2 files from Work to Personal");
    assert!(!prepared.plan.destination_newer);
    assert_eq!(prepared.plan.blockers, Vec::<String>::new());
    assert_eq!(prepared.plan.apps_to_quit, []);
}

#[test]
fn a_newer_copy_at_the_destination_is_flagged() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    transcript(&personal, "s", "2026-09-02T10:00:00Z", "/work/app");
    let homes = [work.clone(), personal.clone()];

    let newer = plan(&work, &personal, &homes, "s", "").unwrap().plan;
    let older = plan(&personal, &work, &homes, "s", "").unwrap().plan;

    assert!(newer.destination_newer);
    assert!(!older.destination_newer);
}

#[test]
fn a_moved_cli_session_keeps_its_dates_and_is_archived_at_the_source() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    let source = transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let homes = [work.clone(), personal.clone()];

    move_session(&work, &personal, &homes, "s").unwrap();

    let copied = personal.config_dir.join("projects/-work-app/s.jsonl");
    assert_eq!(
        modified(&copied),
        SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN)
    );
    assert!(personal
        .config_dir
        .join("projects/-work-app/s/subagents/agent-1.jsonl")
        .exists());
    assert!(personal.config_dir.join("file-history/s/abc@v1").exists());
    assert!(!source.exists());
    let archived = archived_bundles(&work.config_dir);
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].session_id, "s");
}

#[test]
fn a_move_cut_short_leaves_no_transcript_at_the_destination() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    let source = transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let history = personal.config_dir.join("file-history");
    fs::create_dir_all(&history).unwrap();
    fs::set_permissions(&history, fs::Permissions::from_mode(0o555)).unwrap();
    let homes = [work.clone(), personal.clone()];

    let moved = move_session(&work, &personal, &homes, "s");

    fs::set_permissions(&history, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(moved.is_err());
    assert!(!personal
        .config_dir
        .join("projects/-work-app/s.jsonl")
        .exists());
    assert!(source.exists());
    assert_eq!(archived_bundles(&work.config_dir), []);
}

#[test]
fn a_desktop_session_is_listed_under_the_destinations_account_without_the_sources_bindings() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&work, WORK_ACCOUNT, WORK_ORG);
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let source_record = record(&work, "r1", desktop_fields());
    let homes = [work.clone(), personal.clone()];

    let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

    assert_eq!(prepared.plan.desktop, DesktopAction::Add);
    assert_eq!(
        prepared.plan.notes,
        [
            "Connectors and MCP servers come from Personal's settings",
            "On other devices, Remote Control shows only messages sent after the move.",
        ]
    );

    execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap()).unwrap();

    let written = records_dir(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG).join("local_r1.json");
    assert_eq!(
        read_value(&written),
        json!({
            "sessionId": "local_r1",
            "cliSessionId": "s",
            "priorCliSessionIds": [],
            "cwd": "/work/app",
            "title": "Audit the API",
            "model": "claude-opus-5-5",
            "isArchived": false,
        })
    );
    assert_eq!(read_value(&source_record)["isArchived"], json!(true));
    assert!(work.config_dir.join("projects/-work-app/s.jsonl").exists());
}

#[test]
fn a_cli_session_gets_a_record_only_where_the_desktop_app_is_signed_in() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    let side = home(root.path(), "Side");
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let homes = [work.clone(), personal.clone(), side.clone()];

    let unsigned = plan(&work, &side, &homes, "s", "").unwrap().plan;

    assert_eq!(unsigned.desktop, DesktopAction::SignInNeeded);
    assert_eq!(
        unsigned.notes,
        ["Sign in to Claude in Side's desktop app to see it there."]
    );

    move_session(&work, &personal, &homes, "s").unwrap();

    let records = read_records(&personal.gui_data_dir);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].cli_session_id.as_deref(), Some("s"));
    assert_eq!(records[0].cwd.as_deref(), Some("/work/app"));
    assert!(!records[0].archived);
    assert!(records[0]
        .path
        .starts_with(records_dir(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG)));
}

#[test]
fn a_destination_without_a_desktop_app_gets_the_cli_half_only() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    fs::remove_dir(&personal.gui_data_dir).unwrap();
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let homes = [work.clone(), personal.clone()];

    let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

    assert_eq!(prepared.plan.desktop, DesktopAction::NoDesktop);
    assert_eq!(prepared.plan.notes, Vec::<String>::new());
}

#[test]
fn every_transcript_a_record_claims_moves_from_wherever_it_is() {
    let root = tempdir().unwrap();
    let default = home(root.path(), "Default");
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-02T10:00:00Z", "/work/app");
    transcript(&default, "earlier", "2026-09-01T10:00:00Z", "/work/app");
    record(
        &work,
        "r1",
        json!({ "cliSessionId": "s", "priorCliSessionIds": ["earlier"] }),
    );
    let homes = [default.clone(), work.clone(), personal.clone()];

    let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

    assert_eq!(
        planned(&prepared.plan),
        [
            ("file-history/earlier", ItemAction::Copy),
            ("file-history/s", ItemAction::Copy),
            ("projects/-work-app/earlier", ItemAction::Copy),
            ("projects/-work-app/s", ItemAction::Copy),
            ("projects/-work-app/earlier.jsonl", ItemAction::Copy),
            ("projects/-work-app/s.jsonl", ItemAction::Copy),
        ]
    );

    execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap()).unwrap();

    assert!(personal
        .config_dir
        .join("projects/-work-app/earlier.jsonl")
        .exists());
    assert!(default
        .config_dir
        .join("projects/-work-app/earlier.jsonl")
        .exists());
}

#[test]
fn a_plan_file_the_session_wrote_moves_with_it() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    let path = transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let lines = format!(
        "{}{}\n{}\n",
        fs::read_to_string(&path).unwrap(),
        json!({ "type": "assistant", "timestamp": "2026-09-01T10:01:00Z", "slug": "bold-plan" }),
        json!({ "type": "assistant", "timestamp": "2026-09-01T10:02:00Z", "slug": "../escape" }),
    );
    write(&path, &lines);
    write(&work.config_dir.join("plans/bold-plan.md"), "# Plan");
    let homes = [work.clone(), personal.clone()];

    move_session(&work, &personal, &homes, "s").unwrap();

    assert_eq!(
        fs::read_to_string(personal.config_dir.join("plans/bold-plan.md")).unwrap(),
        "# Plan"
    );
}

#[test]
fn project_memory_is_merged_and_its_conflicts_reported() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let memory = "projects/-work-app/memory";
    write(&work.config_dir.join(memory).join("style.md"), "Tabs");
    write(&work.config_dir.join(memory).join("deploy.md"), "Vercel");
    write(
        &personal.config_dir.join(memory).join("deploy.md"),
        "Netlify",
    );
    let homes = [work.clone(), personal.clone()];

    let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

    assert_eq!(
        prepared.plan.summary,
        "Moves 3 files from Work to Personal, and 1 memory file"
    );
    assert!(
        planned(&prepared.plan).contains(&("projects/-work-app/memory/style.md", ItemAction::Copy))
    );

    let report = execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap()).unwrap();

    assert_eq!(report.memory_conflicts, ["deploy.md"]);
    assert!(personal.config_dir.join(memory).join("style.md").exists());
}

#[test]
fn what_stops_a_move_is_listed() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "cli", "2026-09-01T10:00:00Z", "/work/app");
    let scratch = work.gui_data_dir.join("local-agent-mode-sessions/x");
    record(
        &work,
        "r1",
        json!({ "cliSessionId": "scratch", "cwd": scratch.display().to_string() }),
    );
    transcript(&work, "scratch", "2026-09-01T10:00:00Z", "/tmp");
    record(&work, "r2", json!({ "cliSessionId": "gone" }));
    write(
        &work.config_dir.join("sessions/4100.json"),
        &json!({ "pid": 4100, "sessionId": "cli", "entrypoint": "cli" }).to_string(),
    );
    let homes = [work.clone(), personal.clone()];
    let blockers = |session_id| {
        plan(&work, &personal, &homes, session_id, "  4100 claude\n")
            .unwrap()
            .plan
            .blockers
    };

    assert_eq!(
        blockers("scratch"),
        ["Lives in the desktop app's scratch folder"]
    );
    assert_eq!(blockers("gone"), ["Transcript deleted"]);
    assert_eq!(blockers("cli"), ["Close it in the terminal first"]);
}

#[test]
fn a_session_moves_only_to_another_profile_of_the_same_app() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let mut codex = home(root.path(), "Codex");
    codex.app = AppKind::Codex;
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let homes = [work.clone()];

    assert!(matches!(
        plan(&work, &work, &homes, "s", ""),
        Err(AppError::Validation(_))
    ));
    assert!(matches!(
        plan(&work, &codex, &homes, "s", ""),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn the_desktop_apps_in_the_way_are_named() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    record(&work, "r1", desktop_fields());
    let homes = [work.clone(), personal.clone()];
    let ps_output = format!(
        "  900 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n  \
         901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
        work.gui_data_dir.display(),
        personal.gui_data_dir.display()
    );

    let apps = plan(&work, &personal, &homes, "s", &ps_output)
        .unwrap()
        .plan
        .apps_to_quit;

    let labels: Vec<&str> = apps.iter().map(|app| app.label.as_str()).collect();
    assert_eq!(labels, ["Claude (Work)", "Claude (Personal)"]);
}

#[test]
fn moving_again_after_restoring_at_the_source_changes_nothing_at_the_destination() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&work, WORK_ACCOUNT, WORK_ORG);
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let source_record = record(&work, "r1", desktop_fields());
    let homes = [work.clone(), personal.clone()];
    move_session(&work, &personal, &homes, "s").unwrap();
    let restore = archive::check(&work, &homes, "s", SessionAction::Restore, "").unwrap();
    archive::apply(&work, restore.target, SessionAction::Restore).unwrap();
    let dest_dir = records_dir(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    let first = read_value(&dest_dir.join("local_r1.json"));

    let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

    assert!(planned(&prepared.plan)
        .iter()
        .all(|(_, action)| *action == ItemAction::Same));
    assert_eq!(prepared.plan.desktop, DesktopAction::AlreadyListed);

    execute(prepared, "2026-09-24T08:15:00Z".parse().unwrap()).unwrap();

    assert_eq!(fs::read_dir(&dest_dir).unwrap().count(), 1);
    assert_eq!(read_value(&dest_dir.join("local_r1.json")), first);
    assert!(!personal
        .config_dir
        .join("ai-profiles-archive/.replaced")
        .exists());
    assert_eq!(read_value(&source_record)["isArchived"], json!(true));
}

#[test]
fn what_a_move_replaces_is_backed_up() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "s", "2026-09-02T10:00:00Z", "/work/app");
    transcript(&personal, "s", "2026-09-01T10:00:00Z", "/elsewhere");
    let homes = [work.clone(), personal.clone()];

    move_session(&work, &personal, &homes, "s").unwrap();

    let backup = personal
        .config_dir
        .join("ai-profiles-archive/.replaced/s/2026-09-23T08-15-00.000Z");
    assert!(
        fs::read_to_string(backup.join("projects/-work-app/s.jsonl"))
            .unwrap()
            .contains("/elsewhere")
    );
    assert!(
        fs::read_to_string(personal.config_dir.join("projects/-work-app/s.jsonl"))
            .unwrap()
            .contains("/work/app")
    );
}

/// Whether `home`, one of `homes`, lists session `session_id`, and the
/// record it lists it by.
fn listed(home: &Home, homes: &[Home], session_id: &str) -> Option<Option<DesktopRecord>> {
    owned_by(&home.id, &home_scans(homes))
        .into_iter()
        .find(|owned| owned.session_id == session_id)
        .map(|owned| owned.record)
}

/// Restores session `s` at `home`, one of `homes`.
fn restore(home: &Home, homes: &[Home]) {
    let checked = archive::check(home, homes, "s", SessionAction::Restore, "").unwrap();
    assert_eq!(checked.check.blocker, None);
    archive::apply(home, checked.target, SessionAction::Restore).unwrap();
}

#[test]
fn a_cli_session_restored_at_the_source_lists_there_and_at_the_destination() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let homes = [work.clone(), personal.clone()];
    move_session(&work, &personal, &homes, "s").unwrap();

    assert_eq!(listed(&work, &homes, "s"), None);

    restore(&work, &homes);

    assert_eq!(listed(&work, &homes, "s"), Some(None));
    assert!(listed(&personal, &homes, "s").is_some_and(|record| record.is_some()));
}

#[test]
fn a_desktop_session_restored_at_the_source_lists_in_both_by_their_own_records() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&work, WORK_ACCOUNT, WORK_ORG);
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    record(&work, "r1", desktop_fields());
    let homes = [work.clone(), personal.clone()];
    move_session(&work, &personal, &homes, "s").unwrap();

    restore(&work, &homes);

    let at_work = listed(&work, &homes, "s").flatten().unwrap();
    let at_personal = listed(&personal, &homes, "s").flatten().unwrap();
    assert!(!at_work.archived);
    assert!(at_work.path.starts_with(&work.gui_data_dir));
    assert!(!at_personal.archived);
    assert!(at_personal.path.starts_with(&personal.gui_data_dir));
}

#[test]
fn a_session_moved_back_is_active_again_where_it_started() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let homes = [work.clone(), personal.clone()];
    move_session(&work, &personal, &homes, "s").unwrap();

    let back = plan(&personal, &work, &homes, "s", "").unwrap();
    execute(back, "2026-09-24T08:15:00Z".parse().unwrap()).unwrap();

    assert_eq!(listed(&work, &homes, "s"), Some(None));
    assert_eq!(listed(&personal, &homes, "s"), None);
    assert_eq!(archived_bundles(&personal.config_dir).len(), 1);
}

/// The ids of the sessions `home`, one of `homes`, lists.
fn listed_ids(home: &Home, homes: &[Home]) -> Vec<String> {
    owned_by(&home.id, &home_scans(homes))
        .into_iter()
        .map(|owned| owned.session_id)
        .collect()
}

#[test]
fn where_no_desktop_record_is_written_only_the_shown_transcript_moves() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    let side = home(root.path(), "Side");
    fs::remove_dir(&side.gui_data_dir).unwrap();
    sign_in(&work, WORK_ACCOUNT, WORK_ORG);
    transcript(&work, "s", "2026-09-02T10:00:00Z", "/work/app");
    transcript(&work, "earlier", "2026-09-01T10:00:00Z", "/work/app");
    let mut fields = desktop_fields();
    fields["priorCliSessionIds"] = json!(["earlier"]);
    let source_record = record(&work, "r1", fields);
    let homes = [work.clone(), personal.clone(), side.clone()];

    let to_side = plan(&work, &side, &homes, "s", "").unwrap().plan;
    let prepared = plan(&work, &personal, &homes, "s", "").unwrap();

    assert_eq!(to_side.desktop, DesktopAction::NoDesktop);
    assert_eq!(prepared.plan.desktop, DesktopAction::SignInNeeded);
    let only_shown = [
        ("file-history/s", ItemAction::Copy),
        ("projects/-work-app/s", ItemAction::Copy),
        ("projects/-work-app/s.jsonl", ItemAction::Copy),
    ];
    assert_eq!(planned(&to_side), only_shown);
    assert_eq!(planned(&prepared.plan), only_shown);

    execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap()).unwrap();

    assert_eq!(listed_ids(&personal, &homes), ["s"]);
    assert!(!personal
        .config_dir
        .join("projects/-work-app/earlier.jsonl")
        .exists());
    assert_eq!(read_value(&source_record)["isArchived"], json!(true));

    restore(&work, &homes);

    let restored = owned_by(&work.id, &home_scans(&homes))
        .into_iter()
        .find(|owned| owned.session_id == "s")
        .unwrap();
    let lineage: Vec<(&str, &str)> = restored
        .claimed_transcripts
        .iter()
        .map(|held| (held.summary.session_id.as_str(), held.home_id.as_str()))
        .collect();
    assert_eq!(lineage, [("s", "Work"), ("earlier", "Work")]);
    assert!(restored.record.is_some_and(|record| !record.archived));
}

#[test]
fn only_records_of_the_account_the_destination_is_signed_in_to_list_it_there() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    // Moved here once, under the account the app was signed in to then.
    transcript(&personal, "s", "2026-09-01T10:00:00Z", "/work/app");
    let old = records_dir(&personal, WORK_ACCOUNT, WORK_ORG).join("local_old.json");
    write(&old, &json!({ "cliSessionId": "s" }).to_string());
    let homes = [work.clone(), personal.clone()];

    let unsigned = plan(&work, &personal, &homes, "s", "").unwrap().plan;

    assert_eq!(unsigned.desktop, DesktopAction::SignInNeeded);

    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    let signed = plan(&work, &personal, &homes, "s", "").unwrap().plan;

    assert_eq!(signed.desktop, DesktopAction::Add);
}

#[test]
fn a_running_destination_quits_whenever_the_move_writes_there() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let homes = [work.clone(), personal.clone()];
    let ps_output = format!(
        "  901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
        personal.gui_data_dir.display()
    );

    let plan = plan(&work, &personal, &homes, "s", &ps_output)
        .unwrap()
        .plan;

    assert_eq!(plan.desktop, DesktopAction::SignInNeeded);
    let labels: Vec<&str> = plan
        .apps_to_quit
        .iter()
        .map(|app| app.label.as_str())
        .collect();
    assert_eq!(labels, ["Claude (Personal)"]);
}

#[test]
fn nothing_is_written_when_the_destination_app_started_again_since_the_check() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    let source = transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    let homes = [work.clone(), personal.clone()];
    let prepared = plan(&work, &personal, &homes, "s", "").unwrap();
    let mut relaunched = fake_wrapper_process(&root.path().join("app"), &personal.gui_data_dir);

    let moved = execute(prepared, "2026-09-23T08:15:00Z".parse().unwrap());

    relaunched.kill().unwrap();
    relaunched.wait().unwrap();
    assert!(
        matches!(&moved, Err(AppError::Validation(message)) if message == "Claude (Personal) is running again — quit it and try again"),
        "{moved:?}"
    );
    assert!(!personal.config_dir.join("projects").exists());
    assert_eq!(read_records(&personal.gui_data_dir), []);
    assert!(source.exists());
}

#[test]
fn a_move_cut_short_after_replacing_says_where_the_backup_is() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    write(&personal.config_dir.join("file-history/s/abc@v1"), "other");
    let project = personal.config_dir.join("projects/-work-app");
    fs::create_dir_all(&project).unwrap();
    fs::set_permissions(&project, fs::Permissions::from_mode(0o555)).unwrap();
    let homes = [work.clone(), personal.clone()];

    let moved = move_session(&work, &personal, &homes, "s");

    fs::set_permissions(&project, fs::Permissions::from_mode(0o755)).unwrap();
    let message = match moved {
        Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            error.to_string()
        }
        other => panic!("{other:?}"),
    };
    assert!(
        message.contains("ai-profiles-archive/.replaced/s/2026-09-23T08-15-00.000Z"),
        "{message}"
    );
    assert!(message.starts_with("Permission denied"), "{message}");
    assert!(!project.join("s.jsonl").exists());
    assert_eq!(
        fs::read_to_string(personal.config_dir.join("file-history/s/abc@v1")).unwrap(),
        "other"
    );
}

/// Every file under `root`, with its contents, leaving out what the
/// move keeps in the archive folder.
fn outside_archive(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = tree(root);
    files.retain(|path, _| !path.starts_with(root.join("ai-profiles-archive")));
    files
}

#[test]
fn a_move_that_fails_at_the_last_step_is_taken_back_at_the_destination() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    record(&work, "r1", desktop_fields());
    // Archiving at the source can't read its index, so fails last.
    write(
        &records_dir(&work, WORK_ACCOUNT, WORK_ORG).join("archived-sessions.idx"),
        "not json",
    );
    write(&personal.config_dir.join("file-history/s/abc@v1"), "other");
    let there = records_dir(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    write(
        &there.join("local_r1.json"),
        &json!({ "cliSessionId": "s", "isArchived": true }).to_string(),
    );
    write(
        &there.join("archived-sessions.idx"),
        &json!({ "v": 1, "archived": ["local_r1"] }).to_string(),
    );
    let homes = [work.clone(), personal.clone()];
    let (at_work, at_personal) = (
        tree(&work.config_dir),
        outside_archive(&personal.config_dir),
    );
    let records_before = tree(&personal.gui_data_dir);

    let moved = move_session(&work, &personal, &homes, "s");

    let message = moved.unwrap_err().message();
    assert!(message.contains("The move was taken back"), "{message}");
    assert!(
        message.contains("ai-profiles-archive/.replaced/s/2026-09-23T08-15-00.000Z"),
        "{message}"
    );
    assert_eq!(tree(&work.config_dir), at_work);
    assert_eq!(outside_archive(&personal.config_dir), at_personal);
    assert_eq!(tree(&personal.gui_data_dir), records_before);
}

#[test]
fn a_move_backs_up_the_index_it_rewrites() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    sign_in(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG);
    transcript(&work, "s", "2026-09-01T10:00:00Z", "/work/app");
    record(&work, "r1", desktop_fields());
    let index =
        records_dir(&personal, PERSONAL_ACCOUNT, PERSONAL_ORG).join("archived-sessions.idx");
    let listed = json!({ "v": 1, "archived": ["local_r1", "local_other"] }).to_string();
    write(&index, &listed);
    let homes = [work.clone(), personal.clone()];

    move_session(&work, &personal, &homes, "s").unwrap();

    let backup = personal
        .config_dir
        .join("ai-profiles-archive/.replaced/s/2026-09-23T08-15-00.000Z");
    assert_eq!(
        fs::read_to_string(backup.join("desktop-records/archived-sessions.idx")).unwrap(),
        listed
    );
    assert_eq!(
        read_value(&index),
        json!({ "v": 1, "archived": ["local_other"] })
    );
}

#[test]
fn a_session_that_cant_have_a_record_needs_no_sign_in() {
    let root = tempdir().unwrap();
    let work = home(root.path(), "Work");
    let personal = home(root.path(), "Personal");
    let line = json!({
        "type": "user",
        "sessionId": "s",
        "timestamp": "2026-09-01T10:00:00Z",
        "message": { "role": "user", "content": "Fix the login bug" },
    });
    write(
        &work.config_dir.join("projects/-work-app/s.jsonl"),
        &format!("{line}\n"),
    );
    let homes = [work.clone(), personal.clone()];

    let plan = plan(&work, &personal, &homes, "s", "").unwrap().plan;

    assert_eq!(plan.desktop, DesktopAction::NoDesktop);
    assert_eq!(plan.notes, Vec::<String>::new());
}
