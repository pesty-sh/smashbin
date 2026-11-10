//! Step 2. Get the bytes. Do not look at them yet.
//!
//! A path is read from disk. No path means stdin. The bytes stay in memory
//! because the later steps need to walk them more than once.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use crate::error::SmashError;

/// Stop reading once the file is larger than this.
///
/// The gallery sample is about 25 MB. 64 MB leaves room for a larger
/// original and refuses a multi-gigabyte upload before it is allocated.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The bytes of one image.
pub struct Input {
    /// The whole file.
    pub bytes: Vec<u8>,
}

/// Read one image.
///
/// # Errors
///
/// The path is missing, unreadable, or empty.
pub fn load(path: Option<&Path>) -> Result<Input, SmashError> {
    let bytes = if let Some(path) = path {
        read_capped(File::open(path)?, MAX_BYTES)?
    } else {
        read_capped(io::stdin(), MAX_BYTES)?
    };
    if bytes.is_empty() {
        return Err(SmashError::message("the input is empty"));
    }
    Ok(Input { bytes })
}

/// Read until EOF, and stop if the stream passes `cap`.
///
/// # Errors
///
/// The read failed, or the stream is longer than `cap`.
fn read_capped(mut reader: impl Read, cap: u64) -> Result<Vec<u8>, SmashError> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let next = u64::try_from(bytes.len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if next > cap {
            return Err(SmashError::message(
                "this file is larger than 64 MB. This version stops before it reads the rest.",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::io::Cursor;

    use super::read_capped;

    #[test]
    fn a_stream_past_the_cap_is_refused() {
        let error = read_capped(Cursor::new([1u8, 2, 3, 4, 5]), 4).unwrap_err();
        assert!(error.to_string().contains("64 MB"));
    }

    #[test]
    fn a_stream_on_the_cap_is_kept() {
        let bytes = read_capped(Cursor::new([1u8, 2, 3, 4]), 4).unwrap();
        assert_eq!(bytes, [1, 2, 3, 4]);
    }
}
