use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::app_kind::AppSpec;
use crate::error::{AppError, AppResult};
use crate::launchers::{icons, plist, script};
use crate::paths::{gui_launcher_path, gui_launcher_path_with_prefix};
use crate::profiles::Profile;

/// Build the .app bundle for `profile` at `/Applications/<App> (<Name>).app/`.
/// Idempotent: if the bundle already exists it's torn down and rebuilt.
/// Returns the path to the generated .app.
pub fn generate(profile: &Profile, version: &str) -> AppResult<PathBuf> {
    let spec = profile.app.spec();
    let resolved_gui_app = crate::paths::resolve_gui_app(spec)
        .ok_or_else(|| AppError::Validation(format!("{} isn't installed", spec.display_name)))?;
    let bundle = gui_launcher_path(&profile.name, spec);
    if bundle.exists() {
        fs::remove_dir_all(&bundle).map_err(|err| {
            AppError::Io(std::io::Error::new(
                err.kind(),
                format!(
                    "failed to clear existing bundle {}: {err}",
                    bundle.display()
                ),
            ))
        })?;
    }

    let contents = bundle.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    fs::create_dir_all(&macos)?;
    fs::create_dir_all(&resources)?;

    let plist_bytes = plist::info_plist(profile, version)?;
    fs::write(contents.join("Info.plist"), plist_bytes)?;

    let script_text = script::launcher_script(
        &profile.id,
        spec,
        &resolved_gui_app.bundle_path.display().to_string(),
    );
    let launcher_path = macos.join("launcher");
    fs::write(&launcher_path, script_text)?;
    let mut perms = fs::metadata(&launcher_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&launcher_path, perms)?;

    let icns_bytes = icons::render_icns(&profile.color, &resolved_gui_app.bundle_path)?;
    fs::write(resources.join("AppIcon.icns"), icns_bytes)?;

    // Best-effort: clean up a bundle generated under a prefix this app used
    // before a rename (e.g. Codex's launcher_prefix moving from "Codex" to
    // "ChatGPT"), so profiles created before the rename don't end up with a
    // stale, orphaned launcher sitting alongside the freshly regenerated one.
    // Never blocks profile creation/edit on failure.
    for legacy_prefix in spec.legacy_launcher_prefixes {
        let legacy_bundle = gui_launcher_path_with_prefix(&profile.name, legacy_prefix);
        if legacy_bundle != bundle {
            let _ = remove_bundle_if_ours(&legacy_bundle);
        }
    }

    Ok(bundle)
}

/// Remove the launcher bundle for `name` under `spec`'s launcher prefix, if it
/// exists and looks like one we generated (sanity check on the Info.plist
/// contents). No-ops if the bundle doesn't exist.
pub fn remove(name: &str, spec: &AppSpec) -> AppResult<()> {
    remove_bundle_if_ours(&gui_launcher_path(name, spec))
}

/// Shared by [`remove`] and `generate`'s legacy-prefix cleanup: delete
/// `bundle` if it exists and its Info.plist marks it as ours. No-ops if it
/// doesn't exist.
fn remove_bundle_if_ours(bundle: &Path) -> AppResult<()> {
    if !bundle.exists() {
        return Ok(());
    }
    let plist_path = bundle.join("Contents").join("Info.plist");
    if let Ok(body) = fs::read_to_string(&plist_path) {
        // Broadened from "app.ai-profiles.profile." so it matches both the
        // legacy id and the new "app.ai-profiles.<app>.profile." form.
        if !body.contains("app.ai-profiles.") {
            return Err(AppError::Validation(format!(
                "{} exists but is not a ai-profiles launcher; refusing to delete",
                bundle.display()
            )));
        }
    }
    fs::remove_dir_all(bundle)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::Surfaces;

    fn fixture() -> Profile {
        Profile {
            id: "deadbeef-0000-0000-0000-000000000000".into(),
            app: crate::app_kind::AppKind::Claude,
            name: "PhaseTwoTest".into(),
            slug: "phasetwotest".into(),
            color: "#7C3AED".into(),
            created_at: "2026-05-20T12:00:00Z".into(),
            surfaces: Surfaces {
                gui: true,
                cli: false,
            },
            last_used_at: None,
        }
    }

    /// Opt-in end-to-end smoke test. Writes to /Applications, so it's gated
    /// behind AI_PROFILES_E2E=1 — CI / casual `cargo test` runs skip it.
    #[test]
    fn generate_writes_expected_bundle_layout() {
        if std::env::var("AI_PROFILES_E2E").is_err() {
            eprintln!("skipping; set AI_PROFILES_E2E=1 to run");
            return;
        }
        let profile = fixture();
        let path = generate(&profile, "0.1.0").unwrap();
        assert!(path.join("Contents/Info.plist").is_file());
        assert!(path.join("Contents/MacOS/launcher").is_file());
        assert!(path.join("Contents/Resources/AppIcon.icns").is_file());

        let mode = fs::metadata(path.join("Contents/MacOS/launcher"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);

        remove(&profile.name, profile.app.spec()).unwrap();
    }

    /// Opt-in: requires ChatGPT.app installed (so `generate` resolves a GUI
    /// app for ChatGPT) and write access to /Applications. Verifies a bundle
    /// left over from before the Codex launcher_prefix rename ("Codex" ->
    /// "ChatGPT") gets cleaned up automatically on the next `generate`.
    #[test]
    fn generate_removes_a_legacy_prefixed_bundle_for_the_same_profile() {
        if std::env::var("AI_PROFILES_E2E").is_err() {
            eprintln!("skipping; set AI_PROFILES_E2E=1 to run");
            return;
        }
        let mut profile = fixture();
        profile.name = "PhaseTwoLegacyTest".into();
        profile.app = crate::app_kind::AppKind::Codex;
        let spec = profile.app.spec();

        // Simulate a pre-rename install: a bundle at the old "Codex (...)"
        // prefix, generated by hand rather than via `generate` (which now
        // only ever writes at the current prefix).
        let legacy_bundle =
            crate::paths::gui_launcher_path_with_prefix("PhaseTwoLegacyTest", "Codex");
        fs::create_dir_all(legacy_bundle.join("Contents")).unwrap();
        let plist_bytes = plist::info_plist(&profile, "0.1.0").unwrap();
        fs::write(legacy_bundle.join("Contents/Info.plist"), plist_bytes).unwrap();
        assert!(legacy_bundle.exists());

        let path = generate(&profile, "0.1.0").unwrap();

        assert!(path.exists());
        assert!(
            !legacy_bundle.exists(),
            "legacy-prefixed bundle should have been cleaned up"
        );

        remove(&profile.name, spec).unwrap();
    }
}
