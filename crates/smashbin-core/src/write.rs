//! Step 8. Put the bytes somewhere, and print one line to stderr.
//!
//! Stdout is the image only. A report on stdout would corrupt a pipe.
//!
//! The default path is `name_strip_safe.jpg` next to the source. The source is
//! left in place. `--replace` swaps it only when the new file is smaller.
//! Missing folders in an output path are created.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::cli::{Dest, Job};
use crate::error::SmashError;
use crate::ext::Ext;
use crate::lossy::How;
use crate::read::Input;
use crate::sniff::Picture;

/// What stderr should say about `--dpi`.
pub struct DensityLine {
    /// Print `dpi=` on the summary line.
    pub show_dpi: bool,
    /// Extra sentence when the container has no density field.
    pub note: Option<String>,
}

/// Write the image and the report.
///
/// # Errors
///
/// A folder in the output path could not be created, or the disk is full.
pub fn emit(
    job: &Job,
    input: &Input,
    picture: &Picture,
    finished: &[u8],
    how: How,
    density: &DensityLine,
) -> Result<(), SmashError> {
    let (written, kept_original) = match &job.dest {
        Dest::Stdout => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(finished)?;
            stdout.flush()?;
            (finished.len(), false)
        }
        Dest::Create(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            write_replacing(path, finished)?;
            (finished.len(), false)
        }
        Dest::Replace(path) => replace_if_smaller(path, input.bytes.len(), finished)?,
    };
    let reported = if kept_original { How::Lossless } else { how };
    report(
        job,
        picture,
        (input.bytes.len(), written),
        reported,
        kept_original,
        density,
    );
    Ok(())
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
    write_exclusive(temporary, finished)?;
    fs::rename(temporary, path)?;
    Ok((finished.len(), false))
}

/// Write `bytes` to `path`. An existing symlink is replaced, not followed.
///
/// The bytes land in a new file in the same directory, then that name is
/// renamed onto `path`. Rename replaces a directory entry. It does not open
/// a symlink and write through it.
///
/// # Errors
///
/// The directory is missing, or a partial file is already a symlink.
pub(crate) fn write_replacing(path: &Path, bytes: &[u8]) -> Result<(), SmashError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| SmashError::message("the output needs a file name"))?;
    let mut partial_name = std::ffi::OsString::from(".smashbin-");
    partial_name.push(std::process::id().to_string());
    partial_name.push("-");
    partial_name.push(file_name);
    partial_name.push(".partial");
    let partial = if parent.as_os_str().is_empty() {
        PathBuf::from(partial_name)
    } else {
        parent.join(partial_name)
    };
    write_exclusive(&partial, bytes).map_err(|error| {
        SmashError::message(format!("could not write {}: {error}", path.display()))
    })?;
    if let Err(error) = fs::rename(&partial, path) {
        let _ = fs::remove_file(&partial);
        return Err(SmashError::message(format!(
            "could not write {}: {error}",
            path.display()
        )));
    }
    Ok(())
}

fn write_exclusive(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    match create_new(path, bytes) {
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let meta = fs::symlink_metadata(path)?;
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "refusing to follow a link in the output folder",
                ));
            }
            fs::remove_file(path)?;
            create_new(path, bytes)
        }
        other => other,
    }
}

fn create_new(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

fn report(
    job: &Job,
    picture: &Picture,
    sizes: (usize, usize),
    how: How,
    kept_original: bool,
    density: &DensityLine,
) {
    let name = job
        .source
        .as_ref()
        .map_or_else(|| "stdin".to_owned(), |path| path.display().to_string());
    let kept = if kept_original { "  kept-original" } else { "" };
    let dpi_text = match job.dpi {
        Some(dpi) if density.show_dpi && !kept_original => format!("  dpi={dpi}"),
        _ => String::new(),
    };
    let format = if kept_original {
        picture.kind.as_str()
    } else {
        job.ext.map_or_else(|| picture.kind.as_str(), Ext::as_str)
    };
    eprintln!(
        "{name}  {} -> {}  {}  {}x{}  {format}  strip={}{dpi_text}{kept}",
        sizes.0,
        sizes.1,
        how.as_str(),
        picture.width,
        picture.height,
        job.strip.as_str()
    );
    if let Some(note) = density.note.as_deref() {
        if !kept_original {
            eprintln!("{note}");
        }
    }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::write_replacing;

    #[test]
    fn a_symlink_is_replaced_and_the_target_stays() {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("secret.txt");
        std::fs::write(&secret, b"keep").unwrap();
        let link = dir.path().join("cut.jpg");
        symlink(&secret, &link).unwrap();
        write_replacing(&link, b"jpeg-bytes").unwrap();
        assert_eq!(std::fs::read(&secret).unwrap(), b"keep");
        let meta = std::fs::symlink_metadata(&link).unwrap();
        assert!(meta.is_file());
        assert!(!meta.file_type().is_symlink());
        assert_eq!(std::fs::read(&link).unwrap(), b"jpeg-bytes");
    }
}
