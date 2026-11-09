//! smashbin reads an image and writes a smaller one.
//!
//! Open the other files in this order. Each one does a single job:
//!
//! 1. [`cli`] — the words typed after `smashbin`
//! 2. [`read`] — the bytes of the file
//! 3. [`sniff`] — PNG, JPEG, or GIF, plus width and height
//! 4. [`strip`] — private metadata comes off
//! 5. [`resize`] — a new width or height, or a crop when both sides disagree
//! 6. [`lossless`] — the file gets smaller without a second lossy pass
//! 7. [`lossy`] — `--quality` or `--bytes` replaces step 6 with a new JPEG
//! 8. [`convert`] — `--format` writes a different container
//! 9. [`density`] — `--dpi` writes the stored pixels per inch
//! 10. [`write`] — the bytes go to a file or to stdout
//!
//! This file is the only place those steps are called, and they are called
//! in that order. Version 0.1.5 does not pad to a square.

mod cli;
mod config;
mod convert;
mod density;
mod error;
mod ext;
mod lossless;
mod lossy;
mod read;
mod resize;
mod sniff;
mod strip;
mod write;

use std::process::ExitCode;

use cli::Args;
use error::SmashError;

fn main() -> ExitCode {
    let args = cli::parse_args();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("smashbin error: {error}");
            ExitCode::from(1)
        }
    }
}

/// Run every input. A broken file stops the program.
fn run(args: &Args) -> Result<(), SmashError> {
    let jobs = cli::jobs(args)?;
    for job in &jobs {
        smash_one(job)?;
    }
    Ok(())
}

/// One image, one trip through the steps.
fn smash_one(job: &cli::Job) -> Result<(), SmashError> {
    // 1. Bytes in.
    let input = read::load(job.source.as_deref())?;
    // 2. What kind of image is this?
    let source = sniff::sniff(&input.bytes)?;
    let changing = job.ext.is_some_and(|ext| !ext.same_as(source.kind));
    if changing
        && job.ext != Some(ext::Ext::Jpeg)
        && (job.quality.is_some() || job.byte_cap.is_some())
    {
        return Err(SmashError::message(
            "--quality and --bytes write a JPEG. Leave them off, or pass --format jpg.",
        ));
    }
    // 3. Private metadata off.
    let stripped = strip::apply(&input.bytes, &source, job.strip)?;
    // 4. New width and height, or a crop. Skipped when neither flag was typed.
    //    One missing side keeps the aspect ratio. Both sides with a different
    //    aspect crop.
    let geom = resize::plan(
        &source,
        job.width,
        job.height,
        job.ch.as_ref().map(|align| align.percent),
        job.cv.as_ref().map(|align| align.percent),
    )?;
    let keep_jpeg_file = job.quality.is_none();
    let (working, picture, jpeg, base) = match geom {
        Some(resize::Geom::Scale(size)) => {
            let scaled = resize::scale(&stripped, &source, size, keep_jpeg_file)?;
            (
                scaled.bytes,
                scaled.picture,
                scaled.jpeg,
                lossy::How::Resize,
            )
        }
        Some(resize::Geom::Crop(crop)) => {
            let cropped = resize::crop(&stripped, &source, crop, keep_jpeg_file)?;
            (
                cropped.bytes,
                cropped.picture,
                cropped.jpeg,
                lossy::How::Crop,
            )
        }
        None => (stripped, source, None, lossy::How::Lossless),
    };
    let changed = geom.is_some();
    // 5. Shrink the resized file. A quality override encodes the pixels below.
    let lossless_bytes =
        if changing || (changed && picture.kind == sniff::Kind::Jpeg && job.quality.is_some()) {
            Vec::new()
        } else {
            lossless::shrink(&working, &picture)?
        };
    // 6. Quality or a byte cap replaces step 5 when one of those flags was typed.
    let (finished, how) = if changing {
        convert::apply(&working, &picture, job, changed, jpeg.as_ref())?
    } else {
        lossy::apply(&lossless_bytes, &picture, job, base, jpeg.as_ref())?
    };
    // The stored density. Pixels are already final. A container with no
    // field for it keeps going, and stderr names the original density.
    let (finished, density_line) = if let Some(dpi) = job.dpi {
        match density::apply(&finished, job.ext, picture.kind, dpi)? {
            density::Outcome::Stored(bytes) => (
                bytes,
                write::DensityLine {
                    show_dpi: true,
                    note: None,
                },
            ),
            density::Outcome::Skipped { bytes, container } => (
                bytes,
                write::DensityLine {
                    show_dpi: false,
                    note: Some(density::skip_note(
                        dpi,
                        container,
                        density::source_dpi(&input.bytes, source.kind),
                    )),
                },
            ),
        }
    } else {
        (
            finished,
            write::DensityLine {
                show_dpi: false,
                note: None,
            },
        )
    };
    // Bytes out. The report goes to stderr so a pipe stays a pure image.
    write::emit(job, &input, &picture, &finished, how, &density_line)
}
