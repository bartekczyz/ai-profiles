use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::app_kind::AppSpec;
use crate::error::{AppError, AppResult};
use crate::launchers::wrapper::{self, WrapperRequest};
use crate::launchers::{icons, plist, script};
use crate::paths::{
    cli_config_dir, gui_launcher_path, gui_launcher_path_with_prefix, profile_dir, resolve_gui_app,
    ResolvedGuiApp,
};
use crate::profiles::Profile;

/// Build the launcher .app bundle for `profile` at
/// `/Applications/<App> (<Name>).app/`, in the shape the profile asks for: a
/// script that opens the stock app, or, with `distinct_dock_icon`, a wrapper
/// that is an app in its own right and so gets a Dock tile of its own.
/// Idempotent: if the bundle already exists it's torn down and rebuilt.
/// Returns the path to the generated .app.
pub fn generate(profile: &Profile, version: &str) -> AppResult<PathBuf> {
    let spec = profile.app.spec();
    let resolved_gui_app = resolve_gui_app(spec)
        .ok_or_else(|| AppError::Validation(format!("{} isn't installed", spec.display_name)))?;
    let bundle = gui_launcher_path(&profile.name, spec);

    if profile.distinct_dock_icon {
        build_wrapper(profile, &resolved_gui_app, &bundle)?;
    } else {
        build_script_launcher(profile, version, &resolved_gui_app, &bundle)?;
    }

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

/// The shape every profile has had until now: a tiny bundle whose executable
/// is a script that opens the stock app with the profile's `--user-data-dir`.
fn build_script_launcher(
    profile: &Profile,
    version: &str,
    app: &ResolvedGuiApp,
    bundle: &Path,
) -> AppResult<()> {
    let spec = profile.app.spec();
    if bundle.exists() {
        fs::remove_dir_all(bundle).map_err(|err| {
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

    let script_text =
        script::launcher_script(&profile.id, spec, &app.bundle_path.display().to_string());
    let launcher_path = macos.join("launcher");
    fs::write(&launcher_path, script_text)?;
    let mut perms = fs::metadata(&launcher_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&launcher_path, perms)?;

    let icns_bytes = icons::render_icns(&profile.color, &app.bundle_path)?;
    fs::write(resources.join("AppIcon.icns"), icns_bytes)?;
    Ok(())
}

/// Build `profile`'s wrapper at `bundle`, in place of whatever launcher is
/// there. That launcher is kept until the wrapper has been built and verified,
/// and put back if the build fails, so a failed rebuild never leaves the
/// profile without one.
fn build_wrapper(profile: &Profile, app: &ResolvedGuiApp, bundle: &Path) -> AppResult<()> {
    let spec = profile.app.spec();
    let icon = icons::render_icns(&profile.color, &app.bundle_path)?;
    let user_data_dir = profile_dir(&profile.id)?.join("gui-data");
    let config_home = cli_config_dir(&profile.id)?;

    let parked = park_existing(bundle)?;
    let built = wrapper::build(&WrapperRequest {
        vendor_bundle: &app.bundle_path,
        destination: bundle,
        identifier: &plist::bundle_identifier(profile),
        display_name: &plist::display_name(profile),
        icon: &icon,
        user_data_dir: &user_data_dir,
        config_env: spec
            .gui_auth_via_config_env
            .then_some((spec.cli_config_env, config_home.as_path())),
    });

    match (built, parked) {
        (Ok(()), Some(parked)) => {
            let _ = fs::remove_dir_all(parked);
            Ok(())
        }
        (Ok(()), None) => Ok(()),
        (Err(err), Some(parked)) => {
            let _ = fs::rename(parked, bundle);
            Err(err)
        }
        (Err(err), None) => Err(err),
    }
}

/// Move the launcher at `bundle` aside, under a hidden name, so a new one can
/// take its place while the old one is still around to fall back on. `None` if
/// there is nothing there. Refuses to touch an app that isn't one of ours.
fn park_existing(bundle: &Path) -> AppResult<Option<PathBuf>> {
    if !bundle.exists() {
        return Ok(None);
    }
    ensure_ours(bundle, "replace")?;

    let stem = bundle
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parked = bundle.with_file_name(format!(".{stem}.replaced-{}", std::process::id()));
    if parked.exists() {
        fs::remove_dir_all(&parked)?;
    }
    fs::rename(bundle, &parked)?;
    Ok(Some(parked))
}

/// Remove the launcher bundle for `name` under `spec`'s launcher prefix, if it
/// exists and looks like one we generated (sanity check on the Info.plist
/// contents). No-ops if the bundle doesn't exist. Works for both shapes: a
/// wrapper carries our identifier just as a script launcher does.
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
    ensure_ours(bundle, "delete")?;
    fs::remove_dir_all(bundle)?;
    Ok(())
}

/// `Err` unless `bundle` is a launcher of ours; `action` is what was about to
/// happen to it.
fn ensure_ours(bundle: &Path, action: &str) -> AppResult<()> {
    if is_ours(bundle) {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "{} exists but is not a ai-profiles launcher; refusing to {action}",
        bundle.display()
    )))
}

/// Whether `bundle` is a launcher of ours, going by the identifier in its
/// Info.plist, whether that is XML or binary. A bundle with no Info.plist at
/// all counts, since that is what an interrupted build leaves behind; one whose
/// Info.plist cannot be read does not.
fn is_ours(bundle: &Path) -> bool {
    let plist_path = bundle.join("Contents").join("Info.plist");
    if !plist_path.exists() {
        return true;
    }
    ::plist::Value::from_file(&plist_path)
        .ok()
        .and_then(::plist::Value::into_dictionary)
        .and_then(|info| {
            info.get("CFBundleIdentifier")
                .and_then(::plist::Value::as_string)
                .map(|identifier| identifier.starts_with(plist::IDENTIFIER_PREFIX))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launchers::wrapper::WrapperState;
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
            distinct_dock_icon: false,
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

    const OURS: &str = "app.ai-profiles.claude.profile.deadbeef";
    const VENDOR: &str = "com.anthropic.claudefordesktop";

    /// `<dir>/<name>` as a bundle whose Info.plist carries `identifier`, as XML
    /// or in the binary format Apple's own tools write, plus a `marker` file to
    /// tell it from a fresh one.
    fn bundle_with_identifier(dir: &Path, name: &str, identifier: &str, binary: bool) -> PathBuf {
        let bundle = dir.join(name);
        fs::create_dir_all(bundle.join("Contents")).unwrap();
        let mut info = ::plist::Dictionary::new();
        info.insert(
            "CFBundleIdentifier".into(),
            ::plist::Value::String(identifier.into()),
        );
        let info = ::plist::Value::Dictionary(info);
        let path = bundle.join("Contents/Info.plist");
        if binary {
            info.to_file_binary(path).unwrap();
        } else {
            info.to_file_xml(path).unwrap();
        }
        fs::write(bundle.join("marker"), b"kept").unwrap();
        bundle
    }

    fn hidden_leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.'))
            .collect()
    }

    #[test]
    fn a_bundle_is_ours_by_its_identifier_however_its_plist_is_encoded() {
        let dir = tempfile::tempdir().unwrap();
        for binary in [false, true] {
            let ours = bundle_with_identifier(dir.path(), "Ours.app", OURS, binary);
            assert!(is_ours(&ours), "binary={binary}");
            let theirs = bundle_with_identifier(dir.path(), "Theirs.app", VENDOR, binary);
            assert!(!is_ours(&theirs), "binary={binary}");
        }
    }

    #[test]
    fn a_bundle_with_no_plist_is_ours_but_one_with_an_unreadable_plist_is_not() {
        let dir = tempfile::tempdir().unwrap();

        // What an interrupted build leaves behind.
        let half_built = dir.path().join("HalfBuilt.app");
        fs::create_dir_all(half_built.join("Contents")).unwrap();
        assert!(is_ours(&half_built));

        let garbled = dir.path().join("Garbled.app");
        fs::create_dir_all(garbled.join("Contents")).unwrap();
        fs::write(
            garbled.join("Contents/Info.plist"),
            [0xff, 0xfe, 0x00, 0x12],
        )
        .unwrap();
        assert!(!is_ours(&garbled));
    }

    #[test]
    fn removing_a_launcher_leaves_a_foreign_app_alone() {
        let dir = tempfile::tempdir().unwrap();
        let ours = bundle_with_identifier(dir.path(), "Ours.app", OURS, false);
        let theirs = bundle_with_identifier(dir.path(), "Theirs.app", VENDOR, true);

        remove_bundle_if_ours(&ours).unwrap();
        let refused = remove_bundle_if_ours(&theirs);

        assert!(!ours.exists());
        assert!(matches!(refused, Err(AppError::Validation(_))));
        assert!(theirs.join("marker").exists());
        // And nothing there is fine.
        remove_bundle_if_ours(&dir.path().join("Nothing.app")).unwrap();
    }

    #[test]
    fn a_wrapper_is_removed_like_a_script_launcher() {
        // A wrapper's Info.plist is the vendor's with our identifier put in, and
        // carries plenty of other keys, some of them naming the vendor.
        let dir = tempfile::tempdir().unwrap();
        let wrapper = dir.path().join("Claude (Work).app");
        fs::create_dir_all(wrapper.join("Contents")).unwrap();
        let mut info = ::plist::Dictionary::new();
        info.insert(
            "CFBundleIdentifier".into(),
            plist::bundle_identifier(&fixture()).into(),
        );
        info.insert("CFBundleName".into(), "Claude".into());
        info.insert("CFBundleExecutable".into(), "Claude".into());
        info.insert(
            "SUPublicEDKey".into(),
            "com.anthropic.claudefordesktop".into(),
        );
        ::plist::Value::Dictionary(info)
            .to_file_xml(wrapper.join("Contents/Info.plist"))
            .unwrap();

        remove_bundle_if_ours(&wrapper).unwrap();

        assert!(!wrapper.exists());
    }

    #[test]
    fn parking_moves_our_launcher_aside_and_frees_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = bundle_with_identifier(dir.path(), "Claude (Work).app", OURS, false);

        let parked = park_existing(&bundle).unwrap().expect("something to park");

        assert!(!bundle.exists());
        assert_eq!(fs::read(parked.join("marker")).unwrap(), b"kept");
        let name = parked.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with('.') && !name.ends_with(".app"), "{name}");
    }

    #[test]
    fn parking_refuses_a_foreign_app_and_ignores_an_empty_path() {
        let dir = tempfile::tempdir().unwrap();
        let theirs = bundle_with_identifier(dir.path(), "Theirs.app", VENDOR, false);

        assert!(matches!(
            park_existing(&theirs),
            Err(AppError::Validation(_))
        ));
        assert!(theirs.join("marker").exists());
        assert_eq!(
            park_existing(&dir.path().join("Nothing.app")).unwrap(),
            None
        );
    }

    #[test]
    fn a_failed_wrapper_build_puts_the_previous_launcher_back() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = bundle_with_identifier(dir.path(), "Claude (Work).app", OURS, false);
        // A vendor that isn't there makes the build fail after the old launcher
        // has been moved aside.
        let missing = ResolvedGuiApp {
            bundle_path: dir.path().join("NoVendor.app"),
            macos_exec: "Claude",
        };

        let result = build_wrapper(&fixture(), &missing, &bundle);

        assert!(result.is_err());
        assert_eq!(fs::read(bundle.join("marker")).unwrap(), b"kept");
        assert_eq!(hidden_leftovers(dir.path()), Vec::<String>::new());
    }

    /// Opt-in: builds real wrappers under /Applications from the installed
    /// Claude and checks that toggling swaps the launcher's shape and that a
    /// rebuild brings a wrapper from an older vendor version up to date. Gated
    /// behind AI_PROFILES_E2E=1 because it writes to /Applications and signs a
    /// gigabyte or so.
    #[test]
    fn toggling_the_dock_setting_swaps_the_launcher_shape_and_a_rebuild_catches_up_with_the_vendor()
    {
        if std::env::var("AI_PROFILES_E2E").is_err() {
            eprintln!("skipping; set AI_PROFILES_E2E=1 to run");
            return;
        }
        let mut profile = fixture();
        profile.name = "PhaseFiveTest".into();
        let spec = profile.app.spec();
        let Some(vendor) = resolve_gui_app(spec) else {
            eprintln!("Claude not installed; skipping");
            return;
        };
        let macos = |bundle: &Path, file: &str| bundle.join("Contents/MacOS").join(file);

        profile.distinct_dock_icon = true;
        let bundle = generate(&profile, "0.1.0").unwrap();
        assert!(macos(&bundle, "Claude.bin").is_file(), "a wrapper");
        assert!(!macos(&bundle, "launcher").exists());
        assert_eq!(
            wrapper::built_from_version(&bundle),
            wrapper::bundle_version(&vendor.bundle_path)
        );
        let state = || wrapper::state(Some(&vendor.bundle_path), &bundle);
        assert_eq!(state(), WrapperState::Current);

        profile.distinct_dock_icon = false;
        generate(&profile, "0.1.0").unwrap();
        assert!(macos(&bundle, "launcher").is_file(), "back to the script");
        assert!(!macos(&bundle, "Claude.bin").exists());

        profile.distinct_dock_icon = true;
        generate(&profile, "0.1.0").unwrap();
        assert!(macos(&bundle, "Claude.bin").is_file(), "a wrapper again");

        // Pretend the vendor updated since: the wrapper claims an older build.
        let info_path = bundle.join("Contents/Info.plist");
        let mut info = ::plist::Value::from_file(&info_path)
            .unwrap()
            .into_dictionary()
            .unwrap();
        info.insert("AIProfilesVendorVersion".into(), "0.0.0-older".into());
        ::plist::Value::Dictionary(info)
            .to_file_xml(&info_path)
            .unwrap();

        assert_eq!(state(), WrapperState::Stale);
        generate(&profile, "0.1.0").unwrap();
        assert_eq!(state(), WrapperState::Current, "caught up");

        remove(&profile.name, spec).unwrap();
        assert_eq!(state(), WrapperState::Missing);
        assert!(!bundle.exists());
        let applications = bundle.parent().unwrap();
        let leftovers: Vec<String> = hidden_leftovers(applications)
            .into_iter()
            .filter(|name| name.contains("PhaseFiveTest"))
            .collect();
        assert_eq!(leftovers, Vec::<String>::new());
    }
}
