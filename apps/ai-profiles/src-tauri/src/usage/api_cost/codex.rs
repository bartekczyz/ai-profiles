//! Codex's side of the estimate: what a profile's Codex use, in the CLI and
//! the desktop app, would cost at OpenAI API prices.
//!
//! Read from the rollouts in the profile's `CODEX_HOME`, under `sessions/`
//! and `archived_sessions/`: plain (`.jsonl`), or compressed (`.jsonl.zst`),
//! as Codex leaves a rollout after a week idle. Where both are there, the
//! plain one is read, as Codex does.
//!
//! Each response Codex records is followed by a `token_count` event. Its
//! `last_token_usage` is what that request used; `total_token_usage` is the
//! thread's running total, which stands in for it when it is missing and
//! tells a repeated event (the total didn't move) from a new one. Cached and
//! cache-write tokens are part of the input, reasoning tokens part of the
//! output. A request is priced as the model and service tier in effect then:
//! the latest `turn_context`'s model, and the latest
//! `thread_settings_applied`'s tier, standard until one says otherwise. Each
//! hosted web search (`web_search_call`) is billed with the request it was
//! part of.
//!
//! A forked thread, or a subagent's, starts by replaying the usage of the
//! thread it came from. Its leading events that match that thread's, up to
//! the fork, aren't counted again. When they match none (the parent's rollout
//! is gone, or Codex rewrote what it copied), the burst of events Codex wrote
//! within [`BURST_PAUSE`] of each other at its start is taken for the replay,
//! as ccusage does. A thread both in `sessions/` and `archived_sessions/`
//! counts once, from `sessions/`.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::{files, tally, ApiCost, Turn};
use crate::line_cache::{LineCache, LineScan};
use crate::usage::prices::{Catalog, Tier, Tokens};

/// The longest pause between the events Codex writes replaying a parent's
/// usage: it writes them in one go, while a thread's own requests are seconds
/// apart. ccusage measured such bursts at 10 to 40 ms, and the pauses after
/// them at 6 to 15 s.
const BURST_PAUSE: TimeDelta = TimeDelta::seconds(1);

/// A line is only decoded if it holds one of these. Codex writes records
/// without spaces, and in tool output they show up escaped.
const MARKERS: [&str; 5] = [
    "\"type\":\"session_meta\"",
    "\"type\":\"turn_context\"",
    "\"type\":\"token_count\"",
    "\"type\":\"thread_settings_applied\"",
    "\"type\":\"web_search_call\"",
];

/// What a request recorded for a model it doesn't name is shown as.
const UNNAMED_MODEL: &str = "unknown model";

/// Tokens as Codex counts them: `cached_input_tokens` and
/// `cache_write_input_tokens` are part of `input_tokens`, and
/// `reasoning_output_tokens` part of `output_tokens`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default, alias = "cache_creation_input_tokens")]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

impl Usage {
    /// With cache reads and writes no more than the input they are part of.
    fn normalized(mut self) -> Usage {
        self.cached_input_tokens = self.cached_input_tokens.min(self.input_tokens);
        self.cache_write_input_tokens = self
            .cache_write_input_tokens
            .min(self.input_tokens - self.cached_input_tokens);
        self
    }

    /// What was used since `before`, an earlier running total.
    fn since(&self, before: Option<&Usage>) -> Usage {
        let Some(before) = before else {
            return *self;
        };
        Usage {
            input_tokens: self.input_tokens.saturating_sub(before.input_tokens),
            cached_input_tokens: self
                .cached_input_tokens
                .saturating_sub(before.cached_input_tokens),
            cache_write_input_tokens: self
                .cache_write_input_tokens
                .saturating_sub(before.cache_write_input_tokens),
            output_tokens: self.output_tokens.saturating_sub(before.output_tokens),
            reasoning_output_tokens: self
                .reasoning_output_tokens
                .saturating_sub(before.reasoning_output_tokens),
            total_tokens: self.total_tokens.saturating_sub(before.total_tokens),
        }
        .normalized()
    }

    fn is_empty(&self) -> bool {
        self.input_tokens == 0 && self.output_tokens == 0
    }

    /// The tokens as they are billed. Only for normalized usage.
    fn tokens(&self) -> Tokens {
        Tokens {
            input: self.input_tokens - self.cached_input_tokens - self.cache_write_input_tokens,
            output: self.output_tokens,
            cache_write_5m: self.cache_write_input_tokens,
            cache_write_1h: 0,
            cache_read: self.cached_input_tokens,
        }
    }
}

/// The fields of a rollout record read here.
#[derive(Deserialize)]
struct Record {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    /// `session_meta`: the thread's id, folder, and where it came from.
    id: Option<String>,
    cwd: Option<String>,
    forked_from_id: Option<String>,
    source: Option<Value>,
    /// `turn_context`, and some `token_count` events: the model.
    model: Option<String>,
    /// `token_count`.
    info: Option<Info>,
    /// `thread_settings_applied`.
    thread_settings: Option<ThreadSettings>,
}

#[derive(Deserialize)]
struct Info {
    total_token_usage: Option<Usage>,
    last_token_usage: Option<Usage>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct ThreadSettings {
    service_tier: Option<String>,
}

/// One request, as a rollout recorded it.
#[derive(Debug, Clone)]
struct Request {
    at: DateTime<Utc>,
    model: Option<String>,
    tier: Tier,
    usage: Usage,
    web_searches: u64,
}

/// What a rollout's lines say so far.
#[derive(Debug, Clone, Default)]
struct RolloutScan {
    /// The thread's id, from its `session_meta`.
    id: Option<String>,
    /// The thread it was forked from, or the one that spawned it as a
    /// subagent.
    parent: Option<String>,
    /// When it started: for a fork, when it was forked.
    started: Option<DateTime<Utc>>,
    /// The folder it works in.
    cwd: Option<String>,
    /// The model and tier in effect.
    model: Option<String>,
    tier: Option<Tier>,
    /// The running total of the last `token_count`.
    totals: Option<Usage>,
    /// Web searches since the last request.
    web_searches: u64,
    /// Shared with the cache's copy until a line changes them.
    requests: Arc<Vec<Request>>,
}

impl LineScan for RolloutScan {
    fn read(&mut self, line: &[u8]) {
        let Ok(line) = std::str::from_utf8(line) else {
            return;
        };
        if !MARKERS.iter().any(|marker| line.contains(marker)) {
            return;
        }
        let Ok(Record {
            kind,
            timestamp,
            payload: Some(payload),
        }) = serde_json::from_str::<Record>(line)
        else {
            return;
        };
        match (kind.as_deref(), payload.kind.as_deref()) {
            (Some("session_meta"), _) if self.id.is_none() => {
                self.parent = payload
                    .forked_from_id
                    .or_else(|| {
                        let spawner = payload
                            .source
                            .as_ref()?
                            .pointer("/subagent/thread_spawn/parent_thread_id")?;
                        spawner.as_str().map(str::to_owned)
                    })
                    .filter(|parent| !parent.is_empty());
                self.id = payload.id;
                self.cwd = payload.cwd.filter(|cwd| !cwd.is_empty());
                self.started = timestamp.and_then(|at| at.parse().ok());
            }
            (Some("turn_context"), _) => {
                if payload.model.is_some() {
                    self.model = payload.model;
                }
            }
            (Some("event_msg"), Some("thread_settings_applied")) => {
                // Settings that don't name a tier leave it as it was.
                if let Some(tier) = payload
                    .thread_settings
                    .and_then(|settings| settings.service_tier)
                {
                    self.tier = Some(tier_of(&tier));
                }
            }
            (Some("response_item"), Some("web_search_call")) => self.web_searches += 1,
            (Some("event_msg"), Some("token_count")) => self.count(timestamp, payload),
            _ => {}
        }
    }
}

impl RolloutScan {
    /// Take in a `token_count` event at `timestamp`: the request it follows,
    /// unless it only repeats the last one or used nothing.
    fn count(&mut self, timestamp: Option<String>, payload: Payload) {
        let Some(info) = payload.info else {
            return;
        };
        let total = info.total_token_usage.map(Usage::normalized);
        let advanced = total.is_none() || total != self.totals;
        let usage = info
            .last_token_usage
            .filter(|_| advanced)
            .map(Usage::normalized)
            .or_else(|| total.map(|total| total.since(self.totals.as_ref())));
        if total.is_some() {
            self.totals = total;
        }
        let Some(usage) = usage.filter(|usage| !usage.is_empty()) else {
            return;
        };
        let Some(at) = timestamp.and_then(|at| at.parse().ok()) else {
            return;
        };
        if let Some(model) = payload.model.or(info.model) {
            self.model = Some(model);
        }
        let request = Request {
            at,
            model: self.model.clone(),
            tier: self.tier.unwrap_or(Tier::Standard),
            usage,
            web_searches: std::mem::take(&mut self.web_searches),
        };
        Arc::make_mut(&mut self.requests).push(request);
    }
}

/// The tier a `service_tier` setting names. Codex writes `default` or
/// `standard` for the standard tier depending on the client, and `fast` for
/// priority processing in older versions.
fn tier_of(setting: &str) -> Tier {
    match setting {
        "" | "default" | "standard" => Tier::Standard,
        "fast" | "priority" => Tier::Priority,
        "flex" => Tier::Flex,
        _ => Tier::Unknown,
    }
}

/// Rollouts already read, by path.
static ROLLOUT_CACHE: LazyLock<LineCache<RolloutScan, RolloutScan>> =
    LazyLock::new(LineCache::default);

/// The estimated API cost of the Codex use recorded in `codex_home` over
/// `period`, at `prices`. Walks every rollout, so it belongs on a blocking
/// thread.
pub fn read(codex_home: &Path, prices: &Catalog, period: RangeInclusive<NaiveDate>) -> ApiCost {
    ROLLOUT_CACHE.forget_gone();
    let mut paths = Vec::new();
    for dir in ["sessions", "archived_sessions"] {
        files(&codex_home.join(dir), &is_rollout, &mut paths);
    }
    let plain: HashSet<PathBuf> = paths
        .iter()
        .filter(|path| !is_compressed(path))
        .cloned()
        .collect();
    paths.retain(|path| !is_compressed(path) || !plain.contains(&path.with_extension("")));
    let scans: Vec<(&Path, RolloutScan)> = paths
        .iter()
        .filter_map(|path| Some((path.as_path(), scan_of(path)?)))
        .collect();
    let read: HashSet<&Path> = paths.iter().map(PathBuf::as_path).collect();
    ROLLOUT_CACHE.retain(|path| !path.starts_with(codex_home) || read.contains(path));
    let turns = own_requests(&codex_home.join("sessions"), &scans);
    let turns = turns
        .iter()
        .map(|(turn, cwd)| (turn, cwd.as_deref()))
        .collect();
    tally(turns, prices, period)
}

fn is_rollout(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "jsonl")
        || is_compressed(path)
}

fn is_compressed(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|path| path.ends_with(".jsonl.zst"))
}

/// What the rollout at `path` says, the line still being written included.
fn scan_of(path: &Path) -> Option<RolloutScan> {
    let finish = |scan: &RolloutScan, tail: Option<&[u8]>, _| match tail {
        Some(tail) => {
            let mut whole = scan.clone();
            whole.read(tail);
            whole
        }
        None => scan.clone(),
    };
    if is_compressed(path) {
        let decode = |file| zstd::stream::read::Decoder::new(file).map(BufReader::new);
        ROLLOUT_CACHE.read_whole(path, decode, finish)
    } else {
        ROLLOUT_CACHE.read(path, finish)
    }
}

/// The requests the threads in `scans` made themselves, each with the folder
/// its thread works in. A thread in more than one rollout counts once, from
/// the one in `active`, the `sessions/` dir, if any.
fn own_requests(active: &Path, scans: &[(&Path, RolloutScan)]) -> Vec<(Turn, Option<String>)> {
    let mut threads: HashMap<&str, &RolloutScan> = HashMap::new();
    let mut unnamed = Vec::new();
    for (path, scan) in scans {
        let Some(id) = scan.id.as_deref() else {
            unnamed.push(scan);
            continue;
        };
        match threads.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(scan);
            }
            Entry::Occupied(mut entry) if path.starts_with(active) => {
                entry.insert(scan);
            }
            Entry::Occupied(_) => {}
        }
    }
    threads
        .values()
        .copied()
        .chain(unnamed)
        .flat_map(|scan| {
            let parent = scan
                .parent
                .as_deref()
                .filter(|parent| scan.id.as_deref() != Some(parent))
                .map(|parent| {
                    threads
                        .get(parent)
                        .map(|parent| replayable(parent, scan.started))
                });
            let skipped = parent.map_or(0, |parent| replayed(&scan.requests, parent.as_deref()));
            scan.requests[skipped..]
                .iter()
                .map(|request| (turn_of(request), scan.cwd.clone()))
        })
        .collect()
}

/// The usage of the requests `parent` made up to `forked_at`, which a thread
/// forked from it then replays.
fn replayable(parent: &RolloutScan, forked_at: Option<DateTime<Utc>>) -> Vec<Usage> {
    parent
        .requests
        .iter()
        .take_while(|request| forked_at.is_none_or(|forked_at| request.at <= forked_at))
        .map(|request| request.usage)
        .collect()
}

/// How many of a fork's `requests`, at its start, replay its parent's: those
/// matching `parent`'s replayable usage in order or, when none do or the
/// parent's rollout isn't there, a leading burst written within
/// [`BURST_PAUSE`] of each other.
fn replayed(requests: &[Request], parent: Option<&[Usage]>) -> usize {
    let matched = parent.map_or(0, |parent| {
        requests
            .iter()
            .zip(parent)
            .take_while(|(request, usage)| request.usage == **usage)
            .count()
    });
    if matched > 0 {
        return matched;
    }
    let close =
        |pair: &[Request]| (TimeDelta::zero()..=BURST_PAUSE).contains(&(pair[1].at - pair[0].at));
    match requests.windows(2).take_while(|pair| close(pair)).count() {
        0 => 0,
        pairs => pairs + 1,
    }
}

fn turn_of(request: &Request) -> Turn {
    Turn {
        at: request.at,
        model: request
            .model
            .clone()
            .unwrap_or_else(|| UNNAMED_MODEL.to_string()),
        tokens: request.usage.tokens(),
        web_searches: request.web_searches,
        tier: request.tier,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::super::{local_day, Coverage, Period};
    use super::*;
    use crate::usage::prices::tests::catalog_of;

    const NOW: &str = "2026-10-15T12:00:00Z";

    fn today() -> NaiveDate {
        local_day(NOW.parse().unwrap())
    }

    /// `at` plus `seconds`, as Codex writes timestamps.
    fn at(seconds: i64) -> String {
        let at: DateTime<Utc> = NOW.parse().unwrap();
        (at + TimeDelta::seconds(seconds)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    fn prices() -> Catalog {
        let per_token = |rate: f64| rate / 1_000_000.0;
        catalog_of(
            json!({
                "gpt-5.6": {
                    "litellm_provider": "openai",
                    "input_cost_per_token": per_token(4.0),
                    "output_cost_per_token": per_token(20.0),
                    "cache_creation_input_token_cost": per_token(5.0),
                    "cache_read_input_token_cost": per_token(0.4),
                    "input_cost_per_token_priority": per_token(8.0),
                    "output_cost_per_token_priority": per_token(40.0),
                    "search_context_cost_per_query": { "search_context_size_medium": 0.01 },
                },
                "gpt-6-luna": {
                    "litellm_provider": "openai",
                    "input_cost_per_token": per_token(1.0),
                    "output_cost_per_token": per_token(5.0),
                },
                "claude-opus-5-5": {
                    "litellm_provider": "anthropic",
                    "input_cost_per_token": per_token(4.0),
                    "output_cost_per_token": per_token(20.0),
                },
            }),
            "2026-10-14T08:00:00Z",
        )
    }

    fn meta(id: &str, at_seconds: i64, extra: Value) -> Value {
        let mut payload =
            json!({ "id": id, "cwd": "/work/app", "originator": "codex_cli_rs", "source": "cli" });
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({ "timestamp": at(at_seconds), "type": "session_meta", "payload": payload })
    }

    fn turn_context(model: &str) -> Value {
        json!({ "timestamp": at(0), "type": "turn_context", "payload": { "model": model, "cwd": "/work/app" } })
    }

    fn tier(service_tier: &str) -> Value {
        json!({
            "timestamp": at(0),
            "type": "event_msg",
            "payload": { "type": "thread_settings_applied", "thread_settings": { "service_tier": service_tier } },
        })
    }

    fn usage(input: u64, cached: u64, output: u64) -> Value {
        json!({
            "input_tokens": input,
            "cached_input_tokens": cached,
            "cache_write_input_tokens": 0,
            "output_tokens": output,
            "reasoning_output_tokens": output / 2,
            "total_tokens": input + output,
        })
    }

    /// A `token_count` event at `at_seconds`, of the running total `total`
    /// and the last request's `last`.
    fn count(at_seconds: i64, total: Value, last: Value) -> Value {
        json!({
            "timestamp": at(at_seconds),
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": { "total_token_usage": total, "last_token_usage": last, "model_context_window": 258_400 },
                "rate_limits": null,
            },
        })
    }

    /// Writes `lines` to `<home>/<relative>`, one record per line.
    fn write(home: &Path, relative: &str, lines: &[Value]) {
        let path = home.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
        fs::write(path, body).unwrap();
    }

    const ROLLOUT: &str = "sessions/2026/10/15/rollout-2026-10-15T12-00-00-t1.jsonl";

    fn cost_of(home: &Path) -> ApiCost {
        read(home, &prices(), Period::Last30Days.days(today()))
    }

    fn assert_usd(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected ${expected}, got ${actual}"
        );
    }

    #[test]
    fn each_request_counts_once_and_cached_input_is_priced_as_cache_reads() {
        let home = tempdir().unwrap();
        write(
            home.path(),
            ROLLOUT,
            &[
                meta("t1", 0, json!({})),
                turn_context("gpt-5.6"),
                count(
                    10,
                    usage(1_000_000, 750_000, 100_000),
                    usage(1_000_000, 750_000, 100_000),
                ),
                // Codex repeats the last count, its total unmoved.
                count(
                    11,
                    usage(1_000_000, 750_000, 100_000),
                    usage(1_000_000, 750_000, 100_000),
                ),
                // Without the last request's usage, the running total says it.
                json!({
                    "timestamp": at(30),
                    "type": "event_msg",
                    "payload": { "type": "token_count", "info": { "total_token_usage": usage(3_000_000, 750_000, 100_000) } },
                }),
            ],
        );

        let cost = cost_of(home.path());

        // 250k + 2M uncached input at $4, 750k cached at $0.40, 100k output
        // (reasoning included) at $20.
        assert_usd(cost.by_kind.input, 9.0);
        assert_usd(cost.by_kind.cache_read, 0.3);
        assert_usd(cost.by_kind.output, 2.0);
        assert_eq!(cost.coverage, Coverage::Complete);
        assert_eq!(cost.projects[0].path.as_deref(), Some("/work/app"));
    }

    #[test]
    fn cache_writes_are_part_of_the_input_and_priced_apart() {
        let home = tempdir().unwrap();
        let written = json!({
            "input_tokens": 1_000_000,
            "cached_input_tokens": 0,
            "cache_write_input_tokens": 400_000,
            "output_tokens": 0,
            "total_tokens": 1_000_000,
        });
        write(
            home.path(),
            ROLLOUT,
            &[
                meta("t1", 0, json!({})),
                turn_context("gpt-5.6"),
                count(10, written.clone(), written),
            ],
        );

        let cost = cost_of(home.path());

        assert_usd(cost.by_kind.input, 2.4);
        assert_usd(cost.by_kind.cache_write_5m, 2.0);
    }

    #[test]
    fn each_request_is_priced_as_the_model_and_tier_in_effect() {
        let home = tempdir().unwrap();
        let one = usage(1_000_000, 0, 0);
        let two = usage(2_000_000, 0, 0);
        let three = usage(3_000_000, 0, 0);
        let four = usage(4_000_000, 0, 0);
        write(
            home.path(),
            ROLLOUT,
            &[
                meta("t1", 0, json!({})),
                turn_context("gpt-5.6"),
                count(10, one.clone(), one.clone()),
                tier("priority"),
                count(20, two, one.clone()),
                turn_context("gpt-6-luna"),
                tier("default"),
                count(30, three, one.clone()),
                tier("turbo"),
                count(40, four, one),
            ],
        );

        let cost = cost_of(home.path());

        let models: Vec<(&str, f64)> = cost
            .by_model
            .iter()
            .map(|model| (model.model.as_str(), model.usd))
            .collect();
        assert_eq!(models, [("GPT-5.6", 4.0 + 8.0), ("GPT-6 Luna", 1.0)]);
        assert_eq!(cost.coverage, Coverage::Partial);
        assert_eq!(cost.unpriced[0].model, "GPT-6 Luna");
        assert_eq!(cost.unpriced[0].missing, Some("rates for its service tier"));
    }

    #[test]
    fn a_fork_doesnt_count_again_what_it_replays_of_its_parent() {
        let home = tempdir().unwrap();
        let (one, two, three) = (
            usage(1_000_000, 0, 0),
            usage(2_000_000, 0, 0),
            usage(3_000_000, 0, 0),
        );
        write(
            home.path(),
            ROLLOUT,
            &[
                meta("parent", 0, json!({})),
                turn_context("gpt-5.6"),
                count(10, one.clone(), one.clone()),
                count(20, two.clone(), one.clone()),
                // After the fork: not part of what the fork replays.
                count(500, three.clone(), one.clone()),
            ],
        );
        write(
            home.path(),
            "sessions/2026/10/15/rollout-2026-10-15T12-05-00-child.jsonl",
            &[
                meta("child", 300, json!({ "forked_from_id": "parent" })),
                turn_context("gpt-5.6"),
                // Too far apart for a burst: only matching the parent's
                // usage tells them for a replay.
                count(300, one.clone(), one.clone()),
                count(302, two.clone(), one.clone()),
                count(320, three, one),
            ],
        );

        let cost = cost_of(home.path());

        // The parent's three requests and the fork's own one.
        assert_usd(cost.by_kind.input, 16.0);
    }

    #[test]
    fn a_subagent_without_its_parents_rollout_skips_the_burst_it_replayed() {
        let home = tempdir().unwrap();
        let one = usage(1_000_000, 0, 0);
        let source =
            json!({ "subagent": { "thread_spawn": { "parent_thread_id": "gone", "depth": 1 } } });
        write(
            home.path(),
            ROLLOUT,
            &[
                meta("agent", 0, json!({ "source": source })),
                turn_context("gpt-5.6"),
                // Replayed in one go, then the subagent's own request.
                count(0, usage(1_000_000, 0, 0), one.clone()),
                count(0, usage(2_000_000, 0, 0), one.clone()),
                count(15, usage(3_000_000, 0, 0), one),
            ],
        );

        assert_usd(cost_of(home.path()).by_kind.input, 4.0);
    }

    #[test]
    fn a_thread_both_active_and_archived_counts_once() {
        let home = tempdir().unwrap();
        let one = usage(1_000_000, 0, 0);
        let lines = [
            meta("t1", 0, json!({})),
            turn_context("gpt-5.6"),
            count(10, one.clone(), one),
        ];
        write(home.path(), ROLLOUT, &lines);
        write(
            home.path(),
            "archived_sessions/rollout-2026-10-15T12-00-00-t1.jsonl",
            &lines,
        );

        assert_usd(cost_of(home.path()).by_kind.input, 4.0);
    }

    #[test]
    fn a_compressed_rollout_is_read_unless_its_plain_copy_is_there() {
        let home = tempdir().unwrap();
        let one = usage(1_000_000, 0, 0);
        let lines = [
            meta("t1", 0, json!({})),
            turn_context("gpt-5.6"),
            count(10, one.clone(), one),
        ];
        let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
        let compressed = home.path().join(format!("{ROLLOUT}.zst"));
        fs::create_dir_all(compressed.parent().unwrap()).unwrap();
        let mut encoder =
            zstd::stream::write::Encoder::new(fs::File::create(&compressed).unwrap(), 0).unwrap();
        encoder.write_all(body.as_bytes()).unwrap();
        encoder.finish().unwrap();

        assert_usd(cost_of(home.path()).by_kind.input, 4.0);

        // Mid-way through Codex restoring it, both are there.
        write(home.path(), ROLLOUT, &lines);
        assert_usd(cost_of(home.path()).by_kind.input, 4.0);
    }

    #[test]
    fn hosted_web_searches_are_billed_with_their_request() {
        let home = tempdir().unwrap();
        let one = usage(1_000_000, 0, 0);
        let search = json!({
            "timestamp": at(5),
            "type": "response_item",
            "payload": { "type": "web_search_call", "status": "completed", "action": { "type": "search", "query": "q" } },
        });
        write(
            home.path(),
            ROLLOUT,
            &[
                meta("t1", 0, json!({})),
                turn_context("gpt-5.6"),
                search.clone(),
                search,
                count(10, one.clone(), one),
            ],
        );

        assert_usd(cost_of(home.path()).by_kind.web_search, 0.02);
    }

    #[test]
    fn a_request_of_a_model_the_list_doesnt_have_is_unpriced() {
        let home = tempdir().unwrap();
        let one = usage(1_000_000, 0, 0);
        write(
            home.path(),
            ROLLOUT,
            &[meta("t1", 0, json!({})), count(10, one.clone(), one)],
        );

        let cost = cost_of(home.path());

        assert_eq!(cost.coverage, Coverage::Unpriced);
        assert_eq!(cost.unpriced[0].model, UNNAMED_MODEL);
        assert_eq!(cost.unpriced[0].tokens, 1_000_000);
    }
}
