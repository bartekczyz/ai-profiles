//! Which account a profile is signed in under.
//!
//! Read from what the CLI already keeps on disk, never from the network: for
//! Claude, the `oauthAccount` block of `.claude.json`; for Codex, the claims
//! of the ID token in `auth.json`. Both are written by the app itself when it
//! signs in, so a profile that has never been signed in simply has neither.
//!
//! Only what names the account is taken (email, person, organization, plan).
//! Tokens are not read out of these files, and nothing here leaves the
//! machine.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::app_kind::AppKind;
use crate::error::AppResult;
use crate::paths::{cli_config_dir, stock_cli_config_dir};
use crate::profiles;

/// The account a profile is signed in under. Every field is optional: the
/// files differ per app and per sign-in age, and a missing name is better
/// than a wrong one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAccount {
    pub email: Option<String>,
    /// The person's name, as the app recorded it.
    pub name: Option<String>,
    pub organization: Option<String>,
    /// The subscription, e.g. "Max" or "Pro".
    pub plan: Option<String>,
}

impl ProfileAccount {
    fn is_empty(&self) -> bool {
        *self == ProfileAccount::default()
    }
}

/// The account profile `id` (or `default:<app>`) is signed in under, or `None`
/// when nothing on disk names one.
pub fn read(id: &str) -> AppResult<Option<ProfileAccount>> {
    let (kind, config_dir) = match AppKind::from_default_id(id) {
        Some(kind) => (kind, stock_cli_config_dir(kind.spec())?),
        None => {
            let profile = profiles::load()?
                .into_iter()
                .find(|candidate| candidate.id == id)
                .ok_or_else(|| {
                    crate::error::AppError::NotFound(format!("profile {id} not found"))
                })?;
            (profile.app, cli_config_dir(&profile.id)?)
        }
    };
    let stock = AppKind::from_default_id(id).is_some();
    Ok(match kind {
        AppKind::Claude => claude_account(&config_dir, stock),
        AppKind::Codex => codex_account(&config_dir),
    })
}

/// Claude keeps the account in `.claude.json`. A stock install keeps that file
/// in `$HOME`; a profile keeps it inside its config dir, which is what
/// `CLAUDE_CONFIG_DIR` points at.
fn claude_account(config_dir: &Path, stock: bool) -> Option<ProfileAccount> {
    let mut candidates = vec![config_dir.join(".claude.json")];
    if stock {
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(".claude.json"));
        }
    }
    candidates
        .iter()
        .map(PathBuf::as_path)
        .filter_map(read_json)
        .find_map(|document| account_from_claude_json(&document))
}

/// Pure: the account in a parsed `.claude.json`, if it names one.
fn account_from_claude_json(document: &Value) -> Option<ProfileAccount> {
    let oauth = document.get("oauthAccount")?;
    let text = |key: &str| {
        oauth
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let account = ProfileAccount {
        email: text("emailAddress"),
        name: text("displayName").or_else(|| text("fullName")),
        organization: text("organizationName"),
        plan: text("organizationType").as_deref().map(pretty_plan),
    };
    (!account.is_empty()).then_some(account)
}

/// Codex keeps the account in the ID token in `auth.json`, under `CODEX_HOME`.
fn codex_account(config_dir: &Path) -> Option<ProfileAccount> {
    let document = read_json(&config_dir.join("auth.json"))?;
    account_from_codex_auth(&document)
}

/// Pure: the account named by a parsed `auth.json`.
///
/// The ID token's signature is not checked: this is the machine's own token,
/// read only to show whose it is, and a forged one here would mean the config
/// directory was already writable by someone else.
fn account_from_codex_auth(document: &Value) -> Option<ProfileAccount> {
    let claims = jwt_claims(document.get("tokens")?.get("id_token")?.as_str()?)?;
    let text = |value: Option<&Value>| {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let openai = claims.get("https://api.openai.com/auth");
    let account = ProfileAccount {
        email: text(claims.get("email")),
        name: text(claims.get("name")),
        organization: text(
            openai
                .and_then(|auth| auth.get("organizations"))
                .and_then(Value::as_array)
                .and_then(|organizations| {
                    organizations
                        .iter()
                        .find(|organization| {
                            organization
                                .get("is_default")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                        })
                        .or_else(|| organizations.first())
                })
                .and_then(|organization| organization.get("title")),
        ),
        plan: text(openai.and_then(|auth| auth.get("chatgpt_plan_type")))
            .as_deref()
            .map(pretty_plan),
    };
    (!account.is_empty()).then_some(account)
}

/// The claims of `token`, a JWT, without checking its signature.
fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    serde_json::from_slice(&base64_url_decode(payload)?).ok()
}

/// Decode unpadded base64url, as JWT parts are encoded.
fn base64_url_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bits = 0u32;
    let mut held = 0u32;
    let mut bytes = Vec::with_capacity(text.len() * 3 / 4);
    for character in text.trim_end_matches('=').bytes() {
        let value = ALPHABET.iter().position(|entry| *entry == character)? as u32;
        bits = (bits << 6) | value;
        held += 6;
        if held >= 8 {
            held -= 8;
            bytes.push((bits >> held) as u8);
        }
    }
    Some(bytes)
}

/// A plan token from either app as a label: `claude_max` → "Max",
/// `prolite` → "Prolite", `plus` → "Plus".
fn pretty_plan(plan: &str) -> String {
    let trimmed = plan
        .trim()
        .trim_start_matches("claude_")
        .trim_start_matches("chatgpt_");
    let mut characters = trimmed.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => plan.trim().to_owned(),
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_claude_account_preferring_the_display_name() {
        let document = json!({
            "oauthAccount": {
                "emailAddress": "ada@example.com",
                "displayName": "Ada",
                "fullName": "Ada Lovelace",
                "organizationName": "Ada's Organization",
                "organizationType": "claude_max",
            },
            "projects": {},
        });
        assert_eq!(
            account_from_claude_json(&document),
            Some(ProfileAccount {
                email: Some("ada@example.com".into()),
                name: Some("Ada".into()),
                organization: Some("Ada's Organization".into()),
                plan: Some("Max".into()),
            })
        );
    }

    #[test]
    fn a_claude_config_without_a_sign_in_names_no_account() {
        assert_eq!(account_from_claude_json(&json!({ "projects": {} })), None);
        assert_eq!(
            account_from_claude_json(&json!({ "oauthAccount": { "emailAddress": "  " } })),
            None
        );
    }

    /// `{"alg":"none"}` . claims . (empty signature), base64url, unpadded.
    fn fake_id_token(claims: Value) -> String {
        fn encode(bytes: &[u8]) -> String {
            const ALPHABET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let mut block = 0u32;
                for (index, byte) in chunk.iter().enumerate() {
                    block |= u32::from(*byte) << (16 - 8 * index);
                }
                for index in 0..chunk.len() + 1 {
                    out.push(ALPHABET[((block >> (18 - 6 * index)) & 0x3f) as usize] as char);
                }
            }
            out
        }
        format!(
            "{}.{}.",
            encode(br#"{"alg":"none"}"#),
            encode(claims.to_string().as_bytes())
        )
    }

    #[test]
    fn reads_the_codex_account_from_the_id_token() {
        let token = fake_id_token(json!({
            "email": "ada@example.com",
            "name": "Ada Lovelace",
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "prolite",
                "organizations": [
                    { "title": "Other", "is_default": false },
                    { "title": "Personal", "is_default": true },
                ],
            },
        }));
        let document = json!({ "tokens": { "id_token": token }, "auth_mode": "chatgpt" });
        assert_eq!(
            account_from_codex_auth(&document),
            Some(ProfileAccount {
                email: Some("ada@example.com".into()),
                name: Some("Ada Lovelace".into()),
                organization: Some("Personal".into()),
                plan: Some("Prolite".into()),
            })
        );
    }

    #[test]
    fn a_codex_auth_without_a_usable_token_names_no_account() {
        assert_eq!(
            account_from_codex_auth(&json!({ "auth_mode": "apikey" })),
            None
        );
        assert_eq!(
            account_from_codex_auth(&json!({ "tokens": { "id_token": "not.a.jwt" } })),
            None
        );
        let empty = fake_id_token(json!({ "sub": "user-1" }));
        assert_eq!(
            account_from_codex_auth(&json!({ "tokens": { "id_token": empty } })),
            None
        );
    }

    #[test]
    fn base64_url_decode_handles_unpadded_input_and_rejects_junk() {
        assert_eq!(base64_url_decode("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64_url_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_url_decode("-_8").unwrap(), vec![0xfb, 0xff]);
        assert!(base64_url_decode("not base64!").is_none());
    }

    #[test]
    fn plans_read_as_labels() {
        assert_eq!(pretty_plan("claude_max"), "Max");
        assert_eq!(pretty_plan("claude_pro"), "Pro");
        assert_eq!(pretty_plan("plus"), "Plus");
        assert_eq!(pretty_plan("prolite"), "Prolite");
        assert_eq!(pretty_plan("  team  "), "Team");
    }
}
