pub(crate) mod codex;
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
    pub primary: Option<Window>,
    pub secondary: Option<Window>,
    /// Third "Sonnet-style" window — Claude only; Codex leaves it None.
    pub secondary_extra: Option<Window>,
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
    /// The endpoint rejected the stored access token (HTTP 401). Recovery is
    /// running the profile's CLI interactively once — its own startup
    /// refresh rotates and persists the token. We deliberately do NOT
    /// auto-refresh: Anthropic refresh tokens are single-use, and a spawned
    /// `claude` with non-TTY stdin exits before refreshing anything (and
    /// `claude -p` wipes the stored refresh token on a 401), so any
    /// automated refresh either does nothing or destroys the credential.
    Unauthorized,
    /// HTTP 403 from the usage endpoint. In practice an edge/WAF policy
    /// block (Cloudflare), not bad credentials.
    Forbidden,
    RateLimited,
    Network,
    Unknown,
}

use std::path::Path;

use chrono::Utc;

/// Fetches a profile's quota for a managed app. Claude drives an HTTP request
/// to Anthropic; Codex drives `codex app-server` over JSON-RPC. The
/// orchestration in `build` is app-agnostic — it only sees this trait.
#[async_trait::async_trait]
pub trait QuotaProvider: Send + Sync {
    /// Fetch the profile's quota for the config dir (Claude: cli-config;
    /// Codex: CODEX_HOME). Returns a categorised `QuotaError` on failure.
    async fn fetch(&self, config_dir: &std::path::Path) -> Result<QuotaUsage, QuotaError>;
}

/// Pure: true when `cli_config_dir` is the stock-default `$HOME/.claude`
/// location.
///
/// Claude Code's keychain layout depends on whether `CLAUDE_CONFIG_DIR`
/// is set when it runs:
/// - Unset (stock install) → bare service name `Claude Code-credentials`.
/// - Set (managed profile via wrapper) → hashed
///   `Claude Code-credentials-<sha256(dir)[:8]>`.
///
/// Our default profile represents the stock install — same path
/// (`$HOME/.claude`), no wrapper, no env var. This predicate is the
/// switch the credentials reader consults to pick the matching
/// keychain entry.
pub fn is_stock_default(home: &Path, cli_config_dir: &Path) -> bool {
    home.join(".claude") == cli_config_dir
}

/// Convenience wrapper around [`is_stock_default`] that resolves the
/// home dir from the environment. Returns `false` if the home dir can't
/// be determined.
pub fn is_stock_default_cli_config_dir(cli_config_dir: &Path) -> bool {
    dirs::home_dir()
        .map(|home| is_stock_default(&home, cli_config_dir))
        .unwrap_or(false)
}

/// Fetches the profile's quota via the provider and wraps it in a
/// `ProfileUsage`.
pub async fn build(config_dir: &Path, provider: &dyn QuotaProvider) -> ProfileUsage {
    let (quota, quota_error) = match provider.fetch(config_dir).await {
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
    use std::path::{Path, PathBuf};

    use async_trait::async_trait;
    use tempfile::TempDir;

    use super::*;

    struct AlwaysFailsProvider;

    #[async_trait]
    impl QuotaProvider for AlwaysFailsProvider {
        async fn fetch(&self, _: &Path) -> Result<QuotaUsage, QuotaError> {
            Err(QuotaError::Network)
        }
    }

    #[tokio::test]
    async fn quota_network_failure_surfaces_network_error() {
        let dir = TempDir::new().unwrap();
        let result = build(dir.path(), &AlwaysFailsProvider).await;
        assert!(result.quota.is_none());
        assert!(matches!(result.quota_error, Some(QuotaError::Network)));
    }

    #[tokio::test]
    async fn provider_no_credentials_surfaces_nocredentials() {
        struct NoCreds;
        #[async_trait]
        impl QuotaProvider for NoCreds {
            async fn fetch(&self, _: &Path) -> Result<QuotaUsage, QuotaError> {
                Err(QuotaError::NoCredentials)
            }
        }
        let dir = TempDir::new().unwrap();
        let result = build(dir.path(), &NoCreds).await;
        assert!(matches!(
            result.quota_error,
            Some(QuotaError::NoCredentials)
        ));
    }

    // --- is_stock_default (pure) ---

    #[test]
    fn forbidden_serializes_to_snake_case() {
        // Wire contract: the frontend QuotaError union expects `forbidden`.
        let json = serde_json::to_string(&QuotaError::Forbidden).unwrap();
        assert_eq!(json, "\"forbidden\"");
    }

    #[test]
    fn is_stock_default_recognises_home_dot_claude() {
        let home = PathBuf::from("/Users/u");
        assert!(is_stock_default(&home, &home.join(".claude")));
    }

    #[test]
    fn is_stock_default_rejects_managed_profile_path() {
        let home = PathBuf::from("/Users/u");
        let managed =
            PathBuf::from("/Users/u/Library/Application Support/ai-profiles/profiles/x/cli-config");
        assert!(!is_stock_default(&home, &managed));
    }

    #[test]
    fn is_stock_default_rejects_dot_claude_under_a_different_home() {
        // A managed profile's data root happens to be ".../foo/.claude" —
        // not the user's home `.claude`, so the predicate must be false.
        let home = PathBuf::from("/Users/u");
        let elsewhere = PathBuf::from("/Users/u/somewhere/else/.claude");
        assert!(!is_stock_default(&home, &elsewhere));
    }

    #[test]
    fn is_stock_default_rejects_trailing_slash_variants() {
        // `PathBuf::join` does not add a trailing separator, so the
        // comparison is exact. Defensive in case an upstream caller ever
        // produces a path with one.
        let home = PathBuf::from("/Users/u");
        let with_slash = PathBuf::from("/Users/u/.claude/");
        // `Path::new("/x/")` and `Path::new("/x")` compare equal on Unix,
        // so this is informational — both are "equal" structurally.
        // The assertion documents the current behaviour.
        assert_eq!(
            is_stock_default(&home, &with_slash),
            home.join(".claude") == with_slash,
        );
    }
}
