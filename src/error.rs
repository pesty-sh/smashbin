//! Sentences for a failure, and nothing else.
//!
//! Exit 1 is one of these. A written image exits 0.

use std::fmt;

/// A problem that stops the program before a useful image exists.
#[derive(Debug)]
pub enum SmashError {
    /// The disk or the pipe failed.
    Io(std::io::Error),
    /// A rule in this program failed. The text is safe to print.
    Message(String),
}

impl SmashError {
    /// Build a printable failure. Do not put image bytes in `text`.
    pub fn message(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

impl fmt::Display for SmashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "could not read or write the file: {error}"),
            Self::Message(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for SmashError {}

impl From<std::io::Error> for SmashError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
