//! An app's icon as macOS draws it.
//!
//! An app's icon is not only a file. Since macOS 26 the system draws it in the
//! icon style the user has chosen (light, dark, tinted), and puts an icon that
//! predates that on a plate of its own. Asking the system is the only way to get
//! what the Dock shows.

use std::path::Path;
use std::ptr;

use objc2::rc::autoreleasepool;
use objc2::AnyThread;
use objc2_app_kit::{NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext, NSWorkspace};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

/// The icon of the app at `path`, drawn `size`² pixels large: straight-alpha
/// RGBA, row-major. `None` if there is no app at `path` or macOS cannot draw it.
pub fn render(path: &Path, size: u32) -> Option<Vec<u8>> {
    if !path.exists() {
        // The system would answer with a generic document icon.
        return None;
    }
    // AppKit hands back autoreleased objects, and a worker thread has no pool of
    // its own to drain them into.
    autoreleasepool(|_| draw(path, size))
}

fn draw(path: &Path, size: u32) -> Option<Vec<u8>> {
    let side = isize::try_from(size).ok()?;
    let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path.to_str()?));

    // SAFETY: a null `planes` has AppKit allocate the pixel storage, and the rest
    // describes an ordinary 8-bit RGBA bitmap of `side`² pixels.
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            ptr::null_mut(),
            side,
            side,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )
    }?;
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;

    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    image.drawInRect(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(f64::from(size), f64::from(size)),
    ));
    NSGraphicsContext::restoreGraphicsState_class();

    let row_bytes = usize::try_from(bitmap.bytesPerRow()).ok()?;
    let data = bitmap.bitmapData();
    if data.is_null() {
        return None;
    }
    // SAFETY: the bitmap owns `bytesPerRow` bytes for each of its `size` rows, and
    // is kept alive by `bitmap` until after the copy.
    let rows = unsafe { std::slice::from_raw_parts(data, row_bytes * size as usize) };
    Some(unpremultiplied(rows, row_bytes, size))
}

/// Straight-alpha RGBA from `rows` of premultiplied pixels, `row_bytes` bytes to
/// a row, of which the first `size * 4` are pixels and the rest padding.
fn unpremultiplied(rows: &[u8], row_bytes: usize, size: u32) -> Vec<u8> {
    let width = size as usize * 4;
    let mut pixels = Vec::with_capacity(width * size as usize);
    for row in rows.chunks_exact(row_bytes).take(size as usize) {
        for pixel in row[..width].chunks_exact(4) {
            let alpha = u32::from(pixel[3]);
            if alpha == 0 {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            for &channel in &pixel[..3] {
                pixels.push(((u32::from(channel) * 255 + alpha / 2) / alpha).min(255) as u8);
            }
            pixels.push(pixel[3]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpremultiplying_drops_the_padding_at_the_end_of_each_row() {
        // Two rows of two opaque pixels, each row padded to 12 bytes.
        let rows = [
            [10u8, 20, 30, 255, 40, 50, 60, 255, 9, 9, 9, 9],
            [70, 80, 90, 255, 100, 110, 120, 255, 9, 9, 9, 9],
        ]
        .concat();

        assert_eq!(
            unpremultiplied(&rows, 12, 2),
            vec![10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255]
        );
    }

    #[test]
    fn unpremultiplying_restores_the_color_a_partly_transparent_pixel_lost() {
        // Half-transparent orange, as drawn into a premultiplied bitmap.
        let premultiplied = [100u8, 50, 0, 128];
        assert_eq!(
            unpremultiplied(&premultiplied, 4, 1),
            vec![199, 100, 0, 128]
        );
    }

    #[test]
    fn unpremultiplying_leaves_nothing_but_zeros_where_nothing_is_drawn() {
        assert_eq!(unpremultiplied(&[0, 0, 0, 0], 4, 1), vec![0, 0, 0, 0]);
    }

    #[test]
    fn a_path_with_no_app_has_no_icon_to_draw() {
        // The system would answer with a generic document icon.
        assert!(render(Path::new("/nonexistent/Vendor.app"), 64).is_none());
    }

    /// Opt-in: asks macOS to draw the icon of an app every Mac has, so it needs
    /// a window server. Gated behind AI_PROFILES_E2E=1 like the other tests that
    /// need the real system.
    #[test]
    fn draws_an_installed_apps_icon_at_the_size_asked_for() {
        if std::env::var("AI_PROFILES_E2E").is_err() {
            eprintln!("skipping; set AI_PROFILES_E2E=1 to run");
            return;
        }
        let side = 256;

        let pixels =
            render(Path::new("/System/Applications/Calculator.app"), side).expect("macOS draws it");

        assert_eq!(pixels.len(), (side * side * 4) as usize);
        let alpha = |x: u32, y: u32| pixels[((y * side + x) * 4 + 3) as usize];
        assert_eq!(alpha(side / 2, side / 2), 255, "the middle is opaque");
        assert_eq!(alpha(0, 0), 0, "the corner is empty");
    }
}
