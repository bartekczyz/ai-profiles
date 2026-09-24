//! Repairing desktop sessions started before profiles had their own folder.
//!
//! Before profiles pointed their desktop app at their own config dir, a
//! profile's desktop app wrote the transcripts of its Code tab sessions into
//! the stock `~/.claude`. Its records of them stay in the profile's data dir,
//! and the app now looks for their transcripts in the profile's config dir,
//! where they aren't, so it opens them empty. [`check`] finds those sessions
//! and [`apply()`] moves every transcript each one claims (see
//! [`super::ownership`]) into the profile's config dir, at the same path
//! relative to it, bringing the project's memory and the plans the
//! transcripts wrote along. The desktop records are left as they are.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::archive_store::{is_session_dir_name, ArchivedBundle};
use super::copy::compare;
use super::live::{live_sessions, LiveHolder};
use super::ownership::{
    claimed_copy, claimed_ids, copies, kept_archived, needs_repair, owned_by, HeldTranscript,
    HomeScan, Owned,
};
use super::transcript::bundle_paths;
use super::transfer::{memory_merges, relative_to, MemoryMerge, PLANS_DIR};
use crate::error::AppResult;
use crate::sessions::actions::{ActionCheck, AppToQuit, Checked};
use crate::sessions::fs_ops::occupied;
use crate::sessions::instance::{desktop_label, desktop_pid};
use crate::sessions::list::{home_scans, transcript_title, OPEN_IN_TERMINAL};
use crate::sessions::move_plan::ItemAction;
use crate::sessions::Home;

mod apply;

pub use apply::apply;

/// A session a repair left as it was, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedSession {
    /// The session's id.
    pub id: String,
    /// Why it was left as it was.
    pub reason: String,
}

/// What a repair did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    /// How many sessions were repaired.
    pub repaired: u32,
    /// The sessions that needed repair but were left as they were.
    pub skipped: Vec<SkippedSession>,
    /// The memory files both folders have, differently; the profile's own
    /// were kept.
    pub memory_conflicts: Vec<String>,
    /// What the repair did that it didn't mean to, such as a copy it left
    /// behind, for the user to tidy.
    pub warnings: Vec<String>,
}

/// One transcript to move into the repaired home's config dir.
#[derive(Debug, Clone)]
struct TranscriptMove {
    /// The config dir holding it now.
    config_dir: PathBuf,
    /// Its files and folders, relative to either config dir, the transcript
    /// last.
    items: Vec<PathBuf>,
    /// What archiving it where it is says of it, should it have to be copied
    /// rather than moved.
    described: ArchivedBundle,
}

/// A plan file to copy into the repaired home's config dir.
#[derive(Debug, Clone)]
struct PlanCopy {
    /// Where it is now.
    from: PathBuf,
    /// Where it goes, relative to the repaired home's config dir.
    relative: PathBuf,
}

/// What repairing one session does.
#[derive(Debug, Clone)]
struct SessionRepair {
    /// The session's id.
    session_id: String,
    /// The ids of all its transcripts, wherever they are: what a terminal
    /// having it open would register.
    transcript_ids: Vec<String>,
    /// The plan files its transcripts wrote that the home lacks.
    plans: Vec<PlanCopy>,
    /// The memory folders of the projects its transcripts are in.
    memory: Vec<MemoryMerge>,
    /// Its transcripts in other homes' config dirs, the one shown last.
    transcripts: Vec<TranscriptMove>,
}

/// A checked repair, with what [`apply()`] needs to carry it out.
#[derive(Debug)]
pub struct PreparedRepair {
    /// The home whose sessions are repaired.
    home: Home,
    /// Every home of the app.
    homes: Vec<Home>,
    /// The sessions to repair.
    sessions: Vec<SessionRepair>,
    /// The sessions that need repair but can't have it now.
    skipped: Vec<SkippedSession>,
}

/// What repairing the sessions of `home`, one of `homes`, would do, given the
/// output of `ps -ax -o pid=,command=`.
///
/// An active session of `home` needs repair when a transcript it claims is in
/// another home's config dir; archived ones are left as they are. One is
/// skipped, and said why:
/// - while a terminal has one of its transcripts open, or another home's
///   desktop app does, as only `home`'s is quit;
/// - while another home's active desktop record claims the same copy of one
///   of them, as moving it would take it from there; a home keeping its own
///   copy, as one a session was moved to does, claims that one instead, and
///   does so archived too, as Restore there brings it back;
/// - when one of them has an id that isn't a single plain folder name, as its
///   files are found by it;
/// - when `home` has different files where one of them goes: a bulk repair
///   never replaces what a profile has, which a Move of that one session can,
///   with the user's say-so.
///
/// Files `home` has the same of are left in both places. Only `home`'s own
/// desktop app has to quit, as only its config dir is written, and only when
/// there is something to repair.
pub fn check(home: &Home, homes: &[Home], ps_output: &str) -> AppResult<Checked<PreparedRepair>> {
    let scans = home_scans(homes);
    let live = live_by_home(homes, ps_output);
    let listers = listers(&scans);
    let kept = kept_archived(&scans);
    let context = Context {
        home,
        homes,
        live: &live,
        listers: &listers,
        kept: &kept,
    };
    let mut sessions = Vec::new();
    let mut skipped = Vec::new();
    for owned in owned_by(&home.id, &scans) {
        if !needs_repair(&owned, &home.id, &kept) {
            continue;
        }
        match session_repair(&context, &owned) {
            Ok(repair) => sessions.push(repair),
            Err(reason) => skipped.push(SkippedSession {
                id: owned.session_id,
                reason,
            }),
        }
    }
    let app_to_quit = (!sessions.is_empty() && desktop_pid(home, ps_output).is_some())
        .then(|| AppToQuit::of(home));
    Ok(Checked {
        check: ActionCheck {
            blocker: None,
            app_to_quit,
        },
        target: PreparedRepair {
            home: home.clone(),
            homes: homes.to_vec(),
            sessions,
            skipped,
        },
    })
}

/// What [`check`] looks up each session in.
struct Context<'a> {
    /// The home whose sessions are repaired.
    home: &'a Home,
    /// Every home of the app.
    homes: &'a [Home],
    /// The sessions open in each home's config dir, by transcript id.
    live: &'a [(&'a Home, HashMap<String, LiveHolder>)],
    /// The homes whose desktop records claim each copy of a transcript, by
    /// its id and the home holding the copy (see [`listers`]).
    listers: &'a HashMap<(String, String), Vec<String>>,
    /// The copies of transcripts homes keep archived, by the transcript's id
    /// and the home's (see [`kept_archived`]).
    kept: &'a HashSet<(String, String)>,
}

/// The ids of the homes whose desktop records claim each copy of a
/// transcript, by the transcript's id and the id of the home holding the
/// copy. A record claims its own home's copy when there is one (see
/// [`claimed_copy`]), so a home that keeps its own copy of a transcript
/// doesn't list the one a repair takes. An archived record counts only for
/// its own home's copy: that is its session's, to restore, while one in
/// another home is left to be repaired.
fn listers(scans: &[HomeScan]) -> HashMap<(String, String), Vec<String>> {
    let copies = copies(scans);
    let mut listers: HashMap<(String, String), Vec<String>> = HashMap::new();
    for scan in scans {
        for (record, id) in scan
            .records
            .iter()
            .flat_map(|record| claimed_ids(record).map(move |id| (record, id)))
        {
            let Some(copy) = claimed_copy(&scan.home_id, id, &copies) else {
                continue;
            };
            if record.archived && copy != scan.home_id {
                continue;
            }
            let homes = listers
                .entry((id.to_string(), copy.to_string()))
                .or_default();
            if !homes.contains(&scan.home_id) {
                homes.push(scan.home_id.clone());
            }
        }
    }
    listers
}

/// The sessions open in each of `homes`' config dirs, by transcript id,
/// given the output of `ps -ax -o pid=,command=`. A process registers the
/// session it has open in the config dir its transcript is in.
fn live_by_home<'a>(
    homes: &'a [Home],
    ps_output: &str,
) -> Vec<(&'a Home, HashMap<String, LiveHolder>)> {
    homes
        .iter()
        .map(|each| (each, live_sessions(&each.config_dir, ps_output)))
        .collect()
}

/// Why a session of `home` whose transcripts are `ids` can't be moved while
/// the sessions `live` in each home are open, if it can't: a terminal has one
/// open, or another home's desktop app does. `home`'s own desktop app quits
/// before anything moves.
fn in_use(
    ids: &[String],
    home: &Home,
    live: &[(&Home, HashMap<String, LiveHolder>)],
) -> Option<String> {
    let mut reason = None;
    for (each, open) in live {
        for id in ids {
            match open.get(id) {
                Some(LiveHolder::Terminal) => return Some(OPEN_IN_TERMINAL.to_string()),
                Some(LiveHolder::Desktop) if each.id != home.id && reason.is_none() => {
                    reason = Some(format!("{} has it open", desktop_label(each)));
                }
                _ => {}
            }
        }
    }
    reason
}

/// What repairing `owned`, a session of the context's home, does, or why it
/// can't be repaired now.
fn session_repair(context: &Context, owned: &Owned) -> Result<SessionRepair, String> {
    let home = context.home;
    let displayed = owned
        .transcript
        .as_ref()
        .map(|held| held.summary.session_id.as_str());
    let mut orphans: Vec<&HeldTranscript> = owned
        .claimed_transcripts
        .iter()
        .filter(|held| held.home_id != home.id)
        .collect();
    // The transcript shown goes last, so a session cut short is put back
    // from the end it got to.
    orphans.sort_by_key(|held| Some(held.summary.session_id.as_str()) == displayed);
    let mut transcript_ids = vec![owned.session_id.clone()];
    for held in &owned.claimed_transcripts {
        if !transcript_ids.contains(&held.summary.session_id) {
            transcript_ids.push(held.summary.session_id.clone());
        }
    }
    if let Some(reason) = in_use(&transcript_ids, home, context.live) {
        return Err(reason);
    }
    for held in &orphans {
        let other = context
            .listers
            .get(&(held.summary.session_id.clone(), held.home_id.clone()))
            .into_iter()
            .flatten()
            .find(|lister| **lister != home.id);
        if let Some(other) = other {
            let label = context
                .homes
                .iter()
                .find(|each| each.id == *other)
                .map_or_else(|| other.clone(), desktop_label);
            let key = (held.summary.session_id.clone(), other.clone());
            if context.kept.contains(&key) {
                return Err(format!("{label} keeps it archived"));
            }
            return Err(format!("{label} lists it too"));
        }
    }
    let title = owned
        .record
        .as_ref()
        .and_then(|record| record.title.clone());
    let mut transcripts = Vec::new();
    let mut plans: Vec<PlanCopy> = Vec::new();
    for held in &orphans {
        let holder = context
            .homes
            .iter()
            .find(|each| each.id == held.home_id)
            .ok_or_else(|| {
                format!(
                    "Its transcript {:?} is in a profile that's gone",
                    held.summary.session_id
                )
            })?;
        let shown = Some(held.summary.session_id.as_str()) == displayed;
        transcripts.push(transcript_move(
            home,
            holder,
            held,
            title.as_deref(),
            shown,
        )?);
        for slug in &held.summary.plan_slugs {
            let relative = Path::new(PLANS_DIR).join(format!("{slug}.md"));
            let from = holder.config_dir.join(&relative);
            let listed = plans.iter().any(|plan| plan.relative == relative);
            if from.is_file() && !listed && !occupied(&home.config_dir.join(&relative)) {
                plans.push(PlanCopy { from, relative });
            }
        }
    }
    Ok(SessionRepair {
        session_id: owned.session_id.clone(),
        transcript_ids,
        plans,
        memory: memory_merges(orphans, context.homes),
        transcripts,
    })
}

/// What moving `held`, a transcript in `holder`'s config dir, into `home`'s
/// does: the files and folders of its bundle `home` lacks, the transcript
/// last. Should the transcript be copied, it is archived under the title of
/// its session, `title`, else its own; one the session isn't `shown` by is
/// marked the session's earlier part, so the two tell apart in the Archived
/// list. Refused, with the reason, for a transcript whose id isn't a single
/// plain folder name, as its bundle's paths are made of it, and when `home`
/// has different files where one goes.
fn transcript_move(
    home: &Home,
    holder: &Home,
    held: &HeldTranscript,
    title: Option<&str>,
    shown: bool,
) -> Result<TranscriptMove, String> {
    let summary = &held.summary;
    if !is_session_dir_name(&summary.session_id) {
        return Err(format!(
            "Its transcript {:?} has an id that isn't safe to move",
            summary.session_id
        ));
    }
    let mut paths = bundle_paths(&holder.config_dir, summary);
    // The transcript is first of its bundle; it goes last.
    paths.rotate_left(1);
    let mut items = Vec::new();
    for path in paths {
        let relative = relative_to(&path, &holder.config_dir).map_err(|error| error.message())?;
        let action =
            compare(&path, &home.config_dir.join(&relative)).map_err(|error| error.message())?;
        match action {
            ItemAction::Copy => items.push(relative),
            ItemAction::Same => {}
            ItemAction::Replace => {
                return Err(format!("{} already has different files of it", home.label))
            }
        }
    }
    let title = title
        .map(str::to_string)
        .or_else(|| transcript_title(summary))
        .map(|title| {
            if shown {
                title
            } else {
                format!("{title} (earlier part)")
            }
        });
    let described = ArchivedBundle {
        session_id: summary.session_id.clone(),
        title,
        cwd: summary.cwd.clone(),
        last_prompt: summary.last_prompt.clone(),
        last_used_at: summary.last_used_at,
    };
    Ok(TranscriptMove {
        config_dir: holder.config_dir.clone(),
        items,
        described,
    })
}

#[cfg(test)]
mod tests;
