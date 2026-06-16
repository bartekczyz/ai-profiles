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
use crate::usage::{QuotaError, QuotaUsage, Window};

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
#[derive(Clone)]
enum CachedOutcome {
    Success(QuotaUsage),
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
                CachedOutcome::Success(usage) => Ok(usage.clone()),
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
        self.insert(key, CachedOutcome::Success(usage), QUOTA_CACHE_TTL);
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
    // Anthropic's wire fields map onto the generic windows: the 5-hour window
    // is `primary`, the weekly window is `secondary`, and the weekly-Sonnet
    // sub-quota is `secondary_extra` (Claude-only; Codex leaves it None).
    let usage = QuotaUsage {
        primary: parsed.five_hour.map(into_window),
        secondary: parsed.seven_day.map(into_window),
        secondary_extra: parsed.seven_day_sonnet.map(into_window),
    };
    if usage.primary.is_none() && usage.secondary.is_none() && usage.secondary_extra.is_none() {
        return Err(QuotaError::Unknown);
    }
    Ok(usage)
}

/// Anthropic returns `utilization` as a percentage on a 0..=100 scale
/// (e.g. `42.0` means 42%). We accept any finite non-negative value
/// without an upper clamp — values above 100 are unusual but legitimate
/// (over-limit) and we'd rather show "105%" than drop the data. The
/// UI is responsible for capping the visual bar fill at 100%.
fn into_window(raw: ApiWindow) -> Window {
    let utilization = match raw.utilization {
        Some(value) if value.is_finite() && value >= 0.0 => Some(value),
        _ => None,
    };
    Window {
        utilization,
        resets_at: raw.resets_at,
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
    five_hour: Option<ApiWindow>,
    #[serde(default)]
    seven_day: Option<ApiWindow>,
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
        assert!(usage.secondary_extra.is_none());
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
        assert!(usage.secondary_extra.unwrap().utilization.is_none());
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
