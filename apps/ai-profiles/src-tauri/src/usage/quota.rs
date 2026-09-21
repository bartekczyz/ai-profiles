use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex as TokioMutex;

use crate::usage::credentials::read_access_token;
use crate::usage::dead_credentials::DeadCredentialRegistry;
use crate::usage::{QuotaError, QuotaUsage, Spend, Window};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA_HEADER: &str = "oauth-2025-04-20";
const REQUEST_TIMEOUT_SECS: u64 = 8;
/// How long a successful usage response is reused. The numbers move slowly
/// and the endpoint's rate-limit budget is small, so caching for a few
/// minutes collapses the frontend's per-mount + 5-min-poll refetches (one
/// query per profile) into roughly one upstream call per token per window.
const QUOTA_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
/// Cooldown applied to a 429 that carries no usable `Retry-After`.
const DEFAULT_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(60);
/// Upper bound on an honoured `Retry-After`. This endpoint hands out up to
/// ~1h windows; cap there so a bogus header can't lock the card out longer.
const MAX_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(60 * 60);
/// Hard cap on response body size. The real response is well under 1 KiB.
const MAX_BODY_BYTES: usize = 1024 * 1024;

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    /// Parsed `Retry-After` (delta-seconds) from a 429, when present. Drives
    /// how long we negatively cache the rate limit so we stop re-poking the
    /// endpoint during its cooldown.
    pub retry_after: Option<Duration>,
}

/// Pure: parse an HTTP `Retry-After` header value. We support the
/// delta-seconds form (e.g. `"1800"`), which is what this endpoint returns.
/// The HTTP-date form is unsupported and yields `None`, falling back to
/// [`DEFAULT_RATE_LIMIT_COOLDOWN`].
fn parse_retry_after(header: Option<&str>) -> Option<Duration> {
    let seconds: u64 = header?.trim().parse().ok()?;
    Some(Duration::from_secs(seconds))
}

#[async_trait]
pub trait UsageClient: Send + Sync {
    async fn fetch(&self, access_token: &str) -> Result<HttpResponse, QuotaError>;
}

/// Production client using `reqwest`. Tests inject a stub instead.
/// The inner `reqwest::Client` is built once so the connection pool
/// is reused across the 5-minute refresh ticks.
pub struct ReqwestUsageClient {
    client: reqwest::Client,
    user_agent: String,
}

impl ReqwestUsageClient {
    pub fn new(user_agent: String) -> Result<Self, QuotaError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|_| QuotaError::Network)?;
        Ok(Self { client, user_agent })
    }
}

#[async_trait]
impl UsageClient for ReqwestUsageClient {
    async fn fetch(&self, access_token: &str) -> Result<HttpResponse, QuotaError> {
        let mut response = self
            .client
            .get(USAGE_URL)
            .bearer_auth(access_token)
            .header("anthropic-beta", BETA_HEADER)
            .header("user-agent", &self.user_agent)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|_| QuotaError::Network)?;
        let status = response.status().as_u16();
        let retry_after = parse_retry_after(
            response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
        );

        // Pre-flight cap: if the server advertises a body larger than
        // we're willing to read, refuse before buffering a single byte.
        // The real response is well under 1 KiB so this only ever fires
        // on a misconfigured/hostile endpoint.
        if let Some(content_length) = response.content_length() {
            if content_length > MAX_BODY_BYTES as u64 {
                return Err(QuotaError::Unknown);
            }
        }

        // Streamed read with a running cap, so a server that omits
        // Content-Length (or lies about it) still can't OOM us.
        let mut body: Vec<u8> = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
                        return Err(QuotaError::Unknown);
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(_) => return Err(QuotaError::Network),
            }
        }
        Ok(HttpResponse {
            status,
            body,
            retry_after,
        })
    }
}

/// Uncached fetch — kept for tests that exercise status-code mapping in
/// isolation. Production goes through [`fetch_quota_cached`].
#[cfg(test)]
async fn fetch_quota(
    cli_config_dir: &Path,
    client: &dyn UsageClient,
) -> Result<QuotaUsage, QuotaError> {
    let token = read_access_token(cli_config_dir)?;
    let response = client.fetch(&token).await?;
    parse_response(response)
}

/// Fetches quota through a per-token cache. Successful responses are reused
/// for [`QUOTA_CACHE_TTL`]; a `429` is negatively cached for its
/// `Retry-After` window. The frontend refetches usage on every mount and
/// every 5 minutes, per profile — without this, that floods a tiny
/// rate-limit budget. With it, each token makes at most one upstream call
/// per window, and during a cooldown we serve the rate-limited state from
/// cache instead of re-poking (and re-tripping) the endpoint.
pub async fn fetch_quota_cached(
    cli_config_dir: &Path,
    client: &dyn UsageClient,
    cache: &ClaudeQuotaCache,
    dead_credentials: &DeadCredentialRegistry,
) -> Result<QuotaUsage, QuotaError> {
    let token = read_access_token(cli_config_dir)?;
    // Credentials already known unrecoverable: surface NeedsLogin without a
    // network request or any cache poke. Re-auth rotates the token, which is a
    // different hash, so this naturally stops short-circuiting after sign-in.
    if dead_credentials.is_dead(&token) {
        return Err(QuotaError::NeedsLogin);
    }
    let key = token_cache_key(&token);
    if let Some(cached) = cache.get(&key) {
        return cached;
    }

    // Serialize cold fetches for the same token so two profiles refreshing
    // at once make one upstream call, not two.
    let slot = cache.slot(&key);
    let _guard = slot.lock().await;
    if let Some(cached) = cache.get(&key) {
        return cached;
    }

    let response = client.fetch(&token).await?;
    let retry_after = response.retry_after;
    match parse_response(response) {
        Ok(usage) => {
            cache.store_success(key, usage.clone());
            Ok(usage)
        }
        // Only rate limits are negatively cached. Network errors are
        // transient and Unauthorized drives the token-refresh retry in
        // `build_with_cli_refresh`, so neither must be pinned here.
        Err(QuotaError::RateLimited) => {
            cache.store_rate_limited(key, retry_after);
            Err(QuotaError::RateLimited)
        }
        Err(other) => Err(other),
    }
}

/// Maps an HTTP response to a quota result. Shared by the cached and
/// uncached fetch paths.
fn parse_response(response: HttpResponse) -> Result<QuotaUsage, QuotaError> {
    match response.status {
        200 => parse_body(&response.body),
        401 => Err(QuotaError::Unauthorized),
        // 403 is typically an edge/WAF policy block in front of the
        // endpoint, not an auth problem — mapping it to Unauthorized
        // would tell the user to re-auth for a transient block.
        403 => Err(QuotaError::Forbidden),
        429 => Err(QuotaError::RateLimited),
        500..=599 => Err(QuotaError::Network),
        // Other 4xx (400 bad request, 404 endpoint moved, 410 gone, …)
        // are client-side / contract-shape problems that won't resolve
        // by retrying — surface them as `Unknown` so the UI shows
        // "Couldn't load usage stats" rather than "check your connection".
        400..=499 => Err(QuotaError::Unknown),
        _ => Err(QuotaError::Unknown),
    }
}

/// What a cache entry remembers: a fresh successful quota, or that the
/// endpoint is rate-limiting this token right now. Each carries its own
/// expiry via the enclosing [`CacheEntry`].
/// The success payload is boxed so the enum isn't sized by it — `QuotaUsage`
/// carries a string, a vector and several windows, while `RateLimited` is a
/// bare tag.
#[derive(Clone)]
enum CachedOutcome {
    Success(Box<QuotaUsage>),
    RateLimited,
}

#[derive(Clone)]
struct CacheEntry {
    outcome: CachedOutcome,
    expires_at: Instant,
}

/// In-memory cache for Claude quota responses, keyed by a hash of the OAuth
/// access token. Successes cache for [`QUOTA_CACHE_TTL`]; rate limits cache
/// for their `Retry-After` cooldown. Cold fetches serialise per token.
#[derive(Default)]
pub struct ClaudeQuotaCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
    inflight: Mutex<HashMap<String, Arc<TokioMutex<()>>>>,
}

impl ClaudeQuotaCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached outcome for `key` while still fresh, mapped back
    /// to the `Result` a caller would have gotten from the network. Expired
    /// entries are evicted and treated as a miss.
    fn get(&self, key: &str) -> Option<Result<QuotaUsage, QuotaError>> {
        let mut entries = self.entries.lock().unwrap();
        match entries.get(key) {
            Some(entry) if Instant::now() < entry.expires_at => Some(match &entry.outcome {
                CachedOutcome::Success(usage) => Ok((**usage).clone()),
                CachedOutcome::RateLimited => Err(QuotaError::RateLimited),
            }),
            Some(_) => {
                entries.remove(key);
                None
            }
            None => None,
        }
    }

    fn store_success(&self, key: String, usage: QuotaUsage) {
        self.insert(
            key,
            CachedOutcome::Success(Box::new(usage)),
            QUOTA_CACHE_TTL,
        );
    }

    /// Negatively caches a rate limit for its `Retry-After` window, falling
    /// back to [`DEFAULT_RATE_LIMIT_COOLDOWN`] and clamped to
    /// [`MAX_RATE_LIMIT_COOLDOWN`] so a bogus header can't lock us out.
    fn store_rate_limited(&self, key: String, retry_after: Option<Duration>) {
        let cooldown = retry_after
            .unwrap_or(DEFAULT_RATE_LIMIT_COOLDOWN)
            .min(MAX_RATE_LIMIT_COOLDOWN);
        self.insert(key, CachedOutcome::RateLimited, cooldown);
    }

    fn insert(&self, key: String, outcome: CachedOutcome, ttl: Duration) {
        self.entries.lock().unwrap().insert(
            key,
            CacheEntry {
                outcome,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    fn slot(&self, key: &str) -> Arc<TokioMutex<()>> {
        let mut inflight = self.inflight.lock().unwrap();
        inflight
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }
}

fn token_cache_key(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn parse_body(body: &[u8]) -> Result<QuotaUsage, QuotaError> {
    let parsed: ApiResponse = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return Err(QuotaError::Unknown),
    };
    // `limits` is Anthropic's current contract and the only place the
    // per-model weekly sub-quota (and its display name) now appears — the
    // flat `seven_day_sonnet` / `seven_day_opus` fields are still emitted
    // but are permanently null. Fall back to them only when the array
    // yields nothing, so a rolled-back or cached older response still renders.
    let mut windows = windows_from_limits(parsed.limits);
    if windows.is_empty() {
        windows = windows_from_legacy_fields(
            parsed.five_hour,
            parsed.seven_day,
            parsed.seven_day_opus,
            parsed.seven_day_sonnet,
        );
    }
    let usage = QuotaUsage {
        primary: windows.primary,
        secondary: windows.secondary,
        scoped_weekly: windows.scoped_weekly,
        spend: parsed.spend.and_then(into_spend),
        rate_limit_reset_credits: None,
    };
    if usage.primary.is_none()
        && usage.secondary.is_none()
        && usage.scoped_weekly.is_empty()
        && usage.spend.is_none()
    {
        return Err(QuotaError::Unknown);
    }
    Ok(usage)
}

/// The Claude-shaped window set, before it's folded into a [`QuotaUsage`].
/// Exists so the `limits` path and the legacy-fields path can produce the
/// same thing and the caller can pick whichever came back non-empty.
#[derive(Default)]
struct ClaudeWindows {
    primary: Option<Window>,
    secondary: Option<Window>,
    scoped_weekly: Vec<Window>,
}

impl ClaudeWindows {
    fn is_empty(&self) -> bool {
        self.primary.is_none() && self.secondary.is_none() && self.scoped_weekly.is_empty()
    }
}

/// Folds the `limits` array into the window set. Entry kinds map as
/// `session` → primary (the 5-hour window), `weekly_all` → secondary, and
/// each `weekly_scoped` → one labelled per-model weekly row.
fn windows_from_limits(limits: Option<Vec<ApiLimit>>) -> ClaudeWindows {
    let mut windows = ClaudeWindows::default();
    for limit in limits.unwrap_or_default() {
        match limit.kind.as_deref() {
            Some("session") => windows.primary = Some(into_limit_window(limit, None)),
            Some("weekly_all") => windows.secondary = Some(into_limit_window(limit, None)),
            Some("weekly_scoped") => {
                let label = limit
                    .scope
                    .as_ref()
                    .and_then(|scope| scope.model.as_ref())
                    .and_then(|model| model.display_name.clone());
                windows.scoped_weekly.push(into_limit_window(limit, label));
            }
            // Anthropic ships new kinds before clients know what they mean
            // (the payload already carries several codenamed windows). An
            // unrecognised kind is dropped rather than guessed into a slot.
            _ => {}
        }
    }
    windows
}

/// Reads the pre-`limits` top-level fields. Scoped weeklies had a field per
/// model there, so their labels come from the field name rather than the
/// server.
fn windows_from_legacy_fields(
    five_hour: Option<ApiWindow>,
    seven_day: Option<ApiWindow>,
    seven_day_opus: Option<ApiWindow>,
    seven_day_sonnet: Option<ApiWindow>,
) -> ClaudeWindows {
    let mut scoped_weekly = Vec::new();
    if let Some(raw) = seven_day_opus {
        scoped_weekly.push(into_window(raw, Some("Opus".to_string())));
    }
    if let Some(raw) = seven_day_sonnet {
        scoped_weekly.push(into_window(raw, Some("Sonnet".to_string())));
    }
    ClaudeWindows {
        primary: five_hour.map(|raw| into_window(raw, None)),
        secondary: seven_day.map(|raw| into_window(raw, None)),
        scoped_weekly,
    }
}

/// Anthropic returns utilization as a percentage on a 0..=100 scale
/// (e.g. `42.0` means 42%). We accept any finite non-negative value
/// without an upper clamp — values above 100 are unusual but legitimate
/// (over-limit) and we'd rather show "105%" than drop the data. The
/// UI is responsible for capping the visual bar fill at 100%.
fn sanitize_percent(raw: Option<f32>) -> Option<f32> {
    match raw {
        Some(value) if value.is_finite() && value >= 0.0 => Some(value),
        _ => None,
    }
}

fn into_window(raw: ApiWindow, label: Option<String>) -> Window {
    Window {
        window_duration_mins: None,
        label,
        utilization: sanitize_percent(raw.utilization),
        resets_at: raw.resets_at,
    }
}

fn into_limit_window(raw: ApiLimit, label: Option<String>) -> Window {
    Window {
        window_duration_mins: None,
        label,
        utilization: sanitize_percent(raw.percent),
        resets_at: raw.resets_at,
    }
}

/// Maps the `spend` block onto [`Spend`], or `None` when there is nothing
/// worth a row: credits switched off for the account, or no usable amount.
fn into_spend(raw: ApiSpend) -> Option<Spend> {
    if !raw.enabled {
        return None;
    }
    let used = raw.used?;
    // A cap denominated in another currency can't be compared against the
    // used amount, so it's dropped rather than rendered as "£78 of $300".
    let limit_minor = raw
        .limit
        .filter(|limit| limit.currency == used.currency)
        .map(|limit| limit.amount_minor);
    Some(Spend {
        used_minor: used.amount_minor,
        currency: used.currency,
        exponent: sanitize_exponent(used.exponent),
        limit_minor,
        percent: sanitize_percent(raw.percent),
    })
}

/// Minor-unit decimal places. Every currency Anthropic bills in sits in
/// 0..=6 (0 for JPY, 2 for USD/GBP); anything outside that is drift, and
/// two places is the safe assumption rather than a reason to hide the row.
fn sanitize_exponent(raw: Option<i32>) -> u32 {
    match raw {
        Some(value) if (0..=6).contains(&value) => value as u32,
        _ => 2,
    }
}

/// Claude's quota provider: an HTTP request to Anthropic's OAuth usage
/// endpoint, parsed into the generic [`QuotaUsage`] windows.
pub struct ClaudeQuotaProvider {
    client: ReqwestUsageClient,
    cache: &'static ClaudeQuotaCache,
    dead_credentials: &'static DeadCredentialRegistry,
}

impl ClaudeQuotaProvider {
    pub fn new(
        user_agent: String,
        cache: &'static ClaudeQuotaCache,
        dead_credentials: &'static DeadCredentialRegistry,
    ) -> Result<Self, QuotaError> {
        Ok(Self {
            client: ReqwestUsageClient::new(user_agent)?,
            cache,
            dead_credentials,
        })
    }
}

#[async_trait]
impl crate::usage::QuotaProvider for ClaudeQuotaProvider {
    async fn fetch(&self, config_dir: &Path) -> Result<QuotaUsage, QuotaError> {
        fetch_quota_cached(config_dir, &self.client, self.cache, self.dead_credentials).await
    }
}

#[derive(Debug, Deserialize, Default)]
struct ApiResponse {
    #[serde(default)]
    limits: Option<Vec<ApiLimit>>,
    #[serde(default)]
    spend: Option<ApiSpend>,
    #[serde(default)]
    five_hour: Option<ApiWindow>,
    #[serde(default)]
    seven_day: Option<ApiWindow>,
    #[serde(default)]
    seven_day_opus: Option<ApiWindow>,
    #[serde(default)]
    seven_day_sonnet: Option<ApiWindow>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiWindow {
    #[serde(default)]
    utilization: Option<f32>,
    #[serde(default)]
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiLimit {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    percent: Option<f32>,
    #[serde(default)]
    resets_at: Option<String>,
    #[serde(default)]
    scope: Option<ApiScope>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiScope {
    #[serde(default)]
    model: Option<ApiScopeModel>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiScopeModel {
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiSpend {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    used: Option<ApiMoney>,
    #[serde(default)]
    limit: Option<ApiMoney>,
    #[serde(default)]
    percent: Option<f32>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiMoney {
    #[serde(default)]
    amount_minor: i64,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    exponent: Option<i32>,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    struct StubClient {
        status: u16,
        body: Vec<u8>,
    }

    #[async_trait]
    impl UsageClient for StubClient {
        async fn fetch(&self, _: &str) -> Result<HttpResponse, QuotaError> {
            Ok(HttpResponse {
                status: self.status,
                body: self.body.clone(),
                retry_after: None,
            })
        }
    }

    /// Counts upstream calls and can carry a `Retry-After`, for cache tests.
    struct CountingClient {
        calls: std::sync::Mutex<u32>,
        status: u16,
        body: Vec<u8>,
        retry_after: Option<Duration>,
    }

    #[async_trait]
    impl UsageClient for CountingClient {
        async fn fetch(&self, _: &str) -> Result<HttpResponse, QuotaError> {
            *self.calls.lock().unwrap() += 1;
            Ok(HttpResponse {
                status: self.status,
                body: self.body.clone(),
                retry_after: self.retry_after,
            })
        }
    }

    struct ErroringClient {
        error: QuotaError,
    }

    #[async_trait]
    impl UsageClient for ErroringClient {
        async fn fetch(&self, _: &str) -> Result<HttpResponse, QuotaError> {
            Err(self.error)
        }
    }

    fn dir_with_token() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"sk-test"}}"#,
        )
        .unwrap();
        dir
    }

    /// The live payload as of the `limits` rollout, trimmed to the fields we
    /// read. Captured from the real endpoint: the per-model weekly sub-quota
    /// now arrives as a `weekly_scoped` entry naming the model, and the flat
    /// `seven_day_sonnet` / `seven_day_opus` fields are permanently null.
    const LIVE_BODY: &[u8] = br#"{
        "five_hour": {"utilization": 4.0, "resets_at": "2026-09-21T17:20:00+00:00"},
        "seven_day": {"utilization": 24.0, "resets_at": "2026-09-25T02:00:00+00:00"},
        "seven_day_opus": null,
        "seven_day_sonnet": null,
        "nimbus_quill": {"utilization": 0.0, "resets_at": null},
        "limits": [
            {"kind": "session", "group": "session", "percent": 4, "severity": "normal",
             "resets_at": "2026-09-21T17:20:00+00:00", "scope": null, "is_active": false},
            {"kind": "weekly_all", "group": "weekly", "percent": 24, "severity": "normal",
             "resets_at": "2026-09-25T02:00:00+00:00", "scope": null, "is_active": true},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 19, "severity": "normal",
             "resets_at": "2026-09-25T02:00:00+00:00",
             "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null},
             "is_active": false}
        ],
        "spend": {
            "used": {"amount_minor": 7788, "currency": "GBP", "exponent": 2},
            "limit": {"amount_minor": 30000, "currency": "GBP", "exponent": 2},
            "percent": 26, "severity": "normal", "enabled": true
        }
    }"#;

    #[tokio::test]
    async fn happy_path_parses_all_windows() {
        let dir = dir_with_token();
        // Utilization is a percentage on the 0..=100 scale, matching
        // Anthropic's actual response (verified against the live endpoint).
        let body = br#"{
            "five_hour": {"utilization": 63.0, "resets_at": "2099-01-01T00:00:00Z"},
            "seven_day": {"utilization": 21.0, "resets_at": null},
            "seven_day_sonnet": {"utilization": 8.0, "resets_at": null}
        }"#;
        let client = StubClient {
            status: 200,
            body: body.to_vec(),
        };
        let usage = fetch_quota(dir.path(), &client).await.unwrap();
        assert!((usage.primary.unwrap().utilization.unwrap() - 63.0).abs() < 1e-4);
        assert_eq!(usage.secondary.unwrap().resets_at, None);
    }

    // -- the `limits` array --

    #[test]
    fn limits_array_fills_the_five_hour_and_weekly_windows() {
        let usage = parse_body(LIVE_BODY).unwrap();
        let five_hour = usage.primary.unwrap();
        let weekly = usage.secondary.unwrap();
        assert_eq!(five_hour.utilization, Some(4.0));
        assert_eq!(weekly.utilization, Some(24.0));
        assert_eq!(
            weekly.resets_at.as_deref(),
            Some("2026-09-25T02:00:00+00:00")
        );
        // Neither unscoped window names a model — the UI labels those itself.
        assert!(five_hour.label.is_none());
        assert!(weekly.label.is_none());
    }

    #[test]
    fn scoped_weekly_carries_the_server_supplied_model_name() {
        let usage = parse_body(LIVE_BODY).unwrap();
        assert_eq!(usage.scoped_weekly.len(), 1);
        let scoped = &usage.scoped_weekly[0];
        assert_eq!(scoped.label.as_deref(), Some("Fable"));
        assert_eq!(scoped.utilization, Some(19.0));
        assert_eq!(
            scoped.resets_at.as_deref(),
            Some("2026-09-25T02:00:00+00:00")
        );
    }

    #[test]
    fn every_scoped_weekly_gets_its_own_row() {
        // Anthropic has shipped two per-model weeklies at once before
        // (opus + sonnet), so the list must not collapse to one.
        let body = br#"{"limits":[
            {"kind":"weekly_scoped","percent":19,"scope":{"model":{"display_name":"Fable"}}},
            {"kind":"weekly_scoped","percent":3,"scope":{"model":{"display_name":"Opus"}}}
        ]}"#;
        let usage = parse_body(body).unwrap();
        let labels: Vec<_> = usage
            .scoped_weekly
            .iter()
            .map(|window| window.label.as_deref())
            .collect();
        assert_eq!(labels, vec![Some("Fable"), Some("Opus")]);
    }

    #[test]
    fn scoped_weekly_without_a_model_name_has_no_label() {
        let body = br#"{"limits":[{"kind":"weekly_scoped","percent":19,"scope":null}]}"#;
        let usage = parse_body(body).unwrap();
        assert_eq!(usage.scoped_weekly.len(), 1);
        assert!(usage.scoped_weekly[0].label.is_none());
    }

    #[test]
    fn unknown_limit_kinds_are_dropped_not_guessed_into_a_slot() {
        let body = br#"{"limits":[
            {"kind":"session","percent":5},
            {"kind":"monthly_tangelo","percent":80},
            {"kind":null,"percent":90}
        ]}"#;
        let usage = parse_body(body).unwrap();
        assert_eq!(usage.primary.unwrap().utilization, Some(5.0));
        assert!(usage.secondary.is_none());
        assert!(usage.scoped_weekly.is_empty());
    }

    #[test]
    fn legacy_fields_are_used_when_limits_is_absent() {
        // A rolled-back or cached pre-`limits` response must still render.
        let body = br#"{
            "five_hour": {"utilization": 63.0, "resets_at": null},
            "seven_day": {"utilization": 21.0, "resets_at": null},
            "seven_day_opus": {"utilization": 12.0, "resets_at": null},
            "seven_day_sonnet": {"utilization": 8.0, "resets_at": null}
        }"#;
        let usage = parse_body(body).unwrap();
        assert_eq!(usage.primary.unwrap().utilization, Some(63.0));
        assert_eq!(usage.secondary.unwrap().utilization, Some(21.0));
        let scoped: Vec<_> = usage
            .scoped_weekly
            .iter()
            .map(|window| (window.label.as_deref(), window.utilization))
            .collect();
        assert_eq!(
            scoped,
            vec![(Some("Opus"), Some(12.0)), (Some("Sonnet"), Some(8.0))],
        );
    }

    #[test]
    fn an_empty_limits_array_falls_back_to_the_legacy_fields() {
        let body = br#"{"limits":[],"five_hour":{"utilization":63.0,"resets_at":null}}"#;
        let usage = parse_body(body).unwrap();
        assert_eq!(usage.primary.unwrap().utilization, Some(63.0));
    }

    #[test]
    fn limits_win_over_the_legacy_fields_when_both_are_present() {
        // The live payload sends both; `limits` is the current contract.
        let usage = parse_body(LIVE_BODY).unwrap();
        assert_eq!(
            usage.scoped_weekly.len(),
            1,
            "legacy nulls must not add rows"
        );
        assert_eq!(usage.primary.unwrap().utilization, Some(4.0));
    }

    // -- usage credits (`spend`) --

    #[test]
    fn enabled_spend_is_parsed_in_minor_units() {
        let spend = parse_body(LIVE_BODY).unwrap().spend.unwrap();
        assert_eq!(spend.used_minor, 7788);
        assert_eq!(spend.limit_minor, Some(30000));
        assert_eq!(spend.currency, "GBP");
        assert_eq!(spend.exponent, 2);
        assert_eq!(spend.percent, Some(26.0));
    }

    #[test]
    fn disabled_spend_yields_no_row() {
        let body = br#"{
            "five_hour": {"utilization": 5.0},
            "spend": {"used": {"amount_minor": 0, "currency": "USD", "exponent": 2},
                      "limit": null, "percent": 0, "enabled": false}
        }"#;
        let usage = parse_body(body).unwrap();
        assert!(usage.spend.is_none());
    }

    #[test]
    fn uncapped_spend_keeps_the_used_amount() {
        let body = br#"{"spend":{"used":{"amount_minor":500,"currency":"USD","exponent":2},
                                  "limit":null,"percent":null,"enabled":true}}"#;
        let spend = parse_body(body).unwrap().spend.unwrap();
        assert_eq!(spend.used_minor, 500);
        assert!(spend.limit_minor.is_none());
    }

    #[test]
    fn a_cap_in_another_currency_is_dropped_rather_than_mixed() {
        let body = br#"{"spend":{"used":{"amount_minor":7788,"currency":"GBP","exponent":2},
                                  "limit":{"amount_minor":30000,"currency":"USD","exponent":2},
                                  "percent":26,"enabled":true}}"#;
        let spend = parse_body(body).unwrap().spend.unwrap();
        assert_eq!(spend.currency, "GBP");
        assert!(spend.limit_minor.is_none(), "must not show £78 of $300");
    }

    #[test]
    fn an_out_of_range_exponent_falls_back_to_two_places() {
        let body = br#"{"spend":{"used":{"amount_minor":7788,"currency":"GBP","exponent":-3},
                                  "limit":null,"percent":null,"enabled":true}}"#;
        assert_eq!(parse_body(body).unwrap().spend.unwrap().exponent, 2);
    }

    #[test]
    fn a_zero_decimal_currency_keeps_its_exponent() {
        let body = br#"{"spend":{"used":{"amount_minor":900,"currency":"JPY","exponent":0},
                                  "limit":null,"percent":null,"enabled":true}}"#;
        assert_eq!(parse_body(body).unwrap().spend.unwrap().exponent, 0);
    }

    #[test]
    fn a_response_carrying_only_spend_is_not_an_error() {
        // No windows at all but real credit data is still worth a card.
        let body = br#"{"spend":{"used":{"amount_minor":100,"currency":"USD","exponent":2},
                                  "limit":null,"percent":null,"enabled":true}}"#;
        assert!(parse_body(body).unwrap().spend.is_some());
    }

    #[tokio::test]
    async fn no_credentials_returns_no_credentials() {
        let dir = TempDir::new().unwrap();
        let client = StubClient {
            status: 200,
            body: b"{}".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::NoCredentials,
        ));
    }

    #[tokio::test]
    async fn unauthorized_status_maps_to_unauthorized() {
        let dir = dir_with_token();
        let client = StubClient {
            status: 401,
            body: b"{}".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Unauthorized,
        ));
    }

    #[tokio::test]
    async fn forbidden_status_maps_to_forbidden() {
        // 403 here is typically an edge/WAF policy block, not bad credentials.
        // It must NOT map to Unauthorized — that would trigger a refresh spawn
        // that churns the single-use refresh token for nothing.
        let dir = dir_with_token();
        let client = StubClient {
            status: 403,
            body: b"{}".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Forbidden,
        ));
    }

    #[tokio::test]
    async fn rate_limit_maps_to_rate_limited() {
        let dir = dir_with_token();
        let client = StubClient {
            status: 429,
            body: b"".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::RateLimited,
        ));
    }

    #[tokio::test]
    async fn bad_request_maps_to_unknown_not_network() {
        // 400 from a deprecated beta header or schema drift should
        // surface as Unknown so the UI doesn't show "check your
        // connection" for what is actually a permanent contract break.
        let dir = dir_with_token();
        let client = StubClient {
            status: 400,
            body: b"".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Unknown,
        ));
    }

    #[tokio::test]
    async fn not_found_maps_to_unknown_not_network() {
        let dir = dir_with_token();
        let client = StubClient {
            status: 404,
            body: b"".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Unknown,
        ));
    }

    #[tokio::test]
    async fn server_error_maps_to_network() {
        let dir = dir_with_token();
        let client = StubClient {
            status: 503,
            body: b"".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Network,
        ));
    }

    #[tokio::test]
    async fn transport_failure_maps_to_network() {
        let dir = dir_with_token();
        let client = ErroringClient {
            error: QuotaError::Network,
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Network,
        ));
    }

    #[tokio::test]
    async fn garbage_body_maps_to_unknown() {
        let dir = dir_with_token();
        let client = StubClient {
            status: 200,
            body: b"not json".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Unknown,
        ));
    }

    #[tokio::test]
    async fn empty_object_with_no_windows_maps_to_unknown() {
        let dir = dir_with_token();
        let client = StubClient {
            status: 200,
            body: b"{}".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Unknown,
        ));
    }

    #[tokio::test]
    async fn partial_response_keeps_present_windows() {
        let dir = dir_with_token();
        let body = br#"{"five_hour":{"utilization":0.5,"resets_at":"x"}}"#;
        let client = StubClient {
            status: 200,
            body: body.to_vec(),
        };
        let usage = fetch_quota(dir.path(), &client).await.unwrap();
        assert!(usage.primary.is_some());
        assert!(usage.secondary.is_none());
        assert!(usage.scoped_weekly.is_empty());
    }

    #[tokio::test]
    async fn negative_or_null_utilization_is_dropped() {
        let dir = dir_with_token();
        // Only negative and explicit null values are dropped now —
        // values above 100 are allowed since Anthropic can return
        // over-limit percentages (e.g. 105% when overused).
        let body = br#"{
            "five_hour": {"utilization": -0.5},
            "seven_day": {"utilization": null},
            "seven_day_sonnet": {"utilization": -10.0}
        }"#;
        let client = StubClient {
            status: 200,
            body: body.to_vec(),
        };
        let usage = fetch_quota(dir.path(), &client).await.unwrap();
        assert!(usage.primary.unwrap().utilization.is_none());
        assert!(usage.secondary.unwrap().utilization.is_none());
        assert!(usage.scoped_weekly[0].utilization.is_none());
    }

    #[tokio::test]
    async fn over_one_hundred_utilization_is_preserved() {
        let dir = dir_with_token();
        let body = br#"{"five_hour":{"utilization":105.0,"resets_at":null}}"#;
        let client = StubClient {
            status: 200,
            body: body.to_vec(),
        };
        let usage = fetch_quota(dir.path(), &client).await.unwrap();
        assert_eq!(usage.primary.unwrap().utilization, Some(105.0));
    }

    #[tokio::test]
    async fn zero_utilization_is_preserved() {
        let dir = dir_with_token();
        let body = br#"{"five_hour":{"utilization":0.0,"resets_at":null}}"#;
        let client = StubClient {
            status: 200,
            body: body.to_vec(),
        };
        let usage = fetch_quota(dir.path(), &client).await.unwrap();
        assert_eq!(usage.primary.unwrap().utilization, Some(0.0));
    }

    #[tokio::test]
    async fn unknown_top_level_fields_are_ignored() {
        let dir = dir_with_token();
        let body = br#"{
            "five_hour": {"utilization": 0.1},
            "future_window": {"utilization": 0.9},
            "extra": "hi"
        }"#;
        let client = StubClient {
            status: 200,
            body: body.to_vec(),
        };
        let usage = fetch_quota(dir.path(), &client).await.unwrap();
        assert!(usage.primary.is_some());
    }

    // -- caching + back-off --

    fn dir_with_named_token(token: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(".credentials.json"),
            format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}"}}}}"#),
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn cached_fetch_reuses_success_within_ttl() {
        // Two profiles signed into the same account share a token, so the
        // second request is served from cache — one upstream call, not two.
        let first = dir_with_named_token("sk-shared");
        let second = dir_with_named_token("sk-shared");
        let cache = ClaudeQuotaCache::new();
        let registry = DeadCredentialRegistry::new();
        let client = CountingClient {
            calls: std::sync::Mutex::new(0),
            status: 200,
            body: br#"{"five_hour":{"utilization":42.0,"resets_at":null}}"#.to_vec(),
            retry_after: None,
        };

        let a = fetch_quota_cached(first.path(), &client, &cache, &registry)
            .await
            .unwrap();
        let b = fetch_quota_cached(second.path(), &client, &cache, &registry)
            .await
            .unwrap();

        assert_eq!(a.primary.unwrap().utilization, Some(42.0));
        assert_eq!(b.primary.unwrap().utilization, Some(42.0));
        assert_eq!(*client.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn cached_fetch_backs_off_during_rate_limit_cooldown() {
        // The crux of the fix: a 429 is negatively cached for its
        // Retry-After window, so a second request inside the cooldown is
        // served from cache and does NOT poke the endpoint again.
        let dir = dir_with_token();
        let cache = ClaudeQuotaCache::new();
        let registry = DeadCredentialRegistry::new();
        let client = CountingClient {
            calls: std::sync::Mutex::new(0),
            status: 429,
            body: b"".to_vec(),
            retry_after: Some(Duration::from_secs(1800)),
        };

        for _ in 0..3 {
            assert!(matches!(
                fetch_quota_cached(dir.path(), &client, &cache, &registry)
                    .await
                    .unwrap_err(),
                QuotaError::RateLimited,
            ));
        }
        assert_eq!(*client.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn cached_fetch_reprobes_after_cooldown_expires() {
        // Once the Retry-After window elapses the negative entry is evicted
        // and the next request is allowed to hit the endpoint again.
        let dir = dir_with_token();
        let cache = ClaudeQuotaCache::new();
        let registry = DeadCredentialRegistry::new();
        let client = CountingClient {
            calls: std::sync::Mutex::new(0),
            status: 429,
            body: b"".to_vec(),
            retry_after: Some(Duration::from_millis(40)),
        };

        let _ = fetch_quota_cached(dir.path(), &client, &cache, &registry).await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let _ = fetch_quota_cached(dir.path(), &client, &cache, &registry).await;

        assert_eq!(*client.calls.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn cached_fetch_does_not_pin_network_errors() {
        // Network failures are transient — they must NOT be cached, so a
        // later call retries rather than being stuck.
        let dir = dir_with_token();
        let cache = ClaudeQuotaCache::new();
        let registry = DeadCredentialRegistry::new();
        let client = CountingClient {
            calls: std::sync::Mutex::new(0),
            status: 503,
            body: b"".to_vec(),
            retry_after: None,
        };

        let _ = fetch_quota_cached(dir.path(), &client, &cache, &registry).await;
        let _ = fetch_quota_cached(dir.path(), &client, &cache, &registry).await;
        assert_eq!(*client.calls.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn dead_token_short_circuits_without_calling_upstream() {
        // A token marked dead must NOT hit the network — that's the whole point:
        // stop feeding Anthropic's abuse limiter with invalid-auth requests.
        let dir = dir_with_token(); // writes accessToken "sk-test"
        let cache = ClaudeQuotaCache::new();
        let registry = DeadCredentialRegistry::new();
        registry.mark_dead("sk-test");
        let client = CountingClient {
            calls: std::sync::Mutex::new(0),
            status: 200,
            body: br#"{"five_hour":{"utilization":1.0}}"#.to_vec(),
            retry_after: None,
        };

        let result = fetch_quota_cached(dir.path(), &client, &cache, &registry).await;

        assert!(matches!(result.unwrap_err(), QuotaError::NeedsLogin));
        assert_eq!(*client.calls.lock().unwrap(), 0, "must not call upstream");
    }

    #[test]
    fn parse_retry_after_reads_delta_seconds() {
        assert_eq!(
            parse_retry_after(Some("1800")),
            Some(Duration::from_secs(1800))
        );
        assert_eq!(
            parse_retry_after(Some("  60 ")),
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn parse_retry_after_none_for_missing_or_http_date() {
        assert_eq!(parse_retry_after(None), None);
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2099 07:28:00 GMT")),
            None
        );
    }
}
