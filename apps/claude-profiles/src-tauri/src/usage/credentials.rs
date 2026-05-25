use std::path::Path;

use serde::Deserialize;

use crate::usage::QuotaError;

/// Hard cap on the credentials file size. The real file is tiny
/// (a few hundred bytes). Anything larger is treated as suspect
/// and rejected rather than read into memory.
const MAX_CREDENTIALS_BYTES: u64 = 64 * 1024;

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
}

/// Reads the access token from `<cli_config_dir>/.credentials.json`.
/// On any failure (missing file, oversized, unreadable, malformed JSON,
/// empty token) returns a categorised `QuotaError`. Never panics.
pub fn read_access_token(cli_config_dir: &Path) -> Result<String, QuotaError> {
    let path = cli_config_dir.join(".credentials.json");
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(QuotaError::NoCredentials);
        }
        Err(_) => return Err(QuotaError::Unknown),
    };
    if metadata.len() > MAX_CREDENTIALS_BYTES {
        return Err(QuotaError::Unknown);
    }
    let raw = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return Err(QuotaError::Unknown),
    };
    let parsed: CredentialsFile = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return Err(QuotaError::Unknown),
    };
    match parsed.claude_ai_oauth.access_token {
        Some(token) if !token.trim().is_empty() => Ok(token),
        _ => Err(QuotaError::NoCredentials),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn write_creds(dir: &Path, contents: &str) {
        fs::write(dir.join(".credentials.json"), contents).unwrap();
    }

    #[test]
    fn missing_file_returns_no_credentials() {
        let dir = TempDir::new().unwrap();
        let error = read_access_token(dir.path()).unwrap_err();
        assert!(matches!(error, QuotaError::NoCredentials));
    }

    #[test]
    fn happy_path_returns_token() {
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
}
