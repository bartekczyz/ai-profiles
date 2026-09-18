use std::cmp::Reverse;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

use icns::{IconFamily, Image, PixelFormat};

use crate::error::{AppError, AppResult};

/// Standard Apple icon sizes. The icns crate auto-picks the matching IconType
/// for each size, including the 1024×1024 case (which becomes the @2x of 512).
const ICON_SIZES: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];

/// The size every other size is cut from.
const LARGEST_ICON_SIZE: u32 = ICON_SIZES[ICON_SIZES.len() - 1];

/// Color of the ring around the badge, so the badge reads against any artwork.
const BADGE_RING_COLOR: (u8, u8, u8) = (255, 255, 255);

/// How opaque a pixel has to be to count as part of an icon's tile. The shadow
/// around a tile is far fainter.
const TILE_ALPHA_THRESHOLD: u8 = 128;

/// The badge's outer diameter, as a fraction of the tile's side.
const BADGE_DIAMETER: f32 = 0.36;

/// How far the badge's center is from the tile's right and bottom edges, as a
/// fraction of the tile's side. With [`BADGE_DIAMETER`] it keeps the badge clear
/// of the tile's rounded corner, and of the antialiased edge at the small sizes.
const BADGE_INSET: f32 = 0.26;

/// The badge ring's thickness, as a fraction of the badge's diameter.
const BADGE_RING: f32 = 0.13;

/// A rectangle of pixels on a canvas, in pixel units.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Bounds {
    /// The shorter of the two sides.
    fn side(&self) -> f32 {
        (self.right - self.left).min(self.bottom - self.top)
    }

    /// The same rectangle on a canvas `factor` times as large.
    fn scaled(&self, factor: f32) -> Bounds {
        Bounds {
            left: self.left * factor,
            top: self.top * factor,
            right: self.right * factor,
            bottom: self.bottom * factor,
        }
    }
}

/// A vendor icon decoded to straight-alpha RGBA.
struct Artwork {
    /// Side length in pixels; the image is square.
    size: u32,
    /// `size * size * 4` bytes, row-major, straight (not premultiplied) alpha.
    rgba: Vec<u8>,
    /// Where the icon's tile is: the opaque rounded square, without the margin
    /// and shadow that surround it.
    tile: Bounds,
}

impl Artwork {
    /// `None` when nothing in `rgba` is opaque, so there is no tile to badge.
    fn new(size: u32, rgba: Vec<u8>) -> Option<Artwork> {
        let tile = opaque_bounds(&rgba, size)?;
        Some(Artwork { size, rgba, tile })
    }
}

/// Where the profile badge sits on a square canvas. All lengths are in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BadgeGeometry {
    /// Center of the badge.
    center: (f32, f32),
    /// Outer diameter, ring included.
    diameter: f32,
    /// Thickness of the ring around the colored disc.
    ring_width: f32,
}

/// Render a complete `.icns` for a profile's launcher at all sizes in
/// `ICON_SIZES`: the vendor app's own artwork with a badge in the profile
/// color, so the launcher still reads as the app while telling profiles apart.
/// The artwork is the app's icon as macOS draws it now, so the badged copy looks
/// like the app does in the Dock, and only if the system can't say, the icon
/// file the app ships.
///
/// Falls back to a plain rounded square in the profile color when the vendor
/// icon can't be read, so an unexpected vendor bundle never blocks creating a
/// profile. Returns the binary bytes.
pub fn render_icns(color_hex: &str, vendor_bundle: &Path) -> AppResult<Vec<u8>> {
    render_icns_from(
        color_hex,
        system_artwork(vendor_bundle).or_else(|| classic_artwork(vendor_bundle)),
    )
}

/// [`render_icns`] for artwork that has already been chosen.
fn render_icns_from(color_hex: &str, artwork: Option<Artwork>) -> AppResult<Vec<u8>> {
    let color = parse_hex_color(color_hex)?;
    // Every size is cut from one master. Artwork that can't supply the largest
    // size would leave the big sizes soft or missing, so use the plain square
    // rather than mixing sources.
    let artwork = artwork.filter(|artwork| artwork.size >= LARGEST_ICON_SIZE);

    let mut family = IconFamily::new();
    for &size in &ICON_SIZES {
        let pixels = match &artwork {
            Some(artwork) => badged_artwork(artwork, size, color),
            None => render_rounded_square(size, color),
        };
        let image = Image::from_data(PixelFormat::RGBA, size, size, pixels)
            .map_err(|err| AppError::Validation(format!("image build failed: {err}")))?;
        family
            .add_icon(&image)
            .map_err(|err| AppError::Validation(format!("icns add failed: {err}")))?;
    }
    let mut buffer = Cursor::new(Vec::new());
    family
        .write(&mut buffer)
        .map_err(|err| AppError::Validation(format!("icns write failed: {err}")))?;
    Ok(buffer.into_inner())
}

/// Parse `#RRGGBB` into `(r, g, b)`. Validation already happened upstream in
/// `profiles::create`, but we don't want to trust callers.
pub fn parse_hex_color(hex: &str) -> AppResult<(u8, u8, u8)> {
    if hex.len() != 7 || !hex.starts_with('#') {
        return Err(AppError::Validation(format!(
            "color must be #RRGGBB, got '{hex}'"
        )));
    }
    let parse = |start: usize| {
        u8::from_str_radix(&hex[start..start + 2], 16)
            .map_err(|_| AppError::Validation(format!("invalid hex digits in '{hex}'")))
    };
    Ok((parse(1)?, parse(3)?, parse(5)?))
}

/// The vendor app's icon at its largest size, read from the icon file its own
/// `Info.plist` names. `None` if the bundle has no icon we can decode.
///
/// That file is the icon as the app was first designed. It is not necessarily
/// what macOS shows for the app today: the system draws an app's icon in the
/// user's icon style, which a file cannot follow (see [`system_artwork`]).
fn classic_artwork(bundle: &Path) -> Option<Artwork> {
    let contents = bundle.join("Contents");
    let info = plist::Value::from_file(contents.join("Info.plist")).ok()?;
    let icon_file = info.as_dictionary()?.get("CFBundleIconFile")?.as_string()?;
    let icon_file = if icon_file.ends_with(".icns") {
        icon_file.to_owned()
    } else {
        format!("{icon_file}.icns")
    };
    let file = File::open(contents.join("Resources").join(icon_file)).ok()?;
    let family = IconFamily::read(BufReader::new(file)).ok()?;
    decode_largest(&family)
}

/// The largest square representation in `family` that decodes.
fn decode_largest(family: &IconFamily) -> Option<Artwork> {
    let mut icon_types = family.available_icons();
    icon_types.sort_by_key(|icon_type| Reverse(icon_type.pixel_width()));
    icon_types.into_iter().find_map(|icon_type| {
        let image = family.get_icon_with_type(icon_type).ok()?;
        if image.width() != image.height() {
            return None;
        }
        Artwork::new(
            image.width(),
            image.convert_to(PixelFormat::RGBA).into_data().into_vec(),
        )
    })
}

/// The vendor app's icon as macOS draws it at this moment, in the user's icon
/// style, so a badged copy looks like the app does in the Dock. `None` when the
/// system can't say.
///
/// Whatever style was in effect when the icon was made is the one it keeps.
#[cfg(target_os = "macos")]
fn system_artwork(bundle: &Path) -> Option<Artwork> {
    let rgba = crate::launchers::system_icon::render(bundle, LARGEST_ICON_SIZE)?;
    Artwork::new(LARGEST_ICON_SIZE, rgba)
}

#[cfg(not(target_os = "macos"))]
fn system_artwork(_bundle: &Path) -> Option<Artwork> {
    None
}

/// The smallest rectangle holding every pixel of the `size`² image `rgba` that
/// is opaque enough to be part of the tile. `None` if there is none.
fn opaque_bounds(rgba: &[u8], size: u32) -> Option<Bounds> {
    let size = size as usize;
    let (mut left, mut top, mut right, mut bottom) = (size, size, 0, 0);
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        if pixel[3] >= TILE_ALPHA_THRESHOLD {
            let (x, y) = (index % size, index / size);
            left = left.min(x);
            right = right.max(x + 1);
            top = top.min(y);
            bottom = bottom.max(y + 1);
        }
    }
    (left < right).then_some(Bounds {
        left: left as f32,
        top: top as f32,
        right: right as f32,
        bottom: bottom as f32,
    })
}

/// The artwork scaled to `size`² with the profile badge on top. The badge is
/// drawn at the target size rather than scaled with the artwork, so it keeps a
/// legible ring at the small sizes.
fn badged_artwork(artwork: &Artwork, size: u32, color: (u8, u8, u8)) -> Vec<u8> {
    let mut pixels = downsample(&artwork.rgba, artwork.size, size);
    let tile = artwork.tile.scaled(size as f32 / artwork.size as f32);
    composite_badge(&mut pixels, size, badge_geometry(tile), color);
    pixels
}

/// Box-filter `rgba`, a `src_size`² image, down to `dst_size`².
///
/// Averages in premultiplied alpha: a transparent pixel carries no color, so
/// it must not darken the opaque pixels beside it (the artwork has a
/// transparent margin, and edges would otherwise pick up a dark fringe).
fn downsample(rgba: &[u8], src_size: u32, dst_size: u32) -> Vec<u8> {
    let (src, dst) = (src_size as usize, dst_size as usize);
    debug_assert!(dst > 0 && dst <= src, "can only shrink");
    debug_assert_eq!(rgba.len(), src * src * 4);
    if src == dst {
        return rgba.to_vec();
    }

    let mut pixels = Vec::with_capacity(dst * dst * 4);
    for dst_y in 0..dst {
        let rows = dst_y * src / dst..(dst_y + 1) * src / dst;
        for dst_x in 0..dst {
            let columns = dst_x * src / dst..(dst_x + 1) * src / dst;
            let (mut red, mut green, mut blue, mut alpha) = (0u64, 0u64, 0u64, 0u64);
            for y in rows.clone() {
                let row = &rgba[(y * src + columns.start) * 4..(y * src + columns.end) * 4];
                for pixel in row.chunks_exact(4) {
                    let pixel_alpha = u64::from(pixel[3]);
                    red += u64::from(pixel[0]) * pixel_alpha;
                    green += u64::from(pixel[1]) * pixel_alpha;
                    blue += u64::from(pixel[2]) * pixel_alpha;
                    alpha += pixel_alpha;
                }
            }
            if alpha == 0 {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let count = (rows.len() * columns.len()) as u64;
                pixels.extend_from_slice(&[
                    divide_rounding(red, alpha),
                    divide_rounding(green, alpha),
                    divide_rounding(blue, alpha),
                    divide_rounding(alpha, count),
                ]);
            }
        }
    }
    pixels
}

/// `numerator / denominator` rounded to nearest, for values that fit a `u8`.
fn divide_rounding(numerator: u64, denominator: u64) -> u8 {
    ((numerator + denominator / 2) / denominator) as u8
}

/// Where the badge goes on an icon whose tile is `tile`: large enough to read as
/// a dot in the Dock, small enough to leave the artwork recognizable, and never
/// thinner than a whole pixel ring so it survives the small sizes.
///
/// It sits inside the tile, in the bottom-right corner. It must not reach past
/// the tile: macOS draws an icon whose artwork leaves the standard rounded square
/// as a legacy icon, shrunk and on a plate, which is not how the app looks.
fn badge_geometry(tile: Bounds) -> BadgeGeometry {
    let side = tile.side();
    let diameter = side * BADGE_DIAMETER;
    let inset = side * BADGE_INSET;
    BadgeGeometry {
        center: (tile.right - inset, tile.bottom - inset),
        diameter,
        ring_width: (diameter * BADGE_RING).max(1.0),
    }
}

/// Paint the profile badge over `rgba`, a `size`² image: a disc in `color`
/// inside a ring in [`BADGE_RING_COLOR`], edges anti-aliased.
fn composite_badge(rgba: &mut [u8], size: u32, geometry: BadgeGeometry, color: (u8, u8, u8)) {
    let (center_x, center_y) = geometry.center;
    let outer_radius = geometry.diameter / 2.0;
    let inner_radius = outer_radius - geometry.ring_width;

    let range = |center: f32| {
        let first = (center - outer_radius - 1.0).floor().max(0.0) as u32;
        let last = ((center + outer_radius + 1.0).ceil() as u32).min(size);
        first..last
    };
    for y in range(center_y) {
        for x in range(center_x) {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            let offset = ((y * size + x) * 4) as usize;
            let pixel = &mut rgba[offset..offset + 4];
            blend_over(
                pixel,
                BADGE_RING_COLOR,
                edge_coverage(outer_radius - distance),
            );
            blend_over(pixel, color, edge_coverage(inner_radius - distance));
        }
    }
}

/// How much of a pixel lies inside a shape, given how far the pixel's center is
/// inside its edge (negative when outside). Ramps over one pixel.
fn edge_coverage(signed_distance: f32) -> f32 {
    (signed_distance + 0.5).clamp(0.0, 1.0)
}

/// Blend `color` at `coverage` over `pixel` (straight-alpha RGBA) in place.
fn blend_over(pixel: &mut [u8], color: (u8, u8, u8), coverage: f32) {
    if coverage <= 0.0 {
        return;
    }
    let destination_alpha = f32::from(pixel[3]) / 255.0;
    let alpha = coverage + destination_alpha * (1.0 - coverage);
    for (channel, source) in [color.0, color.1, color.2].into_iter().enumerate() {
        let blended = f32::from(source) * coverage
            + f32::from(pixel[channel]) * destination_alpha * (1.0 - coverage);
        pixel[channel] = (blended / alpha).round() as u8;
    }
    pixel[3] = (alpha * 255.0).round() as u8;
}

fn render_rounded_square(size: u32, color: (u8, u8, u8)) -> Vec<u8> {
    let radius = size / 5;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let alpha = rounded_rect_alpha(x, y, size, radius);
            pixels.extend_from_slice(&[color.0, color.1, color.2, alpha]);
        }
    }
    pixels
}

/// Coverage of pixel (x, y) inside a rounded square of side `size` with
/// corner radius `r`. Returns 0–255 via 4×4 supersampling at the corners.
fn rounded_rect_alpha(x: u32, y: u32, size: u32, r: u32) -> u8 {
    if (x >= r && x < size - r) || (y >= r && y < size - r) {
        return 255;
    }
    let cx = if x < r { r } else { size - r - 1 };
    let cy = if y < r { r } else { size - r - 1 };
    let dx = x as i32 - cx as i32;
    let dy = y as i32 - cy as i32;

    const SAMPLES: i32 = 4;
    let r_squared = (r as f32 - 0.5).powi(2);
    let mut hits = 0u32;
    for sy in 0..SAMPLES {
        for sx in 0..SAMPLES {
            let fx = dx as f32 + (sx as f32 + 0.5) / SAMPLES as f32 - 0.5;
            let fy = dy as f32 + (sy as f32 + 0.5) / SAMPLES as f32 - 0.5;
            if fx * fx + fy * fy <= r_squared {
                hits += 1;
            }
        }
    }
    let coverage = hits as f32 / (SAMPLES * SAMPLES) as f32;
    (coverage * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    const PROFILE_COLOR: &str = "#7C3AED";
    const PROFILE_RGB: (u8, u8, u8) = (0x7C, 0x3A, 0xED);
    const ARTWORK_PIXEL: [u8; 4] = [200, 100, 50, 255];

    /// A vendor bundle at `<root>/Vendor.app` whose `Info.plist` names
    /// `icon_file` and whose `Vendor.icns` holds one uniform-color
    /// representation per entry of `sizes`.
    fn vendor_bundle(root: &Path, icon_file: &str, sizes: &[u32]) -> PathBuf {
        let bundle = root.join("Vendor.app");
        let resources = bundle.join("Contents/Resources");
        fs::create_dir_all(&resources).unwrap();

        let mut info = plist::Dictionary::new();
        info.insert(
            "CFBundleIconFile".to_owned(),
            plist::Value::String(icon_file.to_owned()),
        );
        plist::Value::Dictionary(info)
            .to_file_xml(bundle.join("Contents/Info.plist"))
            .unwrap();

        let mut family = IconFamily::new();
        for &size in sizes {
            let pixels = ARTWORK_PIXEL.repeat((size * size) as usize);
            let image = Image::from_data(PixelFormat::RGBA, size, size, pixels).unwrap();
            family.add_icon(&image).unwrap();
        }
        family
            .write(File::create(resources.join("Vendor.icns")).unwrap())
            .unwrap();
        bundle
    }

    /// The representation of `icns` that is `pixel_width` wide, as RGBA.
    fn icon_at(icns: &[u8], pixel_width: u32) -> Image {
        let family = IconFamily::read(Cursor::new(icns)).unwrap();
        let icon_type = family
            .available_icons()
            .into_iter()
            .find(|icon_type| icon_type.pixel_width() == pixel_width)
            .unwrap_or_else(|| panic!("no {pixel_width}px representation"));
        family
            .get_icon_with_type(icon_type)
            .unwrap()
            .convert_to(PixelFormat::RGBA)
    }

    fn pixel_at(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * image.width() + x) * 4) as usize;
        image.data()[offset..offset + 4].try_into().unwrap()
    }

    fn widths(icns: &[u8]) -> HashSet<u32> {
        IconFamily::read(Cursor::new(icns))
            .unwrap()
            .available_icons()
            .into_iter()
            .map(|icon_type| icon_type.pixel_width())
            .collect()
    }

    #[test]
    fn parse_valid_hex_color() {
        assert_eq!(parse_hex_color("#7C3AED").unwrap(), (0x7C, 0x3A, 0xED));
        assert_eq!(parse_hex_color("#000000").unwrap(), (0, 0, 0));
        assert_eq!(parse_hex_color("#FFFFFF").unwrap(), (255, 255, 255));
    }

    #[test]
    fn reject_invalid_hex_color() {
        assert!(parse_hex_color("7C3AED").is_err());
        assert!(parse_hex_color("#7C3AE").is_err());
        assert!(parse_hex_color("#ZZZZZZ").is_err());
    }

    #[test]
    fn rounded_rect_interior_is_fully_opaque() {
        assert_eq!(rounded_rect_alpha(50, 50, 100, 20), 255);
    }

    #[test]
    fn rounded_rect_far_outside_corner_is_transparent() {
        assert_eq!(rounded_rect_alpha(0, 0, 100, 20), 0);
    }

    #[test]
    fn render_rounded_square_size_matches_expected_byte_count() {
        let pixels = render_rounded_square(16, (255, 0, 0));
        assert_eq!(pixels.len(), 16 * 16 * 4);
    }

    #[test]
    fn downsample_output_has_the_destination_byte_count() {
        let source = vec![0u8; 8 * 8 * 4];
        assert_eq!(downsample(&source, 8, 2).len(), 2 * 2 * 4);
        assert_eq!(downsample(&source, 8, 8).len(), 8 * 8 * 4);
    }

    #[test]
    fn downsample_keeps_a_uniform_image_uniform() {
        for pixel in [[10, 20, 30, 255], [10, 20, 30, 128], [0, 0, 0, 0]] {
            let source = pixel.repeat(16 * 16);
            let shrunk = downsample(&source, 16, 4);
            assert!(
                shrunk.chunks(4).all(|chunk| chunk == pixel),
                "{pixel:?} did not stay uniform"
            );
        }
    }

    #[test]
    fn downsample_averages_each_block() {
        // 2×2 → 1×1: the mean of four opaque pixels.
        let source = [
            [100u8, 0, 0, 255],
            [0, 100, 0, 255],
            [0, 0, 100, 255],
            [100, 100, 100, 255],
        ]
        .concat();
        assert_eq!(downsample(&source, 2, 1), vec![50, 50, 50, 255]);
    }

    #[test]
    fn downsample_ignores_the_color_of_transparent_pixels() {
        // Two opaque red pixels beside two transparent black ones: the result
        // is half-transparent *red*, not dark red.
        let source = [
            [255u8, 0, 0, 255],
            [0, 0, 0, 0],
            [255, 0, 0, 255],
            [0, 0, 0, 0],
        ]
        .concat();
        assert_eq!(downsample(&source, 2, 1), vec![255, 0, 0, 128]);
    }

    #[test]
    fn downsample_to_the_same_size_is_the_identity() {
        let source: Vec<u8> = (0..4 * 4 * 4).map(|value| value as u8).collect();
        assert_eq!(downsample(&source, 4, 4), source);
    }

    /// What a vendor icon looks like: an opaque rounded square, more rounded than
    /// macOS's own, filling 80% of a `size`² canvas with a transparent margin.
    fn rounded_tile_canvas(size: u32) -> Vec<u8> {
        let margin = size / 10;
        let side = size - 2 * margin;
        let mut canvas = vec![0u8; (size * size * 4) as usize];
        for y in 0..side {
            for x in 0..side {
                let alpha = rounded_rect_alpha(x, y, side, side / 4);
                let offset = (((y + margin) * size + x + margin) * 4) as usize;
                canvas[offset..offset + 4].copy_from_slice(&[200, 100, 50, alpha]);
            }
        }
        canvas
    }

    #[test]
    fn badging_an_icon_never_changes_its_silhouette() {
        // macOS draws an icon whose artwork leaves the standard rounded square as
        // a legacy icon: shrunk, on a plate of its own. So the badge has to stay
        // on the tile, wherever the tile's corners are.
        let artwork = Artwork::new(1024, rounded_tile_canvas(1024)).unwrap();

        // Not 16px: there the badge cannot stay clear of the tile's antialiased edge.
        for size in [32, 64, 128, 256, 512, 1024] {
            let plain = downsample(&artwork.rgba, artwork.size, size);
            let badged = badged_artwork(&artwork, size, PROFILE_RGB);
            let changed = plain
                .chunks_exact(4)
                .zip(badged.chunks_exact(4))
                .filter(|(before, after)| before[3] != after[3])
                .count();
            assert_eq!(changed, 0, "{size}px: the badge changed the silhouette");
        }
    }

    /// The tile of an icon like the vendor's: 80% of a `size`² canvas, centered.
    fn vendor_like_tile(size: u32) -> Bounds {
        let size = size as f32;
        Bounds {
            left: size * 0.1,
            top: size * 0.1,
            right: size * 0.9,
            bottom: size * 0.9,
        }
    }

    /// The tile of an icon whose artwork fills the whole `size`² canvas.
    fn full_canvas_tile(size: u32) -> Bounds {
        Bounds {
            left: 0.0,
            top: 0.0,
            right: size as f32,
            bottom: size as f32,
        }
    }

    #[test]
    fn opaque_bounds_are_the_tile_without_the_shadow_around_it() {
        let size = 64;
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for y in 4..60 {
            for x in 4..60 {
                // A faint shadow all round, and the opaque tile on top of it.
                let alpha = if (8..56).contains(&x) && (8..56).contains(&y) {
                    255
                } else {
                    40
                };
                let offset = ((y * size + x) * 4) as usize;
                rgba[offset..offset + 4].copy_from_slice(&[10, 20, 30, alpha]);
            }
        }

        let tile = opaque_bounds(&rgba, size).unwrap();

        assert_eq!(
            tile,
            Bounds {
                left: 8.0,
                top: 8.0,
                right: 56.0,
                bottom: 56.0
            }
        );
    }

    #[test]
    fn nothing_opaque_means_no_tile_and_so_no_artwork() {
        let faint = [10u8, 20, 30, 40].repeat(16);
        assert_eq!(opaque_bounds(&faint, 4), None);
        assert!(Artwork::new(4, faint).is_none());
        assert!(Artwork::new(4, vec![0; 4 * 4 * 4]).is_none());
    }

    #[test]
    fn badge_geometry_stays_on_the_tile_at_every_icon_size() {
        for size in ICON_SIZES {
            for tile in [vendor_like_tile(size), full_canvas_tile(size)] {
                let geometry = badge_geometry(tile);
                let radius = geometry.diameter / 2.0;
                let (x, y) = geometry.center;
                assert!(
                    x + radius <= tile.right,
                    "{size}: badge past the right edge"
                );
                assert!(
                    y + radius <= tile.bottom,
                    "{size}: badge past the bottom edge"
                );
                assert!(x - radius >= tile.left, "{size}: badge past the left edge");
                assert!(y - radius >= tile.top, "{size}: badge past the top edge");
                assert!(
                    geometry.ring_width >= 1.0,
                    "{size}: ring thinner than a pixel"
                );
                assert!(
                    geometry.diameter > 2.0 * geometry.ring_width,
                    "{size}: ring leaves no room for the disc"
                );
            }
        }
    }

    #[test]
    fn the_badge_follows_the_tile_not_the_canvas() {
        // The same tile, in the corner of a big canvas or filling a small one,
        // gets the badge in the same place relative to it.
        let corner = badge_geometry(Bounds {
            left: 100.0,
            top: 100.0,
            right: 500.0,
            bottom: 500.0,
        });
        let filling = badge_geometry(full_canvas_tile(400));
        assert_eq!(
            corner.center,
            (filling.center.0 + 100.0, filling.center.1 + 100.0)
        );
        assert_eq!(corner.diameter, filling.diameter);
    }

    #[test]
    fn badge_disc_is_big_enough_to_read_at_32px() {
        let geometry = badge_geometry(vendor_like_tile(32));
        assert!(geometry.diameter - 2.0 * geometry.ring_width >= 6.0);
    }

    #[test]
    fn composite_badge_paints_the_profile_color_at_the_badge_center() {
        for size in [16, 32, 256] {
            let geometry = badge_geometry(full_canvas_tile(size));
            let mut pixels = vec![0u8; (size * size * 4) as usize];
            composite_badge(&mut pixels, size, geometry, PROFILE_RGB);

            let image = Image::from_data(PixelFormat::RGBA, size, size, pixels).unwrap();
            let (x, y) = geometry.center;
            assert_eq!(
                pixel_at(&image, x as u32, y as u32),
                [PROFILE_RGB.0, PROFILE_RGB.1, PROFILE_RGB.2, 255],
                "{size}px"
            );
        }
    }

    #[test]
    fn composite_badge_leaves_everything_outside_the_badge_untouched() {
        let size = 64;
        let before = ARTWORK_PIXEL.repeat((size * size) as usize);
        let mut after = before.clone();
        let geometry = badge_geometry(full_canvas_tile(size));
        composite_badge(&mut after, size, geometry, PROFILE_RGB);

        let badge_edge = geometry.center.0 - geometry.diameter / 2.0 - 1.0;
        for y in 0..size {
            for x in 0..size {
                if (x as f32) < badge_edge || (y as f32) < badge_edge {
                    let offset = ((y * size + x) * 4) as usize;
                    assert_eq!(
                        after[offset..offset + 4],
                        before[offset..offset + 4],
                        "pixel ({x}, {y}) changed"
                    );
                }
            }
        }
        assert_ne!(after, before, "badge left no mark");
    }

    #[test]
    fn composite_badge_rings_the_disc_in_the_ring_color() {
        let size = 256;
        let geometry = badge_geometry(full_canvas_tile(size));
        let mut pixels = ARTWORK_PIXEL.repeat((size * size) as usize);
        composite_badge(&mut pixels, size, geometry, PROFILE_RGB);

        let ring_middle = geometry.diameter / 2.0 - geometry.ring_width / 2.0;
        let x = (geometry.center.0 + ring_middle) as u32;
        let y = geometry.center.1 as u32;
        let offset = ((y * size + x) * 4) as usize;
        assert_eq!(
            pixels[offset..offset + 4],
            [
                BADGE_RING_COLOR.0,
                BADGE_RING_COLOR.1,
                BADGE_RING_COLOR.2,
                255
            ]
        );
    }

    #[test]
    fn classic_artwork_reads_the_icon_file_the_plist_names() {
        let dir = tempfile::tempdir().unwrap();
        // The key may omit the extension, as Apple's own bundles often do.
        let bundle = vendor_bundle(dir.path(), "Vendor", &[16, 32]);

        let artwork = classic_artwork(&bundle).expect("artwork decodes");
        assert_eq!(artwork.size, 32, "largest representation wins");
        assert_eq!(artwork.rgba[..4], ARTWORK_PIXEL);
        assert_eq!(
            artwork.tile,
            full_canvas_tile(32),
            "the fixture is all tile"
        );

        let with_extension = vendor_bundle(&dir.path().join("other"), "Vendor.icns", &[16]);
        assert!(classic_artwork(&with_extension).is_some());
    }

    #[test]
    fn classic_artwork_is_none_for_a_missing_or_undecodable_icon() {
        let dir = tempfile::tempdir().unwrap();
        assert!(classic_artwork(&dir.path().join("Nope.app")).is_none());

        let bundle = vendor_bundle(dir.path(), "Vendor", &[16]);
        fs::write(
            bundle.join("Contents/Resources/Vendor.icns"),
            b"not an icns",
        )
        .unwrap();
        assert!(classic_artwork(&bundle).is_none());
    }

    #[test]
    fn render_icns_badges_the_vendor_artwork() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = vendor_bundle(dir.path(), "Vendor", &[LARGEST_ICON_SIZE]);

        let icns = render_icns_from(PROFILE_COLOR, classic_artwork(&bundle)).unwrap();

        let icon = icon_at(&icns, 32);
        assert_eq!(
            pixel_at(&icon, 1, 1),
            ARTWORK_PIXEL,
            "artwork shows through"
        );
        let (x, y) = badge_geometry(full_canvas_tile(32)).center;
        assert_eq!(
            pixel_at(&icon, x as u32, y as u32),
            [PROFILE_RGB.0, PROFILE_RGB.1, PROFILE_RGB.2, 255],
            "badge sits on top"
        );
    }

    #[test]
    fn render_icns_contains_every_size() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = vendor_bundle(dir.path(), "Vendor", &[LARGEST_ICON_SIZE]);

        for icns in [
            render_icns_from(PROFILE_COLOR, classic_artwork(&bundle)).unwrap(),
            render_icns_from(PROFILE_COLOR, None).unwrap(),
        ] {
            let present = widths(&icns);
            for size in ICON_SIZES {
                assert!(present.contains(&size), "missing {size}px");
            }
        }
    }

    #[test]
    fn render_icns_falls_back_to_a_rounded_square_without_vendor_artwork() {
        let icns = render_icns_from(PROFILE_COLOR, None).unwrap();

        assert_eq!(&icns[0..4], b"icns");
        let icon = icon_at(&icns, 32);
        assert_eq!(pixel_at(&icon, 0, 0)[3], 0, "rounded corner is transparent");
        assert_eq!(
            pixel_at(&icon, 16, 16),
            [PROFILE_RGB.0, PROFILE_RGB.1, PROFILE_RGB.2, 255]
        );
    }

    #[test]
    fn render_icns_falls_back_when_the_artwork_cannot_supply_the_largest_size() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = vendor_bundle(dir.path(), "Vendor", &[16, 32, 128]);

        let icns = render_icns_from(PROFILE_COLOR, classic_artwork(&bundle)).unwrap();

        // Same output as having no artwork at all: never a mix of sources.
        assert_eq!(icns, render_icns_from(PROFILE_COLOR, None).unwrap());
    }

    #[test]
    fn render_icns_fails_for_invalid_color() {
        assert!(render_icns_from("not-a-color", None).is_err());
    }

    /// Opt-in: renders against whichever vendor apps are installed, so it
    /// proves the real `.icns` files decode and get badged rather than falling
    /// back. Gated behind AI_PROFILES_E2E=1 because it needs those apps.
    #[test]
    fn render_icns_badges_the_installed_vendor_apps() {
        if std::env::var("AI_PROFILES_E2E").is_err() {
            eprintln!("skipping; set AI_PROFILES_E2E=1 to run");
            return;
        }
        let nowhere = Path::new("/nonexistent/Vendor.app");
        let fallback = render_icns(PROFILE_COLOR, nowhere).unwrap();
        for spec in [&crate::app_kind::CLAUDE, &crate::app_kind::CODEX] {
            let Some(app) = crate::paths::resolve_gui_app(spec) else {
                eprintln!("{} not installed; skipping", spec.display_name);
                continue;
            };
            let icns = render_icns(PROFILE_COLOR, &app.bundle_path).unwrap();
            assert_ne!(
                icns, fallback,
                "{} fell back to the plain square",
                spec.display_name
            );
            let present = widths(&icns);
            for size in ICON_SIZES {
                assert!(
                    present.contains(&size),
                    "{}: missing {size}px",
                    spec.display_name
                );
            }
        }
    }
}
