//! An estimate of what a profile's Claude Code or Codex use would cost at API
//! prices. It is not the profile's bill: the transcripts don't say how each
//! request was paid for (API key or subscription), so nothing is compared
//! against a plan. Ordinary chats in the Claude and ChatGPT apps and on the
//! web keep no token counts on disk and aren't part of it.
//!
//! Claude Code's use ([`read`]) is read from the transcripts in the profile's config dir,
//! `projects/**/*.jsonl`, subagents' included: each response Claude Code
//! records carries the `usage` the API reported for it. A response is written
//! once per content block, each copy with the same message and request id, and
//! a resumed or forked session copies earlier responses into its new
//! transcript, so responses are counted once per profile. Transcripts left
//! elsewhere (archived, moved to another profile) are not counted here.
//! Codex's ([`codex::read`]) is read from its rollouts.
//!
//! Prices come from LiteLLM's price list ([`super::prices`]): nothing about
//! the use leaves the machine. A request of a model the list doesn't have, or
//! that used something it has no rate for (fast mode, say), is reported as
//! unpriced, never counted as free.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use chrono::{DateTime, Days, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::prices::{Catalog, Tier, Tokens};
use crate::line_cache::{LineCache, LineScan};

pub(crate) mod codex;

/// How many days, today included, the estimate covers. Claude Code
/// deletes transcripts after 30 days unless told otherwise
/// (`cleanupPeriodDays`), so a longer window would mostly be empty.
pub const WINDOW_DAYS: u32 = 30;

/// What a profile's Claude Code or Codex use over the last [`WINDOW_DAYS`]
/// would cost at API list prices, in USD: an estimate of the recorded use, not
/// what the profile was billed, whether it pays by API key or by
/// subscription.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiCost {
    pub total_usd: f64,
    /// What the whole window would cost at the same pace, when the
    /// transcripts on disk cover only part of it.
    pub projected_usd: Option<f64>,
    /// How many days of the window the transcripts on disk reach back to,
    /// today included: 0 when there are none.
    pub covered_days: u32,
    /// Each day of the window, oldest first.
    pub days: Vec<DayCost>,
    /// By what was billed.
    pub by_kind: KindCosts,
    /// By model, most expensive first.
    pub by_model: Vec<ModelCost>,
    /// By the folder each session started in, most expensive first.
    pub projects: Vec<ProjectCost>,
    /// Use the price list has no rates for, by tokens used, most first. Its
    /// tokens are in none of the costs.
    pub unpriced: Vec<UnpricedUse>,
    /// How much of the use in the window the costs cover.
    pub coverage: Coverage,
    /// The day the price list is of, `YYYY-MM-DD`.
    pub prices_as_of: NaiveDate,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayCost {
    /// `YYYY-MM-DD`, in local time.
    pub date: NaiveDate,
    pub total_usd: f64,
    /// By model, in [`ApiCost::by_model`]'s order, models without cost left
    /// out.
    pub by_model: Vec<ModelCost>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    /// The model's name, e.g. "Opus 5.5".
    pub model: String,
    pub usd: f64,
}

/// What was billed, in USD. Codex's cache writes, which have no duration, are
/// in `cache_write_5m`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindCosts {
    pub input: f64,
    pub output: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub web_search: f64,
}

impl KindCosts {
    fn total(&self) -> f64 {
        self.input
            + self.output
            + self.cache_write_5m
            + self.cache_write_1h
            + self.cache_read
            + self.web_search
    }

    fn add(&mut self, other: KindCosts) {
        self.input += other.input;
        self.output += other.output;
        self.cache_write_5m += other.cache_write_5m;
        self.cache_write_1h += other.cache_write_1h;
        self.cache_read += other.cache_read;
        self.web_search += other.web_search;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCost {
    /// The folder, `None` for sessions that don't say.
    pub path: Option<String>,
    pub usd: f64,
}

/// How much of the use in the window has a price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Coverage {
    /// All of it, or there was none.
    Complete,
    /// Some of it: the total leaves the rest out.
    Partial,
    /// None of it: the total of $0 means nothing.
    Unpriced,
}

impl Coverage {
    fn of(priced: bool, unpriced: bool) -> Self {
        match (priced, unpriced) {
            (_, false) => Coverage::Complete,
            (true, true) => Coverage::Partial,
            (false, true) => Coverage::Unpriced,
        }
    }
}

/// Requests of one model left unpriced for one reason.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnpricedUse {
    /// The model's id, as the transcripts record it.
    pub model: String,
    /// What the price list has no rate for, e.g. "fast-mode rates". `None`
    /// when it doesn't have the model.
    pub missing: Option<&'static str>,
    pub tokens: u64,
}

/// One API request, as a transcript recorded it.
#[derive(Debug, Clone)]
struct Turn {
    at: DateTime<Utc>,
    model: String,
    tokens: Tokens,
    web_searches: u64,
    tier: Tier,
}

/// What a transcript's lines say so far.
#[derive(Debug, Clone, Default)]
struct UsageScan {
    /// The folder the session started in: the first `cwd` recorded.
    project: Option<String>,
    /// Its responses, by message and request id. Shared with the cache's copy
    /// until a line changes them.
    turns: Arc<HashMap<String, Turn>>,
}

/// A line is only decoded for usage if it holds this. Claude Code writes
/// records without spaces, and in tool output it shows up escaped, so it only
/// matches a record's own `usage`.
const USAGE_MARKER: &str = "\"usage\":{";

/// The fields of a transcript record read here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    timestamp: Option<String>,
    request_id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    /// The cache writes by how long they last. Without it, all of them are
    /// taken as 5-minute writes, the default.
    cache_creation: Option<CacheCreation>,
    server_tool_use: Option<ServerToolUse>,
    /// `fast` for a request run in fast mode.
    speed: Option<String>,
}

#[derive(Deserialize)]
struct CacheCreation {
    #[serde(default)]
    ephemeral_5m_input_tokens: u64,
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
}

#[derive(Deserialize)]
struct ServerToolUse {
    #[serde(default)]
    web_search_requests: u64,
}

/// A record read only for the folder its session works in.
#[derive(Deserialize)]
struct CwdRecord {
    cwd: Option<String>,
}

impl LineScan for UsageScan {
    /// Take in the record on `line`: its `cwd` while none is known, and the
    /// response it carries, if any. Of the copies of one response, the one
    /// with the most output wins, as one written mid-stream has less.
    fn read(&mut self, line: &[u8]) {
        let Ok(line) = std::str::from_utf8(line) else {
            return;
        };
        if self.project.is_none() && line.contains("\"cwd\":\"") {
            self.project = serde_json::from_str::<CwdRecord>(line)
                .ok()
                .and_then(|record| record.cwd)
                .filter(|cwd| !cwd.is_empty());
        }
        if !line.contains(USAGE_MARKER) {
            return;
        }
        let Some((key, turn)) = serde_json::from_str::<Record>(line).ok().and_then(turn_of) else {
            return;
        };
        let turns = Arc::make_mut(&mut self.turns);
        match turns.get(&key) {
            Some(kept) if kept.tokens.output > turn.tokens.output => {}
            _ => {
                turns.insert(key, turn);
            }
        }
    }
}

/// The response `record` carries, by its message and request id. `None` for
/// a record without one, or one that used nothing, like the stand-ins Claude
/// Code writes for errors (model `<synthetic>`).
fn turn_of(record: Record) -> Option<(String, Turn)> {
    let message = record.message?;
    let usage = message.usage?;
    let key = format!(
        "{}:{}",
        message.id?,
        record.request_id.as_deref().unwrap_or_default()
    );
    let (cache_write_5m, cache_write_1h) = match usage.cache_creation {
        Some(split) => (
            split.ephemeral_5m_input_tokens,
            split.ephemeral_1h_input_tokens,
        ),
        None => (usage.cache_creation_input_tokens, 0),
    };
    let tokens = Tokens {
        input: usage.input_tokens,
        output: usage.output_tokens,
        cache_write_5m,
        cache_write_1h,
        cache_read: usage.cache_read_input_tokens,
    };
    let web_searches = usage
        .server_tool_use
        .map_or(0, |tools| tools.web_search_requests);
    if tokens.total() == 0 && web_searches == 0 {
        return None;
    }
    Some((
        key,
        Turn {
            at: record.timestamp?.parse().ok()?,
            model: message.model.unwrap_or_default(),
            tokens,
            web_searches,
            tier: match usage.speed.as_deref() {
                Some("fast") => Tier::Fast,
                _ => Tier::Standard,
            },
        },
    ))
}

/// Transcripts already read, by path.
static USAGE_CACHE: LazyLock<LineCache<UsageScan, UsageScan>> = LazyLock::new(LineCache::default);

/// The estimated API cost of the Claude Code use recorded in `config_dir`, as
/// of `today`, at `prices`. Walks every transcript, so it belongs on a
/// blocking thread.
pub fn read(config_dir: &Path, prices: &Catalog, today: NaiveDate) -> ApiCost {
    USAGE_CACHE.forget_gone();
    let projects = config_dir.join("projects");
    let mut paths = Vec::new();
    files(
        &projects,
        &|path| path.extension().is_some_and(|ext| ext == "jsonl"),
        &mut paths,
    );
    let scans: Vec<UsageScan> = paths
        .iter()
        .filter_map(|path| {
            USAGE_CACHE.read(path, |scan, tail, _| match tail {
                Some(tail) => {
                    let mut whole = scan.clone();
                    whole.read(tail);
                    whole
                }
                None => scan.clone(),
            })
        })
        .collect();
    let read: HashSet<PathBuf> = paths.into_iter().collect();
    USAGE_CACHE.retain(|path| !path.starts_with(&projects) || read.contains(path));
    tally(once_each(&scans), prices, today)
}

/// Every file in `dir` and the folders in it that is `wanted`, into
/// `found`. Symlinked folders aren't followed.
fn files(dir: &Path, wanted: &dyn Fn(&Path) -> bool, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => files(&path, wanted, found),
            Ok(kind) if kind.is_file() && wanted(&path) => found.push(path),
            _ => {}
        }
    }
}

/// The day `at` falls on, in local time.
fn local_day(at: DateTime<Utc>) -> NaiveDate {
    at.with_timezone(&Local).date_naive()
}

/// The responses in `scans`, each with the folder its session started in. A
/// response in more than one transcript counts once, from the copy with the
/// most output.
fn once_each(scans: &[UsageScan]) -> Vec<(&Turn, Option<&str>)> {
    let mut kept: HashMap<&str, (&Turn, Option<&str>)> = HashMap::new();
    for scan in scans {
        for (key, turn) in scan.turns.iter() {
            match kept.get(key.as_str()) {
                Some((other, _)) if other.tokens.output >= turn.tokens.output => {}
                _ => {
                    kept.insert(key.as_str(), (turn, scan.project.as_deref()));
                }
            }
        }
    }
    kept.into_values().collect()
}

/// The name people know `model` by: "Opus 5.5" for `claude-opus-5-5`,
/// "GPT-5.6 Terra" for `gpt-5.6-terra`. Any other id as it is.
fn label_of(model: &str) -> String {
    let capitalized = |word: &str| {
        let mut chars = word.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    };
    if let Some(rest) = model.strip_prefix("claude-") {
        let mut parts = rest.split('-');
        let family = parts.next().map(capitalized).unwrap_or_default();
        let version: Vec<&str> = parts
            .take_while(|part| part.len() <= 2 && part.bytes().all(|byte| byte.is_ascii_digit()))
            .collect();
        return format!("{family} {}", version.join("."))
            .trim_end()
            .to_string();
    }
    if let Some(rest) = model.strip_prefix("gpt-") {
        let mut parts = rest.split('-');
        let version = parts.next().unwrap_or_default();
        let variant: Vec<String> = parts.map(capitalized).collect();
        return format!("GPT-{version} {}", variant.join(" "))
            .trim_end()
            .to_string();
    }
    model.to_string()
}

/// The estimated API cost at `prices` of the requests in `turns`, each with
/// the folder its session started in, over the window ending `today`.
fn tally(turns: Vec<(&Turn, Option<&str>)>, prices: &Catalog, today: NaiveDate) -> ApiCost {
    let start = today - Days::new(u64::from(WINDOW_DAYS - 1));
    let mut oldest: Option<NaiveDate> = None;
    let mut by_kind = KindCosts::default();
    let mut by_day: HashMap<NaiveDate, HashMap<String, f64>> = HashMap::new();
    let mut by_model: HashMap<String, f64> = HashMap::new();
    let mut by_project: HashMap<Option<&str>, f64> = HashMap::new();
    let mut unpriced: HashMap<(&str, Option<&'static str>), u64> = HashMap::new();
    let mut priced_any = false;
    for (turn, project) in turns {
        let day = local_day(turn.at);
        oldest = Some(oldest.map_or(day, |oldest| oldest.min(day)));
        if day < start || day > today {
            continue;
        }
        let costs = match prices.model(&turn.model) {
            Some(rates) => rates
                .cost(&turn.tokens, turn.web_searches, turn.tier)
                .map_err(Some),
            None => Err(None),
        };
        let costs = match costs {
            Ok(costs) => costs,
            Err(missing) => {
                *unpriced.entry((turn.model.as_str(), missing)).or_default() += turn.tokens.total();
                continue;
            }
        };
        priced_any = true;
        let usd = costs.total();
        let label = label_of(&turn.model);
        by_kind.add(costs);
        *by_day
            .entry(day)
            .or_default()
            .entry(label.clone())
            .or_default() += usd;
        *by_model.entry(label).or_default() += usd;
        *by_project.entry(project).or_default() += usd;
    }

    let mut by_model: Vec<ModelCost> = by_model
        .into_iter()
        .map(|(model, usd)| ModelCost { model, usd })
        .collect();
    by_model.sort_by(|left, right| {
        right
            .usd
            .total_cmp(&left.usd)
            .then(left.model.cmp(&right.model))
    });
    let days = start
        .iter_days()
        .take_while(|day| *day <= today)
        .map(|date| {
            let spent = by_day.remove(&date).unwrap_or_default();
            let by_model: Vec<ModelCost> = by_model
                .iter()
                .filter_map(|model| {
                    Some(ModelCost {
                        model: model.model.clone(),
                        usd: *spent.get(&model.model)?,
                    })
                })
                .collect();
            DayCost {
                date,
                total_usd: by_model.iter().map(|model| model.usd).sum(),
                by_model,
            }
        })
        .collect();
    let mut projects: Vec<ProjectCost> = by_project
        .into_iter()
        .map(|(path, usd)| ProjectCost {
            path: path.map(str::to_owned),
            usd,
        })
        .collect();
    projects.sort_by(|left, right| {
        right
            .usd
            .total_cmp(&left.usd)
            .then(left.path.cmp(&right.path))
    });
    let mut unpriced: Vec<UnpricedUse> = unpriced
        .into_iter()
        .map(|((model, missing), tokens)| UnpricedUse {
            model: model.to_owned(),
            missing,
            tokens,
        })
        .collect();
    unpriced.sort_by(|left, right| {
        right
            .tokens
            .cmp(&left.tokens)
            .then(left.model.cmp(&right.model))
            .then(left.missing.cmp(&right.missing))
    });

    let covered_days = oldest.map_or(0, |oldest| {
        let from = oldest.max(start).min(today);
        u32::try_from((today - from).num_days() + 1).unwrap_or(WINDOW_DAYS)
    });
    let total_usd = by_kind.total();
    let coverage = Coverage::of(priced_any, !unpriced.is_empty());
    ApiCost {
        total_usd,
        projected_usd: (covered_days > 0 && covered_days < WINDOW_DAYS)
            .then(|| total_usd / f64::from(covered_days) * f64::from(WINDOW_DAYS)),
        covered_days,
        days,
        by_kind,
        by_model,
        projects,
        unpriced,
        coverage,
        prices_as_of: prices.as_of(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::*;
    use crate::usage::prices::tests::{catalog_of, openai_model};

    /// Noon UTC on the tests' today, which is the same day in any time zone
    /// a test is likely to run in.
    const NOW: &str = "2026-10-15T12:00:00Z";

    fn today() -> NaiveDate {
        local_day(NOW.parse().unwrap())
    }

    /// An assistant record at `at`, of response `id` from `model`, that used
    /// `usage`.
    fn response(at: &str, id: &str, model: &str, usage: Value) -> Value {
        json!({
            "type": "assistant",
            "timestamp": at,
            "requestId": format!("req_{id}"),
            "cwd": "/work/app",
            "isSidechain": false,
            "message": { "id": id, "model": model, "role": "assistant", "usage": usage },
        })
    }

    /// Writes `lines` to `<config>/projects/<relative>`, one record per line.
    fn write(config: &Path, relative: &str, lines: &[Value]) {
        let path = config.join("projects").join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
        fs::write(path, body).unwrap();
    }

    /// A model's LiteLLM entry, with rates in USD per million tokens.
    fn anthropic(input: f64, output: f64, write_5m: f64, write_1h: f64, read: f64) -> Value {
        let per_token = |rate: f64| rate / 1_000_000.0;
        json!({
            "litellm_provider": "anthropic",
            "input_cost_per_token": per_token(input),
            "output_cost_per_token": per_token(output),
            "cache_creation_input_token_cost": per_token(write_5m),
            "cache_creation_input_token_cost_above_1hr": per_token(write_1h),
            "cache_read_input_token_cost": per_token(read),
            "search_context_cost_per_query": { "search_context_size_medium": 0.01 },
            "supports_fast_mode": true,
        })
    }

    fn prices() -> Catalog {
        catalog_of(
            json!({
                "claude-opus-5-5": anthropic(4.0, 20.0, 5.0, 8.0, 0.2),
                "claude-sonnet-5-5": anthropic(2.0, 10.0, 2.5, 4.0, 0.1),
                "claude-haiku-4-5": anthropic(1.0, 5.0, 1.25, 2.0, 0.1),
                "gpt-5.6": openai_model(),
            }),
            "2026-10-14T08:00:00Z",
        )
    }

    fn cost_of(config: &Path) -> ApiCost {
        read(config, &prices(), today())
    }

    fn assert_usd(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected ${expected}, got ${actual}"
        );
    }

    #[test]
    fn a_response_written_once_per_content_block_counts_once_at_its_fullest() {
        let config = tempdir().unwrap();
        let usage = |output: u64| json!({ "input_tokens": 1_000_000, "output_tokens": output });
        write(
            config.path(),
            "-work-app/s1.jsonl",
            &[
                response(NOW, "msg_1", "claude-opus-5-5", usage(10)),
                response(NOW, "msg_1", "claude-opus-5-5", usage(1_000_000)),
                response(NOW, "msg_1", "claude-opus-5-5", usage(10)),
            ],
        );

        let cost = cost_of(config.path());

        assert_usd(cost.by_kind.input, 4.0);
        assert_usd(cost.by_kind.output, 20.0);
        assert_usd(cost.total_usd, 24.0);
        assert_eq!(cost.coverage, Coverage::Complete);
    }

    #[test]
    fn cache_writes_are_priced_by_how_long_they_last() {
        let config = tempdir().unwrap();
        write(
            config.path(),
            "-work-app/s1.jsonl",
            &[
                response(
                    NOW,
                    "msg_1",
                    "claude-opus-5-5",
                    json!({
                        "cache_creation_input_tokens": 3_000_000,
                        "cache_read_input_tokens": 1_000_000,
                        "cache_creation": {
                            "ephemeral_5m_input_tokens": 1_000_000,
                            "ephemeral_1h_input_tokens": 2_000_000,
                        },
                    }),
                ),
                // Without the split, every write is a 5-minute one.
                response(
                    NOW,
                    "msg_2",
                    "claude-opus-5-5",
                    json!({ "cache_creation_input_tokens": 1_000_000 }),
                ),
            ],
        );

        let cost = cost_of(config.path());

        assert_usd(cost.by_kind.cache_write_5m, 10.0);
        assert_usd(cost.by_kind.cache_write_1h, 16.0);
        assert_usd(cost.by_kind.cache_read, 0.2);
    }

    #[test]
    fn subagents_count_and_a_response_copied_into_another_transcript_counts_once() {
        let config = tempdir().unwrap();
        let usage = json!({ "output_tokens": 1_000_000 });
        write(
            config.path(),
            "-work-app/s1.jsonl",
            &[response(NOW, "msg_1", "claude-sonnet-5-5", usage.clone())],
        );
        // A fork of s1 copies its responses before adding its own.
        write(
            config.path(),
            "-work-app/s2.jsonl",
            &[
                response(NOW, "msg_1", "claude-sonnet-5-5", usage.clone()),
                response(NOW, "msg_2", "claude-sonnet-5-5", usage.clone()),
            ],
        );
        write(
            config.path(),
            "-work-app/s2/subagents/agent-a1.jsonl",
            &[response(NOW, "msg_3", "claude-haiku-4-5-20251001", usage)],
        );

        let cost = cost_of(config.path());

        assert_usd(cost.total_usd, 10.0 + 10.0 + 5.0);
        let models: Vec<(&str, f64)> = cost
            .by_model
            .iter()
            .map(|model| (model.model.as_str(), model.usd))
            .collect();
        assert_eq!(models, [("Sonnet 5.5", 20.0), ("Haiku 4.5", 5.0)]);
        assert_eq!(cost.projects.len(), 1);
        assert_eq!(cost.projects[0].path.as_deref(), Some("/work/app"));
    }

    #[test]
    fn web_searches_cost_extra_and_fast_mode_without_rates_is_unpriced() {
        let config = tempdir().unwrap();
        write(
            config.path(),
            "-work-app/s1.jsonl",
            &[
                response(
                    NOW,
                    "msg_1",
                    "claude-opus-5-5",
                    json!({
                        "input_tokens": 1_000_000,
                        "server_tool_use": { "web_search_requests": 3 },
                    }),
                ),
                // LiteLLM has no fast-mode rates, so this isn't priced at
                // the standard ones.
                response(
                    NOW,
                    "msg_2",
                    "claude-opus-5-5",
                    json!({ "input_tokens": 1_000_000, "speed": "fast" }),
                ),
            ],
        );

        let cost = cost_of(config.path());

        assert_usd(cost.by_kind.input, 4.0);
        assert_usd(cost.by_kind.web_search, 0.03);
        assert_eq!(cost.coverage, Coverage::Partial);
        assert_eq!(cost.unpriced.len(), 1);
        assert_eq!(cost.unpriced[0].model, "claude-opus-5-5");
        assert_eq!(cost.unpriced[0].missing, Some("fast-mode rates"));
        assert_eq!(cost.unpriced[0].tokens, 1_000_000);
    }

    #[test]
    fn a_model_the_price_list_doesnt_have_is_unpriced_not_free() {
        let config = tempdir().unwrap();
        write(
            config.path(),
            "-work-app/s1.jsonl",
            &[
                response(
                    NOW,
                    "msg_1",
                    "claude-opus-9",
                    json!({ "input_tokens": 70, "output_tokens": 30 }),
                ),
                // An error stand-in used nothing and isn't reported.
                response(NOW, "msg_2", "<synthetic>", json!({ "input_tokens": 0 })),
            ],
        );

        let cost = cost_of(config.path());

        assert_usd(cost.total_usd, 0.0);
        assert_eq!(cost.coverage, Coverage::Unpriced);
        assert_eq!(cost.unpriced.len(), 1);
        assert_eq!(cost.unpriced[0].model, "claude-opus-9");
        assert_eq!(cost.unpriced[0].missing, None);
        assert_eq!(cost.unpriced[0].tokens, 100);
    }

    #[test]
    fn use_partly_of_a_model_the_price_list_doesnt_have_is_partial() {
        let config = tempdir().unwrap();
        let usage = json!({ "output_tokens": 1_000_000 });
        write(
            config.path(),
            "-work-app/s1.jsonl",
            &[
                response(NOW, "msg_1", "claude-opus-5-5", usage.clone()),
                response(NOW, "msg_2", "claude-opus-9", usage),
            ],
        );

        let cost = cost_of(config.path());

        assert_usd(cost.total_usd, 20.0);
        assert_eq!(cost.coverage, Coverage::Partial);
        assert_eq!(cost.unpriced[0].model, "claude-opus-9");
    }

    #[test]
    fn a_tool_result_quoting_a_usage_block_is_not_a_response() {
        let config = tempdir().unwrap();
        let quoted = r#"{"message":{"id":"msg_9","usage":{"input_tokens":1000000}}}"#;
        write(
            config.path(),
            "-work-app/s1.jsonl",
            &[json!({
                "type": "user",
                "timestamp": NOW,
                "cwd": "/work/app",
                "message": { "role": "user", "content": [{ "type": "tool_result", "content": quoted }] },
            })],
        );

        assert_eq!(cost_of(config.path()).covered_days, 0);
    }

    #[test]
    fn only_the_window_counts_and_a_shorter_history_is_projected() {
        let config = tempdir().unwrap();
        let usage = json!({ "output_tokens": 100_000 });
        write(
            config.path(),
            "-work-app/s1.jsonl",
            &[
                response(
                    "2026-10-06T12:00:00Z",
                    "msg_1",
                    "claude-opus-5-5",
                    usage.clone(),
                ),
                response(NOW, "msg_2", "claude-opus-5-5", usage),
            ],
        );

        let cost = cost_of(config.path());

        assert_eq!(cost.days.len(), WINDOW_DAYS as usize);
        assert_eq!(cost.days.last().unwrap().date, today());
        assert_usd(cost.days.last().unwrap().total_usd, 2.0);
        assert_eq!(cost.covered_days, 10);
        assert_usd(cost.total_usd, 4.0);
        assert_usd(cost.projected_usd.unwrap(), 12.0);

        // 29 days on, the first response has left the window, which the
        // second still covers in full.
        let later = today() + Days::new(29);
        let cost = read(config.path(), &prices(), later);
        assert_usd(cost.total_usd, 2.0);
        assert_eq!(cost.covered_days, WINDOW_DAYS);
        assert_eq!(cost.projected_usd, None);
    }

    #[test]
    fn a_config_dir_without_transcripts_covers_no_days() {
        let config = tempdir().unwrap();

        let cost = cost_of(config.path());

        assert_eq!(cost.covered_days, 0);
        assert_eq!(cost.projected_usd, None);
        assert_usd(cost.total_usd, 0.0);
        assert_eq!(cost.coverage, Coverage::Complete);
    }

    #[test]
    fn models_are_named_as_people_know_them() {
        assert_eq!(label_of("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(label_of("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(label_of("claude-opus-5"), "Opus 5");
        assert_eq!(label_of("gpt-5.6-terra"), "GPT-5.6 Terra");
        assert_eq!(label_of("gpt-5.3-codex"), "GPT-5.3 Codex");
        assert_eq!(label_of("gpt-6"), "GPT-6");
        assert_eq!(label_of("o4-mini"), "o4-mini");
    }

    #[test]
    fn the_estimate_says_what_day_its_prices_are_of() {
        let config = tempdir().unwrap();

        assert_eq!(
            cost_of(config.path()).prices_as_of.to_string(),
            "2026-10-14"
        );
    }

    #[test]
    fn an_api_key_profile_and_one_beside_a_desktop_subscription_get_the_same_estimate() {
        // The same recorded use, once by a profile signed in with an API key
        // and once by one whose desktop app is on a Max plan: neither is
        // compared against a plan, and both get the same estimate.
        let api_key = tempdir().unwrap();
        let subscribed = tempdir().unwrap();
        fs::write(
            api_key.path().join(".claude.json"),
            json!({ "primaryApiKey": "sk-ant-test" }).to_string(),
        )
        .unwrap();
        fs::write(
            subscribed.path().join(".claude.json"),
            json!({ "oauthAccount": { "organizationType": "claude_max" } }).to_string(),
        )
        .unwrap();
        for config in [&api_key, &subscribed] {
            write(
                config.path(),
                "-work-app/s1.jsonl",
                &[response(
                    NOW,
                    "msg_1",
                    "claude-opus-5-5",
                    json!({ "output_tokens": 1_000_000 }),
                )],
            );
        }

        let estimates = [api_key.path(), subscribed.path()]
            .map(|config| serde_json::to_value(cost_of(config)).unwrap());

        assert_eq!(estimates[0], estimates[1]);
        assert_eq!(estimates[0]["totalUsd"], json!(20.0));
        let fields: Vec<&String> = estimates[0].as_object().unwrap().keys().collect();
        assert!(
            fields
                .iter()
                .all(|field| !field.to_lowercase().contains("plan")),
            "an estimate carries no plan: {fields:?}"
        );
    }
}
