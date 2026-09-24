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

pub use ai_profiles_core::account::ProfileAccount;
use ai_profiles_core::account::{account_from_claude_json, pretty_plan};

use crate::app_kind::AppKind;
use crate::error::AppResult;
use crate::paths::{cli_config_dir, stock_cli_config_dir};
use crate::profiles;

/// Whether a profile is signed in, and as whom, as far as its files say.
///
/// Not knowing is a state of its own: a Claude desktop app signed in as an
/// account no `.claude.json` names is signed in, but not as anyone this can
/// name, and saying "not signed in" there would be wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AccountStatus {
    SignedIn { account: ProfileAccount },
    SignedOut,
    Unknown,
}

impl AccountStatus {
    fn from_account(account: Option<ProfileAccount>, otherwise: AccountStatus) -> AccountStatus {
        account.map_or(otherwise, |account| AccountStatus::SignedIn { account })
    }
}

/// Whether profile `id` (or `default:<app>`) is signed in, and as whom.
pub fn read(id: &str) -> AppResult<AccountStatus> {
    let stock = AppKind::from_default_id(id);
    let (kind, config_dir) = match stock {
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
    let gui_data = PathBuf::from(profiles::paths(id)?.gui_data_dir);
    Ok(match kind {
        AppKind::Claude if stock.is_some() => stock_claude_status(
            // The stock CLI, run without `CLAUDE_CONFIG_DIR`, keeps its
            // sign-in in `$HOME/.claude.json`; `~/.claude/.claude.json` only
            // exists if something ran it with `CLAUDE_CONFIG_DIR=~/.claude`.
            &[
                dirs::home_dir().unwrap_or_default().join(".claude.json"),
                config_dir.join(".claude.json"),
            ],
            &gui_data,
        ),
        AppKind::Claude => claude_status(&config_dir, &gui_data),
        AppKind::Codex => {
            // Codex's desktop app and CLI both sign in through `auth.json`.
            AccountStatus::from_account(codex_account(&config_dir), AccountStatus::SignedOut)
        }
    })
}

/// What a Claude desktop app's data says about its sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Desktop {
    /// No data: the app has never run with this data dir.
    Absent,
    SignedOut,
    /// Signed in as the account with this id.
    SignedIn(String),
    /// There is data, but it doesn't say.
    Unsure,
}

/// The two keys of the desktop app's `config.json` that say who is signed
/// in. The rest of the file, its encrypted token caches included, isn't
/// kept.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopConfig {
    /// The account the app last signed in as. Kept after signing out, so it
    /// says who, not whether.
    last_known_account_uuid: Option<String>,
    /// Whether the window was last laid out signed in; `false` once the app
    /// signs out.
    window_size_was_signed_in: Option<bool>,
}

/// Whether the Claude desktop app with data at `gui_data` is signed in, and
/// as which account.
fn desktop_sign_in(gui_data: &Path) -> Desktop {
    let Ok(text) = fs::read_to_string(gui_data.join("config.json")) else {
        return if gui_data.exists() {
            Desktop::Unsure
        } else {
            Desktop::Absent
        };
    };
    let Ok(config) = serde_json::from_str::<DesktopConfig>(&text) else {
        return Desktop::Unsure;
    };
    match (
        config.window_size_was_signed_in,
        config
            .last_known_account_uuid
            .filter(|id| !id.trim().is_empty()),
    ) {
        (Some(false), _) => Desktop::SignedOut,
        (Some(true), Some(id)) => Desktop::SignedIn(id),
        _ => Desktop::Unsure,
    }
}

/// Claude's stock install: its desktop app, whose data is at `gui_data`,
/// says whether it is signed in and as which account; the first of
/// `claude_jsons` naming that account says who that is.
///
/// `$HOME/.claude.json` goes on naming the account of a stock install that
/// has since been imported into a profile (the import moves `~/.claude` and
/// the desktop data, not that file), so it names the stock account only when
/// the desktop app agrees. Without a desktop app, it is the stock CLI's
/// sign-in, and names it.
fn stock_claude_status(claude_jsons: &[PathBuf], gui_data: &Path) -> AccountStatus {
    let documents: Vec<Value> = claude_jsons
        .iter()
        .filter_map(|path| read_json(path))
        .collect();
    let cli_account = || documents.iter().find_map(account_from_claude_json);
    match desktop_sign_in(gui_data) {
        Desktop::SignedIn(id) => AccountStatus::from_account(
            documents
                .iter()
                .filter(|document| {
                    document
                        .pointer("/oauthAccount/accountUuid")
                        .and_then(Value::as_str)
                        == Some(id.as_str())
                })
                .find_map(account_from_claude_json),
            AccountStatus::Unknown,
        ),
        // The CLI may still be signed in, as whoever the file names, or that
        // may be the file an import left behind: it can't be told which.
        Desktop::SignedOut if cli_account().is_some() => AccountStatus::Unknown,
        Desktop::SignedOut => AccountStatus::SignedOut,
        Desktop::Unsure => AccountStatus::Unknown,
        Desktop::Absent => AccountStatus::from_account(cli_account(), AccountStatus::SignedOut),
    }
}

/// A managed Claude profile keeps its CLI's account in the `.claude.json`
/// inside its config dir, which is what `CLAUDE_CONFIG_DIR` points at. Until
/// its CLI signs in, that file names no one, though its desktop app may be
/// signed in: then who is unknown, not signed out.
fn claude_status(config_dir: &Path, gui_data: &Path) -> AccountStatus {
    if let Some(account) = read_json(&config_dir.join(".claude.json"))
        .as_ref()
        .and_then(account_from_claude_json)
    {
        return AccountStatus::SignedIn { account };
    }
    match desktop_sign_in(gui_data) {
        Desktop::SignedIn(_) | Desktop::Unsure => AccountStatus::Unknown,
        Desktop::SignedOut | Desktop::Absent => AccountStatus::SignedOut,
    }
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

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    /// A `.claude.json` at `path` signed in as account `id`, or signed out
    /// when `id` is `None`.
    fn claude_json(path: &Path, id: Option<&str>, email: &str) -> PathBuf {
        let document = match id {
            Some(id) => json!({"oauthAccount": {"accountUuid": id, "emailAddress": email}}),
            None => json!({"projects": {}}),
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, document.to_string()).unwrap();
        path.to_path_buf()
    }

    /// A desktop app's data at `gui_data`: its `config.json` with these two
    /// keys (left out when `None`), and a token cache that must never matter.
    fn desktop(gui_data: &Path, last_known: Option<&str>, signed_in: Option<bool>) -> PathBuf {
        fs::create_dir_all(gui_data).unwrap();
        let mut config = json!({"oauth:tokenCache": "not-for-reading", "locale": "en-US"});
        if let Some(id) = last_known {
            config["lastKnownAccountUuid"] = json!(id);
        }
        if let Some(signed_in) = signed_in {
            config["windowSizeWasSignedIn"] = json!(signed_in);
        }
        fs::write(gui_data.join("config.json"), config.to_string()).unwrap();
        gui_data.to_path_buf()
    }

    fn signed_in_as(status: AccountStatus) -> Option<String> {
        match status {
            AccountStatus::SignedIn { account } => account.email,
            _ => None,
        }
    }

    #[test]
    fn the_desktop_app_says_whether_it_is_signed_in_and_as_whom() {
        let dir = tempfile::tempdir().unwrap();
        let gui = |name: &str| dir.path().join(name);
        assert_eq!(desktop_sign_in(&gui("none")), Desktop::Absent);
        assert_eq!(
            desktop_sign_in(&desktop(&gui("in"), Some("a"), Some(true))),
            Desktop::SignedIn("a".into())
        );
        // Signing out keeps the last account's id: it says who, not whether.
        assert_eq!(
            desktop_sign_in(&desktop(&gui("out"), Some("a"), Some(false))),
            Desktop::SignedOut
        );
        assert_eq!(
            desktop_sign_in(&desktop(&gui("no-flag"), Some("a"), None)),
            Desktop::Unsure
        );
        fs::create_dir_all(gui("no-config")).unwrap();
        assert_eq!(desktop_sign_in(&gui("no-config")), Desktop::Unsure);
    }

    #[test]
    fn stock_names_the_account_the_desktop_app_is_signed_in_as() {
        let dir = tempfile::tempdir().unwrap();
        let home = claude_json(
            &dir.path().join(".claude.json"),
            Some("a"),
            "ada@example.com",
        );
        let gui = desktop(&dir.path().join("gui"), Some("a"), Some(true));
        assert_eq!(
            signed_in_as(stock_claude_status(&[home], &gui)).as_deref(),
            Some("ada@example.com")
        );
    }

    #[test]
    fn stock_is_unknown_when_the_desktop_app_is_signed_in_as_an_account_no_file_names() {
        // `$HOME/.claude.json` left naming an account imported into a
        // profile, while the stock app is signed in as someone else.
        let dir = tempfile::tempdir().unwrap();
        let home = claude_json(
            &dir.path().join(".claude.json"),
            Some("moved"),
            "old@example.com",
        );
        let gui = desktop(&dir.path().join("gui"), Some("now"), Some(true));
        assert_eq!(stock_claude_status(&[home], &gui), AccountStatus::Unknown);
    }

    #[test]
    fn stock_is_signed_out_only_when_nothing_is_signed_in() {
        let dir = tempfile::tempdir().unwrap();
        let home = claude_json(&dir.path().join(".claude.json"), None, "");
        let gui = desktop(&dir.path().join("gui"), Some("a"), Some(false));
        assert_eq!(
            stock_claude_status(std::slice::from_ref(&home), &gui),
            AccountStatus::SignedOut
        );
        // Nor anything at all on disk.
        assert_eq!(
            stock_claude_status(
                &[dir.path().join("missing.json")],
                &dir.path().join("no-gui")
            ),
            AccountStatus::SignedOut
        );
    }

    #[test]
    fn stock_is_unknown_when_the_desktop_app_signed_out_but_a_file_still_names_someone() {
        // The CLI may still be signed in as them, or the file may be what an
        // import left behind: not "signed out", and not a name.
        let dir = tempfile::tempdir().unwrap();
        let home = claude_json(
            &dir.path().join(".claude.json"),
            Some("a"),
            "ada@example.com",
        );
        let gui = desktop(&dir.path().join("gui"), Some("a"), Some(false));
        assert_eq!(stock_claude_status(&[home], &gui), AccountStatus::Unknown);
    }

    #[test]
    fn a_stock_cli_only_install_names_its_cli_account() {
        let dir = tempfile::tempdir().unwrap();
        let home = claude_json(
            &dir.path().join(".claude.json"),
            Some("a"),
            "ada@example.com",
        );
        assert_eq!(
            signed_in_as(stock_claude_status(&[home], &dir.path().join("no-gui"))).as_deref(),
            Some("ada@example.com")
        );
    }

    #[test]
    fn stock_goes_by_the_desktop_apps_current_account_not_its_code_tab_folders() {
        // Signed in as A, then B, then A again, with B's Code tab folder the
        // newest, and stray files about: only config.json counts.
        let dir = tempfile::tempdir().unwrap();
        let gui = desktop(&dir.path().join("gui"), Some("a"), Some(true));
        for account in ["a", "b"] {
            fs::create_dir_all(gui.join("claude-code-sessions").join(account).join("org")).unwrap();
        }
        fs::write(gui.join("claude-code-sessions/.DS_Store"), b"").unwrap();
        fs::write(gui.join("claude-code-sessions/b/.DS_Store"), b"").unwrap();
        let home = claude_json(&dir.path().join("home.json"), Some("b"), "bea@example.com");
        let config = claude_json(
            &dir.path().join("config.json"),
            Some("a"),
            "ada@example.com",
        );
        assert_eq!(
            signed_in_as(stock_claude_status(&[home, config], &gui)).as_deref(),
            Some("ada@example.com"),
            "the file naming the desktop app's account, wherever it is in the list"
        );
    }

    #[test]
    fn a_stock_cli_only_install_takes_home_claude_json_first() {
        let dir = tempfile::tempdir().unwrap();
        let home = claude_json(&dir.path().join("home.json"), Some("h"), "home@example.com");
        let config = claude_json(
            &dir.path().join("config.json"),
            Some("c"),
            "config@example.com",
        );
        assert_eq!(
            signed_in_as(stock_claude_status(
                &[home, config],
                &dir.path().join("no-gui")
            ))
            .as_deref(),
            Some("home@example.com")
        );
    }

    #[test]
    fn a_profile_signed_in_only_through_its_desktop_app_is_unknown_not_signed_out() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("cli-config");
        claude_json(&config_dir.join(".claude.json"), None, "");
        let gui = desktop(&dir.path().join("gui"), Some("a"), Some(true));
        assert_eq!(claude_status(&config_dir, &gui), AccountStatus::Unknown);

        let signed_out = desktop(&dir.path().join("gui-out"), Some("a"), Some(false));
        assert_eq!(
            claude_status(&config_dir, &signed_out),
            AccountStatus::SignedOut
        );
    }

    #[test]
    fn a_profile_names_its_cli_account() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("cli-config");
        claude_json(
            &config_dir.join(".claude.json"),
            Some("a"),
            "ada@example.com",
        );
        assert_eq!(
            signed_in_as(claude_status(&config_dir, &dir.path().join("no-gui"))).as_deref(),
            Some("ada@example.com")
        );
    }

    #[test]
    fn the_status_reads_as_a_tagged_object() {
        assert_eq!(
            serde_json::to_value(AccountStatus::Unknown).unwrap(),
            json!({"status": "unknown"})
        );
        assert_eq!(
            serde_json::to_value(AccountStatus::SignedIn {
                account: ProfileAccount {
                    email: Some("ada@example.com".into()),
                    ..ProfileAccount::default()
                }
            })
            .unwrap(),
            json!({"status": "signedIn", "account": {"email": "ada@example.com", "name": null, "organization": null, "plan": null}})
        );
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
}
