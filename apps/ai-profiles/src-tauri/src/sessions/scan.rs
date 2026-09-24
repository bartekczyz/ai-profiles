//! Reading a profile's transcripts into a list of sessions.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(crate) use ai_profiles_core::transcript::{
    is_safe_name, read_transcript, transcripts, TranscriptInfo,
};
use serde::Serialize;

use super::desktop::{self, DesktopRecord};
use super::{parse_process_list, running_sessions, Home, RunningSession};
use crate::app_kind::{default_id, AppKind};
use crate::error::AppResult;
use crate::launch::process_list;
use crate::profiles;

/// One session as the Sessions list shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    /// The folder the session last worked in.
    pub cwd: Option<String>,
    /// The name the desktop app shows, else the one set with `/rename`, else
    /// Claude's generated one.
    pub title: Option<String>,
    /// The last thing typed into the session.
    pub last_prompt: Option<String>,
    /// When the transcript was last written, RFC 3339.
    pub updated_at: String,
    pub size_bytes: u64,
    /// A `claude` process has the session open right now.
    pub running: bool,
    /// That process is the desktop app's, which holds a session open until it
    /// quits.
    pub open_in_desktop: bool,
    /// The profile's desktop app lists the session.
    pub in_desktop: bool,
    /// Why the session cannot be moved, if it cannot.
    pub unmovable_reason: Option<String>,
    /// The profile's desktop app lists the session, but its transcript is in
    /// the stock install's folder, where the app wrote it before it had a
    /// config dir of its own. Moving it from there brings it into the profile.
    pub left_in_default: bool,
}

/// Why a session in `home` with working folder `cwd` cannot be moved, if it
/// cannot. A scratch-workspace session works in a folder inside the source
/// profile's desktop data, which the destination has no copy of.
pub(crate) fn unmovable_reason(home: &Home, cwd: Option<&str>) -> Option<String> {
    let scratch = home.gui_data_dir.join("scratch-workspaces");
    match cwd {
        Some(cwd) if Path::new(cwd).starts_with(&scratch) => Some(
            "It works in a scratch folder of this profile's desktop app, which can't be moved yet."
                .to_string(),
        ),
        _ => None,
    }
}

/// The sessions of `home`, newest first. Sessions nothing happened in are left
/// out.
///
/// Before profiles' desktop apps had a config dir of their own, a Code tab
/// session kept its record in the profile but wrote its transcript to the
/// stock install's. Such a session is listed under the profile that has its
/// record, marked [`SessionSummary::left_in_default`], and not under the stock
/// install, unless the stock app lists it too.
pub fn list(home: &Home) -> AppResult<Vec<SessionSummary>> {
    let processes = parse_process_list(&process_list()?);
    if home.stock {
        let hidden = left_in_stock_by_profiles(home)?;
        Ok(list_with(home, None, &hidden, &processes))
    } else {
        let stock = super::home(&default_id(AppKind::Claude)).ok();
        Ok(list_with(home, stock.as_ref(), &HashSet::new(), &processes))
    }
}

/// The sessions of `home`, less `hidden`, plus, given the `stock` install,
/// those `home`'s desktop app left there.
fn list_with(
    home: &Home,
    stock: Option<&Home>,
    hidden: &HashSet<String>,
    processes: &HashMap<i32, String>,
) -> Vec<SessionSummary> {
    let running = running_sessions(&home.config_dir, processes);
    let records = live_records(home);

    let mut sessions: Vec<(SystemTime, SessionSummary)> = Vec::new();
    let own = transcripts(&home.config_dir);
    let own_ids: HashSet<&str> = own.iter().map(|(_, id, _)| id.as_str()).collect();
    for (_, id, path) in &own {
        if hidden.contains(id) {
            continue;
        }
        if let Some(session) = summary(home, id, path, records.get(id), &running, false) {
            sessions.push(session);
        }
    }

    if let Some(stock) = stock {
        let stock_running = running_sessions(&stock.config_dir, processes);
        let in_stock = unique_transcripts(&stock.config_dir);
        for (id, record) in &records {
            if record.archived || own_ids.contains(id.as_str()) {
                continue;
            }
            let Some(path) = in_stock.get(id) else {
                continue;
            };
            if let Some(session) = summary(home, id, path, Some(record), &stock_running, true) {
                sessions.push(session);
            }
        }
    }

    sessions.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    sessions.into_iter().map(|(_, session)| session).collect()
}

/// `home`'s desktop records by session. A session can have an archived record
/// and a live one; the live one wins.
fn live_records(home: &Home) -> HashMap<String, DesktopRecord> {
    let mut records: HashMap<String, DesktopRecord> = HashMap::new();
    for record in desktop::records(&home.gui_data_dir) {
        let replaces = records
            .get(&record.cli_session_id)
            .is_none_or(|kept| kept.archived && !record.archived);
        if replaces {
            records.insert(record.cli_session_id.clone(), record);
        }
    }
    records
}

/// One row for the session `id` whose transcript is at `path`, with when it was
/// last written; `None` if the transcript can't be read or nothing happened in
/// the session.
fn summary(
    home: &Home,
    id: &str,
    path: &Path,
    record: Option<&DesktopRecord>,
    running: &[RunningSession],
    left_in_default: bool,
) -> Option<(SystemTime, SessionSummary)> {
    let metadata = fs::metadata(path).ok()?;
    let info = read_transcript(path).ok()?;
    if info.is_empty() {
        return None;
    }
    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    Some((
        modified,
        SessionSummary {
            title: record
                .and_then(|record| record.title.clone())
                .or_else(|| info.title()),
            unmovable_reason: unmovable_reason(home, info.cwd.as_deref()),
            running: running.iter().any(|open| open.session_id == id),
            open_in_desktop: running
                .iter()
                .any(|open| open.session_id == id && open.desktop),
            in_desktop: record.is_some_and(|record| !record.archived),
            cwd: info.cwd,
            last_prompt: info.last_prompt,
            updated_at: chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339(),
            size_bytes: metadata.len(),
            id: id.to_string(),
            left_in_default,
        },
    ))
}

/// Transcripts in `config_dir` by session, leaving out a session with more
/// than one: there is no telling which is current.
fn unique_transcripts(config_dir: &Path) -> HashMap<String, PathBuf> {
    let mut found: HashMap<String, Option<PathBuf>> = HashMap::new();
    for (_, id, path) in transcripts(config_dir) {
        found
            .entry(id)
            .and_modify(|kept| *kept = None)
            .or_insert(Some(path));
    }
    found
        .into_iter()
        .filter_map(|(id, path)| Some((id, path?)))
        .collect()
}

/// Sessions in the `stock` install that a managed Claude profile's desktop app
/// lists and the stock app doesn't: that profile lists them instead. Only ones
/// with a single transcript, the ones the profile can list.
fn left_in_stock_by_profiles(stock: &Home) -> AppResult<HashSet<String>> {
    let homes: Vec<Home> = profiles::load()?
        .into_iter()
        .filter(|profile| profile.app == AppKind::Claude)
        .filter_map(|profile| super::home(&profile.id).ok())
        .collect();
    Ok(left_in_stock(stock, &homes))
}

/// Pure over the file system: the ids [`left_in_stock_by_profiles`] means,
/// given the profiles' `homes`.
fn left_in_stock(stock: &Home, homes: &[Home]) -> HashSet<String> {
    let stock_listed: HashSet<String> = desktop::records(&stock.gui_data_dir)
        .into_iter()
        .filter(|record| !record.archived)
        .map(|record| record.cli_session_id)
        .collect();
    let in_stock = unique_transcripts(&stock.config_dir);
    let mut left = HashSet::new();
    for home in homes {
        let own: HashSet<String> = transcripts(&home.config_dir)
            .into_iter()
            .map(|(_, id, _)| id)
            .collect();
        left.extend(
            live_records(home)
                .into_values()
                .filter(|record| !record.archived)
                .map(|record| record.cli_session_id)
                .filter(|id| {
                    in_stock.contains_key(id) && !own.contains(id) && !stock_listed.contains(id)
                }),
        );
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_workspace_sessions_are_unmovable() {
        let home = Home {
            id: "p".into(),
            label: "P".into(),
            config_dir: "/p/cli-config".into(),
            gui_data_dir: "/p/gui-data".into(),
            stock: false,
            desktop: true,
        };
        assert!(unmovable_reason(&home, Some("/p/gui-data/scratch-workspaces/a/b/s")).is_some());
        assert!(unmovable_reason(&home, Some("/Users/x/code")).is_none());
        assert!(unmovable_reason(&home, None).is_none());
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// A profile's or the stock install's home under `root`, with the
    /// transcripts (`id`s) and desktop records (`(id, archived)`) given.
    fn home_with(root: &Path, stock: bool, transcripts: &[&str], records: &[(&str, bool)]) -> Home {
        let home = Home {
            id: if stock {
                "default:claude".into()
            } else {
                "p".into()
            },
            label: if stock { "Default".into() } else { "P".into() },
            config_dir: root.join("cli-config"),
            gui_data_dir: root.join("gui-data"),
            stock,
            desktop: true,
        };
        for id in transcripts {
            write(
                &home.config_dir.join(format!("projects/-work/{id}.jsonl")),
                "{\"type\":\"user\",\"cwd\":\"/work\"}\n{\"type\":\"assistant\"}\n",
            );
        }
        for (id, archived) in records {
            write(
                &home
                    .gui_data_dir
                    .join(format!("claude-code-sessions/acct/org/local_{id}.json")),
                &format!(r#"{{"cliSessionId":"{id}","isArchived":{archived},"title":"T {id}"}}"#),
            );
        }
        home
    }

    fn ids(sessions: &[SessionSummary]) -> Vec<(String, bool)> {
        let mut ids: Vec<(String, bool)> = sessions
            .iter()
            .map(|session| (session.id.clone(), session.left_in_default))
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn a_profile_lists_what_its_desktop_app_left_in_the_stock_folder() {
        let dir = tempfile::tempdir().unwrap();
        let stock = home_with(&dir.path().join("stock"), true, &["left", "stock-own"], &[]);
        let profile = home_with(
            &dir.path().join("p"),
            false,
            &["own"],
            &[("left", false), ("own", false), ("gone", false)],
        );
        let sessions = list_with(&profile, Some(&stock), &HashSet::new(), &HashMap::new());
        assert_eq!(
            ids(&sessions),
            vec![("left".into(), true), ("own".into(), false)],
            "its own, and the one left behind; not one whose transcript is nowhere"
        );
        let left = sessions
            .iter()
            .find(|session| session.left_in_default)
            .unwrap();
        assert!(left.in_desktop);
        assert_eq!(left.title.as_deref(), Some("T left"));
    }

    #[test]
    fn an_archived_record_brings_nothing_over() {
        let dir = tempfile::tempdir().unwrap();
        let stock = home_with(&dir.path().join("stock"), true, &["left"], &[]);
        let profile = home_with(&dir.path().join("p"), false, &[], &[("left", true)]);
        assert!(list_with(&profile, Some(&stock), &HashSet::new(), &HashMap::new()).is_empty());
        assert!(left_in_stock(&stock, &[profile]).is_empty());
    }

    #[test]
    fn the_stock_install_leaves_out_what_a_profile_lists_unless_its_own_app_does_too() {
        let dir = tempfile::tempdir().unwrap();
        let stock = home_with(
            &dir.path().join("stock"),
            true,
            &["left", "shared", "stock-own", "copied"],
            &[("shared", false)],
        );
        let profile = home_with(
            &dir.path().join("p"),
            false,
            &["copied"],
            &[("left", false), ("shared", false), ("copied", false)],
        );
        let hidden = left_in_stock(&stock, &[profile]);
        assert_eq!(hidden, HashSet::from(["left".to_string()]));
        assert_eq!(
            ids(&list_with(&stock, None, &hidden, &HashMap::new())),
            vec![
                ("copied".into(), false),
                ("shared".into(), false),
                ("stock-own".into(), false)
            ],
            "a copy the profile has too, and one both apps list, stay"
        );
    }

    #[test]
    fn a_session_with_two_stock_transcripts_is_not_guessed_at() {
        let dir = tempfile::tempdir().unwrap();
        let stock = home_with(&dir.path().join("stock"), true, &["twice"], &[]);
        write(
            &stock.config_dir.join("projects/-other/twice.jsonl"),
            "{\"type\":\"assistant\"}\n",
        );
        let profile = home_with(&dir.path().join("p"), false, &[], &[("twice", false)]);
        assert!(list_with(&profile, Some(&stock), &HashSet::new(), &HashMap::new()).is_empty());
        assert!(
            left_in_stock(&stock, &[profile]).is_empty(),
            "so the stock install keeps listing it"
        );
    }
}
