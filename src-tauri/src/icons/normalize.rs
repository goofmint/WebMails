//! Decodes an arbitrary image byte string and normalises it to a 128×128
//! PNG (design.md §2.2.10, §11.1): aspect ratio preserved, centred on a
//! transparent canvas. SVG (unsupported by the `image` crate) and any
//! other undecodable data are rejected as [`NormalizeError::Decode`], never
//! papered over with a placeholder.

use image::{imageops::FilterType, DynamicImage, GenericImage, ImageFormat, RgbaImage};

/// The side length (in pixels) every cached icon is normalised to
/// (design.md §2.2.10).
pub const ICON_SIZE: u32 = 128;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NormalizeError {
    /// The bytes could not be decoded as a supported raster image format
    /// (e.g. SVG, or corrupt/truncated data).
    #[error("could not decode image data")]
    Decode,
    /// Decoded successfully, but re-encoding the resized result as PNG
    /// failed.
    #[error("could not encode PNG")]
    Encode,
}

/// Decodes `bytes`, resizes (preserving aspect ratio, never upscaling
/// beyond [`ICON_SIZE`]) and centres the result on a transparent
/// `ICON_SIZE`×`ICON_SIZE` canvas, then encodes it as PNG.
pub fn normalize_to_png(bytes: &[u8]) -> Result<Vec<u8>, NormalizeError> {
    // `image::load_from_memory` guesses the format from the byte content
    // itself (magic bytes), not a file extension, and has no SVG decoder
    // registered at all — an SVG payload (or any other unsupported/corrupt
    // data) always falls into this `Err` branch.
    let decoded = image::load_from_memory(bytes).map_err(|_| NormalizeError::Decode)?;

    let resized = decoded.resize(ICON_SIZE, ICON_SIZE, FilterType::Lanczos3);
    let canvas = center_on_transparent_canvas(&resized);

    let mut out = Vec::new();
    canvas
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|_| NormalizeError::Encode)?;
    Ok(out)
}

/// Places `image` (already resized to fit within [`ICON_SIZE`]×[`ICON_SIZE`])
/// centred on a fully transparent `ICON_SIZE`×`ICON_SIZE` canvas.
fn center_on_transparent_canvas(image: &DynamicImage) -> RgbaImage {
    let mut canvas = RgbaImage::new(ICON_SIZE, ICON_SIZE);
    let (w, h) = (image.width(), image.height());
    let x = (ICON_SIZE.saturating_sub(w)) / 2;
    let y = (ICON_SIZE.saturating_sub(h)) / 2;
    // Both `canvas` and `image` (via `resize`) fit within `ICON_SIZE` on
    // every side by construction, so this copy never goes out of bounds.
    let _ = canvas.copy_from(&image.to_rgba8(), x, y);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn encode(image: &RgbaImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut out), format)
            .expect("encode fixture");
        out
    }

    fn solid(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, 255]))
    }

    #[test]
    fn rejects_garbage_bytes() {
        let err = normalize_to_png(b"not an image").unwrap_err();
        assert_eq!(err, NormalizeError::Decode);
    }

    #[test]
    fn rejects_svg_data() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        let err = normalize_to_png(svg).unwrap_err();
        assert_eq!(err, NormalizeError::Decode);
    }

    #[test]
    fn normalizes_a_square_png_to_128x128() {
        let source = encode(&solid(64, 64), ImageFormat::Png);
        let png = normalize_to_png(&source).expect("should normalize");
        let decoded = image::load_from_memory(&png).expect("decode result");
        assert_eq!(decoded.width(), ICON_SIZE);
        assert_eq!(decoded.height(), ICON_SIZE);
    }

    #[test]
    fn preserves_aspect_ratio_of_a_wide_source() {
        let source = encode(&solid(200, 50), ImageFormat::Png);
        let png = normalize_to_png(&source).expect("should normalize");
        let decoded = image::load_from_memory(&png)
            .expect("decode result")
            .to_rgba8();
        assert_eq!(decoded.width(), ICON_SIZE);
        assert_eq!(decoded.height(), ICON_SIZE);

        // The resized content is 128 wide by 32 tall, centred vertically:
        // rows near the top and bottom edges stay transparent.
        assert_eq!(
            decoded.get_pixel(64, 0)[3],
            0,
            "top edge must be transparent"
        );
        assert_eq!(
            decoded.get_pixel(64, ICON_SIZE - 1)[3],
            0,
            "bottom edge must be transparent"
        );
        assert!(
            decoded.get_pixel(64, 64)[3] > 0,
            "vertical centre must hold the image content"
        );
    }

    #[test]
    fn upscales_a_small_source_to_fill_the_canvas() {
        let source = encode(&solid(16, 16), ImageFormat::Png);
        let png = normalize_to_png(&source).expect("should normalize");
        let decoded = image::load_from_memory(&png).expect("decode result");
        assert_eq!(decoded.width(), ICON_SIZE);
        assert_eq!(decoded.height(), ICON_SIZE);
    }
}
