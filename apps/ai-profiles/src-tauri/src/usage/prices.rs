//! API list prices from LiteLLM's model price list, shared by the Claude Code
//! and Codex estimates.
//!
//! The list is [`SOURCE_URL`], fetched as data with a plain GET that carries
//! nothing about the profiles, their transcripts or their accounts. What the
//! app keeps of it, the rates of Anthropic's and OpenAI's own models, is
//! cached in the app's data dir. A cache over a day old is refreshed in the
//! background, at startup and when an estimate is read, and a download only
//! replaces it once it validates. A failed refresh keeps the last valid cache,
//! and without one the app uses the snapshot it was built with
//! (`price-snapshot.json`). Either way an estimate is at that day's rates, not
//! what was billed when the use happened.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, RwLock};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::api_cost::KindCosts;

const SOURCE_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

/// How long a downloaded list is used before it is refreshed.
const MAX_AGE: TimeDelta = TimeDelta::hours(24);

/// How long after trying to refresh the list another try waits, so an app
/// left offline doesn't try on every read.
const RETRY_AFTER: TimeDelta = TimeDelta::hours(1);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The providers whose own models' rates are kept.
const PROVIDERS: &[&str] = &["anthropic", "openai"];

/// The rates a model's entry is kept with, by the names LiteLLM gives them.
/// Each also comes in variants for long prompts (`_above_200k_tokens`),
/// service tiers (`_priority`, `_flex`) and batches, all kept.
const RATE_NAMES: &[&str] = &[
    "input_cost_per_token",
    "output_cost_per_token",
    "cache_creation_input_token_cost",
    "cache_read_input_token_cost",
];

/// What one web search costs, by search context size.
const WEB_SEARCH_RATES: &str = "search_context_cost_per_query";

/// The list as the app was built with it, in the cache's format.
const SNAPSHOT: &str = include_str!("price-snapshot.json");

/// The models' rates, as of when they were fetched. Also the cache's format.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    fetched_at: DateTime<Utc>,
    /// LiteLLM's entries for the models, by id, with only their provider and
    /// rates (USD per token, per search for web searches).
    models: Map<String, Value>,
}

impl Catalog {
    /// The list in `litellm`, LiteLLM's file as downloaded at `fetched_at`,
    /// with only what is kept of it. An error says why it isn't one to use.
    fn from_litellm(litellm: &[u8], fetched_at: DateTime<Utc>) -> Result<Catalog, String> {
        let all: Map<String, Value> = serde_json::from_slice(litellm)
            .map_err(|error| format!("not a price list: {error}"))?;
        let models = all
            .into_iter()
            .filter_map(|(id, entry)| Some((id.clone(), kept(&id, entry)?)))
            .collect();
        Catalog { fetched_at, models }.checked()
    }

    /// The list, once it has a model of each of [`PROVIDERS`] and each model
    /// has valid input and output rates.
    fn checked(self) -> Result<Catalog, String> {
        for provider in PROVIDERS {
            let has_one = self
                .models
                .values()
                .any(|entry| entry["litellm_provider"].as_str() == Some(provider));
            if !has_one {
                return Err(format!("no {provider} models"));
            }
        }
        if let Some((id, _)) = self
            .models
            .iter()
            .find(|(_, entry)| !entry.as_object().is_some_and(valid))
        {
            return Err(format!("invalid rates for {id}"));
        }
        Ok(self)
    }

    /// The day the rates are of, in UTC.
    pub fn as_of(&self) -> NaiveDate {
        self.fetched_at.date_naive()
    }

    fn is_stale(&self, now: DateTime<Utc>) -> bool {
        now - self.fetched_at > MAX_AGE
    }

    /// What `model` costs, if the list has it: by its id, or by the id
    /// without the date some end in (`-20251001`, `-2025-10-01`).
    pub fn model(&self, model: &str) -> Option<Rates<'_>> {
        let entry = self
            .models
            .get(model)
            .or_else(|| self.models.get(undated(model)?))?;
        entry.as_object().map(Rates)
    }
}

/// What is kept of LiteLLM's entry for model `id`: its provider and rates,
/// when it is one of [`PROVIDERS`]' own models (not one served by another
/// platform, `bedrock/…`) with input and output rates (not an image or
/// embedding model).
fn kept(id: &str, entry: Value) -> Option<Value> {
    let Value::Object(entry) = entry else {
        return None;
    };
    let provider = entry.get("litellm_provider")?.as_str()?;
    if id.contains('/') || !PROVIDERS.contains(&provider) {
        return None;
    }
    let has = |rate: &str| entry.get(rate).is_some_and(Value::is_number);
    if !has("input_cost_per_token") || !has("output_cost_per_token") {
        return None;
    }
    let kept = entry
        .into_iter()
        .filter(|(name, _)| {
            name == "litellm_provider"
                || name == WEB_SEARCH_RATES
                || RATE_NAMES.iter().any(|rate| name.starts_with(rate))
        })
        .collect();
    Some(Value::Object(kept))
}

/// Whether a kept entry has a provider, input and output rates, and only
/// rates that are amounts.
fn valid(entry: &Map<String, Value>) -> bool {
    let amount = |value: &Value| {
        value
            .as_f64()
            .is_some_and(|rate| rate.is_finite() && rate >= 0.0)
    };
    entry.get("litellm_provider").is_some_and(Value::is_string)
        && entry.contains_key("input_cost_per_token")
        && entry.contains_key("output_cost_per_token")
        && entry.iter().all(|(name, value)| match name.as_str() {
            "litellm_provider" => true,
            WEB_SEARCH_RATES => value
                .as_object()
                .is_some_and(|sizes| sizes.values().all(amount)),
            _ => amount(value),
        })
}

/// `model` without the date it ends in, if it ends in one.
fn undated(model: &str) -> Option<&str> {
    let all_digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
    if let Some((head, date)) = model.rsplit_once('-') {
        if date.len() == 8 && all_digits(date) {
            return Some(head);
        }
    }
    // `-YYYY-MM-DD`: 11 bytes, with the dashes where they go.
    let split = model.len().checked_sub(11)?;
    let (head, date) = model.split_at_checked(split)?;
    let parts: Vec<&str> = date.split('-').collect();
    (parts.len() == 4
        && parts[0].is_empty()
        && [4, 2, 2] == [parts[1].len(), parts[2].len(), parts[3].len()]
        && parts[1..].iter().all(|part| all_digits(part)))
    .then_some(head)
}

/// Tokens one request was billed for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    /// Input not read from the cache nor written to it.
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
}

impl Tokens {
    /// The prompt's length, as long-prompt rates count it.
    pub fn prompt(&self) -> u64 {
        self.input + self.cache_write_5m + self.cache_write_1h + self.cache_read
    }

    pub fn total(&self) -> u64 {
        self.prompt() + self.output
    }
}

/// How fast a request was served, which some models charge more or less for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Standard,
    /// OpenAI's priority processing (Codex's `fast` or `priority`).
    Priority,
    /// OpenAI's flex processing.
    Flex,
    /// Claude's fast mode. LiteLLM has no rates for it.
    Fast,
    /// A tier the app doesn't know.
    Unknown,
}

/// One model's entry in the [`Catalog`].
pub struct Rates<'a>(&'a Map<String, Value>);

impl Rates<'_> {
    /// What a request for `tokens` and `web_searches`, served at `tier`,
    /// costs. When the list has no rate for something it used, what that is
    /// instead, e.g. "fast-mode rates": the request is then left unpriced
    /// rather than counted short.
    pub fn cost(
        &self,
        tokens: &Tokens,
        web_searches: u64,
        tier: Tier,
    ) -> Result<KindCosts, &'static str> {
        let tier_suffix = match tier {
            Tier::Standard => "",
            Tier::Priority => "_priority",
            Tier::Flex => "_flex",
            Tier::Fast => return Err("fast-mode rates"),
            Tier::Unknown => return Err("rates for its service tier"),
        };
        let long_suffix = self.long_prompt_suffix(tokens.prompt());
        let usd = |name: &str, count: u64, missing: &'static str| {
            if count == 0 {
                return Ok(0.0);
            }
            let rate = self
                .0
                .get(&format!("{name}{long_suffix}{tier_suffix}"))
                .and_then(Value::as_f64);
            match rate {
                Some(rate) => Ok(count as f64 * rate),
                None if tier == Tier::Priority => Err("priority rates"),
                None if tier == Tier::Flex => Err("flex rates"),
                None if !long_suffix.is_empty() => Err("long-prompt rates"),
                None => Err(missing),
            }
        };
        let web_search = match web_searches {
            0 => 0.0,
            count => {
                let rate = self
                    .0
                    .get(WEB_SEARCH_RATES)
                    .and_then(|sizes| sizes.get("search_context_size_medium"))
                    .and_then(Value::as_f64)
                    .ok_or("web search rates")?;
                count as f64 * rate
            }
        };
        Ok(KindCosts {
            input: usd("input_cost_per_token", tokens.input, "input rates")?,
            output: usd("output_cost_per_token", tokens.output, "output rates")?,
            cache_write_5m: usd(
                "cache_creation_input_token_cost",
                tokens.cache_write_5m,
                "cache write rates",
            )?,
            cache_write_1h: usd(
                "cache_creation_input_token_cost_above_1hr",
                tokens.cache_write_1h,
                "1-hour cache write rates",
            )?,
            cache_read: usd(
                "cache_read_input_token_cost",
                tokens.cache_read,
                "cache read rates",
            )?,
            web_search,
        })
    }

    /// What names the rates a prompt of `prompt` tokens pays end in:
    /// `_above_200k_tokens` when the model charges more over 200k tokens and
    /// the prompt is longer, else nothing.
    fn long_prompt_suffix(&self, prompt: u64) -> String {
        self.0
            .keys()
            .filter_map(|name| {
                let thousands = name
                    .strip_prefix("input_cost_per_token_above_")?
                    .strip_suffix("k_tokens")?;
                thousands.parse::<u64>().ok()
            })
            .filter(|thousands| prompt > thousands * 1_000)
            .max()
            .map(|thousands| format!("_above_{thousands}k_tokens"))
            .unwrap_or_default()
    }
}

/// The list in use: loaded when first asked for, replaced by each refresh.
static CURRENT: LazyLock<RwLock<Arc<Catalog>>> =
    LazyLock::new(|| RwLock::new(Arc::new(load(cache_path().as_deref()))));

/// When refreshing the list was last tried.
static LAST_TRY: Mutex<Option<DateTime<Utc>>> = Mutex::new(None);

/// The list in use.
pub fn current() -> Arc<Catalog> {
    CURRENT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Refresh the list in the background when it is over [`MAX_AGE`] old,
/// unless that was tried within [`RETRY_AFTER`]. Needs Tauri's async runtime.
pub fn refresh_if_stale() {
    let now = Utc::now();
    if !current().is_stale(now) {
        return;
    }
    {
        let mut last_try = LAST_TRY.lock().unwrap_or_else(PoisonError::into_inner);
        if last_try.is_some_and(|tried| now - tried < RETRY_AFTER) {
            return;
        }
        *last_try = Some(now);
    }
    tauri::async_runtime::spawn(async {
        let downloaded = download().await;
        match adopt(cache_path().as_deref(), downloaded, Utc::now()) {
            Ok(catalog) => {
                *CURRENT.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(catalog);
            }
            Err(error) => eprintln!(
                "ai-profiles: kept the prices of {}: {error}",
                current().as_of()
            ),
        }
    });
}

async fn download() -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("ai-profiles/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(SOURCE_URL)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.to_string())?;
    let body = response.bytes().await.map_err(|error| error.to_string())?;
    Ok(body.to_vec())
}

/// The list in `downloaded`, fetched at `now`, once it validates, written to
/// the cache at `path` (a cache that can't be written only costs a download
/// next time). An error when it couldn't be downloaded or isn't valid, the
/// cache then left as it was.
fn adopt(
    path: Option<&Path>,
    downloaded: Result<Vec<u8>, String>,
    now: DateTime<Utc>,
) -> Result<Catalog, String> {
    let catalog = Catalog::from_litellm(&downloaded?, now)?;
    if let Some(path) = path {
        let written = serde_json::to_vec(&catalog)
            .map_err(|error| error.to_string())
            .and_then(|body| {
                crate::profiles::atomic_write(path, &body).map_err(|error| error.to_string())
            });
        if let Err(error) = written {
            eprintln!("ai-profiles: couldn't cache the prices: {error}");
        }
    }
    Ok(catalog)
}

fn cache_path() -> Option<PathBuf> {
    crate::paths::app_data_dir()
        .ok()
        .map(|dir| dir.join("price-catalog.json"))
}

/// The newer of the cache at `path`, when it is valid, and the snapshot.
fn load(path: Option<&Path>) -> Catalog {
    let snapshot = snapshot();
    let cached = path
        .and_then(|path| fs::read(path).ok())
        .and_then(|body| serde_json::from_slice::<Catalog>(&body).ok())
        .and_then(|catalog| catalog.checked().ok());
    match cached {
        Some(cached) if cached.fetched_at > snapshot.fetched_at => cached,
        _ => snapshot,
    }
}

fn snapshot() -> Catalog {
    serde_json::from_str::<Catalog>(SNAPSHOT)
        .map_err(|error| error.to_string())
        .and_then(Catalog::checked)
        .expect("the built-in price snapshot is a valid price list")
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    /// A list fetched at `at`, of LiteLLM's entries `models`.
    pub(crate) fn catalog_of(models: Value, at: &str) -> Catalog {
        Catalog::from_litellm(models.to_string().as_bytes(), at.parse().unwrap()).unwrap()
    }

    /// An OpenAI model, which every list needs.
    pub(crate) fn openai_model() -> Value {
        json!({
            "litellm_provider": "openai",
            "input_cost_per_token": 2e-6,
            "output_cost_per_token": 1e-5,
        })
    }

    fn litellm() -> Value {
        json!({
            "sample_spec": { "litellm_provider": "one of the providers", "input_cost_per_token": 0.0 },
            "claude-opus-5-5": {
                "litellm_provider": "anthropic",
                "mode": "chat",
                "max_tokens": 128_000,
                "input_cost_per_token": 4e-6,
                "output_cost_per_token": 2e-5,
                "input_cost_per_token_batches": 2e-6,
                "search_context_cost_per_query": { "search_context_size_medium": 0.01 },
            },
            "bedrock/claude-opus-5-5": {
                "litellm_provider": "bedrock",
                "input_cost_per_token": 4e-6,
                "output_cost_per_token": 2e-5,
            },
            "gpt-image-1": { "litellm_provider": "openai", "input_cost_per_image": 0.04 },
            "gpt-5.6": openai_model(),
        })
    }

    const FETCHED: &str = "2026-10-16T09:00:00Z";

    #[test]
    fn keeps_only_the_providers_own_models_and_their_rates() {
        let catalog = catalog_of(litellm(), FETCHED);

        let ids: Vec<&String> = catalog.models.keys().collect();
        assert_eq!(ids, ["claude-opus-5-5", "gpt-5.6"]);
        let opus = catalog.models["claude-opus-5-5"].as_object().unwrap();
        assert!(opus.contains_key("input_cost_per_token_batches"));
        assert!(opus.contains_key(WEB_SEARCH_RATES));
        assert!(!opus.contains_key("mode") && !opus.contains_key("max_tokens"));
        assert_eq!(catalog.as_of().to_string(), "2026-10-16");
    }

    #[test]
    fn refuses_a_download_that_isnt_a_valid_price_list() {
        let at: DateTime<Utc> = FETCHED.parse().unwrap();
        let refused = |body: &str| Catalog::from_litellm(body.as_bytes(), at).is_err();

        assert!(refused("<html>rate limited</html>"));
        assert!(refused("[]"));
        // Without one provider's models, every estimate of its use would
        // come out unpriced.
        assert!(refused(
            &json!({ "claude-opus-5-5": litellm()["claude-opus-5-5"] }).to_string()
        ));
        let mut negative = litellm();
        negative["gpt-5.6"]["cache_read_input_token_cost"] = json!(-1.0);
        assert!(refused(&negative.to_string()));
        let mut text = litellm();
        text["claude-opus-5-5"][WEB_SEARCH_RATES]["search_context_size_medium"] = json!("0.01");
        assert!(refused(&text.to_string()));
    }

    #[test]
    fn a_valid_download_replaces_the_cache_and_a_failed_one_keeps_it() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("price-catalog.json");
        // Newer than the snapshot, which would be used otherwise.
        let fetched = snapshot().fetched_at + TimeDelta::days(1);
        let first = adopt(
            Some(&cache),
            Ok(litellm().to_string().into_bytes()),
            fetched,
        )
        .unwrap();
        let written = fs::read(&cache).unwrap();

        let later = fetched + TimeDelta::days(2);
        assert!(adopt(Some(&cache), Err("offline".into()), later).is_err());
        assert!(adopt(Some(&cache), Ok(b"{}".to_vec()), later).is_err());

        assert_eq!(fs::read(&cache).unwrap(), written);
        assert_eq!(load(Some(&cache)).fetched_at, first.fetched_at);
    }

    #[test]
    fn without_a_valid_cache_the_snapshot_is_used() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("price-catalog.json");
        assert_eq!(load(Some(&cache)).fetched_at, snapshot().fetched_at);

        fs::write(
            &cache,
            b"{\"fetchedAt\":\"2099-01-01T00:00:00Z\",\"models\":{}}",
        )
        .unwrap();
        assert_eq!(load(Some(&cache)).fetched_at, snapshot().fetched_at);
    }

    #[test]
    fn a_cache_older_than_the_snapshot_gives_way_to_it() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("price-catalog.json");
        adopt(
            Some(&cache),
            Ok(litellm().to_string().into_bytes()),
            "2020-01-01T00:00:00Z".parse().unwrap(),
        )
        .unwrap();

        assert_eq!(load(Some(&cache)).fetched_at, snapshot().fetched_at);
    }

    #[test]
    fn a_list_is_refreshed_once_a_day_old() {
        let catalog = catalog_of(litellm(), FETCHED);
        assert!(!catalog.is_stale("2026-10-17T08:59:00Z".parse().unwrap()));
        assert!(catalog.is_stale("2026-10-17T09:01:00Z".parse().unwrap()));
    }

    #[test]
    fn the_snapshot_has_the_models_claude_code_and_codex_run() {
        let snapshot = snapshot();
        for model in [
            "claude-opus-5-5",
            "claude-haiku-4-5-20251001",
            "gpt-5.6-terra",
        ] {
            assert!(
                snapshot.model(model).is_some(),
                "{model} is in the snapshot"
            );
        }
    }

    #[test]
    fn dated_ids_are_found_as_their_model() {
        let catalog = catalog_of(litellm(), FETCHED);
        assert!(catalog.model("claude-opus-5-5-20261001").is_some());
        assert!(catalog.model("gpt-5.6-2026-10-01").is_some());
        assert!(catalog.model("claude-opus-5-55").is_none());
        assert!(catalog.model("gpt-5.6-2026-1001").is_none());
    }

    /// A list of model `m`, with `entry` as its entry.
    fn rates(entry: Value) -> Catalog {
        let mut models = litellm();
        models["m"] = entry;
        catalog_of(models, FETCHED)
    }

    fn tokens(input: u64, cache_read: u64) -> Tokens {
        Tokens {
            input,
            cache_read,
            ..Tokens::default()
        }
    }

    #[test]
    fn a_long_prompt_pays_the_long_prompt_rates() {
        let catalog = rates(json!({
            "litellm_provider": "anthropic",
            "input_cost_per_token": 1e-7,
            "output_cost_per_token": 5e-7,
            "cache_read_input_token_cost": 1e-8,
            "input_cost_per_token_above_100k_tokens": 5e-7,
            "output_cost_per_token_above_100k_tokens": 2.5e-6,
            "cache_read_input_token_cost_above_100k_tokens": 5e-8,
        }));
        let model = catalog.model("m").unwrap();

        let short = model.cost(&tokens(100_000, 0), 0, Tier::Standard).unwrap();
        // Over the threshold once its cache reads count.
        let long = model.cost(&tokens(100_000, 1), 0, Tier::Standard).unwrap();

        assert!((short.input - 0.01).abs() < 1e-12);
        assert!((long.input - 0.05).abs() < 1e-12);
    }

    #[test]
    fn a_tier_is_priced_at_its_own_rates() {
        let catalog = rates(json!({
            "litellm_provider": "openai",
            "input_cost_per_token": 1e-6,
            "output_cost_per_token": 1e-5,
            "input_cost_per_token_priority": 2e-6,
            "output_cost_per_token_priority": 2e-5,
        }));
        let model = catalog.model("m").unwrap();

        let priority = model
            .cost(&tokens(1_000_000, 0), 0, Tier::Priority)
            .unwrap();

        assert!((priority.input - 2.0).abs() < 1e-12);
        assert_eq!(model.cost(&tokens(1, 0), 0, Tier::Flex), Err("flex rates"));
        assert_eq!(
            model.cost(&tokens(1, 0), 0, Tier::Unknown),
            Err("rates for its service tier")
        );
    }

    #[test]
    fn what_the_list_has_no_rate_for_leaves_the_request_unpriced() {
        let catalog = rates(json!({
            "litellm_provider": "anthropic",
            "input_cost_per_token": 1e-6,
            "output_cost_per_token": 1e-5,
        }));
        let model = catalog.model("m").unwrap();
        let cost = |tokens: Tokens, web_searches, tier| model.cost(&tokens, web_searches, tier);

        assert_eq!(cost(tokens(1, 0), 0, Tier::Fast), Err("fast-mode rates"));
        assert_eq!(
            cost(tokens(0, 1), 0, Tier::Standard),
            Err("cache read rates")
        );
        assert_eq!(
            cost(tokens(1, 0), 1, Tier::Standard),
            Err("web search rates")
        );
        let one_hour = Tokens {
            cache_write_1h: 1,
            ..Tokens::default()
        };
        assert_eq!(
            cost(one_hour, 0, Tier::Standard),
            Err("1-hour cache write rates")
        );
        // What it didn't use needs no rate.
        assert!(cost(tokens(1, 0), 0, Tier::Standard).is_ok());
    }

    #[test]
    fn a_long_prompt_without_long_prompt_rates_is_unpriced() {
        let catalog = rates(json!({
            "litellm_provider": "openai",
            "input_cost_per_token": 1e-6,
            "output_cost_per_token": 1e-5,
            "cache_read_input_token_cost": 1e-7,
            "input_cost_per_token_above_272k_tokens": 2e-6,
        }));
        let model = catalog.model("m").unwrap();

        assert_eq!(
            model.cost(&tokens(1, 300_000), 0, Tier::Standard),
            Err("long-prompt rates")
        );
    }

    /// Writes LiteLLM's current list, as the app keeps it, over the snapshot
    /// the app is built with. Fetches it, so run it by hand:
    /// `cargo test update_the_price_snapshot -- --ignored`.
    #[tokio::test]
    #[ignore = "fetches LiteLLM's price list"]
    async fn update_the_price_snapshot() {
        let catalog = Catalog::from_litellm(&download().await.unwrap(), Utc::now()).unwrap();
        let mut body = serde_json::to_string_pretty(&catalog).unwrap();
        body.push('\n');
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/usage/price-snapshot.json");
        fs::write(path, body).unwrap();
    }
}
