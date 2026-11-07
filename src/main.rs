//! smashbin reads an image and writes a smaller one.
//!
//! Open the other files in this order. Each one does a single job:
//!
//! 1. [`cli`] — the words typed after `smashbin`
//! 2. [`read`] — the bytes of the file
//! 3. [`sniff`] — PNG, JPEG, or GIF, plus width and height
//! 4. [`strip`] — private metadata comes off
//! 5. [`resize`] — a new width or height, when one of those flags was typed
//! 6. [`lossless`] — the file gets smaller without a second lossy pass
//! 7. [`lossy`] — `--quality` or `--bytes` replaces step 6 with a new JPEG
//! 8. [`avatar`] — the GitHub avatar rule
//! 9. [`write`] — the bytes go to a file or to stdout
//!
//! This file is the only place those steps are called, and they are called
//! in that order. Version 0.1.2 does not change the format or pad to a square.

mod avatar;
mod cli;
mod error;
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
        Ok(avatar::Status::Fit) => ExitCode::SUCCESS,
        Ok(avatar::Status::Rejected) => ExitCode::from(2),
        Err(error) => {
            eprintln!("smashbin error: {error}");
            ExitCode::from(1)
        }
    }
}

/// Run every input. A broken file stops the program. A file that misses the
/// avatar rule does not stop the others, and the program ends with status 2.
fn run(args: &Args) -> Result<avatar::Status, SmashError> {
    let jobs = cli::jobs(args)?;
    let mut status = avatar::Status::Fit;
    for job in &jobs {
        if smash_one(job)? == avatar::Status::Rejected {
            status = avatar::Status::Rejected;
        }
    }
    Ok(status)
}

/// One image, one trip through the steps.
fn smash_one(job: &cli::Job) -> Result<avatar::Status, SmashError> {
    // 1. Bytes in.
    let input = read::load(job.source.as_deref())?;
    // 2. What kind of image is this?
    let source = sniff::sniff(&input.bytes)?;
    // 3. Private metadata off.
    let stripped = strip::apply(&input.bytes, &source, job.strip)?;
    // 4. New width and height. Skipped when neither flag was typed.
    //    One missing side keeps the aspect ratio.
    let target = resize::target(&source, job.width, job.height)?;
    let (working, picture, jpeg) = if let Some(size) = target {
        // A JPEG that will be overridden is not written twice.
        let keep_jpeg_file = job.quality.is_none();
        let scaled = resize::scale(&stripped, &source, size, keep_jpeg_file)?;
        (scaled.bytes, scaled.picture, scaled.jpeg)
    } else {
        (stripped, source, None)
    };
    // 5. Shrink the resized file. A quality override encodes the pixels below.
    let lossless_bytes =
        if target.is_some() && picture.kind == sniff::Kind::Jpeg && job.quality.is_some() {
            Vec::new()
        } else {
            lossless::shrink(&working, &picture)?
        };
    // 6. Quality or a byte cap replaces step 5 when one of those flags was typed.
    let (finished, how) = lossy::apply(
        &lossless_bytes,
        &picture,
        job,
        target.is_some(),
        jpeg.as_ref(),
    )?;
    // 7. The GitHub rule, using the size after a resize.
    let gate = avatar::check(picture.width, picture.height, finished.len(), how);
    // 8. Bytes out. The report goes to stderr so a pipe stays a pure image.
    write::emit(job, &input, &picture, &finished, how, &gate)
}
