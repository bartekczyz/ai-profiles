//! Which claude.ai account a Claude Code config directory is signed in under,
//! from the `oauthAccount` block Claude keeps in `.claude.json`. Only what
//! names the account is read; tokens never are.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The account a profile is signed in under. Every field is optional: the
/// files differ per app and per sign-in age, and a missing name is better
/// than a wrong one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    pub fn is_empty(&self) -> bool {
        *self == ProfileAccount::default()
    }
}

/// Pure: the account in a parsed `.claude.json`, if it names one.
pub fn account_from_claude_json(document: &Value) -> Option<ProfileAccount> {
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

/// A plan token from either app as a label: `claude_max` → "Max",
/// `prolite` → "Prolite", `plus` → "Plus".
pub fn pretty_plan(plan: &str) -> String {
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

    #[test]
    fn plans_read_as_labels() {
        assert_eq!(pretty_plan("claude_max"), "Max");
        assert_eq!(pretty_plan("claude_pro"), "Pro");
        assert_eq!(pretty_plan("plus"), "Plus");
        assert_eq!(pretty_plan("prolite"), "Prolite");
        assert_eq!(pretty_plan("  team  "), "Team");
    }
}
