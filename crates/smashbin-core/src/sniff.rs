//! Step 3. Name the format and read the width and height.
//!
//! The file extension is ignored. A `.jpg` that is really a PNG is a PNG.
//! This version reads PNG, JPEG, and GIF. `--format` can write a different
//! container after this step.

use img_parts::jpeg::Jpeg;
use img_parts::Bytes;

use crate::error::SmashError;

/// The longest side this version will decode.
pub(crate) const MAX_SIDE: u32 = 10_000;

/// Stop before decode when the header claims more pixels than this.
///
/// 100 million pixels is a 10 000 by 10 000 picture, about 300 MB of RGB.
/// A 25 MB gallery JPEG is under that. A decompression bomb is not: its
/// header says it is enormous, and this check refuses it before the
/// decoder allocates the picture.
pub(crate) const MAX_PIXELS: u64 = 100_000_000;

/// Refuse a header that is wider, taller, or larger than this version decodes.
///
/// # Errors
///
/// A side is over [`MAX_SIDE`], or the pixel count is over [`MAX_PIXELS`].
pub(crate) fn pixel_budget(width: u32, height: u32) -> Result<(), SmashError> {
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(SmashError::message(format!(
            "this image is {width} by {height}. This version stops at {MAX_SIDE} pixels on a side."
        )));
    }
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels > MAX_PIXELS {
        return Err(SmashError::message(format!(
            "this image is {width} by {height}, which is {pixels} pixels. This version stops at {MAX_PIXELS} pixels before it decodes."
        )));
    }
    Ok(())
}

/// A format this version can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A PNG. Lossless shrink goes through the pure-Rust `png` crate.
    Png,
    /// A JPEG. Metadata is a few segments at the front.
    Jpeg,
    /// A GIF. Comments sit in extension blocks.
    Gif,
}

impl Kind {
    /// The word used in errors.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
        }
    }
}

/// Format plus the size of the picture. A resize replaces the size later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picture {
    /// PNG, JPEG, or GIF.
    pub kind: Kind,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Read the header.
///
/// # Errors
///
/// The bytes are not a PNG, JPEG, or GIF, or the header is cut off.
pub fn sniff(bytes: &[u8]) -> Result<Picture, SmashError> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let (width, height) = png_size(bytes)?;
        return Ok(Picture {
            kind: Kind::Png,
            width,
            height,
        });
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let (width, height) = jpeg_size(bytes)?;
        return Ok(Picture {
            kind: Kind::Jpeg,
            width,
            height,
        });
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let (width, height) = gif_size(bytes)?;
        return Ok(Picture {
            kind: Kind::Gif,
            width,
            height,
        });
    }
    Err(SmashError::message(
        "this version reads PNG, JPG, and GIF. This file is none of those.",
    ))
}

fn png_size(bytes: &[u8]) -> Result<(u32, u32), SmashError> {
    // signature (8) + length (4) + "IHDR" (4) + width (4) + height (4)
    // The first chunk must be IHDR. A later chunk is not the size.
    let length = bytes
        .get(8..12)
        .ok_or_else(|| SmashError::message("this PNG is cut off before its width and height"))?;
    let kind = bytes
        .get(12..16)
        .ok_or_else(|| SmashError::message("this PNG is cut off before its width and height"))?;
    if length != 13u32.to_be_bytes() || kind != b"IHDR" {
        return Err(SmashError::message(
            "this PNG does not start with its header",
        ));
    }
    let header = bytes
        .get(16..24)
        .ok_or_else(|| SmashError::message("this PNG is cut off before its width and height"))?;
    let width = u32::from_be_bytes(header[0..4].try_into().unwrap_or([0; 4]));
    let height = u32::from_be_bytes(header[4..8].try_into().unwrap_or([0; 4]));
    nonzero(width, height)
}

fn jpeg_size(bytes: &[u8]) -> Result<(u32, u32), SmashError> {
    let jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(bytes))
        .map_err(|error| SmashError::message(format!("this JPEG could not be read: {error}")))?;
    let (width, height) = jpeg
        .dimensions()
        .ok_or_else(|| SmashError::message("this JPEG has no width and height in its header"))?;
    nonzero(u32::from(width), u32::from(height))
}

fn gif_size(bytes: &[u8]) -> Result<(u32, u32), SmashError> {
    let header = bytes
        .get(6..10)
        .ok_or_else(|| SmashError::message("this GIF is cut off before its width and height"))?;
    let width = u32::from(u16::from_le_bytes(
        header[0..2].try_into().unwrap_or([0; 2]),
    ));
    let height = u32::from(u16::from_le_bytes(
        header[2..4].try_into().unwrap_or([0; 2]),
    ));
    nonzero(width, height)
}

fn nonzero(width: u32, height: u32) -> Result<(u32, u32), SmashError> {
    if width == 0 || height == 0 {
        return Err(SmashError::message("the image width or height is zero"));
    }
    Ok((width, height))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{sniff, Kind};

    #[test]
    fn a_text_file_is_none_of_the_three() {
        let error = sniff(b"hello").unwrap_err();
        assert!(error.to_string().contains("PNG, JPG, and GIF"));
    }

    #[test]
    fn a_short_png_signature_has_no_size() {
        let error = sniff(b"\x89PNG\r\n\x1a\n").unwrap_err();
        assert!(error.to_string().contains("cut off"));
    }

    #[test]
    fn a_gif_header_stores_width_then_height() {
        let picture = sniff(b"GIF89a\x02\x00\x03\x00").unwrap();
        assert_eq!(picture.kind, Kind::Gif);
        assert_eq!((picture.width, picture.height), (2, 3));
    }

    #[test]
    fn a_gif_with_a_zero_side_is_refused() {
        let error = sniff(b"GIF87a\x00\x00\x01\x00").unwrap_err();
        assert!(error.to_string().contains("zero"));
    }

    #[test]
    fn a_png_must_start_with_ihdr() {
        let mut bytes = vec![0u8; 24];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[12..16].copy_from_slice(b"tEXt");
        bytes[16..20].copy_from_slice(&1u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_be_bytes());
        let error = sniff(&bytes).unwrap_err();
        assert!(error.to_string().contains("header"));
    }

    #[test]
    fn a_side_longer_than_the_cap_is_refused() {
        let error = super::pixel_budget(20_000, 1).unwrap_err();
        assert!(error.to_string().contains("stops at"));
    }
}
