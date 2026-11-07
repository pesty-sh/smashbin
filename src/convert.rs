//! `--ext` writes a different container from the same pixels.
//!
//! The source is still a PNG, JPEG, or GIF. This step runs only when the
//! requested container is a different one. JPEG output uses the same encoder
//! as `--quality`. With no quality typed, that encode is quality 100.
//! WebP is lossless. A GIF animation becomes its first frame.

use std::io::Cursor;

use image::{DynamicImage, ImageEncoder, ImageFormat, RgbImage};

use crate::cli::Job;
use crate::error::SmashError;
use crate::ext::Ext;
use crate::lossy::{self, How};
use crate::sniff::Picture;

/// Encode `bytes` as `job.ext`.
///
/// # Errors
///
/// The picture cannot be decoded, the encoder rejects it, or an ICO is
/// larger than 256 pixels on a side.
pub fn apply(
    bytes: &[u8],
    picture: &Picture,
    job: &Job,
    resized: bool,
    jpeg: Option<&RgbImage>,
) -> Result<(Vec<u8>, How), SmashError> {
    let Some(ext) = job.ext else {
        return Err(SmashError::message("missing --ext"));
    };
    if ext == Ext::Jpeg {
        return write_jpeg(bytes, job, resized, jpeg);
    }
    if ext == Ext::Ico && (picture.width > 256 || picture.height > 256) {
        return Err(SmashError::message(
            "an ICO holds at most 256 by 256. Pass --width or --height.",
        ));
    }
    let image = decode(bytes, jpeg)?;
    let encoded = encode_image(&image, ext)?;
    Ok((encoded, plain(resized)))
}

fn encode_image(image: &DynamicImage, ext: Ext) -> Result<Vec<u8>, SmashError> {
    let mut cursor = Cursor::new(Vec::new());
    if ext == Ext::Hdr {
        write_hdr(&image.to_rgb32f(), &mut cursor)?;
    } else if ext == Ext::Farbfeld {
        write_farbfeld(&image.to_rgba16(), &mut cursor)?;
    } else {
        image.write_to(&mut cursor, image_format(ext)).map_err(|error| {
            SmashError::message(format!("this {} could not be written: {error}", ext.as_str()))
        })?;
    }
    Ok(cursor.into_inner())
}

/// Radiance HDR stores linear floats. The other encoders take 8-bit samples.
fn write_hdr(rgb: &image::Rgb32FImage, cursor: &mut Cursor<Vec<u8>>) -> Result<(), SmashError> {
    let mut bytes = Vec::with_capacity(rgb.as_raw().len() * 4);
    for sample in rgb.as_raw() {
        bytes.extend_from_slice(&sample.to_ne_bytes());
    }
    image::codecs::hdr::HdrEncoder::new(&mut *cursor)
        .write_image(
            &bytes,
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb32F,
        )
        .map_err(|error| SmashError::message(format!("this HDR could not be written: {error}")))
}

/// farbfeld stores 16-bit RGBA, big-endian on disk. The encoder reads native samples.
fn write_farbfeld(
    rgba: &image::ImageBuffer<image::Rgba<u16>, Vec<u16>>,
    cursor: &mut Cursor<Vec<u8>>,
) -> Result<(), SmashError> {
    let mut bytes = Vec::with_capacity(rgba.as_raw().len() * 2);
    for sample in rgba.as_raw() {
        bytes.extend_from_slice(&sample.to_ne_bytes());
    }
    image::codecs::farbfeld::FarbfeldEncoder::new(&mut *cursor)
        .write_image(
            &bytes,
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba16,
        )
        .map_err(|error| SmashError::message(format!("this FARBFELD could not be written: {error}")))
}

fn write_jpeg(
    bytes: &[u8],
    job: &Job,
    resized: bool,
    jpeg: Option<&RgbImage>,
) -> Result<(Vec<u8>, How), SmashError> {
    let owned;
    let rgb = if let Some(rgb) = jpeg {
        rgb
    } else {
        owned = decode(bytes, None)?.to_rgb8();
        &owned
    };
    if let Some(quality) = job.quality {
        return Ok((lossy::encode_rgb(rgb, quality)?, How::Quality(quality)));
    }
    if let Some(cap) = job.byte_cap {
        let cap = usize::try_from(cap).unwrap_or(usize::MAX);
        let (encoded, quality, fit) = lossy::search(rgb, cap)?;
        return Ok((encoded, How::Bytes { quality, fit }));
    }
    Ok((lossy::encode_rgb(rgb, 100)?, plain(resized)))
}

fn plain(resized: bool) -> How {
    if resized {
        How::Resize
    } else {
        How::Convert
    }
}

fn decode(bytes: &[u8], jpeg: Option<&RgbImage>) -> Result<DynamicImage, SmashError> {
    if let Some(rgb) = jpeg {
        return Ok(DynamicImage::ImageRgb8(rgb.clone()));
    }
    image::load_from_memory(bytes)
        .map_err(|error| SmashError::message(format!("this image could not be read: {error}")))
}

fn image_format(ext: Ext) -> ImageFormat {
    match ext {
        Ext::Jpeg => ImageFormat::Jpeg,
        Ext::Png => ImageFormat::Png,
        Ext::Gif => ImageFormat::Gif,
        Ext::Webp => ImageFormat::WebP,
        Ext::Bmp => ImageFormat::Bmp,
        Ext::Tiff => ImageFormat::Tiff,
        Ext::Tga => ImageFormat::Tga,
        Ext::Ico => ImageFormat::Ico,
        Ext::Qoi => ImageFormat::Qoi,
        Ext::Ppm => ImageFormat::Pnm,
        Ext::Hdr => ImageFormat::Hdr,
        Ext::Farbfeld => ImageFormat::Farbfeld,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{encode_image, image_format};
    use crate::ext::Ext;
    use image::{DynamicImage, ImageFormat, RgbImage};

    #[test]
    fn every_free_format_has_an_encoder() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, image::Rgb([9, 8, 7])));
        for ext in [
            Ext::Png,
            Ext::Gif,
            Ext::Webp,
            Ext::Bmp,
            Ext::Tiff,
            Ext::Tga,
            Ext::Ico,
            Ext::Qoi,
            Ext::Ppm,
            Ext::Hdr,
            Ext::Farbfeld,
        ] {
            let encoded = encode_image(&image, ext).unwrap();
            assert!(!encoded.is_empty(), "{}", ext.as_str());
        }
        assert_eq!(image_format(Ext::Jpeg), ImageFormat::Jpeg);
    }
}
