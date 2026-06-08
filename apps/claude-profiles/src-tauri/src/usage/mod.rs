pub(crate) mod codex;
pub(crate) mod credentials;
pub(crate) mod dead_credentials;
pub(crate) mod quota;
pub(crate) mod refresh;

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
    Unauthorized,
    /// Stored credentials are dead and cannot be auto-refreshed — the user
    /// must sign in to this profile again. Distinct from the transient
    /// `Unauthorized`, which a CLI refresh can fix.
    NeedsLogin,
    RateLimited,
    Network,
    Unknown,
}

use std::path::Path;
use std::time::{Duration, Instant};

use chrono::Utc;

/// Fetches a profile's quota for a managed app. Claude drives an HTTP request
/// to Anthropic; Codex drives `codex app-server` over JSON-RPC. The
/// orchestration in `build`/`build_with_cli_refresh` is app-agnostic — it only
/// sees this trait.
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
/// switch the credentials reader and the refresher consult to pick the
/// matching keychain entry / not export `CLAUDE_CONFIG_DIR`.
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

/// Same as `build`, but on a `QuotaError::Unauthorized` first response we
/// ask the `refresher` to trigger Claude Code's own OAuth refresh and
/// then retry the quota fetch once. If the retry also fails the result
/// surfaces the second error to the user, who then sees the same
/// "Token refresh needed" message as before — so this strictly improves
/// the success path without regressing the failure path.
pub async fn build_with_cli_refresh(
    cli_config_dir: &Path,
    provider: &dyn QuotaProvider,
    refresher: &dyn refresh::CliRefresher,
) -> ProfileUsage {
    let first = build(cli_config_dir, provider).await;
    if !matches!(first.quota_error, Some(QuotaError::Unauthorized)) {
        return first;
    }
    // Capture the pre-refresh token so we can wait for it to actually
    // change before retrying. `claude` may exit on stdin-EOF before its
    // async token-persistence write has fsynced; without this poll a
    // fast retry would read the stale token and surface Unauthorized
    // again as if recovery were impossible.
    let token_before = credentials::read_access_token(cli_config_dir).ok();
    refresher.try_refresh(cli_config_dir).await;
    wait_for_token_change(cli_config_dir, token_before.as_deref()).await;
    build(cli_config_dir, provider).await
}

/// Bound on how long we'll wait for the credentials store to reflect a
/// new token after the refresher returned. A token-persistence write
/// from `claude` typically lands in well under 100 ms on warm disks;
/// 1 s is a generous ceiling for cold/sandboxed cases.
const TOKEN_CHANGE_DEADLINE: Duration = Duration::from_millis(1000);
const TOKEN_CHANGE_POLL: Duration = Duration::from_millis(50);

/// Polls the credentials store until the access token differs from
/// `before`, or until the deadline passes. Returns immediately if the
/// store can't be read at all (treated as "changed", since the retry
/// will surface the read failure itself).
async fn wait_for_token_change(cli_config_dir: &Path, before: Option<&str>) {
    let deadline = Instant::now() + TOKEN_CHANGE_DEADLINE;
    loop {
        let current = credentials::read_access_token(cli_config_dir).ok();
        if current.as_deref() != before {
            return;
        }
        if Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(TOKEN_CHANGE_POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use async_trait::async_trait;
    use tempfile::TempDir;

    use super::refresh::CliRefresher;
    use super::*;

    struct AlwaysFailsProvider;

    #[async_trait]
    impl QuotaProvider for AlwaysFailsProvider {
        async fn fetch(&self, _: &Path) -> Result<QuotaUsage, QuotaError> {
            Err(QuotaError::Network)
        }
    }

    /// Returns a queued list of results in order (popped from the end).
    /// Once the queue is empty, every subsequent call returns Network, and
    /// each call is counted.
    struct QueuedProvider {
        responses: Mutex<Vec<Result<QuotaUsage, QuotaError>>>,
        calls: Mutex<u32>,
    }

    impl QueuedProvider {
        fn new(responses: Vec<Result<QuotaUsage, QuotaError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                calls: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl QuotaProvider for QueuedProvider {
        async fn fetch(&self, _: &Path) -> Result<QuotaUsage, QuotaError> {
            *self.calls.lock().unwrap() += 1;
            self.responses
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(Err(QuotaError::Network))
        }
    }

    fn one_window() -> QuotaUsage {
        QuotaUsage {
            primary: Some(Window {
                utilization: Some(50.0),
                resets_at: None,
            }),
            secondary: None,
            secondary_extra: None,
        }
    }

    struct RecordingRefresher {
        calls: Mutex<Vec<PathBuf>>,
    }

    impl RecordingRefresher {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl CliRefresher for RecordingRefresher {
        async fn try_refresh(&self, dir: &Path) {
            self.calls.lock().unwrap().push(dir.to_path_buf());
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

    // --- build_with_cli_refresh orchestration ---

    #[tokio::test]
    async fn refresh_is_not_triggered_when_first_fetch_succeeds() {
        let dir = dir_with_token();
        let provider = QueuedProvider::new(vec![Ok(one_window())]);
        let refresher = RecordingRefresher::new();
        let result = build_with_cli_refresh(dir.path(), &provider, &refresher).await;
        assert!(result.quota.is_some());
        assert!(result.quota_error.is_none());
        assert_eq!(refresher.call_count(), 0);
        assert_eq!(*provider.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn refresh_is_not_triggered_for_non_unauthorized_errors() {
        // Network errors should NOT trigger a refresh — only Unauthorized
        // is plausibly recoverable by re-running `claude`.
        let dir = dir_with_token();
        let provider = QueuedProvider::new(vec![Err(QuotaError::Network)]);
        let refresher = RecordingRefresher::new();
        let result = build_with_cli_refresh(dir.path(), &provider, &refresher).await;
        assert!(matches!(result.quota_error, Some(QuotaError::Network)));
        assert_eq!(refresher.call_count(), 0);
    }

    #[tokio::test]
    async fn refresh_recovers_on_unauthorized_then_success() {
        let dir = dir_with_token();
        // QueuedProvider::pop returns from the end, so order responses in
        // reverse: first call returns Unauthorized, second succeeds.
        let provider = QueuedProvider::new(vec![Ok(one_window()), Err(QuotaError::Unauthorized)]);
        let refresher = RecordingRefresher::new();
        let result = build_with_cli_refresh(dir.path(), &provider, &refresher).await;
        assert!(result.quota.is_some(), "expected recovered quota");
        assert!(result.quota_error.is_none());
        assert_eq!(refresher.call_count(), 1);
        assert_eq!(*provider.calls.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn refresh_only_runs_once_and_surfaces_second_unauthorized() {
        let dir = dir_with_token();
        let provider = QueuedProvider::new(vec![
            Err(QuotaError::Unauthorized),
            Err(QuotaError::Unauthorized),
        ]);
        let refresher = RecordingRefresher::new();
        let result = build_with_cli_refresh(dir.path(), &provider, &refresher).await;
        assert!(matches!(result.quota_error, Some(QuotaError::Unauthorized)));
        assert!(result.quota.is_none());
        assert_eq!(refresher.call_count(), 1);
        assert_eq!(*provider.calls.lock().unwrap(), 2);
    }

    // --- wait_for_token_change ---

    #[tokio::test]
    async fn wait_for_token_change_returns_quickly_when_token_changes() {
        let dir = dir_with_token();
        let path = dir.path().to_path_buf();
        let writer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            fs::write(
                path.join(".credentials.json"),
                r#"{"claudeAiOauth":{"accessToken":"sk-fresh"}}"#,
            )
            .unwrap();
        });
        let started = Instant::now();
        wait_for_token_change(dir.path(), Some("sk-test")).await;
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "expected return shortly after token changed, took {:?}",
            started.elapsed(),
        );
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn wait_for_token_change_returns_at_deadline_when_unchanged() {
        let dir = dir_with_token();
        let started = Instant::now();
        wait_for_token_change(dir.path(), Some("sk-test")).await;
        let elapsed = started.elapsed();
        // Should have waited approximately TOKEN_CHANGE_DEADLINE (1 s).
        assert!(
            elapsed >= Duration::from_millis(900),
            "expected ≥ ~900ms wait, got {elapsed:?}",
        );
        assert!(
            elapsed < Duration::from_millis(1500),
            "expected ≤ ~1500ms wait, got {elapsed:?}",
        );
    }

    #[tokio::test]
    async fn wait_for_token_change_returns_when_credentials_disappear() {
        // If the credentials file vanishes between calls, treat that as
        // "changed" — read_access_token returning Err means `current` is
        // None which differs from a Some(_) baseline, so we exit.
        let dir = dir_with_token();
        fs::remove_file(dir.path().join(".credentials.json")).unwrap();
        let started = Instant::now();
        wait_for_token_change(dir.path(), Some("sk-test")).await;
        assert!(started.elapsed() < Duration::from_millis(200));
    }

    // --- is_stock_default (pure) ---

    #[test]
    fn needs_login_serializes_to_snake_case() {
        // Wire contract: the frontend QuotaError union expects `needs_login`.
        let json = serde_json::to_string(&QuotaError::NeedsLogin).unwrap();
        assert_eq!(json, "\"needs_login\"");
    }

    #[test]
    fn is_stock_default_recognises_home_dot_claude() {
        let home = PathBuf::from("/Users/u");
        assert!(is_stock_default(&home, &home.join(".claude")));
    }

    #[test]
    fn is_stock_default_rejects_managed_profile_path() {
        let home = PathBuf::from("/Users/u");
        let managed = PathBuf::from(
            "/Users/u/Library/Application Support/claude-profiles/profiles/x/cli-config",
        );
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
