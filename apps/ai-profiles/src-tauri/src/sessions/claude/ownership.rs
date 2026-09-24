//! Which home a Claude session belongs to.
//!
//! A session's transcripts and its desktop record can sit in different homes:
//! before profiles pointed their desktop app at their own config dir, a
//! profile's desktop app wrote its transcripts into the stock `~/.claude`. A
//! desktop record claims its current transcript (`cliSessionId`) and the ones
//! the session continued before (`priorCliSessionIds`). So a session belongs
//! to:
//! - every home whose desktop app has a record of it, wherever its transcripts
//!   are; one outside that home's config dir needs repair;
//! - else the home whose config dir holds its transcript.

use std::collections::{HashMap, HashSet};

use super::desktop::DesktopRecord;
use super::transcript::TranscriptSummary;

/// What one home of the app holds, read from disk.
#[derive(Debug, Clone)]
pub struct HomeScan {
    /// The home's id.
    pub home_id: String,
    /// The transcripts in the home's config dir.
    pub transcripts: Vec<TranscriptSummary>,
    /// The records of the home's desktop app.
    pub records: Vec<DesktopRecord>,
}

/// A transcript, with the home whose config dir holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldTranscript {
    /// The transcript.
    pub summary: TranscriptSummary,
    /// The home whose config dir holds it.
    pub home_id: String,
}

/// A session a home owns, with its halves.
#[derive(Debug, Clone, PartialEq)]
pub struct Owned {
    /// The session's id: its shown transcript's, else its record's
    /// `cliSessionId`, else the record's own `local_<uuid>`.
    pub session_id: String,
    /// The transcript shown for the session: the record's current one if it
    /// exists, else the last used of its earlier ones. It comes from the
    /// owning home's config dir when that has it, else from another home's
    /// (an orphan). `None` when no home has any of them.
    pub transcript: Option<HeldTranscript>,
    /// The owning home's desktop record of the session.
    pub record: Option<DesktopRecord>,
    /// Every transcript of the session that exists: the shown one and, for a
    /// desktop session, its earlier ones. A move or repair carries them all.
    pub claimed_transcripts: Vec<HeldTranscript>,
}

/// Every copy of each transcript of the app, by the transcript's id: the
/// transcript as each home holding it has it, with the home's id, in the
/// order of the scans.
pub(super) type Copies<'a> = HashMap<&'a str, Vec<(&'a TranscriptSummary, &'a str)>>;

/// The sessions home `home_id` owns, given what every home of the app holds.
/// A record of a session the desktop app never started Claude Code for names
/// no transcript and is left out. Of several records of one session in the
/// home, as after switching accounts, the last active one is used.
///
/// Every home's transcripts are gone through once, then every home's
/// records once: for the sessions `home_id`'s own records are of, and for the
/// transcripts in `home_id`'s config dir any record claims.
pub fn owned_by(home_id: &str, scans: &[HomeScan]) -> Vec<Owned> {
    let Some(own) = scans.iter().find(|scan| scan.home_id == home_id) else {
        return Vec::new();
    };
    let copies = copies(scans);
    let mut owned: Vec<Owned> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut claimed: HashSet<&str> = HashSet::new();
    for scan in scans {
        for record in &scan.records {
            for id in claimed_ids(record) {
                if claimed_copy(&scan.home_id, id, &copies) == Some(home_id) {
                    claimed.insert(id);
                }
            }
            if scan.home_id != home_id {
                continue;
            }
            let Some(session) = record_session(record, home_id, &copies) else {
                continue;
            };
            if let Some(&position) = by_id.get(&session.session_id) {
                let kept = owned[position].record.as_ref();
                if kept.is_some_and(|kept| kept.last_activity_at < record.last_activity_at) {
                    owned[position] = session;
                }
                continue;
            }
            by_id.insert(session.session_id.clone(), owned.len());
            owned.push(session);
        }
    }
    for transcript in &own.transcripts {
        let session_id = &transcript.session_id;
        if by_id.contains_key(session_id) || claimed.contains(session_id.as_str()) {
            continue;
        }
        let held = HeldTranscript {
            summary: transcript.clone(),
            home_id: home_id.to_string(),
        };
        by_id.insert(session_id.clone(), owned.len());
        owned.push(Owned {
            session_id: session_id.clone(),
            transcript: Some(held.clone()),
            record: None,
            claimed_transcripts: vec![held],
        });
    }
    owned
}

/// Every copy of each transcript `scans` hold (see [`Copies`]).
pub(super) fn copies(scans: &[HomeScan]) -> Copies<'_> {
    let mut copies: Copies = HashMap::new();
    for scan in scans {
        for transcript in &scan.transcripts {
            copies
                .entry(transcript.session_id.as_str())
                .or_default()
                .push((transcript, scan.home_id.as_str()));
        }
    }
    copies
}

/// The copy of transcript `id` a record of home `claimant` claims, of its
/// `copies`, with the home holding it: `claimant`'s own when it has one, else
/// the first other home's (an orphan). So a session moved to another home and
/// restored here still lists here. `None` when no home has it.
fn claimed_held<'a>(
    claimant: &str,
    id: &str,
    copies: &Copies<'a>,
) -> Option<(&'a TranscriptSummary, &'a str)> {
    let held = copies.get(id)?;
    held.iter()
        .find(|(_, home_id)| *home_id == claimant)
        .or_else(|| held.first())
        .copied()
}

/// The id of the home whose copy of transcript `id` a record of home
/// `claimant` claims, of its `copies` (see [`claimed_held`]).
pub(super) fn claimed_copy<'a>(claimant: &str, id: &str, copies: &Copies<'a>) -> Option<&'a str> {
    claimed_held(claimant, id, copies).map(|(_, home_id)| home_id)
}

/// The copies of transcripts a home keeps archived, by the transcript's id
/// and the home's: copies in a home's own config dir that only its archived
/// records claim as their own. Restore there brings them back, so a repair
/// elsewhere never takes them.
pub(crate) fn kept_archived(scans: &[HomeScan]) -> HashSet<(String, String)> {
    let copies = copies(scans);
    let mut archived = HashSet::new();
    let mut active = HashSet::new();
    for scan in scans {
        for record in &scan.records {
            for id in claimed_ids(record) {
                if claimed_copy(&scan.home_id, id, &copies) != Some(scan.home_id.as_str()) {
                    continue;
                }
                let key = (id.to_string(), scan.home_id.clone());
                if record.archived {
                    archived.insert(key);
                } else {
                    active.insert(key);
                }
            }
        }
    }
    archived.retain(|key| !active.contains(key));
    archived
}

/// Whether `owned`, a session of home `home_id`, needs repair: it is active,
/// a transcript it claims sits in another home's config dir, and none of
/// those is a copy that home keeps archived (see [`kept_archived`], `kept`),
/// which a repair leaves where it is.
pub(crate) fn needs_repair(owned: &Owned, home_id: &str, kept: &HashSet<(String, String)>) -> bool {
    let archived = owned.record.as_ref().is_some_and(|record| record.archived);
    let mut orphans = owned
        .claimed_transcripts
        .iter()
        .filter(|held| held.home_id != home_id)
        .peekable();
    if archived || orphans.peek().is_none() {
        return false;
    }
    orphans.all(|held| !kept.contains(&(held.summary.session_id.clone(), held.home_id.clone())))
}

/// The ids of the transcripts `record` claims: its current one, then its
/// earlier ones.
pub(super) fn claimed_ids(record: &DesktopRecord) -> impl Iterator<Item = &str> {
    record
        .cli_session_id
        .iter()
        .chain(&record.prior_cli_session_ids)
        .map(String::as_str)
}

/// The session `record`, of home `home_id`, is of, with the copies of its
/// transcripts it claims of `copies`. `None` for a record that claims no
/// transcript at all.
fn record_session(record: &DesktopRecord, home_id: &str, copies: &Copies) -> Option<Owned> {
    let mut seen = HashSet::new();
    let ids: Vec<&str> = claimed_ids(record).filter(|id| seen.insert(*id)).collect();
    if ids.is_empty() {
        return None;
    }
    let claimed_transcripts: Vec<HeldTranscript> = ids
        .iter()
        .filter_map(|id| claimed_held(home_id, id, copies))
        .map(|(summary, holder)| HeldTranscript {
            summary: summary.clone(),
            home_id: holder.to_string(),
        })
        .collect();
    let current = record.cli_session_id.as_deref();
    let transcript = claimed_transcripts
        .iter()
        .find(|held| Some(held.summary.session_id.as_str()) == current)
        .or_else(|| {
            claimed_transcripts
                .iter()
                .max_by_key(|held| held.summary.last_used_at)
        })
        .cloned();
    let session_id = transcript
        .as_ref()
        .map(|held| held.summary.session_id.clone())
        .or_else(|| record.cli_session_id.clone())
        .unwrap_or_else(|| record.local_id.clone());
    Some(Owned {
        session_id,
        transcript,
        record: Some(record.clone()),
        claimed_transcripts,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use chrono::DateTime;

    use super::*;

    const DEFAULT: &str = "default:claude";
    const PERSONAL: &str = "personal";
    const WORK: &str = "work";

    /// Transcript `session_id`, last used at the tests' usual time.
    fn transcript(session_id: &str) -> TranscriptSummary {
        transcript_used_at(session_id, 1_790_000_000)
    }

    /// Transcript `session_id`, last used `used_at` seconds after the epoch.
    fn transcript_used_at(session_id: &str, used_at: i64) -> TranscriptSummary {
        TranscriptSummary {
            session_id: session_id.to_string(),
            path: PathBuf::from(format!("/config/projects/-work/{session_id}.jsonl")),
            cwd: Some("/work".to_string()),
            custom_title: None,
            ai_title: None,
            first_prompt: None,
            last_prompt: None,
            last_used_at: DateTime::from_timestamp(used_at, 0).unwrap(),
            plan_slugs: BTreeSet::new(),
        }
    }

    /// Desktop record `local_<local>` of transcript `cli_session_id`, last
    /// active `active_at` seconds after the epoch.
    fn record(local: &str, cli_session_id: Option<&str>, active_at: i64) -> DesktopRecord {
        DesktopRecord {
            path: PathBuf::from(format!("/gui/claude-code-sessions/a/o/local_{local}.json")),
            local_id: format!("local_{local}"),
            cli_session_id: cli_session_id.map(str::to_string),
            prior_cli_session_ids: Vec::new(),
            title: None,
            cwd: None,
            created_at: None,
            last_activity_at: DateTime::from_timestamp(active_at, 0),
            archived: false,
        }
    }

    /// Desktop record `local_<local>` of transcript `cli_session_id`, which
    /// continued the transcripts `priors`.
    fn continued(local: &str, cli_session_id: Option<&str>, priors: &[&str]) -> DesktopRecord {
        DesktopRecord {
            prior_cli_session_ids: priors.iter().map(|id| id.to_string()).collect(),
            ..record(local, cli_session_id, 1)
        }
    }

    /// What home `home_id` holds: `transcripts`, by id, and `records`.
    fn scan(home_id: &str, transcripts: &[&str], records: Vec<DesktopRecord>) -> HomeScan {
        HomeScan {
            home_id: home_id.to_string(),
            transcripts: transcripts.iter().map(|id| transcript(id)).collect(),
            records,
        }
    }

    /// Each of `owned`'s id, record and the home of its shown transcript.
    fn summary(owned: &[Owned]) -> Vec<(&str, Option<&str>, Option<&str>)> {
        owned
            .iter()
            .map(|session| {
                (
                    session.session_id.as_str(),
                    session
                        .record
                        .as_ref()
                        .map(|record| record.local_id.as_str()),
                    session
                        .transcript
                        .as_ref()
                        .map(|held| held.home_id.as_str()),
                )
            })
            .collect()
    }

    /// The id and holding home of each transcript `owned` claims.
    fn claimed(owned: &Owned) -> Vec<(&str, &str)> {
        owned
            .claimed_transcripts
            .iter()
            .map(|held| (held.summary.session_id.as_str(), held.home_id.as_str()))
            .collect()
    }

    #[test]
    fn a_home_owns_the_sessions_its_desktop_app_has_records_of() {
        let scans = [
            scan(DEFAULT, &[], vec![]),
            scan(PERSONAL, &["s1"], vec![record("r1", Some("s1"), 1)]),
        ];

        let owned = owned_by(PERSONAL, &scans);

        assert_eq!(summary(&owned), [("s1", Some("local_r1"), Some(PERSONAL))]);
        assert_eq!(
            owned[0].transcript.as_ref().map(|held| &held.summary),
            Some(&transcript("s1"))
        );
        assert_eq!(claimed(&owned[0]), [("s1", PERSONAL)]);
        assert_eq!(owned_by(DEFAULT, &scans), []);
    }

    #[test]
    fn a_home_owns_the_transcripts_in_its_config_dir_no_other_home_has_a_record_of() {
        let scans = [
            scan(DEFAULT, &["cli", "claimed"], vec![]),
            scan(PERSONAL, &[], vec![record("r1", Some("claimed"), 1)]),
        ];

        let owned = owned_by(DEFAULT, &scans);

        assert_eq!(summary(&owned), [("cli", None, Some(DEFAULT))]);
        assert_eq!(claimed(&owned[0]), [("cli", DEFAULT)]);
    }

    #[test]
    fn a_session_whose_transcript_is_in_another_home_is_its_record_homes_orphan() {
        let scans = [
            scan(DEFAULT, &["orphan", "own"], vec![]),
            scan(PERSONAL, &[], vec![record("r1", Some("orphan"), 1)]),
        ];

        let owned = owned_by(PERSONAL, &scans);

        assert_eq!(
            summary(&owned),
            [("orphan", Some("local_r1"), Some(DEFAULT))]
        );
        assert_eq!(claimed(&owned[0]), [("orphan", DEFAULT)]);
        assert_eq!(
            summary(&owned_by(DEFAULT, &scans)),
            [("own", None, Some(DEFAULT))]
        );
    }

    #[test]
    fn a_session_two_homes_have_records_of_lists_in_both() {
        let scans = [
            scan(DEFAULT, &[], vec![]),
            scan(PERSONAL, &["shared"], vec![record("p", Some("shared"), 1)]),
            scan(WORK, &[], vec![record("w", Some("shared"), 1)]),
        ];

        assert_eq!(
            summary(&owned_by(PERSONAL, &scans)),
            [("shared", Some("local_p"), Some(PERSONAL))]
        );
        assert_eq!(
            summary(&owned_by(WORK, &scans)),
            [("shared", Some("local_w"), Some(PERSONAL))]
        );
    }

    #[test]
    fn a_record_without_a_transcript_is_owned_without_one() {
        let scans = [
            scan(DEFAULT, &[], vec![]),
            scan(
                PERSONAL,
                &[],
                vec![
                    record("gone", Some("s1"), 1),
                    continued("earlier-gone", None, &["s0"]),
                ],
            ),
        ];

        let owned = owned_by(PERSONAL, &scans);

        assert_eq!(
            summary(&owned),
            [
                ("s1", Some("local_gone"), None),
                ("local_earlier-gone", Some("local_earlier-gone"), None),
            ]
        );
        assert!(owned
            .iter()
            .all(|session| session.claimed_transcripts.is_empty()));
    }

    #[test]
    fn a_record_the_app_never_started_claude_code_for_is_no_session() {
        let scans = [scan(PERSONAL, &[], vec![record("unstarted", None, 1)])];

        assert_eq!(owned_by(PERSONAL, &scans), []);
    }

    #[test]
    fn a_cli_only_transcript_in_a_profile_is_the_profiles() {
        let scans = [scan(DEFAULT, &[], vec![]), scan(PERSONAL, &["cli"], vec![])];

        assert_eq!(
            summary(&owned_by(PERSONAL, &scans)),
            [("cli", None, Some(PERSONAL))]
        );
        assert_eq!(owned_by(DEFAULT, &scans), []);
    }

    #[test]
    fn of_two_records_of_one_session_in_a_home_the_last_active_is_used() {
        let scans = [scan(
            PERSONAL,
            &["s1"],
            vec![record("old", Some("s1"), 1), record("new", Some("s1"), 2)],
        )];

        assert_eq!(
            summary(&owned_by(PERSONAL, &scans)),
            [("s1", Some("local_new"), Some(PERSONAL))]
        );
    }

    #[test]
    fn a_record_claims_the_transcripts_its_session_continued_before() {
        let scans = [
            scan(DEFAULT, &["before", "cli"], vec![]),
            scan(
                PERSONAL,
                &["now", "earlier"],
                vec![continued("r1", Some("now"), &["before", "earlier", "gone"])],
            ),
        ];

        let owned = owned_by(PERSONAL, &scans);

        assert_eq!(summary(&owned), [("now", Some("local_r1"), Some(PERSONAL))]);
        assert_eq!(
            claimed(&owned[0]),
            [
                ("now", PERSONAL),
                ("before", DEFAULT),
                ("earlier", PERSONAL)
            ]
        );
        assert_eq!(
            summary(&owned_by(DEFAULT, &scans)),
            [("cli", None, Some(DEFAULT))]
        );
    }

    #[test]
    fn without_its_current_transcript_a_session_shows_its_last_used_earlier_one() {
        let mut default = scan(DEFAULT, &[], vec![]);
        default.transcripts = vec![
            transcript_used_at("first", 1_790_000_000),
            transcript_used_at("second", 1_790_000_500),
        ];
        let scans = [
            default,
            scan(
                PERSONAL,
                &[],
                vec![continued("r1", None, &["first", "second"])],
            ),
        ];

        let owned = owned_by(PERSONAL, &scans);

        assert_eq!(
            summary(&owned),
            [("second", Some("local_r1"), Some(DEFAULT))]
        );
        assert_eq!(
            claimed(&owned[0]),
            [("first", DEFAULT), ("second", DEFAULT)]
        );
        assert_eq!(owned_by(DEFAULT, &scans), []);
    }

    #[test]
    fn two_records_of_a_home_that_continued_the_same_transcript_both_claim_it() {
        let scans = [
            scan(DEFAULT, &["before"], vec![]),
            scan(
                PERSONAL,
                &["now", "later"],
                vec![
                    continued("r1", Some("now"), &["before"]),
                    continued("r2", Some("later"), &["before"]),
                ],
            ),
        ];

        let owned = owned_by(PERSONAL, &scans);

        assert_eq!(
            summary(&owned),
            [
                ("now", Some("local_r1"), Some(PERSONAL)),
                ("later", Some("local_r2"), Some(PERSONAL)),
            ]
        );
        assert_eq!(claimed(&owned[0]), [("now", PERSONAL), ("before", DEFAULT)]);
        assert_eq!(
            claimed(&owned[1]),
            [("later", PERSONAL), ("before", DEFAULT)]
        );
        assert_eq!(owned_by(DEFAULT, &scans), []);
    }

    #[test]
    fn a_copy_in_another_home_is_that_homes_when_the_record_has_its_own() {
        let scans = [
            scan(DEFAULT, &["moved"], vec![]),
            scan(PERSONAL, &["moved"], vec![record("r1", Some("moved"), 1)]),
        ];

        assert_eq!(
            summary(&owned_by(DEFAULT, &scans)),
            [("moved", None, Some(DEFAULT))]
        );
        let personal = owned_by(PERSONAL, &scans);
        assert_eq!(
            summary(&personal),
            [("moved", Some("local_r1"), Some(PERSONAL))]
        );
        assert_eq!(claimed(&personal[0]), [("moved", PERSONAL)]);
    }
}
