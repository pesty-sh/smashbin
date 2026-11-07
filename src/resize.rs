//! Step 5. A new width, a new height, or both.
//!
//! Neither flag means this step does nothing. The pixels stay, and step 6
//! can still shrink a PNG without decoding it into a new picture.
//!
//! One flag and not the other keeps the aspect ratio. Both flags mean those
//! exact pixel counts, even when the picture becomes taller or wider than it was.
//!
//! The file stays a PNG, a JPEG, or a GIF. `--quality` or `--bytes` may replace
//! the JPEG afterwards. That replacement encodes these resized pixels once.

use std::io::Cursor;

use image::imageops::FilterType;
use image::{AnimationDecoder, RgbImage};

use crate::error::SmashError;
use crate::lossy;
use crate::sniff::{Kind, Picture};

/// The pixel size after `--width` and `--height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Bytes in the same format, plus JPEG pixels when a later encode needs them.
pub struct Scaled {
    /// The resized file. Empty when a JPEG will be encoded by `--quality` instead.
    pub bytes: Vec<u8>,
    /// Width and height after the resize. The format did not change.
    pub picture: Picture,
    /// Set for a JPEG. `--quality` and `--bytes` encode this, not a second decode.
    pub jpeg: Option<RgbImage>,
}

/// The size to write, or nothing when the flags are absent or already match.
///
/// # Errors
///
/// The missing side would be 0, or either side would be above 65535.
pub fn target(
    picture: &Picture,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<Option<Size>, SmashError> {
    let Some(size) = fit(picture.width, picture.height, width, height)? else {
        return Ok(None);
    };
    if size.width == picture.width && size.height == picture.height {
        return Ok(None);
    }
    Ok(Some(size))
}

/// Resize `bytes` into `size` without changing the format.
///
/// `keep_jpeg_file` writes the JPEG at quality 100. Pass false when
/// `--quality` will write the only JPEG, so the pixels are not encoded twice.
///
/// # Errors
///
/// The file cannot be decoded, or an animated PNG would lose its frames.
pub fn scale(
    bytes: &[u8],
    picture: &Picture,
    size: Size,
    keep_jpeg_file: bool,
) -> Result<Scaled, SmashError> {
    let scaled = Picture {
        kind: picture.kind,
        width: size.width,
        height: size.height,
    };
    match picture.kind {
        Kind::Jpeg => scale_jpeg(bytes, scaled, keep_jpeg_file),
        Kind::Png => Ok(Scaled {
            bytes: scale_png(bytes, picture, size)?,
            picture: scaled,
            jpeg: None,
        }),
        Kind::Gif => Ok(Scaled {
            bytes: scale_gif(bytes, size)?,
            picture: scaled,
            jpeg: None,
        }),
    }
}

/// `None` when both flags are absent.
fn fit(
    src_w: u32,
    src_h: u32,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<Option<Size>, SmashError> {
    let size = match (width, height) {
        (None, None) => return Ok(None),
        (Some(width), Some(height)) => Size { width, height },
        (Some(width), None) => Size {
            width,
            height: other_side(src_w, src_h, width)?,
        },
        (None, Some(height)) => Size {
            width: other_side(src_h, src_w, height)?,
            height,
        },
    };
    Ok(Some(size))
}

/// The side the user did not type, rounded to the nearest pixel and at least 1.
fn other_side(src_known: u32, src_other: u32, new_known: u32) -> Result<u32, SmashError> {
    if src_known == 0 {
        return Err(SmashError::message("this image has a zero side"));
    }
    let known = u64::from(src_known);
    let scaled = (u64::from(src_other) * u64::from(new_known) + known / 2) / known;
    let side = u32::try_from(scaled.max(1)).unwrap_or(u32::MAX);
    if side > u32::from(u16::MAX) {
        return Err(SmashError::message(
            "the new width or height is above 65535. This version stops there.",
        ));
    }
    Ok(side)
}

fn scale_jpeg(bytes: &[u8], picture: Picture, keep_jpeg_file: bool) -> Result<Scaled, SmashError> {
    let decoded = image::load_from_memory(bytes)
        .map_err(|error| SmashError::message(format!("this JPEG could not be decoded: {error}")))?;
    let rgb = image::imageops::resize(
        &decoded.to_rgb8(),
        picture.width,
        picture.height,
        FilterType::Lanczos3,
    );
    let bytes = if keep_jpeg_file {
        lossy::encode_rgb(&rgb, 100)?
    } else {
        Vec::new()
    };
    Ok(Scaled {
        bytes,
        picture,
        jpeg: Some(rgb),
    })
}

fn scale_png(bytes: &[u8], picture: &Picture, size: Size) -> Result<Vec<u8>, SmashError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| SmashError::message(format!("this PNG could not be read: {error}")))?;
    if reader.info().animation_control.is_some() {
        return Err(SmashError::message(
            "this PNG has more than one frame. This version resizes one picture.",
        ));
    }
    let (color, depth) = reader.output_color_type();
    if depth != png::BitDepth::Eight {
        return Err(SmashError::message(
            "this PNG could not be resized at its bit depth",
        ));
    }
    let buffer_size = reader.output_buffer_size().ok_or_else(|| {
        SmashError::message("this PNG is larger than this version will decompress")
    })?;
    let mut pixels = vec![0; buffer_size];
    let written = reader
        .next_frame(&mut pixels)
        .map_err(|error| SmashError::message(format!("this PNG could not be read: {error}")))?;
    let pixels = &pixels[..written.buffer_size()];
    let raw = match color {
        png::ColorType::Grayscale => {
            let image = image_from_raw::<image::Luma<u8>>(picture, pixels)?;
            image::imageops::resize(&image, size.width, size.height, FilterType::Lanczos3)
                .into_raw()
        }
        png::ColorType::GrayscaleAlpha => {
            let image = image_from_raw::<image::LumaA<u8>>(picture, pixels)?;
            image::imageops::resize(&image, size.width, size.height, FilterType::Lanczos3)
                .into_raw()
        }
        png::ColorType::Rgb => {
            let image = image_from_raw::<image::Rgb<u8>>(picture, pixels)?;
            image::imageops::resize(&image, size.width, size.height, FilterType::Lanczos3)
                .into_raw()
        }
        png::ColorType::Rgba => {
            let image = image_from_raw::<image::Rgba<u8>>(picture, pixels)?;
            image::imageops::resize(&image, size.width, size.height, FilterType::Lanczos3)
                .into_raw()
        }
        png::ColorType::Indexed => {
            return Err(SmashError::message(
                "this PNG palette could not be expanded for a resize",
            ));
        }
    };
    write_png(&raw, size, color)
}

fn image_from_raw<P>(
    picture: &Picture,
    pixels: &[u8],
) -> Result<image::ImageBuffer<P, Vec<u8>>, SmashError>
where
    P: image::Pixel<Subpixel = u8> + 'static,
{
    image::ImageBuffer::from_raw(picture.width, picture.height, pixels.to_vec())
        .ok_or_else(|| SmashError::message("this PNG could not be read"))
}

fn write_png(pixels: &[u8], size: Size, color: png::ColorType) -> Result<Vec<u8>, SmashError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size.width, size.height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder
        .write_header()
        .map_err(|error| SmashError::message(format!("this PNG could not be written: {error}")))?;
    writer
        .write_image_data(pixels)
        .map_err(|error| SmashError::message(format!("this PNG could not be written: {error}")))?;
    writer
        .finish()
        .map_err(|error| SmashError::message(format!("this PNG could not be written: {error}")))?;
    Ok(out)
}

fn scale_gif(bytes: &[u8], size: Size) -> Result<Vec<u8>, SmashError> {
    let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
        .map_err(|error| SmashError::message(format!("this GIF could not be read: {error}")))?;
    let frames = decoder
        .into_frames()
        .collect_frames()
        .map_err(|error| SmashError::message(format!("this GIF could not be read: {error}")))?;
    if frames.is_empty() {
        return Err(SmashError::message("this GIF has no frames"));
    }
    let mut out = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut out, 10);
        if frames.len() > 1 {
            encoder
                .set_repeat(image::codecs::gif::Repeat::Infinite)
                .map_err(|error| {
                    SmashError::message(format!("this GIF could not be written: {error}"))
                })?;
        }
        for frame in frames {
            let delay = frame.delay();
            let buffer = frame.into_buffer();
            let resized =
                image::imageops::resize(&buffer, size.width, size.height, FilterType::Lanczos3);
            let next = image::Frame::from_parts(resized, 0, 0, delay);
            encoder.encode_frame(next).map_err(|error| {
                SmashError::message(format!("this GIF could not be written: {error}"))
            })?;
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{fit, target};
    use crate::sniff::{Kind, Picture};

    #[test]
    fn a_missing_side_keeps_the_aspect_ratio() {
        let size = fit(1024, 512, Some(512), None).unwrap().unwrap();
        assert_eq!((size.width, size.height), (512, 256));
        let size = fit(100, 50, None, Some(25)).unwrap().unwrap();
        assert_eq!((size.width, size.height), (50, 25));
        let size = fit(3, 2, Some(2), None).unwrap().unwrap();
        assert_eq!((size.width, size.height), (2, 1));
    }

    #[test]
    fn both_sides_are_used_as_typed() {
        let size = fit(1024, 512, Some(20), Some(10)).unwrap().unwrap();
        assert_eq!((size.width, size.height), (20, 10));
    }

    #[test]
    fn the_same_size_does_not_resample() {
        let picture = Picture {
            kind: Kind::Jpeg,
            width: 4,
            height: 2,
        };
        assert!(target(&picture, Some(4), None).unwrap().is_none());
        assert!(target(&picture, None, None).unwrap().is_none());
    }

    #[test]
    fn a_side_above_65535_is_rejected() {
        let error = fit(2, 65_535, Some(3), None).unwrap_err();
        assert!(error.to_string().contains("65535"));
    }
}
