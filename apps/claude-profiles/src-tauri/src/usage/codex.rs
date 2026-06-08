//! Codex usage provider. Drives `codex app-server` over newline-delimited
//! JSON-RPC to read account rate limits, reusing Codex's own auth + token
//! refresh (per-CODEX_HOME). See plan §"Codex usage — verified live".

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use chrono::TimeZone;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::usage::{credentials, QuotaError, QuotaProvider, QuotaUsage, Window};

const APP_SERVER_TIMEOUT: Duration = Duration::from_secs(12);
const RATE_LIMITS_ID: i64 = 2;

#[derive(Deserialize)]
struct RateLimitsResult {
    #[serde(rename = "rateLimits")]
    rate_limits: RateLimitSnapshot,
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
    let usage = QuotaUsage {
        primary: parsed.rate_limits.primary.map(into_window),
        secondary: parsed.rate_limits.secondary.map(into_window),
        secondary_extra: None,
    };
    if usage.primary.is_none() && usage.secondary.is_none() {
        if parsed.rate_limits.rate_limit_reached_type.is_some() {
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
        utilization,
        resets_at,
    }
}

/// Pure: scan newline-delimited JSON-RPC messages for the response whose `id`
/// matches, returning its `result` body as raw JSON text (or a categorised
/// error for a JSON-RPC error object).
fn extract_result_for_id(lines: &[String], id: i64) -> Result<String, QuotaError> {
    for line in lines {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.get("id").and_then(|candidate| candidate.as_i64()) != Some(id) {
            continue;
        }
        if value.get("error").is_some() {
            return Err(QuotaError::Unauthorized);
        }
        if let Some(result) = value.get("result") {
            return Ok(result.to_string());
        }
    }
    Err(QuotaError::Network)
}

pub struct CodexQuotaProvider {
    user_agent_name: String,
    version: String,
}

impl CodexQuotaProvider {
    pub fn new(user_agent_name: String, version: String) -> Self {
        Self {
            user_agent_name,
            version,
        }
    }
}

#[async_trait]
impl QuotaProvider for CodexQuotaProvider {
    async fn fetch(&self, config_dir: &Path) -> Result<QuotaUsage, QuotaError> {
        // Short-circuit before spawning if the profile has never signed in.
        if !credentials::codex_is_signed_in(config_dir) {
            return Err(QuotaError::NoCredentials);
        }
        let body = self.read_rate_limits(config_dir).await?;
        parse_rate_limits(body.as_bytes())
    }
}

impl CodexQuotaProvider {
    async fn read_rate_limits(&self, codex_home: &Path) -> Result<String, QuotaError> {
        // A Tauri app launched from Finder doesn't inherit the shell PATH, so a
        // bare `codex` spawn would fail with ENOENT and surface as a misleading
        // "couldn't reach" error. Resolve the absolute binary path (and pass the
        // resolved PATH through) so usage works regardless of how we were
        // launched. NoCredentials would be wrong here, so use Unknown.
        let codex_binary =
            crate::deps::resolve_cli_binary_path("codex").ok_or(QuotaError::Unknown)?;
        let mut child = tokio::process::Command::new(&codex_binary)
            .arg("app-server")
            .env("CODEX_HOME", codex_home)
            .env("PATH", crate::deps::shell_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| QuotaError::Network)?;

        let mut stdin = child.stdin.take().ok_or(QuotaError::Network)?;
        let stdout = child.stdout.take().ok_or(QuotaError::Network)?;

        let init = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"clientInfo\":{{\"name\":\"{}\",\"version\":\"{}\"}}}}}}\n",
            self.user_agent_name, self.version
        );
        stdin
            .write_all(init.as_bytes())
            .await
            .map_err(|_| QuotaError::Network)?;
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"initialized\"}\n")
            .await
            .map_err(|_| QuotaError::Network)?;
        stdin
            .write_all(
                format!("{{\"jsonrpc\":\"2.0\",\"id\":{RATE_LIMITS_ID},\"method\":\"account/rateLimits/read\"}}\n")
                    .as_bytes(),
            )
            .await
            .map_err(|_| QuotaError::Network)?;
        stdin.flush().await.map_err(|_| QuotaError::Network)?;
        // Keep stdin OPEN across the read. codex app-server (v0.135.0) stops
        // emitting output the instant stdin hits EOF — closing it here, before
        // the `account/rateLimits/read` reply arrives, yields an empty stream
        // and a spurious Network error. Verified live: immediate EOF → no
        // output at all; stdin held open → the full id:2 result. We therefore
        // hold the handle until after the read completes, then drop it.

        // Read lines until the matching id arrives or the deadline passes.
        let read = async {
            let mut lines: Vec<String> = Vec::new();
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                lines.push(line);
                if let Ok(found) = extract_result_for_id(&lines, RATE_LIMITS_ID) {
                    return Ok(found);
                }
            }
            extract_result_for_id(&lines, RATE_LIMITS_ID)
        };
        let outcome = match tokio::time::timeout(APP_SERVER_TIMEOUT, read).await {
            Ok(result) => result,
            Err(_) => Err(QuotaError::Network),
        };
        drop(stdin);
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured live from codex app-server v0.135.0.
    const RATE_LIMITS_RESULT: &str = r#"{"rateLimits":{"limitId":"codex","limitName":null,"primary":{"usedPercent":1,"windowDurationMins":300,"resetsAt":1780231295},"secondary":{"usedPercent":10,"windowDurationMins":10080,"resetsAt":1780581224},"credits":{"hasCredits":false,"unlimited":false,"balance":null},"planType":"team","rateLimitReachedType":null}}"#;

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
        // Codex has no third meter.
        assert!(usage.secondary_extra.is_none());
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
    fn garbage_maps_to_unknown() {
        assert!(matches!(
            parse_rate_limits(b"not json"),
            Err(QuotaError::Unknown)
        ));
    }

    #[test]
    fn extract_matching_response_skips_interleaved_notifications() {
        let lines = [
            r#"{"id":1,"result":{"codexHome":"/x"}}"#,
            r#"{"method":"remoteControl/status/changed","params":{}}"#,
            r#"{"id":2,"result":{"rateLimits":{"primary":{"usedPercent":5,"windowDurationMins":300,"resetsAt":1}}}}"#,
        ];
        let result = extract_result_for_id(&lines.map(String::from), 2).unwrap();
        assert!(result.contains("\"rateLimits\""));
    }

    #[test]
    fn extract_returns_jsonrpc_error_as_unauthorized() {
        let lines = [r#"{"id":2,"error":{"code":-32000,"message":"not signed in"}}"#.to_string()];
        assert!(matches!(
            extract_result_for_id(&lines, 2),
            Err(QuotaError::Unauthorized)
        ));
    }
}
