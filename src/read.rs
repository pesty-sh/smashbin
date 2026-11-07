//! Step 2. Get the bytes. Do not look at them yet.
//!
//! A path is read from disk. No path means stdin. The bytes stay in memory
//! because the later steps need to walk them more than once.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use crate::error::SmashError;

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
        fs::read(path)?
    } else {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes)?;
        bytes
    };
    if bytes.is_empty() {
        return Err(SmashError::message("the input is empty"));
    }
    Ok(Input { bytes })
}
