use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::usage::credentials::read_access_token;
use crate::usage::{QuotaError, QuotaUsage, Window};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA_HEADER: &str = "oauth-2025-04-20";
const REQUEST_TIMEOUT_SECS: u64 = 8;
/// Hard cap on response body size. The real response is well under 1 KiB.
const MAX_BODY_BYTES: usize = 1024 * 1024;

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
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
        Ok(HttpResponse { status, body })
    }
}

/// Fetches and parses the quota for the profile at `cli_config_dir`,
/// using the provided HTTP client. Returns `Ok(QuotaUsage)` only on
/// 200 with at least one parseable window.
pub async fn fetch_quota(
    cli_config_dir: &Path,
    client: &dyn UsageClient,
) -> Result<QuotaUsage, QuotaError> {
    let token = read_access_token(cli_config_dir)?;
    let response = client.fetch(&token).await?;
    match response.status {
        200 => parse_body(&response.body),
        401 | 403 => Err(QuotaError::Unauthorized),
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

fn parse_body(body: &[u8]) -> Result<QuotaUsage, QuotaError> {
    let parsed: ApiResponse = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return Err(QuotaError::Unknown),
    };
    let usage = QuotaUsage {
        five_hour: parsed.five_hour.map(into_window),
        seven_day: parsed.seven_day.map(into_window),
        seven_day_sonnet: parsed.seven_day_sonnet.map(into_window),
    };
    if usage.five_hour.is_none() && usage.seven_day.is_none() && usage.seven_day_sonnet.is_none() {
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
        assert!((usage.five_hour.unwrap().utilization.unwrap() - 63.0).abs() < 1e-4);
        assert_eq!(usage.seven_day.unwrap().resets_at, None);
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
    async fn forbidden_status_maps_to_unauthorized() {
        let dir = dir_with_token();
        let client = StubClient {
            status: 403,
            body: b"{}".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Unauthorized,
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
        assert!(usage.five_hour.is_some());
        assert!(usage.seven_day.is_none());
        assert!(usage.seven_day_sonnet.is_none());
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
        assert!(usage.five_hour.unwrap().utilization.is_none());
        assert!(usage.seven_day.unwrap().utilization.is_none());
        assert!(usage.seven_day_sonnet.unwrap().utilization.is_none());
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
        assert_eq!(usage.five_hour.unwrap().utilization, Some(105.0));
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
        assert_eq!(usage.five_hour.unwrap().utilization, Some(0.0));
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
        assert!(usage.five_hour.is_some());
    }
}
