//! Display density, in pixels per inch.
//!
//! The pixel grid does not change. A lower number makes the picture claim a
//! larger physical size. A higher number makes it claim a smaller one.
//! JPEG stores this in the JFIF header. PNG stores the same count as pixels
//! per meter in a `pHYs` chunk. WebP stores it in an EXIF block. A container
//! with no place for it is still written, and the report names the original
//! image's density.

use img_parts::jpeg::{Jpeg, JpegSegment};
use img_parts::png::{Png, PngChunk};
use img_parts::Bytes;

use crate::error::SmashError;
use crate::ext::Ext;
use crate::sniff::Kind;

/// Bytes after a density pass, or the same bytes when the container cannot hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// `dpi` is now in the file.
    Stored(Vec<u8>),
    /// The file is unchanged. `container` is the word for the report.
    Skipped {
        /// The finished picture.
        bytes: Vec<u8>,
        /// `GIF`, `BMP`, and the other containers with no density field.
        container: &'static str,
    },
}

/// The density already stored on the source image, in pixels per inch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalDensity {
    /// Horizontal pixels per inch.
    pub x: u16,
    /// Vertical pixels per inch.
    pub y: u16,
}

/// Write `dpi` into the container. The samples stay where they are.
///
/// # Errors
///
/// A JPEG or PNG that should hold the density could not be read.
pub fn apply(bytes: &[u8], ext: Option<Ext>, kind: Kind, dpi: u16) -> Result<Outcome, SmashError> {
    match ext {
        Some(Ext::Jpeg) => Ok(Outcome::Stored(jpeg(bytes, dpi)?)),
        Some(Ext::Png) => Ok(Outcome::Stored(png(bytes, dpi)?)),
        Some(Ext::Webp) => Ok(webp(bytes, dpi)),
        Some(other) => Ok(Outcome::Skipped {
            bytes: bytes.to_vec(),
            container: other.as_str(),
        }),
        None => match kind {
            Kind::Jpeg => Ok(Outcome::Stored(jpeg(bytes, dpi)?)),
            Kind::Png => Ok(Outcome::Stored(png(bytes, dpi)?)),
            Kind::Gif => Ok(Outcome::Skipped {
                bytes: bytes.to_vec(),
                container: kind.as_str(),
            }),
        },
    }
}

/// One sentence for stderr when `--dpi` could not be written.
#[must_use]
pub fn skip_note(dpi: u16, container: &str, original: Option<OriginalDensity>) -> String {
    let kept = match original {
        Some(density) if density.x == density.y => {
            format!("The original image is {} pixels per inch.", density.x)
        }
        Some(density) => format!(
            "The original image is {} by {} pixels per inch.",
            density.x, density.y
        ),
        None => "The original image has no display density.".to_owned(),
    };
    format!("smashbin: --dpi {dpi} is not stored in a {container}. {kept}")
}

/// Density already written on a PNG or JPEG. GIF has none.
#[must_use]
pub fn source_dpi(bytes: &[u8], kind: Kind) -> Option<OriginalDensity> {
    match kind {
        Kind::Jpeg => jpeg_dpi(bytes),
        Kind::Png => png_dpi(bytes),
        Kind::Gif => None,
    }
}

/// `72` dpi is 2835 pixels per meter. PNG counts meters, not inches.
fn pixels_per_meter(dpi: u16) -> u32 {
    let ppm = (u64::from(dpi) * 10_000 + 127) / 254;
    u32::try_from(ppm).unwrap_or(u32::MAX)
}

fn jpeg(bytes: &[u8], dpi: u16) -> Result<Vec<u8>, SmashError> {
    let mut image = Jpeg::from_bytes(Bytes::copy_from_slice(bytes))
        .map_err(|error| SmashError::message(format!("this JPEG could not be read: {error}")))?;
    let segments = image.segments_mut();
    let jfif = segments
        .iter()
        .position(|segment| segment.marker() == 0xE0 && segment.contents().starts_with(b"JFIF\0"));
    if let Some(index) = jfif {
        let old = segments[index].contents();
        if old.len() < 14 {
            return Err(SmashError::message("this JPEG density header is cut off"));
        }
        let mut next = old.to_vec();
        next[7] = 1;
        let pair = dpi.to_be_bytes();
        next[8] = pair[0];
        next[9] = pair[1];
        next[10] = pair[0];
        next[11] = pair[1];
        segments[index] = JpegSegment::new_with_contents(0xE0, Bytes::from(next));
    } else {
        segments.insert(0, JpegSegment::new_with_contents(0xE0, jfif_header(dpi)));
    }
    Ok(encode_jpeg(image))
}

fn jfif_header(dpi: u16) -> Bytes {
    let pair = dpi.to_be_bytes();
    Bytes::from(vec![
        b'J', b'F', b'I', b'F', 0, 1, 2, 1, pair[0], pair[1], pair[0], pair[1], 0, 0,
    ])
}

fn encode_jpeg(image: Jpeg) -> Vec<u8> {
    let mut out = Vec::new();
    for piece in image.encoder() {
        out.extend_from_slice(&piece);
    }
    out
}

fn png(bytes: &[u8], dpi: u16) -> Result<Vec<u8>, SmashError> {
    let mut image = Png::from_bytes(Bytes::copy_from_slice(bytes))
        .map_err(|error| SmashError::message(format!("this PNG could not be read: {error}")))?;
    let chunk = PngChunk::new(*b"pHYs", phys_bytes(dpi));
    let chunks = image.chunks_mut();
    if let Some(index) = chunks.iter().position(|chunk| chunk.kind() == *b"pHYs") {
        chunks[index] = chunk;
    } else if let Some(index) = chunks.iter().position(|chunk| chunk.kind() == *b"IDAT") {
        chunks.insert(index, chunk);
    } else {
        return Err(SmashError::message("this PNG has no picture data"));
    }
    let mut out = Vec::new();
    for piece in image.encoder() {
        out.extend_from_slice(&piece);
    }
    Ok(out)
}

fn webp(bytes: &[u8], dpi: u16) -> Outcome {
    let Some(chunks) = webp_chunks(bytes) else {
        return Outcome::Skipped {
            bytes: bytes.to_vec(),
            container: "WEBP",
        };
    };
    let Some(file) = webp_with_exif(&chunks, dpi) else {
        return Outcome::Skipped {
            bytes: bytes.to_vec(),
            container: "WEBP",
        };
    };
    Outcome::Stored(file)
}

fn webp_chunks(bytes: &[u8]) -> Option<Vec<([u8; 4], Vec<u8>)>> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let mut index = 12;
    let mut chunks = Vec::new();
    while index + 8 <= bytes.len() {
        let tag = bytes[index..index + 4].try_into().ok()?;
        let size = u32::from_le_bytes(bytes[index + 4..index + 8].try_into().ok()?) as usize;
        let start = index + 8;
        let end = start.checked_add(size)?;
        if end > bytes.len() {
            return None;
        }
        chunks.push((tag, bytes[start..end].to_vec()));
        index = end + (size & 1);
    }
    Some(chunks)
}

fn webp_with_exif(chunks: &[([u8; 4], Vec<u8>)], dpi: u16) -> Option<Vec<u8>> {
    let lossless = chunks.iter().find(|chunk| chunk.0 == *b"VP8L")?;
    let (width, height, alpha) = vp8l_size(&lossless.1)?;
    let mut flags = 1 << 3;
    if alpha {
        flags |= 1 << 4;
    }
    if chunks.iter().any(|chunk| chunk.0 == *b"ICCP") {
        flags |= 1 << 5;
    }
    if chunks.iter().any(|chunk| chunk.0 == *b"XMP ") {
        flags |= 1 << 2;
    }
    let mut vp8x = [0u8; 10];
    vp8x[0] = flags;
    vp8x[4..7].copy_from_slice(&(width - 1).to_le_bytes()[..3]);
    vp8x[7..10].copy_from_slice(&(height - 1).to_le_bytes()[..3]);
    let exif = exif_density(dpi);
    let mut body = Vec::new();
    body.extend_from_slice(b"WEBP");
    write_chunk(&mut body, *b"VP8X", &vp8x);
    for (tag, data) in chunks {
        if *tag == *b"VP8X" || *tag == *b"EXIF" {
            continue;
        }
        write_chunk(&mut body, *tag, data);
        if *tag == *b"VP8L" {
            write_chunk(&mut body, *b"EXIF", &exif);
        }
    }
    let mut file = Vec::with_capacity(body.len() + 8);
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(u32::try_from(body.len()).ok()?).to_le_bytes());
    file.extend(body);
    Some(file)
}

fn vp8l_size(data: &[u8]) -> Option<(u32, u32, bool)> {
    if data.len() < 5 || data[0] != 0x2f {
        return None;
    }
    let bits = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
    let width = (bits & 0x3fff) + 1;
    let height = ((bits >> 14) & 0x3fff) + 1;
    let alpha = (bits >> 28) & 1 == 1;
    Some((width, height, alpha))
}

fn exif_density(dpi: u16) -> Vec<u8> {
    let dpi = u32::from(dpi);
    let mut out = Vec::with_capacity(66);
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    push_exif(&mut out, 0x011a, 5, 1, 50);
    push_exif(&mut out, 0x011b, 5, 1, 58);
    push_exif(&mut out, 0x0128, 3, 1, 2);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&dpi.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&dpi.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out
}

fn push_exif(out: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32) {
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_chunk(out: &mut Vec<u8>, tag: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&tag);
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

fn jpeg_dpi(bytes: &[u8]) -> Option<OriginalDensity> {
    let image = Jpeg::from_bytes(Bytes::copy_from_slice(bytes)).ok()?;
    let segment = image
        .segments()
        .iter()
        .find(|segment| segment.marker() == 0xE0 && segment.contents().starts_with(b"JFIF\0"))?;
    let contents = segment.contents();
    if contents.len() < 14 {
        return None;
    }
    let x = u16::from_be_bytes([contents[8], contents[9]]);
    let y = u16::from_be_bytes([contents[10], contents[11]]);
    match contents[7] {
        1 if x >= 1 && y >= 1 => Some(OriginalDensity { x, y }),
        2 => Some(OriginalDensity {
            x: dpcm_to_dpi(x)?,
            y: dpcm_to_dpi(y)?,
        }),
        _ => None,
    }
}

fn dpcm_to_dpi(dots: u16) -> Option<u16> {
    let dpi = (u32::from(dots) * 254 + 50) / 100;
    u16::try_from(dpi).ok().filter(|dpi| *dpi >= 1)
}

fn png_dpi(bytes: &[u8]) -> Option<OriginalDensity> {
    let image = Png::from_bytes(Bytes::copy_from_slice(bytes)).ok()?;
    let chunk = image
        .chunks()
        .iter()
        .find(|chunk| chunk.kind() == *b"pHYs")?;
    let contents = chunk.contents();
    if contents.len() < 9 || contents[8] != 1 {
        return None;
    }
    let x = u32::from_be_bytes(contents[0..4].try_into().ok()?);
    let y = u32::from_be_bytes(contents[4..8].try_into().ok()?);
    Some(OriginalDensity {
        x: ppm_to_dpi(x)?,
        y: ppm_to_dpi(y)?,
    })
}

fn ppm_to_dpi(ppm: u32) -> Option<u16> {
    let dpi = (u64::from(ppm) * 254 + 5_000) / 10_000;
    u16::try_from(dpi).ok().filter(|dpi| *dpi >= 1)
}

fn phys_bytes(dpi: u16) -> Bytes {
    let ppm = pixels_per_meter(dpi);
    let mut contents = Vec::with_capacity(9);
    contents.extend_from_slice(&ppm.to_be_bytes());
    contents.extend_from_slice(&ppm.to_be_bytes());
    contents.push(1);
    Bytes::from(contents)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{apply, pixels_per_meter, skip_note, Outcome};
    use crate::sniff::Kind;
    use image::{GenericImageView, ImageEncoder};

    #[test]
    fn seventy_two_dpi_is_2835_pixels_per_meter() {
        assert_eq!(pixels_per_meter(72), 2835);
        assert_eq!(pixels_per_meter(300), 11_811);
    }

    #[test]
    fn jpeg_density_goes_up_without_moving_pixels() {
        let mut bytes = Vec::new();
        let encoder = jpeg_encoder::Encoder::new(&mut bytes, 90);
        encoder
            .encode(&[255, 0, 0], 1, 1, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let out = stored(&bytes, Kind::Jpeg, 300);
        let start = out
            .windows(5)
            .position(|window| window == *b"JFIF\0")
            .unwrap();
        let pair = 300u16.to_be_bytes();
        assert_eq!(out[start + 7], 1);
        assert_eq!(
            &out[start + 8..start + 12],
            &[pair[0], pair[1], pair[0], pair[1]]
        );
    }

    #[test]
    fn png_density_is_pixels_per_meter() {
        let mut raw = Vec::new();
        let mut encoder = png::Encoder::new(&mut raw, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255, 0, 0]).unwrap();
        writer.finish().unwrap();
        let out = stored(&raw, Kind::Png, 72);
        let image = img_parts::png::Png::from_bytes(img_parts::Bytes::from(out)).unwrap();
        let chunk = image
            .chunks()
            .iter()
            .find(|chunk| chunk.kind() == *b"pHYs")
            .unwrap();
        let ppm = 2835u32.to_be_bytes();
        assert_eq!(&chunk.contents()[..4], &ppm);
        assert_eq!(&chunk.contents()[4..8], &ppm);
        assert_eq!(chunk.contents()[8], 1);
    }

    #[test]
    fn a_gif_keeps_going_without_a_density() {
        let skipped = apply(b"GIF89a", None, Kind::Gif, 72).unwrap();
        let Outcome::Skipped { bytes, container } = skipped else {
            panic!("gif should skip density");
        };
        assert_eq!(bytes, b"GIF89a");
        assert_eq!(container, "GIF");
        let note = skip_note(72, container, None);
        assert!(note.contains("not stored"));
        assert!(note.contains("no display density"));
    }

    #[test]
    fn webp_density_is_an_exif_block() {
        let mut raw = Vec::new();
        let image = image::RgbImage::from_pixel(1, 1, image::Rgb([1, 2, 3]));
        image::codecs::webp::WebPEncoder::new_lossless(&mut raw)
            .write_image(image.as_raw(), 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        let out = match apply(&raw, Some(crate::ext::Ext::Webp), Kind::Png, 72).unwrap() {
            Outcome::Stored(out) => out,
            Outcome::Skipped { .. } => panic!("webp should store density"),
        };
        assert!(out.windows(4).any(|window| window == *b"EXIF"));
        assert!(out.windows(4).any(|window| window == 72u32.to_le_bytes()));
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.dimensions(), (1, 1));
    }

    fn stored(bytes: &[u8], kind: Kind, dpi: u16) -> Vec<u8> {
        match apply(bytes, None, kind, dpi).unwrap() {
            Outcome::Stored(out) => out,
            Outcome::Skipped { .. } => panic!("density should be stored"),
        }
    }
}
