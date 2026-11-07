//! Step 7. The GitHub avatar rule.
//!
//! GitHub's profile picture page says the file must be a PNG, JPG, or GIF,
//! less than 1 MB, and smaller than 3000 by 3000 pixels. The format was
//! already checked in step 3. This step checks the size and the edges.
//!
//! "Less than 1 MB" is treated as 1,000,000 bytes. A file of exactly
//! 1,000,000 bytes does not pass.
//!
//! Failing this check is exit 2, not exit 1. The image was still written.
//! Exit 1 is reserved for a file we could not read or a flag that makes no
//! sense. Pass `--width` or `--height` when an edge is too long.

use crate::lossy::How;

/// Under this many bytes.
pub const MAX_BYTES: usize = 1_000_000;
/// Both edges must be strictly smaller than 3000.
pub const MAX_EDGE: u32 = 3000;

/// Fit, or written-but-rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// GitHub's published rule accepts this file.
    Fit,
    /// The file was written. GitHub would still refuse it.
    Rejected,
}

/// The check, plus a sentence when it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gate {
    /// Exit 0 or exit 2.
    pub status: Status,
    /// `None` when the file fits.
    pub reason: Option<String>,
}

/// Decide. `how` is included so a missed `--bytes` cap is named even when
/// the file is still under 1 MB.
#[must_use]
pub fn check(width: u32, height: u32, len: usize, how: How) -> Gate {
    let mut reasons = Vec::new();
    if width >= MAX_EDGE || height >= MAX_EDGE {
        reasons.push(format!(
            "{width} by {height} is not smaller than {MAX_EDGE} by {MAX_EDGE}. Pass --width or --height."
        ));
    }
    if len >= MAX_BYTES {
        reasons.push(format!(
            "{len} bytes is not under {MAX_BYTES}. Pass --bytes 1mb or --quality."
        ));
    }
    if let How::Bytes {
        fit: false,
        quality,
    } = how
    {
        reasons.push(format!(
            "quality {quality} is as small as this version gets, and the file is still over the byte target."
        ));
    }
    if reasons.is_empty() {
        Gate {
            status: Status::Fit,
            reason: None,
        }
    } else {
        Gate {
            status: Status::Rejected,
            reason: Some(reasons.join(" ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{check, Status, MAX_BYTES, MAX_EDGE};
    use crate::lossy::How;

    #[test]
    fn the_sample_logo_shape_fits() {
        let gate = check(1024, 1024, 82_180, How::Lossless);
        assert_eq!(gate.status, Status::Fit);
        assert!(gate.reason.is_none());
    }

    #[test]
    fn one_megabyte_and_three_thousand_pixels_do_not_fit() {
        let gate = check(MAX_EDGE, 10, MAX_BYTES, How::Lossless);
        assert_eq!(gate.status, Status::Rejected);
    }
}
