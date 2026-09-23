//! Per-profile signed copies: how a Claude profile gets a Dock icon of its own.
//!
//! macOS takes a running app's Dock icon from the bundle it runs from, and a
//! Finder custom icon on that bundle counts. So an untouched copy of the vendor
//! app, with the profile's icon set on it the way Get Info sets one, shows that
//! icon in the Dock, and its folder name (`Claude (Work)`) as its label.
//!
//! None of the files the vendor signed change: a custom icon is an `Icon\r`
//! file at the top of the bundle plus a Finder-info attribute on it, and the
//! signature covers neither. The copy therefore keeps the vendor's signature,
//! where a [wrapper](super::wrapper) is re-signed ad hoc and loses whatever the
//! vendor's own services check it for. Claude's Cowork is one: it won't attach
//! a folder for a wrapper. Sign-in, permissions and passkeys carry over too,
//! because to macOS the copy is the vendor's app.
//!
//! macOS's App Management protection keeps other apps from writing inside a
//! bundle signed by someone else, so ai-profiles never does once the copy is an
//! app: the icon is set while the clone is still staged under a hidden name,
//! and a copy that has to go is moved to the Trash, which only renames it. The
//! copy updates itself the way the stock app does, and an update replaces the
//! bundle and the icon with it. A copy that has lost its icon that way, or that
//! carries the icon of a color the profile no longer has, is replaced by a
//! fresh clone (see [`ensure`]) rather than written into: of itself, if it is
//! newer than the stock app, which a profile's copy often is, since the copy
//! updates whenever the profile runs and the stock app only when it does. The launcher hands such
//! a copy to ai-profiles to replace on the way, as a stale wrapper does.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::launch::process_list;
use crate::launchers::{plist, wrapper};
use crate::paths::profile_dir;
use crate::profiles::Profile;

/// Directory in the profile's folder that holds its copy. Spotlight skips a
/// `.noindex` directory, so it doesn't offer the copy next to the launcher:
/// opened directly, the copy would start without the profile's data dir.
pub const DIR: &str = "app.noindex";

/// The Finder custom icon of a bundle, at its top level.
const CUSTOM_ICON_FILE: &str = "Icon\r";

/// File beside the copy recording the color its icon was made in.
pub const ICON_KEY_FILE: &str = ".icon";

/// Prefix of the hidden name a clone is assembled under.
const STAGING_PREFIX: &str = ".building-";

/// Where `profile_id`'s copy lives.
pub fn dir(profile_id: &str) -> AppResult<PathBuf> {
    Ok(profile_dir(profile_id)?.join(DIR))
}

/// Where `profile`'s copy belongs under its current name: `Claude (Work).app`,
/// which is the label the Dock shows for it.
pub fn path(profile: &Profile) -> AppResult<PathBuf> {
    Ok(dir(&profile.id)?.join(format!("{}.app", plist::display_name(profile))))
}

/// Make sure `profile` has a copy of `vendor` carrying `icon` (`.icns` bytes,
/// in the profile's color), and return where it is.
///
/// A copy that is there with the icon is kept as it is. One with an icon of
/// another color, or none because an update took it, is replaced by a fresh
/// clone of whichever of `vendor` and itself is newer, so an update it has
/// taken is kept. A running copy is never replaced: it is kept until it has
/// quit. One left under an earlier
/// name of the profile is renamed, unless it is running, in which case it is
/// used as it is until the next build.
pub fn ensure(profile: &Profile, vendor: &Path, icon: &[u8]) -> AppResult<PathBuf> {
    let target = path(profile)?;
    let dir = dir(&profile.id)?;
    fs::create_dir_all(&dir)?;
    clear_staging(&dir);

    let existing = bundles_in(&dir)?;
    let processes = if existing.is_empty() {
        String::new()
    } else {
        process_list()?
    };
    for other in existing.iter().filter(|bundle| **bundle != target) {
        if running(&processes, other) {
            return Ok(other.clone());
        }
        if target.exists() {
            trash(other)?;
        } else {
            fs::rename(other, &target)?;
        }
    }

    if !target.exists() {
        clone(vendor, &dir, &target, icon)?;
    } else if stale(profile, &target) && !running(&processes, &target) {
        let newer = !not_older(
            wrapper::bundle_version(vendor).as_deref(),
            wrapper::bundle_version(&target).as_deref(),
        );
        // Cloned under another name first: the source may be the copy itself.
        let fresh = dir.join(format!("{STAGING_PREFIX}{}.app", std::process::id()));
        clone(if newer { &target } else { vendor }, &dir, &fresh, icon)?;
        trash(&target)?;
        fs::rename(&fresh, &target)?;
    } else {
        return Ok(target);
    }
    fs::write(dir.join(ICON_KEY_FILE), &profile.color)?;
    Ok(target)
}

/// Whether `profile` asks for a signed copy that [`ensure`] would make or
/// replace once it isn't running: there is none under its name, or the one
/// there is [stale](stale).
pub fn wanting(profile: &Profile) -> bool {
    let Ok(copy) = path(profile) else {
        return false;
    };
    !copy.exists() || stale(profile, &copy)
}

/// The profile's copy, under whatever name it has, if it has one.
pub fn find(profile_id: &str) -> Option<PathBuf> {
    bundles_in(&dir(profile_id).ok()?).ok()?.into_iter().next()
}

/// Whether `copy` should give way to a fresh clone: an update took its icon,
/// or the icon is of another color than `profile`'s.
fn stale(profile: &Profile, copy: &Path) -> bool {
    let key = copy
        .parent()
        .and_then(|dir| fs::read_to_string(dir.join(ICON_KEY_FILE)).ok());
    !has_icon(copy) || key.as_deref() != Some(profile.color.as_str())
}

/// Move `profile_id`'s copy to the Trash, unless it is running; then it is
/// left for the next build to clear away. For a profile that no longer asks
/// for a Dock icon of its own: the copy is disk it no longer needs.
pub fn remove(profile_id: &str) -> AppResult<()> {
    let dir = dir(profile_id)?;
    if !dir.exists() {
        return Ok(());
    }
    let processes = process_list()?;
    if bundles_in(&dir)?
        .iter()
        .any(|bundle| running(&processes, bundle))
    {
        return Ok(());
    }
    discard(profile_id)
}

/// Move `profile_id`'s copy to the Trash, running or not, and remove what
/// else is in its directory. For a profile that is being deleted, whose folder
/// can't otherwise go: nothing but the vendor may delete inside the copy.
pub fn discard(profile_id: &str) -> AppResult<()> {
    let dir = dir(profile_id)?;
    if !dir.exists() {
        return Ok(());
    }
    for bundle in bundles_in(&dir)? {
        trash(&bundle)?;
    }
    fs::remove_dir_all(&dir)?;
    Ok(())
}

/// Whether the bundle at `bundle` has a Finder custom icon.
fn has_icon(bundle: &Path) -> bool {
    bundle.join(CUSTOM_ICON_FILE).exists()
}

/// Pure: whether a vendor at version `vendor` is at least as new as a copy at
/// `copy`, and so the one to clone from. Unreadable versions say yes when the
/// copy's is the unreadable one, and no otherwise.
fn not_older(vendor: Option<&str>, copy: Option<&str>) -> bool {
    let parts = |version: &str| -> Vec<u64> {
        version
            .split('.')
            .map(|part| part.trim().parse().unwrap_or(0))
            .collect()
    };
    match (vendor, copy) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(vendor), Some(copy)) => parts(vendor) >= parts(copy),
    }
}

/// Clone `source` to `target` with `icon` set on it. The clone is assembled
/// under a hidden name in `dir`, so an interrupted clone never passes for a
/// copy, and so the icon goes on while macOS doesn't yet take it for an app it
/// guards.
fn clone(source: &Path, dir: &Path, target: &Path, icon: &[u8]) -> AppResult<()> {
    let executable = executable(source)?;
    let token = std::process::id();
    let staged = dir.join(format!("{STAGING_PREFIX}{token}"));
    let icns = dir.join(format!("{STAGING_PREFIX}{token}.icns"));
    if staged.exists() {
        fs::remove_dir_all(&staged)?;
    }
    let cloned = wrapper::clone_bundle(source, &staged, &executable)
        .and_then(|()| fs::write(&icns, icon).map_err(AppError::Io))
        .and_then(|()| set_icon(&staged, &icns))
        .and_then(|()| fs::rename(&staged, target).map_err(AppError::Io));
    let _ = fs::remove_file(&icns);
    if cloned.is_err() {
        let _ = fs::remove_dir_all(&staged);
    }
    cloned
}

/// Set the `.icns` at `icns` as the Finder custom icon of `bundle`.
fn set_icon(bundle: &Path, icns: &Path) -> AppResult<()> {
    use objc2::rc::autoreleasepool;
    use objc2::AnyThread;
    use objc2_app_kit::{NSImage, NSWorkspace, NSWorkspaceIconCreationOptions};
    use objc2_foundation::NSString;

    let (bundle_text, icns_text) = (utf8(bundle)?, utf8(icns)?);
    // AppKit hands back autoreleased objects, and a worker thread has no pool of
    // its own to drain them into.
    let set = autoreleasepool(|_| {
        let Some(image) =
            NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(icns_text))
        else {
            return false;
        };
        NSWorkspace::sharedWorkspace().setIcon_forFile_options(
            Some(&image),
            &NSString::from_str(bundle_text),
            NSWorkspaceIconCreationOptions(0),
        )
    });
    if set && has_icon(bundle) {
        Ok(())
    } else {
        Err(AppError::Io(std::io::Error::other(format!(
            "macOS wouldn't set the profile's icon on {}",
            bundle.display()
        ))))
    }
}

/// Move `path` to the Trash. `NSFileManager` does it by renaming, which App
/// Management allows where deleting what is inside a guarded app is not, and
/// without asking to control Finder.
fn trash(path: &Path) -> AppResult<()> {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};

    let mut context = trash::TrashContext::default();
    context.set_delete_method(DeleteMethod::NsFileManager);
    context.delete(path).map_err(|err| {
        AppError::Validation(format!("failed to move {} to Trash: {err}", path.display()))
    })
}

/// The vendor app's `CFBundleExecutable`.
fn executable(vendor: &Path) -> AppResult<String> {
    ::plist::Value::from_file(vendor.join("Contents/Info.plist"))
        .ok()
        .and_then(::plist::Value::into_dictionary)
        .and_then(|info| {
            info.get("CFBundleExecutable")
                .and_then(::plist::Value::as_string)
                .map(str::to_owned)
        })
        .ok_or_else(|| {
            AppError::Validation(format!(
                "{} has no readable CFBundleExecutable",
                vendor.display()
            ))
        })
}

/// Best effort: remove what an interrupted clone left in `dir`.
fn clear_staging(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(STAGING_PREFIX)
        {
            let path = entry.path();
            let _ = fs::remove_dir_all(&path).or_else(|_| fs::remove_file(&path));
        }
    }
}

/// The `.app` bundles directly in `dir`, sorted.
fn bundles_in(dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut bundles: Vec<PathBuf> = fs::read_dir(dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(STAGING_PREFIX))
        })
        .collect();
    bundles.sort();
    Ok(bundles)
}

/// Pure: whether any process in `processes` runs out of `bundle`.
pub fn running(processes: &str, bundle: &Path) -> bool {
    let inside = format!("{}/Contents/", bundle.display());
    processes.lines().any(|line| line.contains(&inside))
}

fn utf8(path: &Path) -> AppResult<&str> {
    path.to_str()
        .ok_or_else(|| AppError::Validation(format!("{} is not valid UTF-8", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_matches_any_process_inside_the_bundle() {
        let bundle = Path::new("/p/app.noindex/Claude (Work).app");
        let processes =
            "  12 /p/app.noindex/Claude (Work).app/Contents/MacOS/Claude --user-data-dir=/d\n";
        assert!(running(processes, bundle));
        let helper = "  13 /p/app.noindex/Claude (Work).app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=gpu\n";
        assert!(running(helper, bundle));
    }

    #[test]
    fn running_ignores_other_bundles_and_the_stock_app() {
        let bundle = Path::new("/p/app.noindex/Claude (Work).app");
        let processes =
            "  12 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir=/d\n  \
                         14 /p/app.noindex/Claude (Work) 2.app/Contents/MacOS/Claude\n";
        assert!(!running(processes, bundle));
    }

    #[test]
    fn bundles_in_lists_only_apps_and_skips_staging() {
        let temp = tempfile::tempdir().unwrap();
        for name in [
            "Claude (B).app",
            "Claude (A).app",
            ".building-1",
            ".building-2.app",
            "notes.txt",
        ] {
            fs::create_dir_all(temp.path().join(name)).unwrap();
        }
        let names: Vec<String> = bundles_in(temp.path())
            .unwrap()
            .iter()
            .map(|bundle| bundle.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["Claude (A).app", "Claude (B).app"]);
    }

    #[test]
    fn clear_staging_removes_only_interrupted_clones() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["Claude (A).app", ".building-7"] {
            fs::create_dir_all(temp.path().join(name)).unwrap();
        }
        clear_staging(temp.path());
        assert!(temp.path().join("Claude (A).app").exists());
        assert!(!temp.path().join(".building-7").exists());
    }

    #[test]
    fn clear_staging_removes_a_left_over_icon_file_too() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(".building-7.icns"), b"").unwrap();
        clear_staging(temp.path());
        assert!(!temp.path().join(".building-7.icns").exists());
    }

    #[test]
    fn a_fresh_clone_replaces_a_copy_only_when_the_vendor_is_not_older() {
        assert!(not_older(Some("1.2.0"), Some("1.2.0")));
        assert!(not_older(Some("1.10.0"), Some("1.9.3")));
        assert!(
            !not_older(Some("1.9.3"), Some("1.10.0")),
            "would undo its update"
        );
        assert!(not_older(Some("2"), Some("1.99")));
    }

    #[test]
    fn unreadable_versions_keep_the_copy_unless_its_own_is_the_unreadable_one() {
        assert!(not_older(Some("1.0"), None));
        assert!(not_older(None, None));
        assert!(!not_older(None, Some("1.0")));
    }

    #[test]
    fn has_icon_looks_for_the_custom_icon_file() {
        let temp = tempfile::tempdir().unwrap();
        assert!(!has_icon(temp.path()));
        fs::write(temp.path().join("Icon\r"), b"").unwrap();
        assert!(has_icon(temp.path()));
    }
}
