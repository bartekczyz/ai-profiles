use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io;
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};
use tempfile::tempdir;

use super::apply::{apply_with, move_exclusive, still_also_at};
use super::*;

use crate::error::AppError;
use crate::sessions::actions::{AppToQuit, SessionAction};
use crate::sessions::claude::archive_store::{archived_bundles, replaced_dir};
use crate::sessions::claude::{archive, transfer};
use crate::sessions::list::claude_sessions;
use crate::test_support::{fake_wrapper_process, opened_claude_home as home, tree};

const ACCOUNT: &str = "a99c6b36-dd42-44d7-b3ae-9496265549fd";
const ORG: &str = "527aadd2-01c3-49a6-a770-e65e047242c3";

/// When the tests' transcripts were last written.
const WRITTEN: u64 = 1_780_000_000;

/// When the tests repair.
const AT: &str = "2026-09-24T08:15:00Z";

/// How long a process listing is taken to stay current in tests that don't
/// start or stop anything while repairing.
const A_WHILE: Duration = Duration::from_secs(60);

/// Write `contents` to `path`, making its folder, dated [`WRITTEN`].
fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(written())
        .unwrap();
}

/// [`WRITTEN`], as a time.
fn written() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(WRITTEN)
}

/// Where transcript `session` of the tests is, relative to a config dir.
fn transcript_path(session: &str) -> String {
    format!("projects/-work-app/{session}.jsonl")
}

/// Gives `home` transcript `session`, with a subagent transcript beside it
/// and a file history.
fn transcript(home: &Home, session: &str) {
    let line = json!({
        "type": "user",
        "sessionId": session,
        "timestamp": "2026-09-01T10:00:00Z",
        "cwd": "/work/app",
        "message": { "role": "user", "content": "Fix the login bug" },
    });
    let config = &home.config_dir;
    write(&config.join(transcript_path(session)), &format!("{line}\n"));
    write(
        &config.join(format!(
            "projects/-work-app/{session}/subagents/agent-1.jsonl"
        )),
        "{}\n",
    );
    write(&config.join(format!("file-history/{session}/abc@v1")), "v1");
}

/// The paths of transcript `session`'s bundle, relative to a config dir.
fn bundle(session: &str) -> [String; 3] {
    [
        transcript_path(session),
        format!("projects/-work-app/{session}/subagents/agent-1.jsonl"),
        format!("file-history/{session}/abc@v1"),
    ]
}

/// Where `home`'s desktop record `local_<uuid>` is.
fn record_path(home: &Home, uuid: &str) -> PathBuf {
    home.gui_data_dir
        .join("claude-code-sessions")
        .join(ACCOUNT)
        .join(ORG)
        .join(format!("local_{uuid}.json"))
}

/// Writes `home`'s desktop record `local_<uuid>` of `fields`.
fn record(home: &Home, uuid: &str, fields: Value) {
    write(&record_path(home, uuid), &fields.to_string());
}

/// Checks the repair of `home`'s sessions, then carries it out.
fn repair(home: &Home, homes: &[Home], ps_output: &str) -> RepairReport {
    let checked = check(home, homes, ps_output).unwrap();
    apply(checked.target, AT.parse().unwrap()).unwrap()
}

/// The Default home with two transcripts Personal's desktop app started
/// there, `now` continuing `before`, and one of its own, `cli`.
fn orphaned(root: &Path) -> (Home, Home, Vec<Home>) {
    let default = home(root, "Default");
    let personal = home(root, "Personal");
    transcript(&default, "before");
    transcript(&default, "now");
    transcript(&default, "cli");
    record(
        &personal,
        "r1",
        json!({ "cliSessionId": "now", "priorCliSessionIds": ["before"], "cwd": "/work/app" }),
    );
    let homes = vec![default.clone(), personal.clone()];
    (default, personal, homes)
}

#[test]
fn repair_moves_every_transcript_a_session_claims_into_the_profile() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let record_before = fs::read(record_path(&personal, "r1")).unwrap();
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);

    let report = repair(&personal, &homes, "");

    assert_eq!(
        report,
        RepairReport {
            repaired: 1,
            ..RepairReport::default()
        }
    );
    let listed = claude_sessions(&personal, &homes, "");
    assert_eq!(listed.repair_count, 0);
    assert_eq!(listed.sessions.len(), 1);
    assert_eq!(listed.sessions[0].id, "now");
    for session in ["before", "now"] {
        for path in bundle(session) {
            let moved = personal.config_dir.join(&path);
            assert!(moved.exists(), "{path} isn't in the profile");
            assert_eq!(fs::metadata(&moved).unwrap().modified().unwrap(), written());
        }
    }
    assert_eq!(
        fs::read(record_path(&personal, "r1")).unwrap(),
        record_before
    );
    assert!(default.config_dir.join(transcript_path("cli")).exists());
}

#[test]
fn the_default_home_no_longer_holds_what_was_repaired() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());

    repair(&personal, &homes, "");

    for session in ["before", "now"] {
        for path in bundle(session) {
            assert!(
                !default.config_dir.join(&path).exists(),
                "{path} is still in Default"
            );
        }
        assert!(!default
            .config_dir
            .join(format!("projects/-work-app/{session}"))
            .exists());
    }
    assert_eq!(archived_bundles(&default.config_dir), []);
    let listed = claude_sessions(&default, &homes, "");
    let ids: Vec<&str> = listed
        .sessions
        .iter()
        .map(|session| session.id.as_str())
        .collect();
    assert_eq!(ids, ["cli"]);
}

#[test]
fn a_session_open_in_a_terminal_is_skipped_and_the_rest_repaired() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    transcript(&default, "other");
    record(&personal, "r2", json!({ "cliSessionId": "other" }));
    write(
        &default.config_dir.join("sessions/4100.json"),
        &json!({ "pid": 4100, "sessionId": "before", "entrypoint": "cli" }).to_string(),
    );

    let report = repair(&personal, &homes, "  4100 claude\n");

    assert_eq!(report.repaired, 1);
    assert_eq!(
        report.skipped,
        [SkippedSession {
            id: "now".to_string(),
            reason: "Close it in the terminal first".to_string(),
        }]
    );
    assert!(personal.config_dir.join(transcript_path("other")).exists());
    assert!(default.config_dir.join(transcript_path("now")).exists());
    assert!(default.config_dir.join(transcript_path("before")).exists());
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);
}

#[test]
fn a_second_repair_changes_nothing() {
    let root = tempdir().unwrap();
    let (_, personal, homes) = orphaned(root.path());
    repair(&personal, &homes, "");
    let before = tree(root.path());
    let running = format!(
        "  901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
        personal.gui_data_dir.display()
    );

    let checked = check(&personal, &homes, &running).unwrap();
    let report = apply(checked.target, AT.parse().unwrap()).unwrap();

    assert_eq!(checked.check, ActionCheck::default());
    assert_eq!(report, RepairReport::default());
    assert_eq!(tree(root.path()), before);
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 0);
}

#[test]
fn across_volumes_the_transcripts_are_copied_and_archived_where_they_were() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let checked = check(&personal, &homes, "").unwrap();

    let report = apply_with(
        checked.target,
        AT.parse().unwrap(),
        &mut |_, _| Err(io::ErrorKind::CrossesDevices.into()),
        &mut || Ok(String::new()),
        A_WHILE,
    )
    .unwrap();

    assert_eq!(report.repaired, 1);
    for session in ["before", "now"] {
        for path in bundle(session) {
            let copied = personal.config_dir.join(&path);
            assert_eq!(
                fs::metadata(&copied).unwrap().modified().unwrap(),
                written()
            );
            assert!(!default.config_dir.join(&path).exists());
        }
    }
    let mut archived: Vec<(String, Option<String>)> = archived_bundles(&default.config_dir)
        .into_iter()
        .map(|bundle| (bundle.session_id, bundle.title))
        .collect();
    archived.sort();
    assert_eq!(
        archived,
        [
            (
                "before".to_string(),
                Some("Fix the login bug (earlier part)".to_string())
            ),
            ("now".to_string(), Some("Fix the login bug".to_string())),
        ]
    );
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 0);
}

#[test]
fn a_session_that_cant_all_be_put_back_says_what_stays_in_the_profile() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let checked = check(&personal, &homes, "").unwrap();
    let into_profile = personal.config_dir.clone();

    // The shown transcript won't move, and then the earlier one won't go
    // back.
    let report = apply_with(
        checked.target,
        AT.parse().unwrap(),
        &mut |from, to| {
            let back = from.starts_with(&into_profile) && from.ends_with("before.jsonl");
            if from.ends_with("now.jsonl") || back {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            move_exclusive(from, to).map(drop)
        },
        &mut || Ok(String::new()),
        A_WHILE,
    )
    .unwrap();

    assert_eq!(report.repaired, 0);
    let reason = &report.skipped[0].reason;
    assert!(
        reason.contains("projects/-work-app/before.jsonl"),
        "{reason}"
    );
    assert!(
        reason.contains(&personal.config_dir.display().to_string()),
        "{reason}"
    );
    assert!(personal.config_dir.join(transcript_path("before")).exists());
    assert!(default
        .config_dir
        .join("file-history/before/abc@v1")
        .exists());
    assert!(default.config_dir.join(transcript_path("now")).exists());
}

#[test]
fn processes_are_listed_again_only_for_a_session_a_process_registered() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    transcript(&default, "other");
    record(&personal, "r2", json!({ "cliSessionId": "other" }));
    let checked = check(&personal, &homes, "").unwrap();
    write(
        &default.config_dir.join("sessions/4100.json"),
        &json!({ "pid": 4100, "sessionId": "other", "entrypoint": "cli" }).to_string(),
    );
    let mut listed = 0;

    let report = apply_with(
        checked.target,
        AT.parse().unwrap(),
        &mut |from, to| move_exclusive(from, to).map(drop),
        &mut || {
            listed += 1;
            Ok(String::new())
        },
        A_WHILE,
    )
    .unwrap();

    assert_eq!(report.repaired, 2);
    assert_eq!(listed, 2);
}

#[test]
fn sessions_left_once_the_profiles_desktop_app_started_again_are_skipped() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    transcript(&default, "other");
    record(&personal, "r2", json!({ "cliSessionId": "other" }));
    let checked = check(&personal, &homes, "").unwrap();
    let running = format!(
        "  901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
        personal.gui_data_dir.display()
    );
    let mut listed = 0;

    let report = apply_with(
        checked.target,
        AT.parse().unwrap(),
        &mut |from, to| move_exclusive(from, to).map(drop),
        &mut || {
            listed += 1;
            Ok(if listed == 1 {
                String::new()
            } else {
                running.clone()
            })
        },
        Duration::ZERO,
    )
    .unwrap();

    assert_eq!(report.repaired, 1);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(
        report.skipped[0].reason,
        "Claude (Personal) is running again — quit it and try again"
    );
}

#[test]
fn a_session_whose_processes_cant_be_listed_stays_where_it_is() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let checked = check(&personal, &homes, "").unwrap();
    write(
        &default.config_dir.join("sessions/4100.json"),
        &json!({ "pid": 4100, "sessionId": "now", "entrypoint": "cli" }).to_string(),
    );
    let mut listed = 0;

    let report = apply_with(
        checked.target,
        AT.parse().unwrap(),
        &mut |from, to| move_exclusive(from, to).map(drop),
        &mut || {
            listed += 1;
            if listed == 1 {
                return Ok(String::new());
            }
            Err(AppError::Io(io::Error::other("ps failed")))
        },
        A_WHILE,
    )
    .unwrap();

    assert_eq!(report.repaired, 0);
    assert_eq!(
        report.skipped,
        [SkippedSession {
            id: "now".to_string(),
            reason: "ps failed".to_string(),
        }]
    );
    assert!(default.config_dir.join(transcript_path("now")).exists());
}

#[test]
fn across_volumes_what_a_session_cut_short_had_copied_is_set_aside_out_of_the_backups_way() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let checked = check(&personal, &homes, "").unwrap();
    // Nothing can be archived where the transcripts are.
    write(&default.config_dir.join("ai-profiles-archive"), "");

    let report = apply_with(
        checked.target,
        AT.parse().unwrap(),
        &mut |_, _| Err(io::ErrorKind::CrossesDevices.into()),
        &mut || Ok(String::new()),
        A_WHILE,
    )
    .unwrap();

    assert_eq!(report.repaired, 0);
    let backup = replaced_dir(&personal.config_dir, "now", AT.parse().unwrap()).unwrap();
    for path in bundle("before") {
        assert!(backup.join("undone").join(&path).exists(), "{path}");
        assert!(!backup.join(&path).exists(), "{path}");
        assert!(!personal.config_dir.join(&path).exists(), "{path}");
        assert!(default.config_dir.join(&path).exists(), "{path}");
    }
}

#[test]
fn a_file_left_at_its_old_place_too_is_named_as_still_there() {
    assert_eq!(
        still_also_at(Path::new("/p/s.jsonl"), Path::new("/d/s.jsonl")),
        "/p/s.jsonl is still also at /d/s.jsonl"
    );
}

#[test]
fn a_session_another_profiles_desktop_app_has_open_is_skipped() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    write(
        &default.config_dir.join("sessions/4200.json"),
        &json!({ "pid": 4200, "sessionId": "now", "entrypoint": "claude-desktop" }).to_string(),
    );
    let running = "  4200 /Users/me/Library/Application Support/Claude/claude-code/2.1.9/claude\n";
    let before = tree(root.path());

    let report = repair(&personal, &homes, running);

    assert_eq!(report.repaired, 0);
    assert_eq!(
        report.skipped,
        [SkippedSession {
            id: "now".to_string(),
            reason: "Claude (Default) has it open".to_string(),
        }]
    );
    assert_eq!(tree(root.path()), before);
}

#[test]
fn a_session_the_profile_has_different_files_of_is_skipped_untouched() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    write(
        &personal.config_dir.join("file-history/before/abc@v1"),
        "theirs",
    );
    let before = tree(root.path());

    let report = repair(&personal, &homes, "");

    assert_eq!(report.repaired, 0);
    assert_eq!(
        report.skipped,
        [SkippedSession {
            id: "now".to_string(),
            reason: "Personal already has different files of it".to_string(),
        }]
    );
    assert_eq!(tree(root.path()), before);
    assert!(default.config_dir.join(transcript_path("now")).exists());
}

#[test]
fn a_transcript_another_profiles_desktop_app_lists_too_stays_where_it_is() {
    let root = tempdir().unwrap();
    let (default, personal, mut homes) = orphaned(root.path());
    let work = home(root.path(), "Work");
    record(&work, "w1", json!({ "cliSessionId": "now" }));
    homes.push(work);

    let report = repair(&personal, &homes, "");

    assert_eq!(report.repaired, 0);
    assert_eq!(
        report.skipped,
        [SkippedSession {
            id: "now".to_string(),
            reason: "Claude (Work) lists it too".to_string(),
        }]
    );
    assert!(default.config_dir.join(transcript_path("now")).exists());
}

/// Signs `home`'s desktop app in to the tests' account.
fn sign_in(home: &Home) {
    write(
        &home.gui_data_dir.join("config.json"),
        &json!({ "lastKnownAccountUuid": ACCOUNT }).to_string(),
    );
    write(
        &home.config_dir.join(".claude.json"),
        &json!({ "oauthAccount": { "accountUuid": ACCOUNT, "organizationUuid": ORG } }).to_string(),
    );
}

#[test]
fn a_session_restored_after_moving_it_away_is_repaired_and_the_banner_clears() {
    let root = tempdir().unwrap();
    let (default, personal, mut homes) = orphaned(root.path());
    let work = home(root.path(), "Work");
    sign_in(&work);
    homes.push(work.clone());
    let moving = transfer::plan(&personal, &work, &homes, "now", "").unwrap();
    transfer::execute(moving, AT.parse().unwrap()).unwrap();
    let restoring = archive::check(&personal, &homes, "now", SessionAction::Restore, "").unwrap();
    archive::apply(&personal, restoring.target, SessionAction::Restore).unwrap();
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);
    let at_work = tree(&work.config_dir);

    let report = repair(&personal, &homes, "");

    assert_eq!(
        report,
        RepairReport {
            repaired: 1,
            ..RepairReport::default()
        }
    );
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 0);
    for session in ["before", "now"] {
        assert!(personal.config_dir.join(transcript_path(session)).exists());
        assert!(!default.config_dir.join(transcript_path(session)).exists());
    }
    assert_eq!(tree(&work.config_dir), at_work);
    let listed = claude_sessions(&work, &homes, "");
    assert_eq!(listed.repair_count, 0);
    assert!(listed.sessions.iter().any(|session| session.id == "now"));
}

#[test]
fn an_archived_record_in_another_profile_doesnt_keep_a_session_from_repair() {
    let root = tempdir().unwrap();
    let (default, personal, mut homes) = orphaned(root.path());
    let work = home(root.path(), "Work");
    record(
        &work,
        "w1",
        json!({ "cliSessionId": "now", "isArchived": true }),
    );
    homes.push(work);

    let report = repair(&personal, &homes, "");

    assert_eq!(report.repaired, 1);
    assert!(!default.config_dir.join(transcript_path("now")).exists());
}

#[test]
fn a_copy_an_archived_record_claims_as_its_own_stays_in_its_profile_and_needs_no_repair() {
    let root = tempdir().unwrap();
    let personal = home(root.path(), "Personal");
    let work = home(root.path(), "Work");
    transcript(&personal, "s");
    record(
        &personal,
        "p1",
        json!({ "cliSessionId": "s", "isArchived": true }),
    );
    record(&work, "w1", json!({ "cliSessionId": "s" }));
    let homes = vec![personal.clone(), work.clone()];
    let at_personal = tree(&personal.config_dir);

    assert_eq!(claude_sessions(&work, &homes, "").repair_count, 0);

    let report = repair(&work, &homes, "");

    assert_eq!(report, RepairReport::default());
    assert_eq!(tree(&personal.config_dir), at_personal);
    assert!(!work.config_dir.join(transcript_path("s")).exists());
}

#[test]
fn project_memory_and_plans_come_along_and_memory_conflicts_are_reported() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let memory = "projects/-work-app/memory";
    write(&default.config_dir.join(memory).join("style.md"), "Tabs");
    write(&default.config_dir.join(memory).join("deploy.md"), "Vercel");
    write(
        &personal.config_dir.join(memory).join("deploy.md"),
        "Netlify",
    );
    let now = default.config_dir.join(transcript_path("now"));
    let lines = format!(
        "{}{}\n",
        fs::read_to_string(&now).unwrap(),
        json!({ "type": "assistant", "timestamp": "2026-09-01T10:01:00Z", "slug": "bold-plan" })
    );
    write(&now, &lines);
    write(&default.config_dir.join("plans/bold-plan.md"), "# Plan");

    let report = repair(&personal, &homes, "");

    assert_eq!(report.repaired, 1);
    assert_eq!(report.memory_conflicts, ["deploy.md"]);
    assert_eq!(
        fs::read_to_string(personal.config_dir.join(memory).join("style.md")).unwrap(),
        "Tabs"
    );
    assert_eq!(
        fs::read_to_string(personal.config_dir.join(memory).join("deploy.md")).unwrap(),
        "Netlify"
    );
    assert!(default.config_dir.join(memory).join("style.md").exists());
    assert_eq!(
        fs::read_to_string(personal.config_dir.join("plans/bold-plan.md")).unwrap(),
        "# Plan"
    );
}

#[test]
fn only_the_profiles_own_desktop_app_has_to_quit_and_only_when_something_moves() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let running = format!(
        "  900 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n  \
         901 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={}\n",
        default.gui_data_dir.display(),
        personal.gui_data_dir.display()
    );

    let checked = check(&personal, &homes, &running).unwrap();

    assert_eq!(
        checked.check,
        ActionCheck {
            blocker: None,
            app_to_quit: Some(AppToQuit::of(&personal)),
        }
    );
    assert_eq!(
        check(&default, &homes, &running).unwrap().check,
        ActionCheck::default()
    );
}

#[test]
fn nothing_moves_when_the_profiles_desktop_app_started_again_since_the_check() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let checked = check(&personal, &homes, "").unwrap();
    let before = tree(root.path());
    let mut relaunched = fake_wrapper_process(&root.path().join("app"), &personal.gui_data_dir);

    let repaired = apply(checked.target, AT.parse().unwrap());

    relaunched.kill().unwrap();
    relaunched.wait().unwrap();
    assert!(
        matches!(&repaired, Err(AppError::Validation(message)) if message == "Claude (Personal) is running again — quit it and try again"),
        "{repaired:?}"
    );
    let mut after = tree(root.path());
    after.retain(|path, _| !path.starts_with(root.path().join("app")));
    assert_eq!(after, before);
    assert!(default.config_dir.join(transcript_path("now")).exists());
}

#[test]
fn two_sessions_that_continued_the_same_transcript_are_both_repaired() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    transcript(&default, "later");
    record(
        &personal,
        "r2",
        json!({ "cliSessionId": "later", "priorCliSessionIds": ["before"], "cwd": "/work/app" }),
    );

    let report = repair(&personal, &homes, "");

    assert_eq!(
        report,
        RepairReport {
            repaired: 2,
            ..RepairReport::default()
        }
    );
    for session in ["before", "now", "later"] {
        for path in bundle(session) {
            assert!(
                personal.config_dir.join(&path).exists(),
                "{path} isn't in the profile"
            );
            assert!(
                !default.config_dir.join(&path).exists(),
                "{path} is still in Default"
            );
        }
    }
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 0);
}

#[test]
fn an_archived_session_is_neither_counted_nor_repaired() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    transcript(&default, "shelved");
    record(
        &personal,
        "r2",
        json!({ "cliSessionId": "shelved", "isArchived": true }),
    );
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);

    let report = repair(&personal, &homes, "");

    assert_eq!(
        report,
        RepairReport {
            repaired: 1,
            ..RepairReport::default()
        }
    );
    assert!(default.config_dir.join(transcript_path("shelved")).exists());
    assert!(!personal
        .config_dir
        .join(transcript_path("shelved"))
        .exists());
}

#[test]
fn a_session_moves_whole_or_stays_where_it_was() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    // The profile has a file history folder already, empty.
    fs::create_dir_all(personal.config_dir.join("file-history")).unwrap();
    let checked = check(&personal, &homes, "").unwrap();
    let before = tree(root.path());

    // The shown transcript moves last, after the earlier one has.
    let report = apply_with(
        checked.target,
        AT.parse().unwrap(),
        &mut |from, to| {
            if from.ends_with("now.jsonl") {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            move_exclusive(from, to).map(drop)
        },
        &mut || Ok(String::new()),
        A_WHILE,
    )
    .unwrap();

    assert_eq!(report.repaired, 0);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].id, "now");
    assert!(
        report.skipped[0].reason.starts_with("permission denied"),
        "{}",
        report.skipped[0].reason
    );
    assert_eq!(tree(root.path()), before);
    assert!(default.config_dir.join(transcript_path("before")).exists());
    assert_eq!(claude_sessions(&personal, &homes, "").repair_count, 1);
    assert!(!personal.config_dir.join("projects").exists());
    assert!(!personal.config_dir.join("file-history/before").exists());
    assert!(personal.config_dir.join("file-history").is_dir());
}

#[test]
fn a_session_claiming_a_transcript_with_an_unsafe_id_is_skipped() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    transcript(&default, ".hidden");
    record(
        &personal,
        "r1",
        json!({ "cliSessionId": "now", "priorCliSessionIds": ["before", ".hidden"], "cwd": "/work/app" }),
    );
    let before = tree(root.path());

    let report = repair(&personal, &homes, "");

    assert_eq!(report.repaired, 0);
    assert_eq!(
        report.skipped,
        [SkippedSession {
            id: "now".to_string(),
            reason: "Its transcript \".hidden\" has an id that isn't safe to move".to_string(),
        }]
    );
    assert_eq!(tree(root.path()), before);
}

#[test]
fn a_session_opened_in_a_terminal_since_the_check_is_skipped() {
    let root = tempdir().unwrap();
    let (default, personal, homes) = orphaned(root.path());
    let checked = check(&personal, &homes, "").unwrap();
    write(
        &default.config_dir.join("sessions/4100.json"),
        &json!({ "pid": 4100, "sessionId": "now", "entrypoint": "cli" }).to_string(),
    );

    let report = apply_with(
        checked.target,
        AT.parse().unwrap(),
        &mut |from, to| move_exclusive(from, to).map(drop),
        &mut || Ok("  4100 claude\n".to_string()),
        A_WHILE,
    )
    .unwrap();

    assert_eq!(report.repaired, 0);
    assert_eq!(
        report.skipped,
        [SkippedSession {
            id: "now".to_string(),
            reason: "Close it in the terminal first".to_string(),
        }]
    );
    assert!(default.config_dir.join(transcript_path("now")).exists());
    assert!(default.config_dir.join(transcript_path("before")).exists());
}

#[test]
fn a_transcript_whose_home_is_unknown_is_a_reason_to_skip() {
    let root = tempdir().unwrap();
    let (_, personal, homes) = orphaned(root.path());
    let scans = home_scans(&homes);
    let owned = owned_by(&personal.id, &scans)
        .into_iter()
        .find(|owned| owned.session_id == "now")
        .unwrap();
    let only_personal = [personal.clone()];
    let context = Context {
        home: &personal,
        homes: &only_personal,
        live: &[],
        listers: &HashMap::new(),
        kept: &HashSet::new(),
    };

    let repaired = session_repair(&context, &owned);

    assert_eq!(
        repaired.map(|repair| repair.session_id),
        Err("Its transcript \"before\" is in a profile that's gone".to_string())
    );
}

#[test]
fn a_copy_another_profile_keeps_archived_is_said_to_be_kept_so() {
    let root = tempdir().unwrap();
    let (default, personal, mut homes) = orphaned(root.path());
    let work = home(root.path(), "Work");
    homes.push(work);
    let scans = home_scans(&homes);
    let owned = owned_by(&personal.id, &scans)
        .into_iter()
        .find(|owned| owned.session_id == "now")
        .unwrap();
    let copy = ("now".to_string(), default.id.clone());
    let listers = HashMap::from([(copy.clone(), vec!["Work".to_string()])]);
    let kept = HashSet::from([("now".to_string(), "Work".to_string())]);
    let context = Context {
        home: &personal,
        homes: &homes,
        live: &[],
        listers: &listers,
        kept: &kept,
    };

    let repaired = session_repair(&context, &owned);

    assert_eq!(
        repaired.map(|repair| repair.session_id),
        Err("Claude (Work) keeps it archived".to_string())
    );
}

#[test]
fn a_repair_report_crosses_the_bridge_in_camel_case() {
    let report = RepairReport {
        repaired: 2,
        skipped: vec![SkippedSession {
            id: "s".to_string(),
            reason: "Close it in the terminal first".to_string(),
        }],
        memory_conflicts: vec!["deploy.md".to_string()],
        warnings: vec!["/p/a.jsonl is still also at /a.jsonl".to_string()],
    };

    assert_eq!(
        serde_json::to_value(report).unwrap(),
        json!({
            "repaired": 2,
            "skipped": [{ "id": "s", "reason": "Close it in the terminal first" }],
            "memoryConflicts": ["deploy.md"],
            "warnings": ["/p/a.jsonl is still also at /a.jsonl"],
        })
    );
}
