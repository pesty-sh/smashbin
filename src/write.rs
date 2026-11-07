//! Step 8. Put the bytes somewhere, and print one line to stderr.
//!
//! Stdout is the image only. A report on stdout would corrupt a pipe.
//!
//! The default path is `name_strip_safe.jpg` next to the source. The source is
//! left in place. `--replace` swaps it only when the new file is smaller.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::avatar::{self, Gate};
use crate::cli::{Dest, Job};
use crate::error::SmashError;
use crate::lossy::How;
use crate::read::Input;
use crate::sniff::Picture;

/// Write the image and the report.
///
/// # Errors
///
/// The destination directory is missing, or the disk is full.
pub fn emit(
    job: &Job,
    input: &Input,
    picture: &Picture,
    finished: &[u8],
    how: How,
    gate: &Gate,
) -> Result<avatar::Status, SmashError> {
    let (written, kept_original) = match &job.dest {
        Dest::Stdout => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(finished)?;
            stdout.flush()?;
            (finished.len(), false)
        }
        Dest::Create(path) => {
            fs::write(path, finished)?;
            (finished.len(), false)
        }
        Dest::Replace(path) => replace_if_smaller(path, input.bytes.len(), finished)?,
    };
    let reported = if kept_original { How::Lossless } else { how };
    let gate = if kept_original {
        avatar::check(picture.width, picture.height, written, reported)
    } else {
        gate.clone()
    };
    report(
        job,
        picture,
        input.bytes.len(),
        written,
        reported,
        &gate,
        kept_original,
    );
    Ok(gate.status)
}

/// Keep the original when the new bytes are not smaller.
fn replace_if_smaller(
    path: &Path,
    before: usize,
    finished: &[u8],
) -> Result<(usize, bool), SmashError> {
    if finished.len() >= before {
        return Ok((before, true));
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".smashbin-tmp");
    let temporary = Path::new(&temporary);
    fs::write(temporary, finished)?;
    fs::rename(temporary, path)?;
    Ok((finished.len(), false))
}

fn report(
    job: &Job,
    picture: &Picture,
    before: usize,
    after: usize,
    how: How,
    gate: &Gate,
    kept_original: bool,
) {
    let name = job
        .source
        .as_ref()
        .map_or_else(|| "stdin".to_owned(), |path| path.display().to_string());
    let verdict = match gate.status {
        avatar::Status::Fit => "avatar=ok",
        avatar::Status::Rejected => "avatar=no",
    };
    let kept = if kept_original { "  kept-original" } else { "" };
    eprintln!(
        "{name}  {before} -> {after}  {}  {}x{}  {}  strip={}  {verdict}{kept}",
        how.as_str(),
        picture.width,
        picture.height,
        picture.kind.as_str(),
        job.strip.as_str()
    );
    if let Some(reason) = &gate.reason {
        eprintln!("{reason}");
    }
}
