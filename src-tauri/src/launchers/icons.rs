use std::io::Cursor;

use icns::{IconFamily, Image, PixelFormat};

use crate::error::{AppError, AppResult};

/// Standard Apple icon sizes. The icns crate auto-picks the matching IconType
/// for each size, including the 1024×1024 case (which becomes the @2x of 512).
const ICON_SIZES: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];

/// Render a complete `.icns` for the given hex color, at all sizes in
/// `ICON_SIZES`. Returns the binary bytes.
pub fn render_icns(color_hex: &str) -> AppResult<Vec<u8>> {
    let color = parse_hex_color(color_hex)?;
    let mut family = IconFamily::new();
    for &size in &ICON_SIZES {
        let pixels = render_rounded_square(size, color);
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
    use super::*;

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
    fn render_icns_produces_nonempty_output_for_valid_color() {
        let bytes = render_icns("#7C3AED").unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..4], b"icns");
    }

    #[test]
    fn render_icns_fails_for_invalid_color() {
        assert!(render_icns("not-a-color").is_err());
    }
}
