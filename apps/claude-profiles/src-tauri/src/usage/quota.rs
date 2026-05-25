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
        let response = self
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
        let bytes = response.bytes().await.map_err(|_| QuotaError::Network)?;
        if bytes.len() > MAX_BODY_BYTES {
            return Err(QuotaError::Unknown);
        }
        Ok(HttpResponse {
            status,
            body: bytes.to_vec(),
        })
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
    // 429 and 5xx are explicitly split out from the catch-all even
    // though both currently map to Network — they represent a
    // distinct "transient, retry will probably help" category that
    // may grow its own variant later (e.g. RateLimited for backoff).
    match response.status {
        200 => parse_body(&response.body),
        401 | 403 => Err(QuotaError::Unauthorized),
        429 | 500..=599 => Err(QuotaError::Network),
        _ => Err(QuotaError::Network),
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

fn into_window(raw: ApiWindow) -> Window {
    let utilization = match raw.utilization {
        Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => Some(value),
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
        let body = br#"{
            "five_hour": {"utilization": 0.63, "resets_at": "2099-01-01T00:00:00Z"},
            "seven_day": {"utilization": 0.21, "resets_at": null},
            "seven_day_sonnet": {"utilization": 0.08, "resets_at": null}
        }"#;
        let client = StubClient {
            status: 200,
            body: body.to_vec(),
        };
        let usage = fetch_quota(dir.path(), &client).await.unwrap();
        assert!((usage.five_hour.unwrap().utilization.unwrap() - 0.63).abs() < 1e-4);
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
    async fn rate_limit_maps_to_network() {
        let dir = dir_with_token();
        let client = StubClient {
            status: 429,
            body: b"".to_vec(),
        };
        assert!(matches!(
            fetch_quota(dir.path(), &client).await.unwrap_err(),
            QuotaError::Network,
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
    async fn out_of_range_or_nan_utilization_is_dropped() {
        let dir = dir_with_token();
        let body = br#"{
            "five_hour": {"utilization": 1.7},
            "seven_day": {"utilization": -0.2},
            "seven_day_sonnet": {"utilization": null}
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
