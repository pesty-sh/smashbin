//! Step 4. Take private metadata off the file.
//!
//! JPEG keeps the picture data (the scan after SOS) byte for byte.
//! GIF keeps the frames and drops comment blocks.
//! PNG drops text, EXIF, and time chunks. The color profile stays unless
//! the strip is `all`. The pixels are recompressed in the next step.

use img_parts::jpeg::Jpeg;
use img_parts::png::Png;
use img_parts::Bytes;

use crate::cli::Strip;
use crate::error::SmashError;
use crate::sniff::{Kind, Picture};

const ICC_PREFIX: &[u8] = b"ICC_PROFILE\0";
const NETSCAPE: &[u8] = b"NETSCAPE2.0";
const ANIMEXTS: &[u8] = b"ANIMEXTS1.0";
const GIF_ICC: &[u8] = b"ICCRGBG1012";

/// Remove metadata. The picture bytes stay.
///
/// # Errors
///
/// The container is cut off or is not the format `picture` names.
pub fn apply(bytes: &[u8], picture: &Picture, strip: Strip) -> Result<Vec<u8>, SmashError> {
    match picture.kind {
        Kind::Jpeg => strip_jpeg(bytes, strip),
        Kind::Gif => strip_gif(bytes, strip),
        Kind::Png => strip_png(bytes, strip),
    }
}

fn strip_png(bytes: &[u8], strip: Strip) -> Result<Vec<u8>, SmashError> {
    let mut png = Png::from_bytes(Bytes::copy_from_slice(bytes))
        .map_err(|error| SmashError::message(format!("this PNG could not be read: {error}")))?;
    png.chunks_mut()
        .retain(|chunk| keep_png_chunk(chunk.kind(), strip));
    let mut out = Vec::new();
    for piece in png.encoder() {
        out.extend_from_slice(&piece);
    }
    Ok(out)
}

const PNG_PICTURE: &[[u8; 4]] = &[*b"tRNS", *b"bKGD", *b"acTL", *b"fcTL", *b"fdAT"];
const PNG_COLOR: &[[u8; 4]] = &[
    *b"iCCP", *b"sRGB", *b"gAMA", *b"cHRM", *b"cICP", *b"sBIT", *b"pHYs",
];

/// Critical chunks stay. So do the chunks a viewer needs to paint the same
/// picture. Text, time, and EXIF go. The color profile goes only for `all`.
fn keep_png_chunk(kind: [u8; 4], strip: Strip) -> bool {
    if kind[0].is_ascii_uppercase() {
        return true;
    }
    if PNG_PICTURE.contains(&kind) {
        return true;
    }
    strip == Strip::Safe && PNG_COLOR.contains(&kind)
}

fn strip_jpeg(bytes: &[u8], strip: Strip) -> Result<Vec<u8>, SmashError> {
    let mut jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(bytes))
        .map_err(|error| SmashError::message(format!("this JPEG could not be read: {error}")))?;
    jpeg.segments_mut()
        .retain(|segment| keep_jpeg_segment(segment.marker(), segment.contents(), strip));
    let mut out = Vec::new();
    for piece in jpeg.encoder() {
        out.extend_from_slice(&piece);
    }
    Ok(out)
}

/// Keep the pieces a decoder needs. Drop everything that is only a note.
fn keep_jpeg_segment(marker: u8, contents: &[u8], strip: Strip) -> bool {
    match marker {
        // JFIF header, and Adobe's color marker. Decoders need these.
        0xE0 | 0xEE => true,
        // The color profile. `safe` keeps it. `all` drops it.
        0xE2 if contents.starts_with(ICC_PREFIX) => strip == Strip::Safe,
        // Other APP blocks (EXIF, XMP, Photoshop, C2PA) and comments.
        0xE0..=0xEF | 0xFE => false,
        // Huffman tables, the scan, the frame header. This is the picture.
        _ => true,
    }
}

fn strip_gif(bytes: &[u8], strip: Strip) -> Result<Vec<u8>, SmashError> {
    if bytes.len() < 13 || !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return Err(SmashError::message("this GIF could not be read"));
    }
    let packed = bytes[10];
    let global_colors = if packed & 0x80 == 0 {
        0
    } else {
        3 * (1usize << ((packed & 0x07) + 1))
    };
    let mut index = 13 + global_colors;
    if index > bytes.len() {
        return Err(SmashError::message("this GIF is cut off"));
    }
    let mut out = Vec::from(&bytes[..index]);
    while index < bytes.len() {
        match bytes[index] {
            0x3B => {
                out.push(0x3B);
                break;
            }
            0x2C => {
                let start = index;
                index = skip_gif_image(bytes, index)?;
                out.extend_from_slice(&bytes[start..index]);
            }
            0x21 => {
                let start = index;
                index = skip_gif_subblocks(bytes, index.saturating_add(2))?;
                if keep_gif_extension(&bytes[start..index], strip) {
                    out.extend_from_slice(&bytes[start..index]);
                }
            }
            _ => {
                return Err(SmashError::message(
                    "this GIF has a block this version does not know",
                ))
            }
        }
    }
    Ok(out)
}

fn keep_gif_extension(block: &[u8], strip: Strip) -> bool {
    if block.len() < 2 || block[0] != 0x21 {
        return true;
    }
    match block[1] {
        // Comment. Always private notes, never the picture.
        0xFE => false,
        0xFF => {
            // Animation needs the Netscape block. Drop other application notes.
            block
                .windows(11)
                .any(|window| window == NETSCAPE || window == ANIMEXTS)
                || (strip == Strip::Safe && block.windows(11).any(|window| window == GIF_ICC))
        }
        _ => true,
    }
}

fn skip_gif_image(bytes: &[u8], start: usize) -> Result<usize, SmashError> {
    // Image separator plus the 9-byte descriptor.
    let descriptor = bytes
        .get(start..start + 10)
        .ok_or_else(|| SmashError::message("this GIF image is cut off"))?;
    let mut index = start + 10;
    if descriptor[9] & 0x80 != 0 {
        let colors = 3 * (1usize << ((descriptor[9] & 0x07) + 1));
        index += colors;
    }
    // One byte of LZW minimum code size, then sub-blocks.
    index += 1;
    skip_gif_subblocks(bytes, index)
}

fn skip_gif_subblocks(bytes: &[u8], mut index: usize) -> Result<usize, SmashError> {
    while index < bytes.len() {
        let size = usize::from(bytes[index]);
        index += 1;
        if size == 0 {
            return Ok(index);
        }
        index = index
            .checked_add(size)
            .ok_or_else(|| SmashError::message("this GIF is cut off"))?;
        if index > bytes.len() {
            return Err(SmashError::message("this GIF is cut off"));
        }
    }
    Err(SmashError::message("this GIF is cut off"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::apply;
    use crate::cli::Strip;
    use crate::sniff::{Kind, Picture};

    #[test]
    fn gif_comment_is_removed_and_the_frame_stays() {
        // 1x1 GIF89a plus a comment extension "ABC" before the trailer.
        let mut bytes = vec![
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x4C, 0x01, 0x00,
        ];
        bytes.extend_from_slice(&[0x21, 0xFE, 0x03, b'A', b'B', b'C', 0x00, 0x3B]);
        let picture = Picture {
            kind: Kind::Gif,
            width: 1,
            height: 1,
        };
        let stripped = apply(&bytes, &picture, Strip::Safe).unwrap();
        assert!(!stripped.windows(3).any(|window| window == b"ABC"));
        assert_eq!(*stripped.last().unwrap(), 0x3B);
        assert!(stripped.windows(1).any(|window| window == [0x2C]));
    }

    #[test]
    fn png_text_is_removed_and_the_pixel_stays() {
        let mut raw = Vec::new();
        let mut encoder = png::Encoder::new(&mut raw, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255, 0, 0]).unwrap();
        writer.finish().unwrap();

        let mut png = img_parts::png::Png::from_bytes(img_parts::Bytes::from(raw)).unwrap();
        let end = png
            .chunks()
            .iter()
            .position(|chunk| chunk.kind() == *b"IEND")
            .unwrap();
        png.chunks_mut().insert(
            end,
            img_parts::png::PngChunk::new(*b"tEXt", img_parts::Bytes::from_static(b"Comment\0hi")),
        );
        let mut with_text = Vec::new();
        for piece in png.encoder() {
            with_text.extend_from_slice(&piece);
        }

        let picture = Picture {
            kind: Kind::Png,
            width: 1,
            height: 1,
        };
        let stripped = apply(&with_text, &picture, Strip::Safe).unwrap();
        assert!(stripped.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(!stripped.windows(4).any(|window| window == *b"tEXt"));
    }
}
