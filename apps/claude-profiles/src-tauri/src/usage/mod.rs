pub(crate) mod credentials;
pub(crate) mod quota;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUsage {
    pub quota: Option<QuotaUsage>,
    pub quota_error: Option<QuotaError>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsage {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub seven_day_sonnet: Option<Window>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub utilization: Option<f32>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaError {
    NoCredentials,
    Unauthorized,
    Network,
    Unknown,
}

use std::path::Path;

use chrono::Utc;

/// Fetches the profile's quota and wraps it in a `ProfileUsage`.
pub async fn build(cli_config_dir: &Path, client: &dyn quota::UsageClient) -> ProfileUsage {
    let (quota, quota_error) = match quota::fetch_quota(cli_config_dir, client).await {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };
    ProfileUsage {
        quota,
        quota_error,
        fetched_at: Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use async_trait::async_trait;
    use tempfile::TempDir;

    use super::quota::{HttpResponse, UsageClient};
    use super::*;

    struct AlwaysFailsClient;

    #[async_trait]
    impl UsageClient for AlwaysFailsClient {
        async fn fetch(&self, _: &str) -> Result<HttpResponse, QuotaError> {
            Err(QuotaError::Network)
        }
    }

    #[tokio::test]
    async fn quota_network_failure_surfaces_network_error() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"sk"}}"#,
        )
        .unwrap();
        let result = build(dir.path(), &AlwaysFailsClient).await;
        assert!(result.quota.is_none());
        assert!(matches!(result.quota_error, Some(QuotaError::Network)));
    }

    #[tokio::test]
    async fn missing_credentials_returns_nocredentials() {
        let dir = TempDir::new().unwrap();
        let client = AlwaysFailsClient;
        let result = build(dir.path(), &client).await;
        assert!(matches!(
            result.quota_error,
            Some(QuotaError::NoCredentials)
        ));
    }
}
