//! Whether a stored login belongs to the entry reading it.
//!
//! A Keychain slot holds whichever account last signed in through it: a CLI run
//! without `CLAUDE_CONFIG_DIR` overwrites the stock entry's whatever that
//! entry is meant to be. The usage endpoint answers for any valid token and
//! doesn't say whose it is, so quota is only shown once the profile endpoint
//! has named the token's account and it's one of the entry's own.
//!
//! This fails open: when the endpoint can't answer or the entry's files can't
//! say who it should be, usage shows as it would have. A flaky undocumented
//! endpoint shouldn't hide correct numbers.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::Deserialize;

use crate::accounts::ClaudeSignIn;
use crate::usage::credentials::read_access_token;
use crate::usage::quota::{token_cache_key, HttpResponse, ReqwestUsageClient};
use crate::usage::{ProfileUsage, QuotaError};

const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";

/// The account a token belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenAccount {
    uuid: String,
    email: Option<String>,
}

#[derive(Deserialize)]
struct ProfileResponse {
    account: Option<ProfileResponseAccount>,
}

#[derive(Deserialize)]
struct ProfileResponseAccount {
    uuid: Option<String>,
    email: Option<String>,
}

/// Pure: the account a profile response names, if it was a success that does.
fn parse_token_account(response: &HttpResponse) -> Option<TokenAccount> {
    if response.status != 200 {
        return None;
    }
    let account = serde_json::from_slice::<ProfileResponse>(&response.body)
        .ok()?
        .account?;
    let uuid = account.uuid.filter(|uuid| !uuid.trim().is_empty())?;
    Some(TokenAccount {
        uuid,
        email: account.email.filter(|email| !email.trim().is_empty()),
    })
}

#[async_trait]
pub trait TokenAccountClient: Send + Sync {
    /// The account `access_token` belongs to; `None` when that can't be found out.
    async fn fetch_account(&self, access_token: &str) -> Option<TokenAccount>;
}

#[async_trait]
impl TokenAccountClient for ReqwestUsageClient {
    async fn fetch_account(&self, access_token: &str) -> Option<TokenAccount> {
        let response = self.get(PROFILE_URL, access_token).await.ok()?;
        parse_token_account(&response)
    }
}

/// Accounts already resolved, by token hash. A token's account never changes
/// and a rotated token is a new key, so entries don't expire. Failures aren't
/// kept: the next poll asks again.
#[derive(Default)]
pub struct TokenAccountCache {
    accounts: Mutex<HashMap<String, TokenAccount>>,
}

impl TokenAccountCache {
    pub fn new() -> Self {
        Self::default()
    }

    async fn account(&self, token: &str, client: &dyn TokenAccountClient) -> Option<TokenAccount> {
        let key = token_cache_key(token);
        if let Some(account) = self.accounts.lock().unwrap().get(&key) {
            return Some(account.clone());
        }
        let account = client.fetch_account(token).await?;
        self.accounts.lock().unwrap().insert(key, account.clone());
        Some(account)
    }
}

/// Withholds `usage` unless the login it was read with belongs to this entry,
/// and tells a desktop-only sign-in apart from no sign-in at all.
///
/// Runs after the fetch and any refresh it needed, so the token checked is the
/// one the numbers came from.
pub async fn guard(
    usage: ProfileUsage,
    cli_config_dir: &Path,
    sign_in: &ClaudeSignIn,
    client: &dyn TokenAccountClient,
    cache: &TokenAccountCache,
) -> ProfileUsage {
    if matches!(usage.quota_error, Some(QuotaError::NoCredentials)) && sign_in.desktop_signed_in {
        return ProfileUsage {
            quota_error: Some(QuotaError::DesktopOnly),
            ..usage
        };
    }
    if usage.quota.is_none() || sign_in.account_ids.is_empty() {
        return usage;
    }
    let Ok(token) = read_access_token(cli_config_dir) else {
        return usage;
    };
    let Some(account) = cache.account(&token, client).await else {
        return usage;
    };
    if belongs_to(&account, sign_in) {
        return usage;
    }
    ProfileUsage {
        quota: None,
        quota_error: Some(QuotaError::AccountMismatch),
        stored_account_email: account.email,
        fetched_at: usage.fetched_at,
    }
}

/// Pure: whether `account` is one the entry is signed in under.
fn belongs_to(account: &TokenAccount, sign_in: &ClaudeSignIn) -> bool {
    sign_in
        .account_ids
        .iter()
        .any(|id| id.trim().eq_ignore_ascii_case(account.uuid.trim()))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use tempfile::TempDir;

    use super::*;
    use crate::usage::QuotaUsage;

    struct StubClient {
        account: Option<TokenAccount>,
        calls: AtomicU32,
    }

    impl StubClient {
        fn naming(uuid: &str) -> Self {
            Self {
                account: Some(TokenAccount {
                    uuid: uuid.to_string(),
                    email: Some("work@example.com".to_string()),
                }),
                calls: AtomicU32::new(0),
            }
        }

        fn unreachable() -> Self {
            Self {
                account: None,
                calls: AtomicU32::new(0),
            }
        }
    }

    #[async_trait]
    impl TokenAccountClient for StubClient {
        async fn fetch_account(&self, _: &str) -> Option<TokenAccount> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.account.clone()
        }
    }

    fn usage_with_quota() -> ProfileUsage {
        ProfileUsage {
            quota: Some(QuotaUsage {
                primary: None,
                secondary: None,
                scoped_weekly: Vec::new(),
                spend: None,
                rate_limit_reset_credits: None,
            }),
            quota_error: None,
            stored_account_email: None,
            fetched_at: "now".to_string(),
        }
    }

    fn usage_with_error(error: QuotaError) -> ProfileUsage {
        ProfileUsage {
            quota: None,
            quota_error: Some(error),
            stored_account_email: None,
            fetched_at: "now".to_string(),
        }
    }

    fn signed_in_as(ids: &[&str]) -> ClaudeSignIn {
        ClaudeSignIn {
            account_ids: ids.iter().map(|id| id.to_string()).collect(),
            desktop_signed_in: false,
        }
    }

    fn config_dir_with_token(token: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(".credentials.json"),
            format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}"}}}}"#),
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn another_accounts_token_withholds_the_quota_and_names_that_account() {
        let dir = config_dir_with_token("t");

        let guarded = guard(
            usage_with_quota(),
            dir.path(),
            &signed_in_as(&["default-account"]),
            &StubClient::naming("work-account"),
            &TokenAccountCache::new(),
        )
        .await;

        assert!(guarded.quota.is_none());
        assert!(matches!(
            guarded.quota_error,
            Some(QuotaError::AccountMismatch)
        ));
        assert_eq!(
            guarded.stored_account_email.as_deref(),
            Some("work@example.com")
        );
    }

    #[tokio::test]
    async fn a_token_of_any_of_the_entrys_accounts_keeps_the_quota() {
        let dir = config_dir_with_token("t");

        let guarded = guard(
            usage_with_quota(),
            dir.path(),
            &signed_in_as(&["desktop-account", "CLI-ACCOUNT"]),
            &StubClient::naming("cli-account"),
            &TokenAccountCache::new(),
        )
        .await;

        assert!(guarded.quota.is_some());
        assert!(guarded.quota_error.is_none());
    }

    #[tokio::test]
    async fn an_endpoint_that_cant_answer_keeps_the_quota() {
        let dir = config_dir_with_token("t");

        let guarded = guard(
            usage_with_quota(),
            dir.path(),
            &signed_in_as(&["default-account"]),
            &StubClient::unreachable(),
            &TokenAccountCache::new(),
        )
        .await;

        assert!(guarded.quota.is_some());
    }

    #[tokio::test]
    async fn an_entry_that_cant_say_who_it_is_keeps_the_quota_without_asking() {
        let dir = config_dir_with_token("t");
        let client = StubClient::naming("work-account");

        let guarded = guard(
            usage_with_quota(),
            dir.path(),
            &signed_in_as(&[]),
            &client,
            &TokenAccountCache::new(),
        )
        .await;

        assert!(guarded.quota.is_some());
        assert_eq!(client.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_token_is_only_resolved_once_until_it_rotates() {
        let dir = config_dir_with_token("first");
        let client = StubClient::naming("default-account");
        let cache = TokenAccountCache::new();
        let sign_in = signed_in_as(&["default-account"]);

        for _ in 0..2 {
            guard(usage_with_quota(), dir.path(), &sign_in, &client, &cache).await;
        }
        assert_eq!(client.calls.load(Ordering::SeqCst), 1);

        let rotated = config_dir_with_token("second");
        guard(
            usage_with_quota(),
            rotated.path(),
            &sign_in,
            &client,
            &cache,
        )
        .await;
        assert_eq!(client.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_failed_lookup_is_asked_again_next_time() {
        let dir = config_dir_with_token("t");
        let client = StubClient::unreachable();
        let cache = TokenAccountCache::new();
        let sign_in = signed_in_as(&["default-account"]);

        for _ in 0..2 {
            guard(usage_with_quota(), dir.path(), &sign_in, &client, &cache).await;
        }

        assert_eq!(client.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn no_stored_login_beside_a_signed_in_desktop_app_is_desktop_only() {
        let dir = TempDir::new().unwrap();
        let sign_in = ClaudeSignIn {
            account_ids: vec!["app".to_string()],
            desktop_signed_in: true,
        };

        let guarded = guard(
            usage_with_error(QuotaError::NoCredentials),
            dir.path(),
            &sign_in,
            &StubClient::unreachable(),
            &TokenAccountCache::new(),
        )
        .await;

        assert!(matches!(guarded.quota_error, Some(QuotaError::DesktopOnly)));
    }

    #[tokio::test]
    async fn no_stored_login_and_no_desktop_sign_in_stays_no_credentials() {
        let dir = TempDir::new().unwrap();

        let guarded = guard(
            usage_with_error(QuotaError::NoCredentials),
            dir.path(),
            &signed_in_as(&[]),
            &StubClient::unreachable(),
            &TokenAccountCache::new(),
        )
        .await;

        assert!(matches!(
            guarded.quota_error,
            Some(QuotaError::NoCredentials)
        ));
    }

    // --- manual end-to-end smoke (real endpoint + keychain) ---
    // Gated behind an env var pointing at a profile's cli-config dir, since it
    // reads the real token and calls the real endpoint. Prints no token, id or
    // email. `AI_PROFILES_ACCOUNT_SMOKE_JSON` names the `.claude.json` to compare
    // with, for the stock entry, whose file isn't in its config dir. Run with:
    //   AI_PROFILES_ACCOUNT_SMOKE="<cli-config dir>" cargo test \
    //     --manifest-path apps/ai-profiles/src-tauri/Cargo.toml \
    //     usage::token_account::tests::profile_endpoint_smoke -- --nocapture --ignored
    #[tokio::test]
    #[ignore = "calls the real endpoint; opt in via AI_PROFILES_ACCOUNT_SMOKE"]
    async fn profile_endpoint_smoke() {
        let Ok(dir) = std::env::var("AI_PROFILES_ACCOUNT_SMOKE") else {
            eprintln!("set AI_PROFILES_ACCOUNT_SMOKE=<cli-config dir> to run");
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let json = std::env::var("AI_PROFILES_ACCOUNT_SMOKE_JSON")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| dir.join(".claude.json"));
        let token = read_access_token(&dir).expect("a stored token");
        let client = ReqwestUsageClient::new("ai-profiles/smoke".to_string()).unwrap();
        let response = client.get(PROFILE_URL, &token).await.unwrap();
        eprintln!("profile_endpoint_smoke: status={}", response.status);
        let account = parse_token_account(&response).expect("a named account");
        let named = std::fs::read_to_string(json).unwrap_or_default();
        eprintln!(
            "profile_endpoint_smoke: has_email={} matches_claude_json={}",
            account.email.is_some(),
            named.contains(&account.uuid),
        );
    }

    #[test]
    fn parses_the_account_a_profile_response_names() {
        let response = HttpResponse {
            status: 200,
            body: br#"{"account":{"uuid":"abc","email":"a@example.com","full_name":"A"},"organization":{"uuid":"org"}}"#.to_vec(),
            retry_after: None,
        };
        assert_eq!(
            parse_token_account(&response),
            Some(TokenAccount {
                uuid: "abc".to_string(),
                email: Some("a@example.com".to_string()),
            })
        );
    }

    #[test]
    fn a_failed_or_nameless_profile_response_names_no_one() {
        let response = |status, body: &[u8]| HttpResponse {
            status,
            body: body.to_vec(),
            retry_after: None,
        };
        assert_eq!(
            parse_token_account(&response(401, br#"{"account":{"uuid":"abc"}}"#)),
            None
        );
        assert_eq!(parse_token_account(&response(200, b"not json")), None);
        assert_eq!(
            parse_token_account(&response(200, br#"{"account":{"uuid":" "}}"#)),
            None
        );
        assert_eq!(parse_token_account(&response(200, b"{}")), None);
    }
}
