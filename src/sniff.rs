//! Step 3. Name the format and read the width and height.
//!
//! The file extension is ignored. A `.jpg` that is really a PNG is a PNG.
//! This version reads PNG, JPEG, and GIF. `--ext` can write a different
//! container after this step.

use img_parts::jpeg::Jpeg;
use img_parts::Bytes;

use crate::error::SmashError;

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
