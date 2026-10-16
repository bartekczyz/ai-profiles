//! A launcher icon the user picked for a profile, in place of the generated one.
//!
//! The launcher's icon is not a file anyone keeps: every build renders it anew
//! (see [`icons::render_icns`]), and builds happen on their own — on an edit,
//! when the vendor app updates, after an upgrade of this app. An icon swapped
//! into the bundle by hand is gone at the next one. So the picked image is kept
//! with the profile's data, and every build of its launcher reads it from there.

use std::fs;
use std::io::{Cursor, ErrorKind};
use std::path::{Path, PathBuf};

use icns::{Image, PixelFormat};
use tauri::ipc::{InvokeBody, Request, Response};

use crate::app_kind::AppKind;
use crate::error::{AppError, AppResult};
use crate::launchers::{gui, icons};
use crate::paths::{profile_dir, resolve_gui_app};
use crate::profiles::{self, Profile};

/// What the picked image is kept as, in the profile's data dir. Deleting the
/// profile's data takes it along.
const FILE_NAME: &str = "custom-icon.png";

/// The smallest side an icon may have: the largest size a launcher's icon comes
/// in. Anything smaller would have to be blown up, and look it.
const MIN_SIDE: u32 = icons::LARGEST_ICON_SIZE;

/// The largest side an icon may have. Four times the largest icon size leaves
/// room for any reasonable source image, while capping the decoded image at
/// 64 MB, as it is held in memory whole while it's cut down.
const MAX_SIDE: u32 = 4096;

/// The largest file accepted. Compressed, a PNG of [`MAX_SIDE`]² is far smaller
/// than this; anything bigger is not a plausible icon.
const MAX_BYTES: usize = 32 * 1024 * 1024;

/// The header [`set_custom_icon`] takes the profile's id from. Its body is the
/// image itself, which travels as raw bytes rather than as a JSON array of
/// numbers, which would be several times its size.
const PROFILE_ID_HEADER: &str = "profile-id";

/// The side of [`generated_icon_preview`]'s image: one of the sizes a launcher's
/// icon comes in, and enough for a Dock tile drawn small at any display scale.
const PREVIEW_SIDE: u32 = 128;

/// The first eight bytes of every PNG file.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// The image profile `id` picked for its launcher, as the PNG it was given, for
/// the UI to show; an empty body when it has none. Sent as it is kept, so one
/// that can no longer be read still says the profile has an icon of its own,
/// which the user can then take away; the launcher uses the generated icon
/// meanwhile.
#[tauri::command(async)]
pub fn custom_icon(id: String) -> AppResult<Response> {
    Ok(Response::new(kept_icon(&id)?.unwrap_or_default()))
}

/// The icon a launcher of `app` gets in `color` without a picked image, as a
/// [`PREVIEW_SIDE`]² PNG: rendered as a build renders it, from the vendor app
/// installed now. Takes the color rather than a profile, so the preview can
/// follow a color picked in the edit form before it is saved.
#[tauri::command(async)]
pub fn generated_icon_preview(app: AppKind, color: String) -> AppResult<Response> {
    // Without the vendor app, the build falls back to a plain square in the
    // color, and so does this.
    let vendor = resolve_gui_app(app.spec()).map(|resolved| resolved.bundle_path);
    let vendor = vendor.as_deref().unwrap_or(Path::new("/nonexistent"));
    Ok(Response::new(icons::render_png(
        &color,
        vendor,
        PREVIEW_SIDE,
    )?))
}

/// What [`custom_icon`] sends: the bytes kept for managed profile `id`, if any.
fn kept_icon(id: &str) -> AppResult<Option<Vec<u8>>> {
    let profile = find(id)?;
    match fs::read(icon_path(&profile.id)?) {
        Ok(png) => Ok(Some(png)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Check that the PNG in the request's body can be a launcher icon, changing
/// nothing: square, and between [`MIN_SIDE`] and [`MAX_SIDE`] pixels a side.
/// One that can't is refused with what is wrong with it. The edit form asks as
/// soon as an image is picked, so it can say so then rather than on save.
#[tauri::command(async)]
pub fn check_custom_icon(request: Request<'_>) -> AppResult<()> {
    decode(request_png(&request)?).map(|_| ())
}

/// Give a profile the icon in the request's body, a PNG, and rebuild its
/// launcher with it. The profile's id comes in the `profile-id` header. The
/// image is checked as [`check_custom_icon`] does, and one that fails it
/// changes nothing.
#[tauri::command(async)]
pub fn set_custom_icon(request: Request<'_>) -> AppResult<()> {
    let id = request
        .headers()
        .get(PROFILE_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::Validation("no profile given for the icon".into()))?;
    let png = request_png(&request)?;
    decode(png)?;
    replace(id, Some(png))
}

/// The PNG a request carries as its raw body.
fn request_png<'a>(request: &'a Request<'_>) -> AppResult<&'a [u8]> {
    match request.body() {
        InvokeBody::Raw(png) => Ok(png),
        InvokeBody::Json(_) => Err(AppError::Validation(
            "the icon has to be sent as the PNG file's bytes".into(),
        )),
    }
}

/// Take a profile's own icon away, so its launcher gets the generated one again.
#[tauri::command(async)]
pub fn clear_custom_icon(id: String) -> AppResult<()> {
    replace(&id, None)
}

/// The icon for `profile`'s launcher, as `.icns` bytes: the one the user picked
/// if there is one that can be read, and otherwise the vendor's icon with the
/// profile's badge.
///
/// A picked image that has gone missing or bad never fails the build. The
/// launcher is what opens the profile, so it gets the generated icon instead.
pub fn launcher_icns(profile: &Profile, vendor_bundle: &Path) -> AppResult<Vec<u8>> {
    match custom_icns(&profile.id) {
        Some(icns) => Ok(icns),
        None => icons::render_icns(&profile.color, vendor_bundle),
    }
}

/// The `.icns` rendered from profile `id`'s picked image. `None` when there is
/// none, or it can't be read; the latter is logged, as nothing else will say so.
fn custom_icns(id: &str) -> Option<Vec<u8>> {
    let path = icon_path(id).ok()?;
    let rendered = match fs::read(&path) {
        Err(err) if err.kind() == ErrorKind::NotFound => return None,
        Err(err) => Err(AppError::Io(err)),
        Ok(png) => decode(&png).and_then(|icon| icons::render_plain_icns(&icon.rgba, icon.side)),
    };
    rendered
        .inspect_err(|err| {
            eprintln!(
                "ai-profiles: using the generated icon, as {} can't be used: {}",
                path.display(),
                err.message()
            );
        })
        .ok()
}

fn icon_path(id: &str) -> AppResult<PathBuf> {
    Ok(profile_dir(id)?.join(FILE_NAME))
}

/// The managed profile `id`. The stock install isn't one: its icon is the
/// vendor's own, not ours to change.
fn find(id: &str) -> AppResult<Profile> {
    profiles::load()?
        .into_iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| AppError::NotFound(format!("profile {id} not found")))
}

/// Keep `png` as profile `id`'s icon, or with `None` drop the one it has, and
/// rebuild the profile's launcher to match.
///
/// Refused while the profile runs from its wrapper, as every rebuild is (see
/// `ensure_wrapper_not_running`), and done under the store lock, so a rename
/// running at the same time can't have the launcher built under the old name.
fn replace(id: &str, png: Option<&[u8]>) -> AppResult<()> {
    crate::commands::ensure_wrapper_not_running(id)?;
    let _store = profiles::lock_store();
    let profile = find(id)?;
    swap_and_rebuild(&icon_path(&profile.id)?, png, || {
        if profile.surfaces.gui {
            gui::generate(&profile, env!("CARGO_PKG_VERSION"))?;
        }
        Ok(())
    })
}

/// Put `png` at `path` (or remove the file, for `None`), then `rebuild`. If the
/// rebuild fails, the file is put back as it was, so a change that failed
/// leaves the profile with the icon it had: the next build of its launcher,
/// which a failed one leaves to the next edit or launch, uses that.
fn swap_and_rebuild(
    path: &Path,
    png: Option<&[u8]>,
    rebuild: impl FnOnce() -> AppResult<()>,
) -> AppResult<()> {
    let previous = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };
    write_or_remove(path, png)?;
    if let Err(err) = rebuild() {
        let _ = write_or_remove(path, previous.as_deref());
        return Err(err);
    }
    Ok(())
}

/// Write `png` to `path` through a temporary file, so a build never reads half
/// an image, or remove `path` for `None`. Removing what isn't there is fine.
fn write_or_remove(path: &Path, png: Option<&[u8]>) -> AppResult<()> {
    let Some(png) = png else {
        return match fs::remove_file(path) {
            Err(err) if err.kind() != ErrorKind::NotFound => Err(err.into()),
            _ => Ok(()),
        };
    };
    let parent = path
        .parent()
        .ok_or_else(|| AppError::NotFound(format!("{} has no parent", path.display())))?;
    fs::create_dir_all(parent)?;
    let staged = parent.join(format!(".{FILE_NAME}.tmp"));
    fs::write(&staged, png)?;
    fs::rename(&staged, path)?;
    Ok(())
}

/// A picked image, decoded to straight-alpha RGBA.
struct CustomIcon {
    /// Side length in pixels; the image is square.
    side: u32,
    /// `side * side * 4` bytes, row-major.
    rgba: Vec<u8>,
}

/// Decode `png` into a [`CustomIcon`], or say in words the user can act on why
/// it can't be one. The size is checked from the PNG's header before anything
/// is decoded, so an image far too large is turned away without being unpacked.
fn decode(png: &[u8]) -> AppResult<CustomIcon> {
    if png.len() > MAX_BYTES {
        return Err(AppError::Validation(format!(
            "The image is {} MB. Pick a PNG of at most {} MB.",
            png.len().div_ceil(1024 * 1024),
            MAX_BYTES / (1024 * 1024)
        )));
    }
    let (width, height) = png_dimensions(png)
        .ok_or_else(|| AppError::Validation("That file isn't a PNG image.".into()))?;
    check_dimensions(width, height)?;
    let image = Image::read_png(Cursor::new(png))
        .map_err(|err| AppError::Validation(format!("The PNG image can't be read: {err}")))?;
    Ok(CustomIcon {
        side: width,
        rgba: image.convert_to(PixelFormat::RGBA).into_data().into_vec(),
    })
}

/// Width and height from a PNG's header. `None` if `png` doesn't start like a
/// PNG: the signature, then the `IHDR` chunk, which must come first.
fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.get(..8)? != PNG_SIGNATURE || png.get(12..16)? != b"IHDR" {
        return None;
    }
    let read = |at: usize| Some(u32::from_be_bytes(png.get(at..at + 4)?.try_into().ok()?));
    Some((read(16)?, read(20)?))
}

fn check_dimensions(width: u32, height: u32) -> AppResult<()> {
    let problem = if width != height {
        "The icon has to be square".to_owned()
    } else if width < MIN_SIDE {
        format!("The icon has to be at least {MIN_SIDE} × {MIN_SIDE} pixels")
    } else if width > MAX_SIDE {
        format!("The icon can be at most {MAX_SIDE} × {MAX_SIDE} pixels")
    } else {
        return Ok(());
    };
    Err(AppError::Validation(format!(
        "{problem}, and this image is {width} × {height}."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_kind::AppKind;
    use crate::profiles::Surfaces;
    use crate::test_support::APP_DIR_TEST_LOCK;

    const PIXEL: [u8; 4] = [30, 144, 255, 255];

    /// A PNG of `width` × `height` in one color.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = Image::from_data(
            PixelFormat::RGBA,
            width,
            height,
            PIXEL.repeat((width * height) as usize),
        )
        .unwrap();
        let mut bytes = Vec::new();
        image.write_png(&mut bytes).unwrap();
        bytes
    }

    fn validation_message(result: AppResult<CustomIcon>) -> String {
        match result {
            Err(AppError::Validation(message)) => message,
            Err(other) => panic!("not a validation error: {other:?}"),
            Ok(icon) => panic!("accepted a {0}×{0} icon", icon.side),
        }
    }

    #[test]
    fn a_square_png_of_a_good_size_decodes() {
        for side in [MIN_SIDE, 2048] {
            let icon = decode(&png(side, side)).unwrap();
            assert_eq!(icon.side, side);
            assert_eq!(icon.rgba.len(), (side * side * 4) as usize);
            assert_eq!(icon.rgba[..4], PIXEL);
        }
    }

    #[test]
    fn an_image_that_is_not_square_is_refused_with_its_size() {
        let message = validation_message(decode(&png(1200, 1024)));
        assert!(message.contains("square"), "{message}");
        assert!(message.contains("1200 × 1024"), "{message}");
    }

    #[test]
    fn an_image_smaller_than_the_largest_icon_size_is_refused() {
        let message = validation_message(decode(&png(512, 512)));
        assert!(message.contains("at least 1024 × 1024"), "{message}");
        assert!(message.contains("512 × 512"), "{message}");
    }

    #[test]
    fn an_image_too_large_is_refused_from_its_header_alone() {
        // Only the start of a PNG of 8192²: refused before any of it is decoded.
        let header = &png(16, 16)[..24];
        let mut large = header.to_vec();
        large[16..20].copy_from_slice(&8192u32.to_be_bytes());
        large[20..24].copy_from_slice(&8192u32.to_be_bytes());

        let message = validation_message(decode(&large));
        assert!(message.contains("at most 4096 × 4096"), "{message}");
    }

    #[test]
    fn a_file_too_large_is_refused_before_it_is_looked_at() {
        let message = validation_message(decode(&vec![0; MAX_BYTES + 1]));
        assert!(message.contains("at most 32 MB"), "{message}");
    }

    #[test]
    fn a_file_that_is_not_a_png_is_refused() {
        for bytes in [&b""[..], b"GIF89a", &[0xff, 0xd8, 0xff, 0xe0, 0, 0x10]] {
            let message = validation_message(decode(bytes));
            assert_eq!(message, "That file isn't a PNG image.");
        }
    }

    #[test]
    fn a_png_cut_short_is_refused() {
        let bytes = png(MIN_SIDE, MIN_SIDE);
        let message = validation_message(decode(&bytes[..bytes.len() / 2]));
        assert!(
            message.starts_with("The PNG image can't be read"),
            "{message}"
        );
    }

    #[test]
    fn the_size_is_read_from_the_header() {
        assert_eq!(png_dimensions(&png(1024, 768)), Some((1024, 768)));
        assert_eq!(png_dimensions(&PNG_SIGNATURE), None, "no IHDR");
    }

    fn profile(id: &str) -> Profile {
        Profile {
            id: id.into(),
            app: AppKind::Claude,
            name: "Icon".into(),
            slug: "icon".into(),
            color: "#7C3AED".into(),
            created_at: "2026-05-20T12:00:00Z".into(),
            // No launchers, so nothing is built outside the test's data dir.
            surfaces: Surfaces {
                gui: false,
                cli: false,
            },
            distinct_dock_icon: false,
            last_used_at: None,
        }
    }

    #[test]
    fn the_launcher_uses_the_picked_icon_and_then_the_generated_one_again() {
        let _guard = APP_DIR_TEST_LOCK.lock().unwrap();
        let id = "custom-icon-roundtrip";
        profiles::save_all(&[profile(id)]).unwrap();
        // No vendor app here, so the generated icon is the plain colored square.
        let vendor = Path::new("/nonexistent/Vendor.app");
        let generated = icons::render_icns("#7C3AED", vendor).unwrap();

        assert_eq!(kept_icon(id).unwrap(), None);
        replace(id, Some(&png(MIN_SIDE, MIN_SIDE))).unwrap();
        assert_eq!(kept_icon(id).unwrap(), Some(png(MIN_SIDE, MIN_SIDE)));
        let picked = launcher_icns(&profile(id), vendor).unwrap();
        assert_ne!(picked, generated);
        assert_eq!(
            picked,
            icons::render_plain_icns(&PIXEL.repeat((MIN_SIDE * MIN_SIDE) as usize), MIN_SIDE)
                .unwrap()
        );

        clear_custom_icon(id.into()).unwrap();
        assert_eq!(kept_icon(id).unwrap(), None);
        assert_eq!(launcher_icns(&profile(id), vendor).unwrap(), generated);
        // Taking away an icon that isn't there is fine.
        clear_custom_icon(id.into()).unwrap();
    }

    #[test]
    fn an_unreadable_icon_falls_back_to_the_generated_one() {
        let _guard = APP_DIR_TEST_LOCK.lock().unwrap();
        let id = "custom-icon-unreadable";
        let path = icon_path(id).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let vendor = Path::new("/nonexistent/Vendor.app");
        let generated = icons::render_icns("#7C3AED", vendor).unwrap();

        for bad in [b"not a png".to_vec(), png(64, 64)] {
            fs::write(&path, bad).unwrap();
            assert_eq!(launcher_icns(&profile(id), vendor).unwrap(), generated);
        }
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn only_a_managed_profile_has_an_icon_to_change() {
        let _guard = APP_DIR_TEST_LOCK.lock().unwrap();
        profiles::save_all(&[profile("custom-icon-known")]).unwrap();
        for id in ["default:claude", "../../elsewhere"] {
            assert!(matches!(kept_icon(id), Err(AppError::NotFound(_))));
            assert!(matches!(
                replace(id, Some(&png(MIN_SIDE, MIN_SIDE))),
                Err(AppError::NotFound(_))
            ));
        }
    }

    /// Opt-in: builds a real launcher of each shape under /Applications from the
    /// installed Claude, so it's gated behind AI_PROFILES_E2E=1 like the tests in
    /// `gui`. Both shapes carry the picked icon, and the generated one again
    /// once it is taken away.
    #[test]
    fn both_launcher_shapes_carry_the_picked_icon() {
        if std::env::var("AI_PROFILES_E2E").is_err() {
            eprintln!("skipping; set AI_PROFILES_E2E=1 to run");
            return;
        }
        let _guard = APP_DIR_TEST_LOCK.lock().unwrap();
        let mut profile = Profile {
            id: "deadbeef-0000-0000-0000-00000000c1c0".into(),
            name: "CustomIconTest".into(),
            slug: "customicontest".into(),
            ..profile("")
        };
        profile.surfaces.gui = true;
        let spec = profile.app.spec();
        let Some(vendor) = crate::paths::resolve_gui_app(spec) else {
            eprintln!("Claude not installed; skipping");
            return;
        };
        let picked =
            icons::render_plain_icns(&PIXEL.repeat((MIN_SIDE * MIN_SIDE) as usize), MIN_SIDE)
                .unwrap();
        let icon_of = |bundle: &Path| fs::read(bundle.join("Contents/Resources/AppIcon.icns")).ok();

        for distinct_dock_icon in [false, true] {
            profile.distinct_dock_icon = distinct_dock_icon;
            profiles::save_all(std::slice::from_ref(&profile)).unwrap();
            let bundle = gui::generate(&profile, "0.1.0").unwrap();
            let generated = icon_of(&bundle);
            assert_ne!(generated.as_deref(), Some(picked.as_slice()));

            replace(&profile.id, Some(&png(MIN_SIDE, MIN_SIDE))).unwrap();
            assert_eq!(
                icon_of(&bundle).as_deref(),
                Some(picked.as_slice()),
                "distinct_dock_icon={distinct_dock_icon}"
            );

            replace(&profile.id, None).unwrap();
            assert_eq!(
                icon_of(&bundle),
                Some(icons::render_icns(&profile.color, &vendor.bundle_path).unwrap()),
                "distinct_dock_icon={distinct_dock_icon}"
            );
            gui::remove(&profile.name, spec).unwrap();
        }
    }

    #[test]
    fn a_failed_rebuild_puts_the_previous_icon_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let failing = || Err(AppError::Validation("Claude isn't installed".into()));

        // Nothing there before: nothing there after.
        assert!(swap_and_rebuild(&path, Some(b"new"), failing).is_err());
        assert!(!path.exists());

        // An icon there before: the same icon after, whether replacing or clearing.
        fs::write(&path, b"old").unwrap();
        assert!(swap_and_rebuild(&path, Some(b"new"), failing).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert!(swap_and_rebuild(&path, None, failing).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");

        swap_and_rebuild(&path, Some(b"new"), || Ok(())).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        let leftovers: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no staged file left behind");
    }
}
