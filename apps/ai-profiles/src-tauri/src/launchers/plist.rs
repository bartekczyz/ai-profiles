use plist::{Dictionary, Value};

use crate::error::{AppError, AppResult};
use crate::profiles::Profile;

/// Build the Info.plist contents for a profile's launcher .app bundle.
///
/// `version` is typically `env!("CARGO_PKG_VERSION")`.
pub fn info_plist(profile: &Profile, version: &str) -> AppResult<Vec<u8>> {
    let spec = profile.app.spec();
    let display_name = format!("{} ({})", spec.display_name, profile.name);
    let identifier = format!(
        "app.ai-profiles.{}.profile.{}",
        profile.app.as_str(),
        profile.id
    );

    let mut dict = Dictionary::new();
    dict.insert("CFBundleName".into(), Value::String(display_name.clone()));
    dict.insert("CFBundleDisplayName".into(), Value::String(display_name));
    dict.insert("CFBundleIdentifier".into(), Value::String(identifier));
    dict.insert(
        "CFBundleExecutable".into(),
        Value::String("launcher".into()),
    );
    dict.insert("CFBundleIconFile".into(), Value::String("AppIcon".into()));
    dict.insert("CFBundlePackageType".into(), Value::String("APPL".into()));
    dict.insert(
        "CFBundleShortVersionString".into(),
        Value::String(version.to_string()),
    );
    dict.insert("CFBundleVersion".into(), Value::String(version.to_string()));
    dict.insert(
        "LSMinimumSystemVersion".into(),
        Value::String("12.0".into()),
    );
    dict.insert("NSHighResolutionCapable".into(), Value::Boolean(true));
    // Don't hide the Dock icon — the launcher exits quickly after exec-ing the
    // real Claude.app, so a fleeting tile is acceptable. Set explicitly to
    // make the intent unambiguous.
    dict.insert("LSUIElement".into(), Value::Boolean(false));

    let mut bytes = Vec::new();
    plist::to_writer_xml(&mut bytes, &Value::Dictionary(dict))
        .map_err(|err| AppError::Validation(format!("plist serialisation failed: {err}")))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::Surfaces;

    fn fixture(name: &str) -> Profile {
        Profile {
            id: "11111111-1111-1111-1111-111111111111".into(),
            app: crate::app_kind::AppKind::Claude,
            name: name.into(),
            slug: "test".into(),
            color: "#7C3AED".into(),
            created_at: "2026-05-20T12:00:00Z".into(),
            surfaces: Surfaces {
                gui: true,
                cli: false,
            },
            last_used_at: None,
        }
    }

    #[test]
    fn plist_contains_required_keys() {
        let body = info_plist(&fixture("Personal"), "0.1.0").unwrap();
        let xml = String::from_utf8(body).unwrap();
        assert!(xml.contains("<key>CFBundleName</key>"));
        assert!(xml.contains("<key>CFBundleDisplayName</key>"));
        assert!(xml.contains("<key>CFBundleIdentifier</key>"));
        assert!(xml.contains("<key>CFBundleExecutable</key>"));
        assert!(xml.contains("<key>CFBundleIconFile</key>"));
        assert!(xml.contains("<key>CFBundlePackageType</key>"));
        assert!(xml.contains("<key>CFBundleShortVersionString</key>"));
        assert!(xml.contains("<key>LSMinimumSystemVersion</key>"));
        assert!(xml.contains("<key>NSHighResolutionCapable</key>"));
    }

    #[test]
    fn plist_display_name_uses_app_display_name() {
        let xml = String::from_utf8(info_plist(&fixture("Personal"), "0.1.0").unwrap()).unwrap();
        assert!(xml.contains("<string>Claude (Personal)</string>"));
    }

    #[test]
    fn plist_identifier_includes_app_segment() {
        let xml = String::from_utf8(info_plist(&fixture("Personal"), "0.1.0").unwrap()).unwrap();
        assert!(xml.contains(
            "<string>app.ai-profiles.claude.profile.11111111-1111-1111-1111-111111111111</string>"
        ));
    }

    #[test]
    fn plist_for_codex_profile_uses_codex_display_and_id() {
        let mut profile = fixture("Work");
        profile.app = crate::app_kind::AppKind::Codex;
        let xml = String::from_utf8(info_plist(&profile, "0.1.0").unwrap()).unwrap();
        assert!(xml.contains("<string>Codex (Work)</string>"));
        assert!(xml.contains("app.ai-profiles.codex.profile."));
    }

    #[test]
    fn plist_escapes_xml_unsafe_chars_in_name() {
        let body = info_plist(&fixture("Acme & Co"), "0.1.0").unwrap();
        let xml = String::from_utf8(body).unwrap();
        assert!(xml.contains("<string>Claude (Acme &amp; Co)</string>"));
        assert!(!xml.contains("Acme & Co"));
    }

    #[test]
    fn plist_uses_supplied_version_string() {
        let body = info_plist(&fixture("Personal"), "1.2.3-beta").unwrap();
        let xml = String::from_utf8(body).unwrap();
        assert!(xml.contains("<string>1.2.3-beta</string>"));
    }
}
