//! Step 1. The words typed after the program name.
//!
//! `smashbin logo.jpg` has one argument, the file.
//! `smashbin --strip all --bytes 1mb logo.jpg` has flags and a file.
//! No file means "read the image from stdin".

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

use crate::config::Spec;
use crate::error::SmashError;

/// The whole command line.
#[derive(Debug, Parser)]
#[command(
    name = "smashbin",
    version,
    about = "Strip metadata and shrink a PNG, JPG, or GIF for a GitHub avatar.",
    disable_help_flag = true,
    after_help = "\
Steps, in order:
  1. Remove private metadata. The color profile stays unless you pass --strip all.
  2. Resize when --width or --height is set. A missing side keeps the aspect ratio.
  3. Shrink that file without a second lossy pass.
  4. If you passed --quality or --bytes, that replaces step 3 with a new JPEG.
  5. Check the GitHub avatar rule: PNG, JPG, or GIF, under 1 MB, smaller than 3000x3000.

A smasher.json or smasher.yaml next to the image runs each job from top to bottom.
-c names a smasher file that lives somewhere else.

This version does not change the format or pad a picture into a square."
)]
pub struct Args {
    /// Show the flags. `-h` is height.
    #[arg(long, action = clap::ArgAction::Help)]
    #[allow(dead_code)]
    help: Option<bool>,

    /// `safe` drops GPS, EXIF, comments, and editor notes. `all` also drops the color profile.
    #[arg(long, value_enum, default_value_t = Strip::Safe)]
    strip: Strip,

    /// New width in pixels, from 1 to 65535. Without `--height`, the height keeps the aspect ratio.
    #[arg(short = 'w', long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..=65535))]
    width: Option<u32>,

    /// New height in pixels, from 1 to 65535. Without `--width`, the width keeps the aspect ratio.
    #[arg(short = 'h', long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..=65535))]
    height: Option<u32>,

    /// Make a new JPEG at this quality, from 1 to 100. Cannot be combined with --bytes.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=100), conflicts_with = "bytes")]
    quality: Option<u8>,

    /// Highest JPEG quality that fits in this many bytes. `800000`, `800kb`, or `1mb`.
    #[arg(long, conflicts_with = "quality")]
    bytes: Option<String>,

    /// Smasher file to use instead of the one next to the image.
    #[arg(short = 'c', long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Write here. A directory keeps the generated name inside it.
    #[arg(short, long, conflicts_with = "stdout")]
    out: Option<PathBuf>,

    /// Replace the file, and only when the new file is smaller.
    #[arg(long, conflicts_with = "stdout")]
    replace: bool,

    /// Write the image to stdout. The one-line report stays on stderr.
    #[arg(long)]
    stdout: bool,

    /// Images to smash. Omit this to read one image from stdin.
    #[arg(value_name = "FILE")]
    files: Vec<PathBuf>,
}

/// Which metadata is allowed to remain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Strip {
    /// Drop private metadata. Keep the color profile.
    Safe,
    /// Drop private metadata and the color profile.
    All,
}

impl Strip {
    /// The word used in the stderr report.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::All => "all",
        }
    }
}

/// One image and the flags that apply to it.
pub struct Job {
    /// `None` means stdin.
    pub source: Option<PathBuf>,
    /// Where the bytes go.
    pub dest: Dest,
    /// Metadata policy.
    pub strip: Strip,
    /// Set only when the user typed `--quality`.
    pub quality: Option<u8>,
    /// Set only when the user typed `--bytes`.
    pub byte_cap: Option<u64>,
    /// Set only when the user typed `--width`.
    pub width: Option<u32>,
    /// Set only when the user typed `--height`.
    pub height: Option<u32>,
}

#[cfg(test)]
impl Job {
    fn dest_path(&self) -> Option<&Path> {
        match &self.dest {
            Dest::Create(path) | Dest::Replace(path) => Some(path),
            Dest::Stdout => None,
        }
    }
}

/// Where step 8 writes.
#[derive(Debug)]
pub enum Dest {
    /// The image bytes go to stdout.
    Stdout,
    /// Write this new path. The source file stays where it is.
    Create(PathBuf),
    /// Replace the source path when the new bytes are smaller.
    Replace(PathBuf),
}

/// Clap already checked that `--quality` and `--bytes` were not both set.
pub fn parse_args() -> Args {
    Args::parse()
}

const CONFIG_FLAGS: &str = "the smasher file lists the jobs. Leave off --strip, --width, --height, --quality, and --bytes.";

/// Turn the command line into one job per file.
///
/// A smasher file replaces that single job with every entry in the file,
/// top to bottom, on the same image.
///
/// # Errors
///
/// A bad `--bytes` value, stdin combined with `--replace`, several files
/// combined with `--stdout` or a single output file, or a smasher file
/// combined with the flags that belong inside it.
pub fn jobs(args: &Args) -> Result<Vec<Job>, SmashError> {
    let explicit = match &args.config {
        Some(path) => Some(crate::config::load(path)?),
        None => None,
    };
    if explicit.is_some() && cli_sets_a_job(args) {
        return Err(SmashError::message(CONFIG_FLAGS));
    }

    if args.files.is_empty() {
        if args.replace {
            return Err(SmashError::message(
                "--replace needs a file. Stdin has nothing to replace.",
            ));
        }
        let specs = match &explicit {
            Some(specs) => specs.clone(),
            None => vec![spec_from_cli(args)],
        };
        return stdin_jobs(args, &specs);
    }

    if args.stdout && args.files.len() > 1 {
        return Err(SmashError::message(
            "--stdout can take one image. Pass one file, or leave the file off and use stdin.",
        ));
    }
    if let Some(path) = &args.out {
        if args.files.len() > 1 && !path.is_dir() {
            return Err(SmashError::message(
                "--out must be a directory when you pass more than one file.",
            ));
        }
    }

    let mut jobs = Vec::new();
    for file in &args.files {
        let specs = specs_for(args, file, explicit.as_ref())?;
        push_file_jobs(args, file, &specs, &mut jobs)?;
    }
    Ok(jobs)
}

fn cli_sets_a_job(args: &Args) -> bool {
    args.strip != Strip::Safe
        || args.width.is_some()
        || args.height.is_some()
        || args.quality.is_some()
        || args.bytes.is_some()
}

fn spec_from_cli(args: &Args) -> Spec {
    Spec {
        strip: Some(args.strip),
        width: args.width,
        height: args.height,
        quality: args.quality,
        bytes: args.bytes.clone(),
    }
}

fn specs_for(
    args: &Args,
    file: &Path,
    explicit: Option<&Vec<Spec>>,
) -> Result<Vec<Spec>, SmashError> {
    if let Some(specs) = explicit {
        return Ok(specs.clone());
    }
    if let Some(path) = crate::config::beside(file)? {
        if cli_sets_a_job(args) {
            return Err(SmashError::message(CONFIG_FLAGS));
        }
        return crate::config::load(&path);
    }
    Ok(vec![spec_from_cli(args)])
}

fn stdin_jobs(args: &Args, specs: &[Spec]) -> Result<Vec<Job>, SmashError> {
    if specs.len() > 1 {
        match &args.out {
            Some(path) if path.is_dir() => {}
            _ => {
                return Err(SmashError::message(
                    "--out must be a directory when the smasher file has more than one job.",
                ));
            }
        }
    }
    let mut jobs = Vec::with_capacity(specs.len());
    for spec in specs {
        let parsed = parsed_job(spec)?;
        let dest = match &args.out {
            Some(path) if path.is_dir() || specs.len() > 1 => {
                Dest::Create(path.join(output_name(Path::new("image.img"), &parsed)))
            }
            Some(path) => Dest::Create(path.clone()),
            None => Dest::Stdout,
        };
        jobs.push(Job {
            source: None,
            dest,
            strip: parsed.strip,
            quality: parsed.quality,
            byte_cap: parsed.byte_cap,
            width: parsed.width,
            height: parsed.height,
        });
    }
    Ok(jobs)
}

fn push_file_jobs(
    args: &Args,
    file: &Path,
    specs: &[Spec],
    jobs: &mut Vec<Job>,
) -> Result<(), SmashError> {
    if specs.len() > 1 && args.stdout {
        return Err(SmashError::message(
            "--stdout can take one job. The smasher file has more than one.",
        ));
    }
    if specs.len() > 1 && args.replace {
        return Err(SmashError::message(
            "--replace can take one job. The smasher file has more than one.",
        ));
    }
    if specs.len() > 1 {
        if let Some(path) = &args.out {
            if !path.is_dir() {
                return Err(SmashError::message(
                    "--out must be a directory when the smasher file has more than one job.",
                ));
            }
        }
    }
    for spec in specs {
        let parsed = parsed_job(spec)?;
        let name = output_name(file, &parsed);
        let dest = if args.stdout {
            Dest::Stdout
        } else if args.replace {
            Dest::Replace(file.to_path_buf())
        } else if let Some(path) = &args.out {
            if path.is_dir() || args.files.len() > 1 {
                Dest::Create(path.join(name))
            } else {
                Dest::Create(path.clone())
            }
        } else {
            Dest::Create(file.with_file_name(name))
        };
        jobs.push(Job {
            source: Some(file.to_path_buf()),
            dest,
            strip: parsed.strip,
            quality: parsed.quality,
            byte_cap: parsed.byte_cap,
            width: parsed.width,
            height: parsed.height,
        });
    }
    Ok(())
}

struct ParsedJob {
    strip: Strip,
    quality: Option<u8>,
    byte_cap: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
    bytes: Option<String>,
}

fn parsed_job(spec: &Spec) -> Result<ParsedJob, SmashError> {
    let byte_cap = spec
        .bytes
        .as_deref()
        .map(parse_bytes)
        .transpose()
        .map_err(SmashError::message)?;
    Ok(ParsedJob {
        strip: spec.strip.unwrap_or(Strip::Safe),
        quality: spec.quality,
        byte_cap,
        width: spec.width,
        height: spec.height,
        bytes: spec.bytes.clone(),
    })
}

/// `logo.jpg` with no extra flags becomes `logo_strip_safe.jpg`.
///
/// The original stem stays first. Then `_`, then the input vars in this
/// fixed order: `strip`, `width`, `height`, `quality`, `bytes`. A var that
/// was not set is left out. `--quality` and `--bytes` cannot both be set.
fn output_name(path: &Path, job: &ParsedJob) -> OsString {
    let stem = path.file_stem().unwrap_or(std::ffi::OsStr::new("image"));
    let extension = path.extension().unwrap_or(std::ffi::OsStr::new("img"));
    let mut name = stem.to_os_string();
    name.push("_strip_");
    name.push(job.strip.as_str());
    if let Some(width) = job.width {
        name.push("_width_");
        name.push(width.to_string());
    }
    if let Some(height) = job.height {
        name.push("_height_");
        name.push(height.to_string());
    }
    if let Some(quality) = job.quality {
        name.push("_quality_");
        name.push(quality.to_string());
    }
    if let Some(bytes) = job.bytes.as_deref() {
        name.push("_bytes_");
        name.push(byte_token(bytes));
    }
    name.push(".");
    name.push(extension);
    name
}

/// `40KB` becomes `40kb`, so the file name matches the typed size.
fn byte_token(text: &str) -> String {
    text.trim()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// `800000`, `800kb`, `800kib`, `1mb`, `1mib`.
///
/// `kb` is 1000. `kib` is 1024. GitHub's "1 MB" is the decimal one, so
/// `--bytes 1mb` means 1,000,000 bytes.
fn parse_bytes(text: &str) -> Result<u64, String> {
    let lower = text.trim().to_ascii_lowercase();
    let (digits, multiplier) = if let Some(rest) = lower.strip_suffix("kib") {
        (rest, 1024)
    } else if let Some(rest) = lower.strip_suffix("mib") {
        (rest, 1024 * 1024)
    } else if let Some(rest) = lower.strip_suffix("kb") {
        (rest, 1_000)
    } else if let Some(rest) = lower.strip_suffix("mb") {
        (rest, 1_000_000)
    } else {
        (lower.as_str(), 1)
    };
    let digits = digits.trim();
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("use a number of bytes, like 800000, 800kb, or 1mb".to_owned());
    }
    let value: u64 = digits
        .parse()
        .map_err(|_| "that number is too large".to_owned())?;
    if value == 0 {
        return Err("the byte target has to be at least 1".to_owned());
    }
    value
        .checked_mul(multiplier)
        .ok_or_else(|| "that number is too large".to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::Path;

    use clap::Parser;

    use super::{parse_bytes, Args};

    #[test]
    fn byte_suffixes() {
        assert_eq!(parse_bytes("800000").unwrap(), 800_000);
        assert_eq!(parse_bytes("800kb").unwrap(), 800_000);
        assert_eq!(parse_bytes("1mb").unwrap(), 1_000_000);
        assert_eq!(parse_bytes("1kib").unwrap(), 1024);
        assert!(parse_bytes("0").is_err());
        assert!(parse_bytes("nope").is_err());
    }

    #[test]
    fn the_default_name_is_the_original_plus_strip() {
        let args = Args::try_parse_from(["smashbin", "dir/logo.jpg"]).unwrap();
        let jobs = super::jobs(&args).unwrap();
        assert_eq!(
            jobs[0].dest_path(),
            Some(Path::new("dir/logo_strip_safe.jpg"))
        );
    }

    #[test]
    fn names_keep_strip_then_quality_then_bytes() {
        let quality =
            Args::try_parse_from(["smashbin", "--quality", "80", "--strip", "all", "logo.jpg"])
                .unwrap();
        let jobs = super::jobs(&quality).unwrap();
        assert_eq!(
            jobs[0].dest_path(),
            Some(Path::new("logo_strip_all_quality_80.jpg"))
        );

        let bytes = Args::try_parse_from(["smashbin", "--bytes", "40KB", "logo.jpg"]).unwrap();
        let jobs = super::jobs(&bytes).unwrap();
        assert_eq!(
            jobs[0].dest_path(),
            Some(Path::new("logo_strip_safe_bytes_40kb.jpg"))
        );

        let sized = Args::try_parse_from([
            "smashbin",
            "--height",
            "10",
            "--quality",
            "80",
            "--strip",
            "all",
            "--width",
            "20",
            "logo.jpg",
        ])
        .unwrap();
        let jobs = super::jobs(&sized).unwrap();
        assert_eq!(
            jobs[0].dest_path(),
            Some(Path::new(
                "logo_strip_all_width_20_height_10_quality_80.jpg"
            ))
        );
    }
}
