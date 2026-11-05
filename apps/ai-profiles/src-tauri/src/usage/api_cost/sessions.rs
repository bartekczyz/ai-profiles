//! What each session of a profile would cost at API prices, as the Sessions
//! list shows them: Claude Code's ([`claude`]) and Codex's
//! ([`super::codex::session_costs`]). Like the profile's estimate, it is the
//! recorded use at API list prices, what an API key or Claude's extra usage is
//! billed for it, and it covers each session's whole life.
//!
//! A resumed or forked Claude Code session copies earlier responses into its
//! new transcript, each copy keeping the `sessionId` of the session that
//! recorded it, as a subagent's records carry their session's. So a response
//! counts once, for the session whose transcripts include the one its
//! `sessionId` names. When no session has that transcript any more (Claude
//! Code cleans old ones up), it counts for the session holding a copy that
//! took over first: the one whose own first response is the earliest.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use super::{
    files, is_transcript, label_of, priced, ranked_models, ranked_unpriced, usage_scan, Coverage,
    ModelCost, Turn, UnpricedUse, UsageScan, USAGE_CACHE,
};
use crate::usage::prices::Catalog;

/// What one session's use would cost at API list prices, in USD, over its
/// whole life.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCost {
    pub total_usd: f64,
    /// By model, most expensive first.
    pub by_model: Vec<ModelCost>,
    /// Use the price list has no rates for, most tokens first. Its tokens are
    /// in none of the costs.
    pub unpriced: Vec<UnpricedUse>,
    /// How much of the session's use the costs cover.
    pub coverage: Coverage,
}

/// What the sessions of a profile that used anything would cost, by their
/// ids in the Sessions list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCosts {
    pub sessions: HashMap<String, SessionCost>,
    /// The day the price list is of, `YYYY-MM-DD`.
    pub prices_as_of: NaiveDate,
}

/// A Claude Code session, as the transcripts it is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTranscripts {
    /// Its id in the Sessions list.
    pub id: String,
    /// The ids of its own transcripts, their file names without `.jsonl`: a
    /// response recorded under one of them is the session's.
    pub transcript_ids: Vec<String>,
    /// Where its responses are: transcripts, and folders whose transcripts, at
    /// any depth, are read too (its subagents').
    pub paths: Vec<PathBuf>,
}

/// What one session's requests add up to so far.
#[derive(Debug, Default)]
pub(super) struct SessionTab {
    by_model: HashMap<String, f64>,
    unpriced: HashMap<(String, Option<&'static str>), u64>,
    priced_any: bool,
}

impl SessionTab {
    /// Takes in `turn`, at `prices`.
    pub(super) fn add(&mut self, turn: &Turn, prices: &Catalog) {
        let model = label_of(&turn.model);
        match priced(turn, prices) {
            Ok(costs) => {
                self.priced_any = true;
                *self.by_model.entry(model).or_default() += costs.total();
            }
            Err(missing) => {
                *self.unpriced.entry((model, missing)).or_default() += turn.tokens.total();
            }
        }
    }

    pub(super) fn finish(self) -> SessionCost {
        let coverage = Coverage::of(self.priced_any, !self.unpriced.is_empty());
        let by_model = ranked_models(self.by_model);
        SessionCost {
            total_usd: by_model.iter().map(|model| model.usd).sum(),
            by_model,
            unpriced: ranked_unpriced(self.unpriced),
            coverage,
        }
    }
}

/// What each of the Claude Code `sessions` would cost at `prices`. Reads
/// every transcript of theirs, so it belongs on a blocking thread.
pub fn claude(sessions: &[SessionTranscripts], prices: &Catalog) -> SessionCosts {
    USAGE_CACHE.forget_gone();
    let scans: Vec<Vec<UsageScan>> = sessions.iter().map(scans_of).collect();
    // The session each transcript is part of, by the transcript's id.
    let claimed: HashMap<&str, usize> = sessions
        .iter()
        .enumerate()
        .flat_map(|(index, session)| {
            session
                .transcript_ids
                .iter()
                .map(move |id| (id.as_str(), index))
        })
        .collect();
    let recorder = |turn: &Turn| {
        turn.session
            .as_deref()
            .and_then(|id| claimed.get(id).copied())
    };
    // When each session recorded its own first response.
    let took_over: Vec<Option<DateTime<Utc>>> = scans
        .iter()
        .enumerate()
        .map(|(index, held)| {
            held.iter()
                .flat_map(|scan| scan.turns.values())
                .filter(|turn| recorder(turn) == Some(index))
                .map(|turn| turn.at)
                .min()
        })
        .collect();
    // Each response, by message and request id: its copy with the most
    // output, and the sessions holding a copy.
    let mut responses: HashMap<&str, (&Turn, Vec<usize>)> = HashMap::new();
    for (index, held) in scans.iter().enumerate() {
        for (key, turn) in held.iter().flat_map(|scan| scan.turns.iter()) {
            let (fullest, holders) = responses
                .entry(key.as_str())
                .or_insert_with(|| (turn, Vec::new()));
            if turn.tokens.output > fullest.tokens.output {
                *fullest = turn;
            }
            if !holders.contains(&index) {
                holders.push(index);
            }
        }
    }
    let mut tabs: HashMap<usize, SessionTab> = HashMap::new();
    for (turn, holders) in responses.into_values() {
        let owner = recorder(turn)
            .filter(|index| holders.contains(index))
            .or_else(|| {
                holders.iter().copied().min_by_key(|index| {
                    let started = took_over[*index];
                    (started.is_none(), started, &sessions[*index].id)
                })
            });
        if let Some(owner) = owner {
            tabs.entry(owner).or_default().add(turn, prices);
        }
    }
    SessionCosts {
        sessions: tabs
            .into_iter()
            .map(|(index, tab)| (sessions[index].id.clone(), tab.finish()))
            .collect(),
        prices_as_of: prices.as_of(),
    }
}

/// What the transcripts of `session` say.
fn scans_of(session: &SessionTranscripts) -> Vec<UsageScan> {
    let mut paths = Vec::new();
    for path in &session.paths {
        if path.is_dir() {
            files(path, &is_transcript, &mut paths);
        } else {
            paths.push(path.clone());
        }
    }
    paths.iter().filter_map(|path| usage_scan(path)).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::super::tests::{assert_usd, prices};
    use super::*;

    /// An assistant record of `session` at `at`, of response `id` from
    /// `model`, with 1M output tokens.
    fn response(session: &str, at: &str, id: &str, model: &str) -> Value {
        json!({
            "type": "assistant",
            "sessionId": session,
            "timestamp": at,
            "requestId": format!("req_{id}"),
            "cwd": "/work/app",
            "message": { "id": id, "model": model, "usage": { "output_tokens": 1_000_000 } },
        })
    }

    const OPUS: &str = "claude-opus-5-5";

    /// Writes `lines` to `<dir>/<relative>`, one record per line. Returns
    /// the file.
    fn write(dir: &Path, relative: &str, lines: &[Value]) -> PathBuf {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
        fs::write(&path, body).unwrap();
        path
    }

    /// Session `id`, made of `transcripts` (ids) in `dir`, each with the
    /// folder beside it.
    fn session(dir: &Path, id: &str, transcripts: &[&str]) -> SessionTranscripts {
        SessionTranscripts {
            id: id.to_string(),
            transcript_ids: transcripts.iter().map(|each| each.to_string()).collect(),
            paths: transcripts
                .iter()
                .flat_map(|each| [dir.join(format!("{each}.jsonl")), dir.join(each)])
                .collect(),
        }
    }

    fn total(costs: &SessionCosts, id: &str) -> f64 {
        costs.sessions[id].total_usd
    }

    #[test]
    fn a_response_a_fork_copied_counts_for_the_session_that_recorded_it() {
        let dir = tempdir().unwrap();
        let first = response("s1", "2026-10-15T10:00:00Z", "msg_1", OPUS);
        write(dir.path(), "s1.jsonl", std::slice::from_ref(&first));
        write(
            dir.path(),
            "s2.jsonl",
            &[first, response("s2", "2026-10-15T11:00:00Z", "msg_2", OPUS)],
        );
        let sessions = [
            session(dir.path(), "s1", &["s1"]),
            session(dir.path(), "s2", &["s2"]),
        ];

        let costs = claude(&sessions, &prices());

        assert_usd(total(&costs, "s1"), 20.0);
        assert_usd(total(&costs, "s2"), 20.0);
    }

    #[test]
    fn a_subagents_responses_count_for_its_session() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "s1.jsonl",
            &[response("s1", "2026-10-15T10:00:00Z", "msg_1", OPUS)],
        );
        write(
            dir.path(),
            "s1/subagents/agent-a1.jsonl",
            &[response(
                "s1",
                "2026-10-15T10:01:00Z",
                "msg_2",
                "claude-haiku-4-5-20251001",
            )],
        );

        let costs = claude(&[session(dir.path(), "s1", &["s1"])], &prices());

        let cost = &costs.sessions["s1"];
        assert_usd(cost.total_usd, 20.0 + 5.0);
        let models: Vec<&str> = cost
            .by_model
            .iter()
            .map(|model| model.model.as_str())
            .collect();
        assert_eq!(models, ["Opus 5.5", "Haiku 4.5"]);
        assert_eq!(cost.coverage, Coverage::Complete);
    }

    #[test]
    fn a_session_of_several_transcripts_counts_a_response_they_share_once() {
        let dir = tempdir().unwrap();
        let first = response("a", "2026-10-15T10:00:00Z", "msg_1", OPUS);
        write(dir.path(), "a.jsonl", std::slice::from_ref(&first));
        write(
            dir.path(),
            "b.jsonl",
            &[first, response("b", "2026-10-15T11:00:00Z", "msg_2", OPUS)],
        );

        let costs = claude(&[session(dir.path(), "desktop", &["a", "b"])], &prices());

        assert_usd(total(&costs, "desktop"), 40.0);
    }

    #[test]
    fn copies_whose_session_is_gone_count_for_the_session_that_took_over_first() {
        let dir = tempdir().unwrap();
        let gone = response("gone", "2026-10-15T09:00:00Z", "msg_1", OPUS);
        let second = response("s2", "2026-10-15T10:00:00Z", "msg_2", OPUS);
        write(dir.path(), "s2.jsonl", &[gone.clone(), second.clone()]);
        write(
            dir.path(),
            "s3.jsonl",
            &[
                gone,
                second,
                response("s3", "2026-10-15T11:00:00Z", "msg_3", OPUS),
            ],
        );
        let sessions = [
            session(dir.path(), "s3", &["s3"]),
            session(dir.path(), "s2", &["s2"]),
        ];

        let costs = claude(&sessions, &prices());

        assert_usd(total(&costs, "s2"), 40.0);
        assert_usd(total(&costs, "s3"), 20.0);
    }

    #[test]
    fn use_without_a_price_is_reported_and_a_session_without_use_has_no_cost() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "s1.jsonl",
            &[response(
                "s1",
                "2026-10-15T10:00:00Z",
                "msg_1",
                "claude-opus-9",
            )],
        );
        let mut fast = response("s2", "2026-10-15T10:00:00Z", "msg_2", OPUS);
        fast["message"]["usage"]["speed"] = json!("fast");
        write(
            dir.path(),
            "s2.jsonl",
            &[response("s2", "2026-10-15T09:00:00Z", "msg_3", OPUS), fast],
        );
        write(
            dir.path(),
            "s3.jsonl",
            &[json!({ "type": "user", "sessionId": "s3" })],
        );
        let sessions = [
            session(dir.path(), "s1", &["s1"]),
            session(dir.path(), "s2", &["s2"]),
            session(dir.path(), "s3", &["s3"]),
        ];

        let costs = claude(&sessions, &prices());

        let unpriced = &costs.sessions["s1"];
        assert_eq!(unpriced.coverage, Coverage::Unpriced);
        assert_eq!(unpriced.unpriced[0].model, "Opus 9");
        let partial = &costs.sessions["s2"];
        assert_eq!(partial.coverage, Coverage::Partial);
        assert_usd(partial.total_usd, 20.0);
        assert_eq!(partial.unpriced[0].missing, Some("fast-mode rates"));
        assert!(!costs.sessions.contains_key("s3"));
        assert_eq!(costs.prices_as_of.to_string(), "2026-10-14");
    }
}
