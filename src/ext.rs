//! The container named by `--ext` / `-e`.
//!
//! These are the formats the pure-Rust `image` crate can write without a C
//! library. WebP here is lossless. A lossy WebP encoder is `libwebp`, which
//! is C, so it is not offered. AVIF is the same: the C decoder is out, and
//! the Rust encoder is a second large library.

use crate::sniff::Kind;

/// One output container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ext {
    /// JPEG. `jpg`, `jpeg`, and `jfif` all mean this.
    Jpeg,
    /// PNG.
    Png,
    /// GIF. An animation becomes its first frame.
    Gif,
    /// Lossless WebP.
    Webp,
    /// BMP.
    Bmp,
    /// TIFF.
    Tiff,
    /// TGA.
    Tga,
    /// ICO. The picture must be 256 by 256 or smaller.
    Ico,
    /// QOI.
    Qoi,
    /// PPM, the color Netpbm file.
    Ppm,
    /// Radiance HDR.
    Hdr,
    /// farbfeld.
    Farbfeld,
}

impl Ext {
    /// The words `--ext` accepts, for errors.
    pub const LIST: &'static str =
        "jpg, png, gif, webp, bmp, tif, tga, ico, qoi, ppm, hdr, or ff";

    /// Read a typed extension. The leading dot is optional.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_start_matches('.').to_ascii_lowercase();
        Some(match text.as_str() {
            "jpg" | "jpeg" | "jfif" => Self::Jpeg,
            "png" => Self::Png,
            "gif" => Self::Gif,
            "webp" => Self::Webp,
            "bmp" => Self::Bmp,
            "tif" | "tiff" => Self::Tiff,
            "tga" => Self::Tga,
            "ico" => Self::Ico,
            "qoi" => Self::Qoi,
            "ppm" | "pnm" => Self::Ppm,
            "hdr" => Self::Hdr,
            "ff" | "farbfeld" => Self::Farbfeld,
            _ => return None,
        })
    }

    /// The word in the stderr report.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Gif => "GIF",
            Self::Webp => "WEBP",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
            Self::Tga => "TGA",
            Self::Ico => "ICO",
            Self::Qoi => "QOI",
            Self::Ppm => "PPM",
            Self::Hdr => "HDR",
            Self::Farbfeld => "FARBFELD",
        }
    }

    /// The suffix written on a generated file name.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Gif => "gif",
            Self::Webp => "webp",
            Self::Bmp => "bmp",
            Self::Tiff => "tif",
            Self::Tga => "tga",
            Self::Ico => "ico",
            Self::Qoi => "qoi",
            Self::Ppm => "ppm",
            Self::Hdr => "hdr",
            Self::Farbfeld => "ff",
        }
    }

    /// The source file is already this container, so the bytes can stay.
    #[must_use]
    pub const fn same_as(self, kind: Kind) -> bool {
        matches!(
            (self, kind),
            (Self::Jpeg, Kind::Jpeg) | (Self::Png, Kind::Png) | (Self::Gif, Kind::Gif)
        )
    }

}
