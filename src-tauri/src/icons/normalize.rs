//! Decodes an arbitrary image byte string and normalises it to a 128×128
//! PNG (design.md §2.2.10, §11.1): aspect ratio preserved, centred on a
//! transparent canvas. SVG (unsupported by the `image` crate) and any
//! other undecodable data are rejected as [`NormalizeError::Decode`], never
//! papered over with a placeholder.

use std::io::Cursor;

use image::{
    imageops::FilterType, DynamicImage, GenericImage, ImageFormat, ImageReader, Limits, RgbaImage,
};

/// The side length (in pixels) every cached icon is normalised to
/// (design.md §2.2.10).
pub const ICON_SIZE: u32 = 128;

/// The maximum width/height [`normalize_to_png`] will decode an incoming
/// candidate at (design.md §2.2.10): comfortably larger than any real
/// favicon or app icon, and small enough that an image whose *encoded*
/// bytes are tiny but whose declared pixel dimensions are enormous (a
/// decompression bomb) is rejected — as [`NormalizeError::Decode`] — before
/// it is ever decoded into memory.
const MAX_DECODE_DIMENSION: u32 = 4096;

/// The maximum total bytes the decoder may allocate for one image
/// (design.md §2.2.10) — [`MAX_DECODE_DIMENSION`] squared at 4 bytes/pixel
/// (4096×4096×4 = 64 MiB) is exactly this cap, so a legitimate image at the
/// dimension limit still decodes, while anything needing more (whether from
/// larger declared dimensions or a decoder's own intermediate buffers) is
/// rejected instead.
const MAX_DECODE_ALLOC_BYTES: u64 = 64 * 1024 * 1024;

/// The [`Limits`] every decode in this module runs under. `Limits` is
/// `#[non_exhaustive]`, so built by mutating [`Limits::default`]'s fields
/// rather than a struct literal.
fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    limits
}

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
    // `with_guessed_format` guesses the format from the byte content itself
    // (magic bytes), not a file extension, and has no SVG decoder registered
    // at all — an SVG payload (or any other unsupported/corrupt data) always
    // falls into one of these `Err` branches. `decode_limits` caps both the
    // claimed pixel dimensions and the decoder's allocation, so a
    // decompression bomb — tiny encoded bytes, enormous declared dimensions
    // — is rejected as `Decode` instead of being decoded into memory.
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| NormalizeError::Decode)?;
    reader.limits(decode_limits());
    let decoded = reader.decode().map_err(|_| NormalizeError::Decode)?;

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

    // --- decode limits (decompression-bomb rejection) -----------------------

    /// The standard CRC-32 (IEEE 802.3, reflected, polynomial `0xEDB88320`)
    /// PNG chunks are checksummed with — implemented by hand here so
    /// [`oversized_png_header`] can hand-craft a well-formed PNG chunk
    /// stream without pulling in a CRC dependency just for this one test.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in bytes {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        crc ^ 0xFFFF_FFFF
    }

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut crc_input = Vec::with_capacity(4 + data.len());
        crc_input.extend_from_slice(kind);
        crc_input.extend_from_slice(data);
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        chunk
    }

    /// A hand-crafted, well-formed PNG byte stream whose `IHDR` declares a
    /// 50000×50000 image — far beyond [`MAX_DECODE_DIMENSION`] — but which
    /// carries no pixel data at all (just `IHDR` and `IEND`): a
    /// decompression bomb's defining trait is tiny encoded bytes with an
    /// enormous *declared* size, and the dimension check must reject this
    /// before any pixel buffer is ever allocated, so there is nothing to
    /// decompress in the first place.
    fn oversized_png_header() -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&50_000u32.to_be_bytes()); // width
        ihdr.extend_from_slice(&50_000u32.to_be_bytes()); // height
        ihdr.push(8); // bit depth
        ihdr.push(6); // color type: RGBA
        ihdr.push(0); // compression method
        ihdr.push(0); // filter method
        ihdr.push(0); // interlace method
        bytes.extend(png_chunk(b"IHDR", &ihdr));
        bytes.extend(png_chunk(b"IEND", &[]));
        bytes
    }

    #[test]
    fn rejects_a_png_whose_declared_dimensions_exceed_the_decode_limit() {
        let bomb = oversized_png_header();
        let err = normalize_to_png(&bomb).unwrap_err();
        assert_eq!(err, NormalizeError::Decode);
    }

    #[test]
    fn the_oversized_header_is_specifically_rejected_by_the_limits_check() {
        // Same fixture as above, but exercised one layer down so the
        // failure can be confirmed to be `image::ImageError::Limits` and
        // not some other, incidental decode failure (e.g. the missing
        // pixel data).
        let bomb = oversized_png_header();
        let mut reader = ImageReader::new(Cursor::new(&bomb))
            .with_guessed_format()
            .expect("format is guessable from the PNG signature");
        reader.limits(decode_limits());
        let err = reader
            .decode()
            .expect_err("oversized dimensions must be rejected");
        assert!(
            matches!(err, image::ImageError::Limits(_)),
            "expected a Limits error, got {err:?}"
        );
    }
}
