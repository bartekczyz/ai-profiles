//! Codex usage provider. Drives `codex app-server` over newline-delimited
//! JSON-RPC to read account rate limits, reusing Codex's own auth + token
//! refresh (per-CODEX_HOME).

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use chrono::TimeZone;
use serde::Deserialize;
use serde_json::Value;

use crate::codex_rpc::{CodexRpc, CodexRpcError, CodexTransport};
use crate::usage::{
    credentials, QuotaError, QuotaProvider, QuotaUsage, RateLimitResetCredits, Window,
};

const APP_SERVER_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Deserialize)]
struct RateLimitsResult {
    #[serde(default, rename = "rateLimitResetCredits")]
    rate_limit_reset_credits: Option<RateLimitResetCredits>,
    #[serde(rename = "rateLimits")]
    rate_limits: RateLimitSnapshot,
    #[serde(default, rename = "rateLimitsByLimitId")]
    rate_limits_by_limit_id: Option<HashMap<String, RateLimitSnapshot>>,
}

#[derive(Deserialize)]
struct RateLimitSnapshot {
    #[serde(default)]
    primary: Option<RateLimitWindow>,
    #[serde(default)]
    secondary: Option<RateLimitWindow>,
    #[serde(default, rename = "rateLimitReachedType")]
    rate_limit_reached_type: Option<String>,
}

#[derive(Deserialize)]
struct RateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: f32,
    #[serde(default, rename = "windowDurationMins")]
    window_duration_mins: Option<i64>,
    #[serde(default, rename = "resetsAt")]
    resets_at: Option<i64>,
}

/// Pure: parse a `GetAccountRateLimitsResponse` result body into QuotaUsage.
///
/// A reached rate limit (`rateLimitReachedType` set) is NOT an error: the
/// snapshot still carries the real windows (`usedPercent`, `resetsAt`), so we
/// render those — a 100% bar with its reset time is exactly the information
/// the user wants, far better than collapsing to a bare "rate limited" card.
/// We only fall back to `RateLimited` when the limit is reached AND no window
/// data is present to show.
fn parse_rate_limits(body: &[u8]) -> Result<QuotaUsage, QuotaError> {
    let parsed: RateLimitsResult = serde_json::from_slice(body).map_err(|_| QuotaError::Unknown)?;
    let snapshot = parsed
        .rate_limits_by_limit_id
        .and_then(|mut buckets| buckets.remove("codex"))
        .unwrap_or(parsed.rate_limits);
    let usage = QuotaUsage {
        primary: snapshot.primary.map(into_window),
        secondary: snapshot.secondary.map(into_window),
        scoped_weekly: Vec::new(),
        spend: None,
        rate_limit_reset_credits: parsed.rate_limit_reset_credits,
    };
    if usage.primary.is_none()
        && usage.secondary.is_none()
        && usage.rate_limit_reset_credits.is_none()
    {
        if snapshot.rate_limit_reached_type.is_some() {
            return Err(QuotaError::RateLimited);
        }
        return Err(QuotaError::Unknown);
    }
    Ok(usage)
}

fn into_window(raw: RateLimitWindow) -> Window {
    let utilization = if raw.used_percent.is_finite() && raw.used_percent >= 0.0 {
        Some(raw.used_percent)
    } else {
        None
    };
    let resets_at = raw.resets_at.and_then(|secs| {
        chrono::Utc
            .timestamp_opt(secs, 0)
            .single()
            .map(|dt| dt.to_rfc3339())
    });
    Window {
        window_duration_mins: raw.window_duration_mins.filter(|minutes| *minutes > 0),
        label: None,
        utilization,
        resets_at,
    }
}

/// Reads a Codex home's rate limits through its `codex app-server`.
pub struct CodexQuotaProvider;

#[async_trait]
impl QuotaProvider for CodexQuotaProvider {
    async fn fetch(&self, config_dir: &Path) -> Result<QuotaUsage, QuotaError> {
        // Short-circuit before spawning if the profile has never signed in.
        if !credentials::codex_is_signed_in(config_dir) {
            return Err(QuotaError::NoCredentials);
        }
        let body = read_rate_limits(config_dir).await?;
        parse_rate_limits(body.as_bytes())
    }
}

/// The `account/rateLimits/read` result for `codex_home`, as raw JSON text.
///
/// App-server is launched without waiting for its answer to `initialize`, so
/// only the rate limits' answer decides the outcome.
async fn read_rate_limits(codex_home: &Path) -> Result<String, QuotaError> {
    read_rate_limits_from(CodexRpc::launch(codex_home), APP_SERVER_TIMEOUT).await
}

/// The `account/rateLimits/read` result, as raw JSON text, read from the
/// app-server `start` gives, all within `budget`.
async fn read_rate_limits_from(
    start: impl Future<Output = Result<CodexRpc, CodexRpcError>>,
    budget: Duration,
) -> Result<String, QuotaError> {
    let read = async {
        let mut rpc = start.await?;
        rpc.request("account/rateLimits/read", Value::Null).await
    };
    match tokio::time::timeout(budget, read).await {
        Ok(Ok(result)) => Ok(result.to_string()),
        Ok(Err(error)) => Err(quota_error(&error)),
        Err(_) => Err(QuotaError::Network),
    }
}

/// How a failed app-server call shows on the usage card. A missing binary
/// isn't a missing login, so it is `Unknown` rather than `NoCredentials`; a
/// JSON-RPC error answer means the home can't read its limits (signed out).
fn quota_error(error: &CodexRpcError) -> QuotaError {
    match error {
        CodexRpcError::NotInstalled => QuotaError::Unknown,
        CodexRpcError::Rpc(_) => QuotaError::Unauthorized,
        CodexRpcError::Io(_)
        | CodexRpcError::Closed
        | CodexRpcError::Timeout
        | CodexRpcError::Unexpected(_) => QuotaError::Network,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use tokio::process::Command;

    use super::*;

    // Captured live from codex app-server v0.135.0.
    const RATE_LIMITS_RESULT: &str = r#"{"rateLimits":{"limitId":"codex","limitName":null,"primary":{"usedPercent":1,"windowDurationMins":300,"resetsAt":1780231295},"secondary":{"usedPercent":10,"windowDurationMins":10080,"resetsAt":1780581224},"credits":{"hasCredits":false,"unlimited":false,"balance":null},"planType":"team","rateLimitReachedType":null}}"#;

    #[test]
    fn prefers_codex_bucket_and_preserves_weekly_primary_duration() {
        let body = br#"{"rateLimits":{"primary":{"usedPercent":99}},"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":14,"windowDurationMins":10080},"secondary":null}}}"#;
        let usage = parse_rate_limits(body).unwrap();
        let json = serde_json::to_value(usage).unwrap();
        assert_eq!(json["primary"]["utilization"].as_f64(), Some(14.0));
        assert_eq!(json["primary"]["windowDurationMins"], 10080);
        assert!(json["secondary"].is_null());
    }

    #[test]
    fn parses_primary_and_secondary_windows() {
        let usage = parse_rate_limits(RATE_LIMITS_RESULT.as_bytes()).unwrap();
        let primary = usage.primary.unwrap();
        assert_eq!(primary.utilization, Some(1.0));
        // resetsAt 1780231295 epoch seconds → RFC3339 (UTC).
        assert_eq!(
            primary.resets_at.as_deref(),
            Some("2026-05-31T12:41:35+00:00")
        );
        assert_eq!(usage.secondary.unwrap().utilization, Some(10.0));
        // Codex has no per-model weekly sub-quota and no credit spend.
        assert!(usage.scoped_weekly.is_empty());
        assert!(usage.spend.is_none());
    }

    #[test]
    fn rate_limit_reached_still_shows_usage_windows() {
        // A reached limit that still carries window data must render the
        // bars (100% + reset time), not collapse to a bare error.
        let body = r#"{"rateLimits":{"primary":{"usedPercent":100,"windowDurationMins":300,"resetsAt":1780499820},"secondary":{"usedPercent":16,"windowDurationMins":10080,"resetsAt":1781086620},"rateLimitReachedType":"workspace_member_credits_depleted"}}"#;
        let usage = parse_rate_limits(body.as_bytes()).unwrap();
        assert_eq!(usage.primary.unwrap().utilization, Some(100.0));
        assert_eq!(usage.secondary.unwrap().utilization, Some(16.0));
    }

    #[test]
    fn rate_limit_reached_without_windows_maps_to_rate_limited() {
        // Only when there's no window data to show do we fall back to the
        // rate-limited error state.
        let body = r#"{"rateLimits":{"rateLimitReachedType":"workspace_member_credits_depleted"}}"#;
        assert!(matches!(
            parse_rate_limits(body.as_bytes()),
            Err(QuotaError::RateLimited)
        ));
    }

    #[test]
    fn preserves_reset_count_and_nullable_expiry_without_windows() {
        let body = br#"{"rateLimits":{},"rateLimitResetCredits":{"availableCount":2,"credits":[{"title":"Full reset","status":"available","expiresAt":1791079750},{"title":null,"status":"available","expiresAt":null}]}}"#;
        let usage = parse_rate_limits(body).unwrap();
        let resets = usage.rate_limit_reset_credits.unwrap();
        assert_eq!(resets.available_count, 2);
        let credits = resets.credits.unwrap();
        assert_eq!(credits[0].expires_at, Some(1791079750));
        assert_eq!(credits[1].expires_at, None);
    }

    #[test]
    fn garbage_maps_to_unknown() {
        assert!(matches!(
            parse_rate_limits(b"not json"),
            Err(QuotaError::Unknown)
        ));
    }

    /// A stand-in app-server that answers `initialize` with an error, right
    /// away or, if `deferred`, only once the rate limits are asked for, and
    /// answers those.
    fn refusing_initialize(dir: &Path, deferred: bool) -> Command {
        let (on_initialize, on_read) = if deferred {
            ("", REFUSED)
        } else {
            (REFUSED, "")
        };
        let script = format!(
            "#!/bin/sh
while IFS= read -r line; do
  case \"$line\" in
    *'\"method\":\"initialize\"'*) {on_initialize} ;;
    *'\"method\":\"account/rateLimits/read\"'*)
      {on_read}
      echo '{{\"id\":2,\"result\":{{\"rateLimits\":{{\"primary\":{{\"usedPercent\":5}}}}}}}}' ;;
  esac
done
"
        );
        let path = dir.join("app-server");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Command::new(path)
    }

    /// Answers `initialize` with a JSON-RPC error.
    const REFUSED: &str =
        "echo '{\"id\":1,\"error\":{\"code\":-32600,\"message\":\"Not initialized\"}}'";

    #[tokio::test]
    async fn the_rate_limits_decide_whatever_initialize_said() {
        let dir = tempfile::tempdir().unwrap();

        let body = read_rate_limits_from(
            CodexRpc::introduce(refusing_initialize(dir.path(), false), APP_SERVER_TIMEOUT),
            APP_SERVER_TIMEOUT,
        )
        .await
        .unwrap();

        assert_eq!(
            parse_rate_limits(body.as_bytes())
                .unwrap()
                .primary
                .unwrap()
                .utilization,
            Some(5.0)
        );
    }

    #[tokio::test]
    async fn the_rate_limits_are_asked_for_without_waiting_for_initialize() {
        let dir = tempfile::tempdir().unwrap();

        let body = read_rate_limits_from(
            CodexRpc::introduce(refusing_initialize(dir.path(), true), APP_SERVER_TIMEOUT),
            Duration::from_secs(2),
        )
        .await;

        assert!(body.is_ok(), "{:?}", body.err());
    }

    #[test]
    fn a_jsonrpc_error_maps_to_unauthorized() {
        assert!(matches!(
            quota_error(&CodexRpcError::Rpc("not signed in".to_string())),
            QuotaError::Unauthorized
        ));
    }

    #[test]
    fn a_missing_binary_maps_to_unknown_and_a_dead_server_to_network() {
        assert!(matches!(
            quota_error(&CodexRpcError::NotInstalled),
            QuotaError::Unknown
        ));
        assert!(matches!(
            quota_error(&CodexRpcError::Closed),
            QuotaError::Network
        ));
        assert!(matches!(
            quota_error(&CodexRpcError::Timeout),
            QuotaError::Network
        ));
    }
}
