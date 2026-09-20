//! The entitlements a wrapper is signed with: the vendor's, minus what an
//! ad-hoc signature cannot carry.
//!
//! An ad-hoc signature has no team identifier, and the kernel refuses to launch
//! a binary that has none but claims entitlements scoped to a team
//! (`Launchd job spawn failed`, POSIX 163). `codesign` itself accepts them, so
//! nothing short of launching the app reveals the mistake.

use plist::{Dictionary, Value};

/// Entitlements only a binary signed by the vendor's team can hold, whatever
/// their values.
const TEAM_SCOPED_KEYS: [&str; 6] = [
    "com.apple.application-identifier",
    "com.apple.developer.team-identifier",
    "keychain-access-groups",
    "com.apple.security.application-groups",
    "com.apple.developer.aps-environment",
    "com.apple.developer.associated-domains",
];

/// Entitlement that lets the wrapper's ad-hoc executable load the vendor's
/// frameworks. Hardened runtime implies library validation, which rejects a
/// framework signed by a different team than the process (`dyld: … different
/// Team IDs`), and the vendor frameworks always are.
pub const DISABLE_LIBRARY_VALIDATION: &str = "com.apple.security.cs.disable-library-validation";

/// `entitlements` as the wrapper may carry them: without the team-scoped keys,
/// without any other value that names the vendor's team (`<team>.…`), and with
/// library validation disabled.
///
/// `team_id` is `None` for a vendor with no team (an ad-hoc signed build), in
/// which case only the fixed list of team-scoped keys is removed.
pub fn strip_team_entitlements(entitlements: &Dictionary, team_id: Option<&str>) -> Dictionary {
    let prefix = team_id.map(|team| format!("{team}."));
    let mut stripped: Dictionary = entitlements
        .iter()
        .filter(|(key, _)| !TEAM_SCOPED_KEYS.contains(&key.as_str()))
        .filter_map(|(key, value)| match &prefix {
            Some(prefix) => without_team_strings(value, prefix).map(|value| (key.clone(), value)),
            None => Some((key.clone(), value.clone())),
        })
        .collect();
    stripped.insert(DISABLE_LIBRARY_VALIDATION.to_owned(), Value::Boolean(true));
    stripped
}

/// The keys of `entitlements` that [`strip_team_entitlements`] would remove or
/// change. Empty means nothing team-scoped is left, which is what a finished
/// wrapper must show.
pub fn team_scoped_entitlements(entitlements: &Dictionary, team_id: Option<&str>) -> Vec<String> {
    let prefix = team_id.map(|team| format!("{team}."));
    entitlements
        .iter()
        .filter(|(key, value)| {
            TEAM_SCOPED_KEYS.contains(&key.as_str())
                || prefix.as_ref().is_some_and(|prefix| {
                    without_team_strings(value, prefix).as_ref() != Some(*value)
                })
        })
        .map(|(key, _)| key.clone())
        .collect()
}

/// The team a vendor's entitlements claim to belong to, for when its signature
/// does not report one.
pub fn claimed_team_id(entitlements: &Dictionary) -> Option<String> {
    entitlements
        .get("com.apple.developer.team-identifier")
        .and_then(Value::as_string)
        .filter(|team| !team.is_empty())
        .map(str::to_owned)
}

/// `value` with every string starting with `prefix` dropped, or `None` if that
/// leaves nothing of it: a matching string, or a container that had contents
/// and now has none.
fn without_team_strings(value: &Value, prefix: &str) -> Option<Value> {
    match value {
        Value::String(text) => (!text.starts_with(prefix)).then(|| value.clone()),
        Value::Array(items) => {
            let kept: Vec<Value> = items
                .iter()
                .filter_map(|item| without_team_strings(item, prefix))
                .collect();
            (items.is_empty() || !kept.is_empty()).then(|| Value::Array(kept))
        }
        Value::Dictionary(entries) => {
            let kept: Dictionary = entries
                .iter()
                .filter_map(|(key, item)| {
                    without_team_strings(item, prefix).map(|item| (key.clone(), item))
                })
                .collect();
            (entries.is_empty() || !kept.is_empty()).then(|| Value::Dictionary(kept))
        }
        _ => Some(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const CLAUDE: &str = include_str!("fixtures/claude-entitlements.plist");
    const CLAUDE_TEAM: &str = "Q6L2SF6YDW";
    const CHATGPT: &str = include_str!("fixtures/chatgpt-entitlements.plist");
    const CHATGPT_TEAM: &str = "2DC432GLL2";

    fn parse(xml: &str) -> Dictionary {
        Value::from_reader(Cursor::new(xml.as_bytes()))
            .unwrap()
            .into_dictionary()
            .unwrap()
    }

    fn strings(values: &[&str]) -> Value {
        Value::Array(
            values
                .iter()
                .map(|value| Value::String((*value).to_owned()))
                .collect(),
        )
    }

    #[test]
    fn strip_removes_every_team_scoped_key_from_the_real_apps() {
        for (name, xml, team) in [
            ("Claude", CLAUDE, CLAUDE_TEAM),
            ("ChatGPT", CHATGPT, CHATGPT_TEAM),
        ] {
            let vendor = parse(xml);
            assert!(
                !team_scoped_entitlements(&vendor, Some(team)).is_empty(),
                "{name}: fixture should start out team-scoped"
            );

            let stripped = strip_team_entitlements(&vendor, Some(team));

            assert_eq!(
                team_scoped_entitlements(&stripped, Some(team)),
                Vec::<String>::new(),
                "{name}"
            );
            for key in TEAM_SCOPED_KEYS {
                assert!(!stripped.contains_key(key), "{name}: {key} survived");
            }
        }
    }

    #[test]
    fn strip_leaves_no_string_naming_the_team_anywhere() {
        for (xml, team) in [(CLAUDE, CLAUDE_TEAM), (CHATGPT, CHATGPT_TEAM)] {
            let mut serialized = Vec::new();
            Value::Dictionary(strip_team_entitlements(&parse(xml), Some(team)))
                .to_writer_xml(&mut serialized)
                .unwrap();
            assert!(!String::from_utf8(serialized).unwrap().contains(team));
        }
    }

    #[test]
    fn strip_keeps_the_entitlements_the_apps_need_to_run() {
        let claude = strip_team_entitlements(&parse(CLAUDE), Some(CLAUDE_TEAM));
        for key in [
            "com.apple.security.cs.allow-jit",
            "com.apple.security.device.camera",
            "com.apple.security.virtualization",
            "com.apple.security.automation.apple-events",
        ] {
            assert_eq!(
                claude.get(key),
                Some(&Value::Boolean(true)),
                "Claude: {key}"
            );
        }

        let chatgpt = strip_team_entitlements(&parse(CHATGPT), Some(CHATGPT_TEAM));
        for key in [
            "com.apple.security.cs.allow-jit",
            "com.apple.security.cs.allow-unsigned-executable-memory",
            "com.apple.security.network.client",
            "com.apple.security.files.user-selected.read-write",
        ] {
            assert_eq!(
                chatgpt.get(key),
                Some(&Value::Boolean(true)),
                "ChatGPT: {key}"
            );
        }
        // An explicit `false` is still a statement worth keeping.
        assert_eq!(
            chatgpt.get("com.apple.security.app-sandbox"),
            Some(&Value::Boolean(false))
        );
    }

    #[test]
    fn strip_disables_library_validation() {
        for (xml, team) in [(CLAUDE, CLAUDE_TEAM), (CHATGPT, CHATGPT_TEAM)] {
            let stripped = strip_team_entitlements(&parse(xml), Some(team));
            assert_eq!(
                stripped.get(DISABLE_LIBRARY_VALIDATION),
                Some(&Value::Boolean(true))
            );
        }
    }

    #[test]
    fn strip_removes_team_prefixed_values_under_any_key() {
        let mut vendor = Dictionary::new();
        vendor.insert(
            "com.example.string".into(),
            Value::String("TEAM.one".into()),
        );
        vendor.insert(
            "com.example.mixed".into(),
            strings(&["TEAM.one", "other.two"]),
        );
        vendor.insert(
            "com.example.all-team".into(),
            strings(&["TEAM.one", "TEAM.two"]),
        );
        vendor.insert("com.example.empty".into(), Value::Array(Vec::new()));
        vendor.insert("com.example.flag".into(), Value::Boolean(true));
        let mut nested = Dictionary::new();
        nested.insert("inner".into(), strings(&["TEAM.one", "other.two"]));
        vendor.insert("com.example.nested".into(), Value::Dictionary(nested));

        let stripped = strip_team_entitlements(&vendor, Some("TEAM"));

        assert!(!stripped.contains_key("com.example.string"));
        assert_eq!(
            stripped.get("com.example.mixed"),
            Some(&strings(&["other.two"]))
        );
        assert!(!stripped.contains_key("com.example.all-team"));
        assert_eq!(
            stripped.get("com.example.empty"),
            Some(&Value::Array(Vec::new())),
            "an array that was already empty is not the team's doing"
        );
        assert_eq!(
            stripped.get("com.example.flag"),
            Some(&Value::Boolean(true))
        );
        let inner = stripped
            .get("com.example.nested")
            .and_then(Value::as_dictionary)
            .and_then(|nested| nested.get("inner"));
        assert_eq!(inner, Some(&strings(&["other.two"])));
    }

    #[test]
    fn strip_does_not_treat_a_lookalike_prefix_as_the_team() {
        let mut vendor = Dictionary::new();
        // Same characters, but not `TEAM.` followed by more.
        vendor.insert("com.example.a".into(), Value::String("TEAMS.one".into()));
        vendor.insert("com.example.b".into(), Value::String("TEAM".into()));

        let stripped = strip_team_entitlements(&vendor, Some("TEAM"));

        assert!(stripped.contains_key("com.example.a"));
        assert!(stripped.contains_key("com.example.b"));
    }

    #[test]
    fn strip_without_a_team_only_removes_the_fixed_keys() {
        let mut vendor = Dictionary::new();
        vendor.insert(
            "com.apple.application-identifier".into(),
            Value::String("TEAM.app".into()),
        );
        vendor.insert(
            "com.example.string".into(),
            Value::String("TEAM.one".into()),
        );

        let stripped = strip_team_entitlements(&vendor, None);

        assert!(!stripped.contains_key("com.apple.application-identifier"));
        assert!(stripped.contains_key("com.example.string"));
    }

    #[test]
    fn team_scoped_entitlements_names_the_keys_that_would_be_stripped() {
        let vendor = parse(CHATGPT);

        let mut found = team_scoped_entitlements(&vendor, Some(CHATGPT_TEAM));
        found.sort();

        assert_eq!(
            found,
            [
                "com.apple.application-identifier",
                "com.apple.developer.aps-environment",
                "com.apple.developer.team-identifier",
                "com.apple.security.application-groups",
                "keychain-access-groups",
            ]
        );
    }

    #[test]
    fn claimed_team_id_reads_the_team_identifier_entitlement() {
        assert_eq!(
            claimed_team_id(&parse(CLAUDE)).as_deref(),
            Some(CLAUDE_TEAM)
        );
        assert_eq!(
            claimed_team_id(&parse(CHATGPT)).as_deref(),
            Some(CHATGPT_TEAM)
        );
        assert_eq!(claimed_team_id(&Dictionary::new()), None);
    }
}
