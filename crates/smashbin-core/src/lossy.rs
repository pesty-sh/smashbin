//! Step 6. A new JPEG, and only when the user asked.
//!
//! No `--quality` and no `--bytes`: return the lossless bytes and stop.
//! `--bytes`, and the lossless file is already small enough: still stop.
//! A loose cap must not add damage. A resize that already fits is kept.
//!
//! PNG and GIF have no quality knob. Asking for one is an error. A byte cap
//! they already meet is fine. A byte cap they miss stays the same format.

use image::RgbImage;
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

use crate::cli::Job;
use crate::error::SmashError;
use crate::sniff::{self, Kind, Picture};

/// How step 6 treated the file. The report prints this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// Pixels did not change.
    Lossless,
    /// The picture was resized. No quality override was applied.
    Resize,
    /// The picture was cropped. Pixels were not stretched.
    Crop,
    /// `--format` wrote a different container.
    Convert,
    /// One JPEG encode at a chosen quality.
    Quality(u8),
    /// Several JPEG encodes. `fit` is false when even quality 1 is too big.
    Bytes { quality: u8, fit: bool },
}

impl How {
    /// The word used in the stderr report.
    #[must_use]
    pub fn as_str(self) -> String {
        match self {
            Self::Lossless => "lossless".to_owned(),
            Self::Resize => "resize".to_owned(),
            Self::Crop => "crop".to_owned(),
            Self::Convert => "convert".to_owned(),
            Self::Quality(quality) => format!("quality={quality}"),
            Self::Bytes { quality, fit } => {
                if fit {
                    format!("bytes quality={quality}")
                } else {
                    format!("bytes quality={quality} over-cap")
                }
            }
        }
    }
}

/// Apply the override, or keep the lossless bytes.
///
/// # Errors
///
/// `--quality` or `--bytes` was asked of a PNG or GIF, or the JPEG could
/// not be decoded.
///
/// `base` is lossless, resize, or crop from step 5. `jpeg` is that
/// JPEG, so this step does not decode it again.
pub fn apply(
    lossless_bytes: &[u8],
    picture: &Picture,
    job: &Job,
    base: How,
    jpeg: Option<&RgbImage>,
) -> Result<(Vec<u8>, How), SmashError> {
    let kept = base;
    if job.quality.is_none() && job.byte_cap.is_none() {
        return Ok((lossless_bytes.to_vec(), kept));
    }
    if let Some(cap) = job.byte_cap {
        let cap = usize::try_from(cap).unwrap_or(usize::MAX);
        if lossless_bytes.len() <= cap {
            return Ok((lossless_bytes.to_vec(), kept));
        }
    }
    if picture.kind != Kind::Jpeg {
        return Err(not_jpeg(picture.kind, job));
    }
    if let Some(quality) = job.quality {
        let encoded = encode_source(lossless_bytes, jpeg, quality)?;
        return Ok((encoded, How::Quality(quality)));
    }
    let Some(cap) = job.byte_cap else {
        return Ok((lossless_bytes.to_vec(), kept));
    };
    let cap = usize::try_from(cap).unwrap_or(usize::MAX);
    let (encoded, quality, fit) = if let Some(rgb) = jpeg {
        search(rgb, cap)?
    } else {
        let rgb = decode_rgb(lossless_bytes)?;
        search(&rgb, cap)?
    };
    Ok((encoded, How::Bytes { quality, fit }))
}

fn not_jpeg(kind: Kind, job: &Job) -> SmashError {
    let asked = if job.quality.is_some() {
        "--quality"
    } else {
        "--bytes"
    };
    SmashError::message(format!(
        "{asked} writes a new JPEG. This file is a {}. This version does not change format.",
        kind.as_str()
    ))
}

fn decode_rgb(bytes: &[u8]) -> Result<RgbImage, SmashError> {
    Ok(decode_limited(bytes)?.into_rgb8())
}

/// Decoder limits shared by every entrance. A header check runs first.
/// These limits are the second gate, inside the decoder.
#[must_use]
pub(crate) fn image_limits() -> image::Limits {
    let mut limits = image::Limits::no_limits();
    limits.max_image_width = Some(sniff::MAX_SIDE);
    limits.max_image_height = Some(sniff::MAX_SIDE);
    limits.max_alloc = Some(sniff::MAX_PIXELS.saturating_mul(8));
    limits
}

/// Decode one still picture with [`image_limits`].
///
/// # Errors
///
/// The bytes are not a picture this crate can decode, or they exceed the limits.
pub(crate) fn decode_limited(bytes: &[u8]) -> Result<image::DynamicImage, SmashError> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| {
            SmashError::message(format!("this image could not be decoded: {error}"))
        })?;
    reader.limits(image_limits());
    reader.decode().map_err(|error| image_error(&error))
}

/// Refuse a decoder that advertises a buffer bigger than the pixel cap.
///
/// # Errors
///
/// `total_bytes` is over eight bytes per pixel at [`sniff::MAX_PIXELS`].
pub(crate) fn within_alloc(total_bytes: u64) -> Result<(), SmashError> {
    if total_bytes > sniff::MAX_PIXELS.saturating_mul(8) {
        return Err(SmashError::message(
            "this image asks for more memory than this version will use",
        ));
    }
    Ok(())
}

pub(crate) fn image_error(error: &image::ImageError) -> SmashError {
    let text = error.to_string();
    if text.len() > 180 {
        SmashError::message("this image could not be decoded")
    } else {
        SmashError::message(format!("this image could not be decoded: {text}"))
    }
}

fn encode_jpeg(bytes: &[u8], quality: u8) -> Result<Vec<u8>, SmashError> {
    let rgb = decode_rgb(bytes)?;
    encode_rgb(&rgb, quality)
}

fn encode_source(
    bytes: &[u8],
    jpeg: Option<&RgbImage>,
    quality: u8,
) -> Result<Vec<u8>, SmashError> {
    if let Some(rgb) = jpeg {
        encode_rgb(rgb, quality)
    } else {
        encode_jpeg(bytes, quality)
    }
}

/// Write one JPEG. Quality 100 is the resized file when no override was typed.
///
/// # Errors
///
/// The encoder rejects the dimensions or the quality.
pub(crate) fn encode_rgb(rgb: &RgbImage, quality: u8) -> Result<Vec<u8>, SmashError> {
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, quality);
    // 90 and above keep full color on each pixel, which holds a hard edge.
    // Below 90 the encoder averages color in 2x2 blocks. That is smaller.
    let sampling = if quality >= 90 {
        SamplingFactor::R_4_4_4
    } else {
        SamplingFactor::R_4_2_0
    };
    encoder.set_sampling_factor(sampling);
    encoder.set_progressive(true);
    encoder.set_optimized_huffman_tables(true);
    let (width, height) = rgb.dimensions();
    let width = u16::try_from(width)
        .map_err(|_| SmashError::message("this JPEG is wider than this version can re-encode"))?;
    let height = u16::try_from(height)
        .map_err(|_| SmashError::message("this JPEG is taller than this version can re-encode"))?;
    encoder
        .encode(rgb.as_raw(), width, height, ColorType::Rgb)
        .map_err(|error| SmashError::message(format!("the JPEG could not be written: {error}")))?;
    Ok(out)
}

/// Highest quality whose file is at or under `cap`.
///
/// Eight or so encodes. The decoded pixels are reused, so only the encode
/// repeats. Quality is not perfectly monotonic, but it is close enough for
/// a byte ceiling.
pub(crate) fn search(rgb: &RgbImage, cap: usize) -> Result<(Vec<u8>, u8, bool), SmashError> {
    let mut low = 1u8;
    let mut high = 100u8;
    let mut best: Option<(Vec<u8>, u8)> = None;
    while low <= high {
        let quality = low + (high - low) / 2;
        let encoded = encode_rgb(rgb, quality)?;
        if encoded.len() <= cap {
            best = Some((encoded, quality));
            if quality == 100 {
                break;
            }
            low = quality + 1;
        } else if quality == 1 {
            return Ok((encoded, 1, false));
        } else {
            high = quality - 1;
        }
    }
    let (encoded, quality) =
        best.ok_or_else(|| SmashError::message("the byte target could not be searched"))?;
    Ok((encoded, quality, true))
}
