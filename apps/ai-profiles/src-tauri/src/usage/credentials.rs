use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::usage::QuotaError;

/// Hard cap on the credentials file size. The real file is tiny
/// (a few hundred bytes). Anything larger is treated as suspect
/// and rejected rather than read into memory.
const MAX_CREDENTIALS_BYTES: u64 = 64 * 1024;

/// Hard cap on the keychain secret size. The real secret is well under 1 KiB.
#[cfg(target_os = "macos")]
const MAX_KEYCHAIN_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize, Default)]
struct CredentialsFile {
    #[serde(default)]
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: ClaudeOauth,
}

#[derive(Debug, Deserialize, Default)]
struct ClaudeOauth {
    #[serde(default)]
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    /// Unix milliseconds at which Claude Code expects the access token to
    /// stop being accepted. Absent in older blobs.
    #[serde(default)]
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

/// A profile's stored Claude Code login, as far as the usage layer needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCredential {
    pub access_token: String,
    /// When the access token expires, in Unix milliseconds, if the blob
    /// records it.
    pub expires_at: Option<i64>,
}

impl StoredCredential {
    /// Pure: whether the access token is past its recorded expiry at
    /// `now_ms`. A blob without an expiry is taken at face value.
    ///
    /// This matters because the usage endpoint doesn't answer an expired
    /// token with a 401: it answers 429 with an hour-long `Retry-After`,
    /// which reads as "rate limited" to anyone who only looks at the status.
    /// Knowing the expiry locally is how we tell the two apart.
    pub fn is_expired_at(&self, now_ms: i64) -> bool {
        self.expires_at
            .is_some_and(|expires_at| now_ms >= expires_at)
    }

    /// [`Self::is_expired_at`] against the wall clock.
    pub fn is_expired(&self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or(0);
        self.is_expired_at(now_ms)
    }
}

/// Reads the OAuth access token for a profile. See [`read_credential`].
pub fn read_access_token(cli_config_dir: &Path) -> Result<String, QuotaError> {
    read_credential(cli_config_dir).map(|credential| credential.access_token)
}

/// Reads the stored OAuth login for a profile.
///
/// Order:
/// 1. `<cli_config_dir>/.credentials.json` (Linux/Windows; older macOS Claude Code).
/// 2. macOS only: Keychain, querying
///    `Claude Code-credentials-<sha256(cli_config_dir)[:8]>` against the
///    current user (newer Claude Code on macOS — the default).
///
/// On non-macOS platforms the keychain step is skipped — if the file
/// isn't present we return `NoCredentials` so the UI shows the standard
/// "sign in to Claude Code once with this profile" message rather than
/// failing as `Unknown` from a missing `/usr/bin/security`.
///
/// On any failure returns a categorised `QuotaError`. Never panics.
pub fn read_credential(cli_config_dir: &Path) -> Result<StoredCredential, QuotaError> {
    let file_path = cli_config_dir.join(".credentials.json");
    if file_path.exists() {
        return read_from_file(&file_path);
    }
    read_from_keychain(cli_config_dir)
}

fn read_from_file(path: &Path) -> Result<StoredCredential, QuotaError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(QuotaError::NoCredentials);
        }
        Err(_) => return Err(QuotaError::Unknown),
    };
    if metadata.len() > MAX_CREDENTIALS_BYTES {
        return Err(QuotaError::Unknown);
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Err(QuotaError::Unknown),
    };
    extract_credential(&raw)
}

#[cfg(target_os = "macos")]
fn read_from_keychain(cli_config_dir: &Path) -> Result<StoredCredential, QuotaError> {
    let service = keychain_service_for(cli_config_dir);
    // Account name is the current macOS user. Fall back to the empty
    // string if USER isn't set — `security` will then return whatever
    // first matching entry it finds, which may be enough.
    let account = std::env::var("USER").unwrap_or_default();

    let mut command = Command::new("/usr/bin/security");
    command.arg("find-generic-password").arg("-s").arg(&service);
    if !account.is_empty() {
        command.arg("-a").arg(&account);
    }
    command.arg("-w");

    let output = match command.output() {
        Ok(output) => output,
        Err(_) => return Err(QuotaError::Unknown),
    };

    if !output.status.success() {
        // Exit code 44 = errSecItemNotFound. Treat any non-zero as
        // "no credentials in keychain" — the user hasn't signed in
        // for this profile, or the entry is under a different account.
        return Err(QuotaError::NoCredentials);
    }
    if output.stdout.len() > MAX_KEYCHAIN_BYTES {
        return Err(QuotaError::Unknown);
    }

    let raw = match String::from_utf8(output.stdout) {
        Ok(text) => text,
        Err(_) => return Err(QuotaError::Unknown),
    };
    extract_credential(raw.trim())
}

/// Picks the macOS Keychain service name Claude Code actually wrote to for
/// this `cli_config_dir`:
/// - Stock install (no `CLAUDE_CONFIG_DIR` — our synthetic default profile,
///   `$HOME/.claude`) → bare `Claude Code-credentials`.
/// - Anything else (managed profile via wrapper, or any explicit
///   `CLAUDE_CONFIG_DIR`) → hashed `Claude Code-credentials-<sha256(dir)[:8]>`.
fn keychain_service_for(cli_config_dir: &Path) -> String {
    if crate::usage::is_stock_default_cli_config_dir(cli_config_dir) {
        "Claude Code-credentials".to_string()
    } else {
        keychain_service_name(cli_config_dir)
    }
}

/// Non-macOS stub: there's no Apple Keychain to query, so if the
/// credentials file wasn't present the user simply hasn't signed in
/// to Claude Code on this profile yet. Surfacing `NoCredentials` keeps
/// the UI consistent across platforms.
#[cfg(not(target_os = "macos"))]
fn read_from_keychain(_cli_config_dir: &Path) -> Result<StoredCredential, QuotaError> {
    Err(QuotaError::NoCredentials)
}

/// Pure: compute the macOS Keychain service name Claude Code uses for
/// a given `CLAUDE_CONFIG_DIR`. The derivation is undocumented Anthropic
/// internals — see the docblock in `launchers/cli.rs` for context.
fn keychain_service_name(cli_config_dir: &Path) -> String {
    let path_str = cli_config_dir.to_string_lossy();
    let digest = Sha256::digest(path_str.as_bytes());
    let mut hex = String::with_capacity(8);
    for byte in digest.iter().take(4) {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    format!("Claude Code-credentials-{hex}")
}

/// True when `<codex_home>/auth.json` exists and carries a non-empty access
/// token — i.e. the user has signed in to Codex for this profile.
pub fn codex_is_signed_in(codex_home: &Path) -> bool {
    let path = codex_home.join("auth.json");
    let Ok(metadata) = std::fs::metadata(&path) else {
        return false;
    };
    if metadata.len() > MAX_CREDENTIALS_BYTES {
        return false;
    }
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    value
        .get("tokens")
        .and_then(|tokens| tokens.get("access_token"))
        .and_then(|token| token.as_str())
        .map(|token| !token.trim().is_empty())
        .unwrap_or(false)
}

/// Pure: parse the credentials JSON blob (same shape on disk and in
/// keychain) and extract the login. Empty / missing token →
/// `NoCredentials`. Malformed JSON → `Unknown`.
fn extract_credential(raw: &str) -> Result<StoredCredential, QuotaError> {
    let parsed: CredentialsFile = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return Err(QuotaError::Unknown),
    };
    // Trim whitespace from the stored token before returning it. A
    // trailing newline or stray space would otherwise be sent as part
    // of the `Authorization: Bearer …` header and rejected as 401.
    let Some(token) = parsed.claude_ai_oauth.access_token else {
        return Err(QuotaError::NoCredentials);
    };
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(QuotaError::NoCredentials);
    }
    Ok(StoredCredential {
        access_token: trimmed.to_string(),
        expires_at: parsed.claude_ai_oauth.expires_at,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    fn write_creds(dir: &Path, contents: &str) {
        fs::write(dir.join(".credentials.json"), contents).unwrap();
    }

    // -- file-based tests (existing behavior preserved) --

    #[test]
    fn happy_path_returns_token_from_file() {
        let dir = TempDir::new().unwrap();
        write_creds(
            dir.path(),
            r#"{"claudeAiOauth":{"accessToken":"sk-test-abc"}}"#,
        );
        assert_eq!(read_access_token(dir.path()).unwrap(), "sk-test-abc");
    }

    #[test]
    fn malformed_json_returns_unknown() {
        let dir = TempDir::new().unwrap();
        write_creds(dir.path(), "not json");
        assert!(matches!(
            read_access_token(dir.path()).unwrap_err(),
            QuotaError::Unknown,
        ));
    }

    #[test]
    fn missing_token_field_returns_no_credentials() {
        let dir = TempDir::new().unwrap();
        write_creds(dir.path(), r#"{"claudeAiOauth":{}}"#);
        assert!(matches!(
            read_access_token(dir.path()).unwrap_err(),
            QuotaError::NoCredentials,
        ));
    }

    #[test]
    fn empty_token_returns_no_credentials() {
        let dir = TempDir::new().unwrap();
        write_creds(dir.path(), r#"{"claudeAiOauth":{"accessToken":"   "}}"#);
        assert!(matches!(
            read_access_token(dir.path()).unwrap_err(),
            QuotaError::NoCredentials,
        ));
    }

    #[test]
    fn unknown_top_level_fields_are_ignored() {
        let dir = TempDir::new().unwrap();
        write_creds(
            dir.path(),
            r#"{"claudeAiOauth":{"accessToken":"t","extra":42},"other":"hi"}"#,
        );
        assert_eq!(read_access_token(dir.path()).unwrap(), "t");
    }

    #[test]
    fn oversized_file_returns_unknown() {
        let dir = TempDir::new().unwrap();
        let huge = "x".repeat((MAX_CREDENTIALS_BYTES + 1) as usize);
        write_creds(dir.path(), &huge);
        assert!(matches!(
            read_access_token(dir.path()).unwrap_err(),
            QuotaError::Unknown,
        ));
    }

    // -- keychain derivation (pure function tests) --

    #[test]
    fn keychain_service_name_uses_first_eight_hex_of_sha256() {
        // SHA-256("test") = 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
        let dir = PathBuf::from("test");
        assert_eq!(
            keychain_service_name(&dir),
            "Claude Code-credentials-9f86d081"
        );
    }

    #[test]
    fn keychain_service_name_changes_with_path() {
        let dir_a = PathBuf::from(
            "/Users/u/Library/Application Support/ai-profiles/profiles/aaa/cli-config",
        );
        let dir_b = PathBuf::from(
            "/Users/u/Library/Application Support/ai-profiles/profiles/bbb/cli-config",
        );
        let name_a = keychain_service_name(&dir_a);
        let name_b = keychain_service_name(&dir_b);
        assert_ne!(name_a, name_b);
        assert!(name_a.starts_with("Claude Code-credentials-"));
        assert!(name_b.starts_with("Claude Code-credentials-"));
        // Each suffix is exactly 8 hex chars.
        assert_eq!(name_a.len(), "Claude Code-credentials-".len() + 8);
    }

    // -- cascade behavior --

    #[test]
    fn missing_file_returns_no_credentials() {
        // No credentials file → on macOS we fall through to keychain
        // (no entry exists for a random tempdir → exit 44 → NoCredentials);
        // on other platforms the keychain step is stubbed out and we
        // return NoCredentials directly. Either way the user sees the
        // same "sign in once" message.
        let dir = TempDir::new().unwrap();
        let outcome = read_access_token(dir.path()).unwrap_err();
        assert!(
            matches!(outcome, QuotaError::NoCredentials),
            "expected NoCredentials, got {outcome:?}",
        );
    }

    // -- extract_token (pure) --

    #[test]
    fn extract_credential_parses_keychain_style_blob() {
        let blob = r#"{"claudeAiOauth":{"accessToken":"sk-ant-from-keychain","refreshToken":"sk-ant-refresh","email":"x@y.z","expiresAt":1234567890000}}"#;
        assert_eq!(
            extract_credential(blob).unwrap(),
            StoredCredential {
                access_token: "sk-ant-from-keychain".to_string(),
                expires_at: Some(1234567890000),
            }
        );
    }

    #[test]
    fn a_blob_without_an_expiry_still_yields_the_token() {
        let blob = r#"{"claudeAiOauth":{"accessToken":"sk-old-format"}}"#;
        let credential = extract_credential(blob).unwrap();
        assert_eq!(credential.access_token, "sk-old-format");
        assert_eq!(credential.expires_at, None);
    }

    // -- expiry (pure) --

    fn credential_expiring_at(expires_at: Option<i64>) -> StoredCredential {
        StoredCredential {
            access_token: "sk-test".to_string(),
            expires_at,
        }
    }

    #[test]
    fn a_token_before_its_expiry_is_not_expired() {
        assert!(!credential_expiring_at(Some(10_000)).is_expired_at(9_999));
    }

    #[test]
    fn a_token_at_or_past_its_expiry_is_expired() {
        assert!(credential_expiring_at(Some(10_000)).is_expired_at(10_000));
        assert!(credential_expiring_at(Some(10_000)).is_expired_at(10_001));
    }

    #[test]
    fn a_token_without_a_recorded_expiry_is_never_taken_as_expired() {
        // Older blobs don't say; the endpoint's answer has to decide.
        assert!(!credential_expiring_at(None).is_expired_at(i64::MAX));
    }

    #[test]
    fn codex_is_signed_in_detects_auth_json() {
        let dir = TempDir::new().unwrap();
        assert!(!codex_is_signed_in(dir.path()));
        fs::write(
            dir.path().join("auth.json"),
            r#"{"tokens":{"access_token":"x"}}"#,
        )
        .unwrap();
        assert!(codex_is_signed_in(dir.path()));
    }

    #[test]
    fn extract_token_trims_surrounding_whitespace() {
        // A trailing newline or stray space in the stored value would
        // otherwise be sent as part of the Bearer header and 401.
        let blob = r#"{"claudeAiOauth":{"accessToken":"  sk-ant-padded \n"}}"#;
        assert_eq!(
            extract_credential(blob).unwrap().access_token,
            "sk-ant-padded"
        );
    }
}
