//! Step 5. Make the file smaller without changing a pixel.
//!
//! PNG: the `png` crate writes the same samples again with a tighter
//! deflate stream. That crate is Rust. It does not compile a C library.
//! An animated PNG is left as step 4 wrote it, so the frames stay in order.
//!
//! JPEG: the scan is already a lossy encoding. Rewriting it from pixels,
//! even at quality 100, would be a second generation. This step only keeps
//! the stripped file from step 4. The coefficients do not move.
//!
//! GIF: the frames were already kept in step 4. There is no second pass.

use std::io::Cursor;

use crate::error::SmashError;
use crate::sniff::{Kind, Picture};

/// Shrink what can be shrunk without a new set of pixels.
///
/// # Errors
///
/// The PNG is damaged and the decoder refuses it.
pub fn shrink(bytes: &[u8], picture: &Picture) -> Result<Vec<u8>, SmashError> {
    match picture.kind {
        Kind::Png => shrink_png(bytes),
        Kind::Jpeg | Kind::Gif => Ok(bytes.to_vec()),
    }
}

fn shrink_png(bytes: &[u8]) -> Result<Vec<u8>, SmashError> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|error| SmashError::message(format!("this PNG could not be read: {error}")))?;
    if reader.info().animation_control.is_some() {
        return Ok(bytes.to_vec());
    }
    let buffer_size = reader.output_buffer_size().ok_or_else(|| {
        SmashError::message("this PNG is larger than this version will decompress")
    })?;
    let mut pixels = vec![0; buffer_size];
    let written = reader
        .next_frame(&mut pixels)
        .map_err(|error| SmashError::message(format!("this PNG could not be read: {error}")))?;
    let mut info = reader.info().clone();
    // The decoder hands back a flat picture. Write it that way.
    info.interlaced = false;
    info.exif_metadata = None;
    info.uncompressed_latin1_text.clear();
    info.compressed_latin1_text.clear();
    info.utf8_text.clear();

    let mut out = Vec::new();
    let mut encoder = png::Encoder::with_info(&mut out, info)
        .map_err(|error| SmashError::message(format!("this PNG could not be written: {error}")))?;
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder
        .write_header()
        .map_err(|error| SmashError::message(format!("this PNG could not be written: {error}")))?;
    writer
        .write_image_data(&pixels[..written.buffer_size()])
        .map_err(|error| SmashError::message(format!("this PNG could not be written: {error}")))?;
    writer
        .finish()
        .map_err(|error| SmashError::message(format!("this PNG could not be written: {error}")))?;
    Ok(out)
}
