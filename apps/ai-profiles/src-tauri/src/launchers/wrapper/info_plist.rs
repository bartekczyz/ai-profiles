//! The wrapper's `Info.plist`: the vendor's, patched.

use plist::{Dictionary, Value};
use profile_shim::{
    CONFIG_ENV_NAME_KEY, CONFIG_ENV_VALUE_KEY, HOST_BINARY_KEY, PROFILE_ID_KEY, USER_DATA_DIR_KEY,
    VENDOR_BUNDLE_KEY,
};

use crate::error::{AppError, AppResult};

/// Records the vendor `CFBundleVersion` a wrapper was cloned from, so a later
/// vendor update can be told from an up-to-date wrapper by comparing the two.
/// Defined with the shim's other keys, because the shim reads it too.
pub use profile_shim::VENDOR_VERSION_KEY;

/// Records the ai-profiles version that built the wrapper.
///
/// A wrapper carries a copy of the shim and whatever else this app puts in one,
/// so an upgrade that changes either has to reach the wrappers already on disk.
/// The vendor's version keys cannot say this — they belong to the vendor, and
/// the app shows them in its About box — so this is kept separately.
pub const BUILT_BY_KEY: &str = "AIProfilesBuiltBy";

/// What makes one wrapper's `Info.plist` differ from the vendor's.
pub struct Patch<'a> {
    /// `CFBundleIdentifier`, unique per profile so LaunchServices and the Dock
    /// see each wrapper as its own app.
    pub identifier: &'a str,
    /// `CFBundleDisplayName`, which is the Dock label.
    pub display_name: &'a str,
    /// Name, without extension, of the `.icns` in `Contents/Resources` that is
    /// the wrapper's icon.
    pub icon_file: &'a str,
    /// The profile's `--user-data-dir`, which the shim adds at launch.
    pub user_data_dir: &'a str,
    /// The profile's id, which the shim passes to [`Patch::host_binary`] to
    /// have this wrapper rebuilt once it has fallen behind the vendor app.
    pub profile_id: &'a str,
    /// The vendor `.app` being cloned, which the shim reads the installed
    /// version out of to notice that it has.
    pub vendor_bundle: &'a str,
    /// The ai-profiles executable the shim hands a launch back to. A wrapper
    /// cannot rebuild itself: the rebuild replaces the bundle the shim is
    /// running from.
    pub host_binary: &'a str,
    /// The ai-profiles version doing the building, so an upgrade that changes
    /// what a wrapper contains reaches the ones already on disk.
    pub built_by: &'a str,
    /// `(name, value)` env var the shim sets before starting the vendor binary.
    pub config_env: Option<(&'a str, &'a str)>,
}

/// The vendor's `Info.plist` turned into the wrapper's.
///
/// `CFBundleName` is deliberately left alone: Electron finds its helper apps by
/// it and dies with "Unable to find helper app" if it changes.
pub fn patch(vendor: &Dictionary, patch: &Patch<'_>) -> AppResult<Dictionary> {
    let vendor_version = vendor
        .get("CFBundleVersion")
        .and_then(Value::as_string)
        .ok_or_else(|| AppError::Validation("the vendor Info.plist has no CFBundleVersion".into()))?
        .to_owned();

    let mut info: Dictionary = vendor
        .iter()
        .filter(|(key, _)| !dropped_from_wrapper(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();

    info.insert("CFBundleIdentifier".into(), string(patch.identifier));
    info.insert("CFBundleDisplayName".into(), string(patch.display_name));
    // With `CFBundleIconName` present macOS 26 reads the icon from the vendor's
    // `Assets.car` and ignores `CFBundleIconFile` entirely, which is why the
    // name key is dropped above.
    info.insert("CFBundleIconFile".into(), string(patch.icon_file));

    info.insert(USER_DATA_DIR_KEY.into(), string(patch.user_data_dir));
    if let Some((name, value)) = patch.config_env {
        info.insert(CONFIG_ENV_NAME_KEY.into(), string(name));
        info.insert(CONFIG_ENV_VALUE_KEY.into(), string(value));
    }
    info.insert(VENDOR_VERSION_KEY.into(), Value::String(vendor_version));

    // What the shim needs to have this wrapper rebuilt when the version above
    // stops matching the vendor's. Written together: the shim only hands a
    // launch back when it has all three.
    info.insert(PROFILE_ID_KEY.into(), string(patch.profile_id));
    info.insert(VENDOR_BUNDLE_KEY.into(), string(patch.vendor_bundle));
    info.insert(HOST_BINARY_KEY.into(), string(patch.host_binary));
    info.insert(BUILT_BY_KEY.into(), string(patch.built_by));

    // A Sparkle app would otherwise go looking for updates to the wrapper and
    // replace it with the vendor's own bundle.
    if vendor.keys().any(|key| key.starts_with("SU")) {
        info.insert("SUEnableAutomaticChecks".into(), Value::Boolean(false));
    }
    Ok(info)
}

/// Keys the wrapper must not inherit. The URL and document types would make
/// every wrapper register the vendor's schemes and file types with
/// LaunchServices (ChatGPT claims `http` and `https`, so N wrappers would be N
/// default-browser candidates), and the icon name is explained in [`patch`].
fn dropped_from_wrapper(key: &str) -> bool {
    matches!(
        key,
        "CFBundleIconName" | "CFBundleURLTypes" | "CFBundleDocumentTypes"
    ) || (key.starts_with("UT") && key.ends_with("TypeDeclarations"))
}

fn string(text: &str) -> Value {
    Value::String(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(fields: &[(&str, &str)]) -> Value {
        let mut dictionary = Dictionary::new();
        for (key, value) in fields {
            dictionary.insert((*key).to_owned(), string(value));
        }
        Value::Dictionary(dictionary)
    }

    /// A vendor `Info.plist` with the keys the patch cares about, shaped like
    /// the real Electron apps'.
    fn vendor() -> Dictionary {
        let mut info = Dictionary::new();
        for (key, value) in [
            ("CFBundleName", "Claude"),
            ("CFBundleDisplayName", "Claude"),
            ("CFBundleIdentifier", "com.anthropic.claudefordesktop"),
            ("CFBundleExecutable", "Claude"),
            ("CFBundleIconFile", "electron.icns"),
            ("CFBundleIconName", "Claude"),
            ("CFBundleVersion", "2.2553.1"),
            ("CFBundleShortVersionString", "2.2553.1"),
        ] {
            info.insert(key.to_owned(), string(value));
        }
        info.insert(
            "CFBundleURLTypes".into(),
            Value::Array(vec![entry(&[("CFBundleURLName", "Claude")])]),
        );
        info.insert(
            "CFBundleDocumentTypes".into(),
            Value::Array(vec![entry(&[("CFBundleTypeName", "Text")])]),
        );
        info.insert(
            "UTExportedTypeDeclarations".into(),
            Value::Array(Vec::new()),
        );
        info.insert(
            "UTImportedTypeDeclarations".into(),
            Value::Array(Vec::new()),
        );
        info.insert("LSEnvironment".into(), entry(&[("MallocNanoZone", "0")]));
        info.insert(
            "ElectronAsarIntegrity".into(),
            entry(&[("Resources/app.asar", "abc")]),
        );
        info
    }

    fn request<'a>() -> Patch<'a> {
        Patch {
            identifier: "app.ai-profiles.claude.profile.1",
            display_name: "Claude (Work)",
            icon_file: "AppIcon",
            user_data_dir: "/data/gui-data",
            profile_id: "1",
            vendor_bundle: "/Applications/Claude.app",
            host_binary: "/Applications/ai-profiles.app/Contents/MacOS/ai-profiles",
            built_by: "1.3.0",
            config_env: None,
        }
    }

    #[test]
    fn patch_gives_the_wrapper_its_own_identity() {
        let info = patch(&vendor(), &request()).unwrap();

        assert_eq!(
            info.get("CFBundleIdentifier"),
            Some(&string("app.ai-profiles.claude.profile.1"))
        );
        assert_eq!(
            info.get("CFBundleDisplayName"),
            Some(&string("Claude (Work)"))
        );
    }

    #[test]
    fn patch_never_changes_the_bundle_name() {
        let info = patch(&vendor(), &request()).unwrap();

        assert_eq!(info.get("CFBundleName"), Some(&string("Claude")));
    }

    #[test]
    fn patch_replaces_the_icon_name_with_an_icon_file() {
        let info = patch(&vendor(), &request()).unwrap();

        assert!(!info.contains_key("CFBundleIconName"));
        assert_eq!(info.get("CFBundleIconFile"), Some(&string("AppIcon")));
    }

    #[test]
    fn patch_drops_the_url_document_and_uti_declarations() {
        let info = patch(&vendor(), &request()).unwrap();

        for key in [
            "CFBundleURLTypes",
            "CFBundleDocumentTypes",
            "UTExportedTypeDeclarations",
            "UTImportedTypeDeclarations",
        ] {
            assert!(!info.contains_key(key), "{key} survived");
        }
    }

    #[test]
    fn patch_keeps_what_the_vendor_binary_relies_on() {
        let info = patch(&vendor(), &request()).unwrap();

        for key in [
            "CFBundleExecutable",
            "CFBundleVersion",
            "CFBundleShortVersionString",
            "LSEnvironment",
            "ElectronAsarIntegrity",
        ] {
            assert_eq!(info.get(key), vendor().get(key), "{key} changed");
        }
    }

    #[test]
    fn patch_hands_the_shim_its_parameters() {
        let info = patch(&vendor(), &request()).unwrap();

        assert_eq!(info.get(USER_DATA_DIR_KEY), Some(&string("/data/gui-data")));
        assert!(!info.contains_key(CONFIG_ENV_NAME_KEY));
        assert!(!info.contains_key(CONFIG_ENV_VALUE_KEY));

        let with_env = Patch {
            config_env: Some(("CODEX_HOME", "/data/cli-config")),
            ..request()
        };
        let info = patch(&vendor(), &with_env).unwrap();

        assert_eq!(info.get(CONFIG_ENV_NAME_KEY), Some(&string("CODEX_HOME")));
        assert_eq!(
            info.get(CONFIG_ENV_VALUE_KEY),
            Some(&string("/data/cli-config"))
        );
    }

    #[test]
    fn patch_records_the_vendor_version_it_was_cloned_from() {
        let info = patch(&vendor(), &request()).unwrap();

        assert_eq!(info.get(VENDOR_VERSION_KEY), Some(&string("2.2553.1")));
    }

    #[test]
    fn patch_records_what_the_shim_needs_to_ask_for_a_rebuild() {
        let info = patch(&vendor(), &request()).unwrap();

        assert_eq!(info.get(PROFILE_ID_KEY), Some(&string("1")));
        assert_eq!(
            info.get(VENDOR_BUNDLE_KEY),
            Some(&string("/Applications/Claude.app"))
        );
        assert_eq!(
            info.get(HOST_BINARY_KEY),
            Some(&string(
                "/Applications/ai-profiles.app/Contents/MacOS/ai-profiles"
            ))
        );
        assert_eq!(info.get(BUILT_BY_KEY), Some(&string("1.3.0")));
    }

    #[test]
    fn patch_fails_without_a_vendor_version() {
        let mut vendor = vendor();
        vendor.remove("CFBundleVersion");

        assert!(patch(&vendor, &request()).is_err());
    }

    #[test]
    fn patch_turns_off_sparkle_only_where_the_vendor_has_it() {
        let claude = patch(&vendor(), &request()).unwrap();
        assert!(!claude.contains_key("SUEnableAutomaticChecks"));

        let mut sparkle = vendor();
        sparkle.insert("SUPublicEDKey".into(), string("key"));
        let chatgpt = patch(&sparkle, &request()).unwrap();
        assert_eq!(
            chatgpt.get("SUEnableAutomaticChecks"),
            Some(&Value::Boolean(false))
        );
    }

    #[test]
    fn patched_plist_serialises_as_xml_naming_the_ai_profiles_app() {
        // The launcher cleanup decides whether a bundle is ours by reading the
        // Info.plist as text, so it has to be XML and carry our identifier.
        let info = patch(&vendor(), &request()).unwrap();
        let mut bytes = Vec::new();
        Value::Dictionary(info).to_writer_xml(&mut bytes).unwrap();

        let xml = String::from_utf8(bytes).unwrap();
        assert!(xml.contains("app.ai-profiles."));
    }
}
