//! When a profile was last used, as its data dir shows it.
//!
//! `last_used_at` is stamped only by the in-app Open and Copy CLI command. A
//! profile opened from its own launcher or `<cli>-<slug>` wrapper never reports
//! back, but the app it runs leaves traces: Chromium writes the desktop
//! user-data dir on every launch and quit, and each CLI session writes a
//! transcript. A profile's last use is the later of the two.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Utc};

use crate::app_kind::AppKind;
use crate::paths::{profile_dir, refresh_cwd_dir};
use crate::profiles::Profile;

/// `profile`, its `last_used_at` moved up to its newest activity on disk when
/// that is later. Anything that can't be read leaves the stamp as it was.
pub fn with_activity(mut profile: Profile) -> Profile {
    let Ok(data_dir) = profile_dir(&profile.id) else {
        return profile;
    };
    let refresh = refresh_cwd_dir().ok().map(|dir| project_dir_name(&dir));
    let seen = last_activity(profile.app, &data_dir, refresh.as_deref());
    profile.last_used_at = later(profile.last_used_at, seen);
    profile
}

/// The later of a `stamp` and activity `seen`. A stamp that doesn't parse
/// loses to any activity.
fn later(stamp: Option<String>, seen: Option<DateTime<Utc>>) -> Option<String> {
    let Some(seen) = seen else {
        return stamp;
    };
    let stamped = stamp
        .as_deref()
        .and_then(|stamp| DateTime::parse_from_rfc3339(stamp).ok());
    match stamped {
        Some(stamped) if stamped >= seen => stamp,
        _ => Some(seen.to_rfc3339()),
    }
}

/// The newest activity under a profile's `data_dir`: its desktop user-data dir
/// or a CLI transcript. Claude transcripts in the `skipped` project folder are
/// left out: they are the token refresh's, not the user's.
fn last_activity(app: AppKind, data_dir: &Path, skipped: Option<&str>) -> Option<DateTime<Utc>> {
    let config = data_dir.join("cli-config");
    let transcripts = match app {
        // projects/<project>/<session>.jsonl
        AppKind::Claude => newest_transcript(&config.join("projects"), 2, skipped.map(OsStr::new)),
        // sessions/<yyyy>/<mm>/<dd>/rollout-….jsonl
        AppKind::Codex => newest_transcript(&config.join("sessions"), 4, None),
    };
    newest_entry(&data_dir.join("gui-data"))
        .max(transcripts)
        .map(DateTime::from)
}

/// When `dir` or anything directly in it last changed. Chromium creates and
/// removes its `Singleton*` links there on launch and quit, and keeps writing
/// files there while it runs. Links are not followed: `SingletonLock` points
/// nowhere.
fn newest_entry(dir: &Path) -> Option<SystemTime> {
    let own = modified(dir);
    let entries = fs::read_dir(dir).into_iter().flatten().flatten();
    entries
        .filter_map(|entry| modified(&entry.path()))
        .max()
        .max(own)
}

/// When the newest `.jsonl` file at most `depth` levels under `dir` was last
/// written, leaving out any folder named `skipped`.
fn newest_transcript(dir: &Path, depth: usize, skipped: Option<&OsStr>) -> Option<SystemTime> {
    let entries = fs::read_dir(dir).into_iter().flatten().flatten();
    entries
        .filter_map(|entry| {
            let path = entry.path();
            let kind = entry.file_type().ok()?;
            if kind.is_dir() {
                let descend = depth > 1 && Some(entry.file_name().as_os_str()) != skipped;
                descend.then(|| newest_transcript(&path, depth - 1, skipped))?
            } else if path.extension() == Some(OsStr::new("jsonl")) {
                modified(&path)
            } else {
                None
            }
        })
        .max()
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::symlink_metadata(path).ok()?.modified().ok()
}

/// The folder Claude Code keeps the transcripts of sessions run in `cwd` in,
/// under `projects/`: the path with everything but ASCII letters and digits
/// turned into `-`.
fn project_dir_name(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn utc(secs: u64) -> DateTime<Utc> {
        DateTime::from(at(secs))
    }

    /// Writes `path` (and its parents), last modified at `secs`.
    fn write_at(path: &Path, secs: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "{}\n").unwrap();
        set_at(path, secs);
    }

    fn set_at(path: &Path, secs: u64) {
        fs::File::open(path)
            .unwrap()
            .set_modified(at(secs))
            .unwrap();
    }

    /// A data dir with a desktop user-data dir last changed at `secs`.
    fn data_dir(secs: u64) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let gui_data = root.path().join("gui-data");
        fs::create_dir_all(&gui_data).unwrap();
        set_at(&gui_data, secs);
        root
    }

    #[test]
    fn an_untouched_profile_has_no_activity() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(last_activity(AppKind::Claude, root.path(), None), None);
    }

    #[test]
    fn a_desktop_launch_shows_in_the_user_data_dir() {
        let root = data_dir(100);
        write_at(&root.path().join("gui-data/Cookies"), 500);
        set_at(&root.path().join("gui-data"), 100);
        assert_eq!(
            last_activity(AppKind::Claude, root.path(), None),
            Some(utc(500))
        );
    }

    #[test]
    fn the_newest_claude_transcript_counts() {
        let root = data_dir(100);
        let projects = root.path().join("cli-config/projects");
        write_at(&projects.join("-work-app/old.jsonl"), 200);
        write_at(&projects.join("-work-site/new.jsonl"), 900);
        assert_eq!(
            last_activity(AppKind::Claude, root.path(), None),
            Some(utc(900))
        );
    }

    #[test]
    fn token_refresh_transcripts_are_not_activity() {
        let root = data_dir(100);
        let projects = root.path().join("cli-config/projects");
        write_at(&projects.join("-work-app/s.jsonl"), 300);
        write_at(&projects.join("-ai-profiles-refresh-cwd/s.jsonl"), 900);
        let seen = last_activity(
            AppKind::Claude,
            root.path(),
            Some("-ai-profiles-refresh-cwd"),
        );
        assert_eq!(seen, Some(utc(300)));
    }

    #[test]
    fn other_cli_config_files_are_not_activity() {
        let root = data_dir(100);
        write_at(&root.path().join("cli-config/.claude.json"), 900);
        assert_eq!(
            last_activity(AppKind::Claude, root.path(), None),
            Some(utc(100))
        );
    }

    #[test]
    fn the_newest_codex_rollout_counts() {
        let root = data_dir(100);
        let sessions = root.path().join("cli-config/sessions");
        write_at(&sessions.join("2026/11/01/rollout-a.jsonl"), 400);
        write_at(&sessions.join("2026/10/30/rollout-b.jsonl"), 700);
        assert_eq!(
            last_activity(AppKind::Codex, root.path(), None),
            Some(utc(700))
        );
    }

    #[test]
    fn the_later_of_stamp_and_activity_wins() {
        let stamp = || Some("1970-01-01T00:10:00+00:00".to_string());
        assert_eq!(later(stamp(), None), stamp());
        assert_eq!(later(stamp(), Some(utc(300))), stamp());
        assert_eq!(later(stamp(), Some(utc(900))), Some(utc(900).to_rfc3339()));
        assert_eq!(later(None, Some(utc(300))), Some(utc(300).to_rfc3339()));
        assert_eq!(
            later(Some("garbled".into()), Some(utc(300))),
            Some(utc(300).to_rfc3339())
        );
    }

    #[test]
    fn project_dir_name_dashes_everything_but_letters_and_digits() {
        let cwd = Path::new("/Users/me/Library/Application Support/ai-profiles/refresh-cwd");
        assert_eq!(
            project_dir_name(cwd),
            "-Users-me-Library-Application-Support-ai-profiles-refresh-cwd"
        );
    }
}
