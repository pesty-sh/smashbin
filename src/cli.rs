//! Step 1. The words typed after the program name.
//!
//! `smashbin logo.jpg` has one argument, the file.
//! `smashbin --strip all --bytes 1mb logo.jpg` has flags and a file.
//! No file means "read the image from stdin".

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

use crate::config::Spec;
use crate::error::SmashError;
use crate::ext::Ext;

/// The whole command line.
#[derive(Debug, Parser)]
#[command(
    name = "smashbin",
    version,
    about = "Strip metadata and shrink a PNG, JPG, or GIF.",
    disable_help_flag = true,
    after_help = "\
Steps, in order:
  1. Remove private metadata. The color profile stays unless you pass --strip all.
  2. Resize when --width or --height is set. A missing side keeps the aspect ratio.
     Both sides with a different aspect crop. --cv and --ch place the slice.
     --dpi sets the stored pixels per inch. The pixel grid does not change.
  3. Shrink that file without a second lossy pass.
  4. If you passed --quality or --bytes, that replaces step 3 with a new JPEG.
  5. --format writes a different container. WebP is lossless. A GIF keeps its first frame.

A smasher.json or smasher.yaml next to the image runs each job from top to bottom.
-c names a smasher file that lives somewhere else.

--format can write jpg, png, gif, webp, bmp, tif, tga, ico, qoi, ppm, hdr, or ff.
This version does not pad a picture into a square."
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

    /// Vertical crop. `top`, `middle`, `bottom`, or 0 to 100. 0 is the bottom, 100 is the top. `-c` is the smasher file, so this flag is `--cv`.
    #[arg(long = "cv", value_name = "POS", value_parser = parse_cv_arg)]
    cv: Option<Align>,

    /// Horizontal crop. `left`, `middle`, `right`, or 0 to 100. 0 is the left, 100 is the right.
    #[arg(long = "ch", value_name = "POS", value_parser = parse_ch_arg)]
    ch: Option<Align>,

    /// Display density in pixels per inch, from 1 to 65535. The pixel grid stays the same.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..=65535))]
    dpi: Option<u16>,

    /// Write this container. `jpg`, `png`, `gif`, `webp`, `bmp`, `tif`, `tga`, `ico`, `qoi`, `ppm`, `hdr`, or `ff`.
    #[arg(short = 'f', long = "format", value_name = "FORMAT", value_parser = parse_ext_arg)]
    ext: Option<Ext>,

    /// Make a new JPEG at this quality, from 1 to 100. Cannot be combined with --bytes.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=100), conflicts_with = "bytes")]
    quality: Option<u8>,

    /// Highest JPEG quality that fits in this many bytes. `800000`, `800kb`, or `1mb`.
    #[arg(long, conflicts_with = "quality")]
    bytes: Option<String>,

    /// Smasher file to use instead of the one next to the image.
    #[arg(short = 'c', long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Write here. A directory keeps the generated name inside it. Missing folders are created. `./folder` and `.\folder` sit next to the input file.
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

/// A crop edge, as a percent of the leftover space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Align {
    /// 0 to 100. For `--cv`, 0 is the bottom and 100 is the top. For `--ch`, 0 is the left and 100 is the right.
    pub percent: u8,
    /// The word or number as typed, for the generated file name.
    pub token: String,
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
    /// Set only when the user typed `--cv`.
    pub cv: Option<Align>,
    /// Set only when the user typed `--ch`.
    pub ch: Option<Align>,
    /// Set only when the user typed `--dpi`.
    pub dpi: Option<u16>,
    /// Set only when the user typed `--format`.
    pub ext: Option<Ext>,
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

const CONFIG_FLAGS: &str = "the smasher file lists the jobs. Leave off --strip, --width, --height, --cv, --ch, --dpi, --format, --quality, and --bytes.";

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

    let mut jobs = Vec::new();
    for file in &args.files {
        let specs = specs_for(args, file, explicit.as_ref())?;
        push_file_jobs(args, file, &specs, &mut jobs)?;
    }
    Ok(jobs)
}

/// Make `path` a directory. A path that is already a file is an error.
fn prepare_out_dir(path: &Path) -> Result<(), SmashError> {
    if path.is_dir() {
        return Ok(());
    }
    if path.exists() {
        return Err(SmashError::message(
            "--out must be a directory when you pass more than one file or job.",
        ));
    }
    fs::create_dir_all(path)?;
    Ok(())
}

/// `./folder` and `.\folder` land next to the input. Other paths stay as typed.
fn dest_from_out(
    out: &Path,
    source: Option<&Path>,
    name: OsString,
    many: bool,
) -> Result<Dest, SmashError> {
    let resolved = resolve_out(out, source);
    if out_is_folder(out, &resolved, many) {
        prepare_out_dir(&resolved)?;
        Ok(Dest::Create(resolved.join(name)))
    } else {
        Ok(Dest::Create(resolved))
    }
}

fn resolve_out(out: &Path, source: Option<&Path>) -> PathBuf {
    let Some(rest) = dot_relative(out) else {
        return out.to_path_buf();
    };
    let Some(source) = source else {
        return out.to_path_buf();
    };
    let parent = match source.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if rest.as_os_str().is_empty() {
        parent.to_path_buf()
    } else {
        parent.join(rest)
    }
}

fn dot_relative(out: &Path) -> Option<&Path> {
    let text = out.to_str()?;
    if text == "." || text == "./" || text == ".\\" {
        return Some(Path::new(""));
    }
    text.strip_prefix("./")
        .or_else(|| text.strip_prefix(".\\"))
        .map(Path::new)
}

fn out_is_folder(typed: &Path, resolved: &Path, many: bool) -> bool {
    many
        || resolved.is_dir()
        || (dot_relative(typed).is_some() && resolved.extension().is_none() && !resolved.exists())
}

fn cli_sets_a_job(args: &Args) -> bool {
    args.strip != Strip::Safe
        || args.width.is_some()
        || args.height.is_some()
        || args.cv.is_some()
        || args.ch.is_some()
        || args.dpi.is_some()
        || args.ext.is_some()
        || args.quality.is_some()
        || args.bytes.is_some()
}

fn spec_from_cli(args: &Args) -> Spec {
    Spec {
        strip: Some(args.strip),
        width: args.width,
        height: args.height,
        cv: args.cv.clone(),
        ch: args.ch.clone(),
        dpi: args.dpi,
        ext: args.ext,
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
    if let Some(path) = crate::config::beside(file) {
        if cli_sets_a_job(args) {
            return Err(SmashError::message(CONFIG_FLAGS));
        }
        return crate::config::load(&path);
    }
    Ok(vec![spec_from_cli(args)])
}

fn stdin_jobs(args: &Args, specs: &[Spec]) -> Result<Vec<Job>, SmashError> {
    if specs.len() > 1 && args.out.is_none() {
        return Err(SmashError::message(
            "--out must be a directory when the smasher file has more than one job.",
        ));
    }
    let mut jobs = Vec::with_capacity(specs.len());
    for spec in specs {
        let parsed = parsed_job(spec)?;
        let name = output_name(Path::new("image.img"), &parsed);
        let dest = match &args.out {
            Some(path) => dest_from_out(path, None, name, specs.len() > 1)?,
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
            cv: parsed.cv,
            ch: parsed.ch,
            dpi: parsed.dpi,
            ext: parsed.ext,
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
    for spec in specs {
        let parsed = parsed_job(spec)?;
        let name = output_name(file, &parsed);
        let dest = if args.stdout {
            Dest::Stdout
        } else if args.replace {
            Dest::Replace(file.to_path_buf())
        } else if let Some(path) = &args.out {
            dest_from_out(path, Some(file), name, args.files.len() > 1 || specs.len() > 1)?
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
            cv: parsed.cv,
            ch: parsed.ch,
            dpi: parsed.dpi,
            ext: parsed.ext,
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
    cv: Option<Align>,
    ch: Option<Align>,
    dpi: Option<u16>,
    ext: Option<Ext>,
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
        cv: spec.cv.clone(),
        ch: spec.ch.clone(),
        dpi: spec.dpi,
        ext: spec.ext,
        bytes: spec.bytes.clone(),
    })
}

/// `logo.jpg` with no extra flags becomes `logo_strip_safe.jpg`.
///
/// The original stem stays first. Then `_`, then the input vars in this
/// fixed order: `strip`, `width`, `height`, `cv`, `ch`, `dpi`, `quality`, `bytes`.
/// A var that was not set is left out. `--quality` and `--bytes` cannot both be set.
fn output_name(path: &Path, job: &ParsedJob) -> OsString {
    let stem = path.file_stem().unwrap_or(std::ffi::OsStr::new("image"));
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
    if let Some(cv) = job.cv.as_ref() {
        name.push("_cv_");
        name.push(&cv.token);
    }
    if let Some(ch) = job.ch.as_ref() {
        name.push("_ch_");
        name.push(&ch.token);
    }
    if let Some(dpi) = job.dpi {
        name.push("_dpi_");
        name.push(dpi.to_string());
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
    if let Some(ext) = job.ext {
        name.push(ext.extension());
    } else {
        name.push(path.extension().unwrap_or(std::ffi::OsStr::new("img")));
    }
    name
}

fn parse_ext_arg(text: &str) -> Result<Ext, String> {
    Ext::parse(text).ok_or_else(|| format!("--format is {}. `{text}` is none of those.", Ext::LIST))
}

fn parse_cv_arg(text: &str) -> Result<Align, String> {
    parse_align(
        text,
        &[("top", 100), ("middle", 50), ("center", 50), ("bottom", 0)],
        "--cv is top, middle, bottom, or 0 to 100",
    )
}

fn parse_ch_arg(text: &str) -> Result<Align, String> {
    parse_align(
        text,
        &[("left", 0), ("middle", 50), ("center", 50), ("right", 100)],
        "--ch is left, middle, right, or 0 to 100",
    )
}

/// A name, a number, or a number with `%`.
pub fn parse_align(text: &str, names: &[(&str, u8)], error: &str) -> Result<Align, String> {
    let trimmed = text.trim();
    let key = trimmed.trim_end_matches('%').trim().to_ascii_lowercase();
    if let Some((_, percent)) = names.iter().find(|(name, _)| *name == key) {
        return Ok(Align {
            percent: *percent,
            token: key,
        });
    }
    if let Ok(percent) = key.parse::<u8>() {
        if percent <= 100 {
            return Ok(Align {
                percent,
                token: percent.to_string(),
            });
        }
    }
    Err(error.to_owned())
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
/// `kb` is 1000. `kib` is 1024. `--bytes 1mb` means 1,000,000 bytes.
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

        let density = Args::try_parse_from(["smashbin", "--dpi", "144", "logo.jpg"]).unwrap();
        let jobs = super::jobs(&density).unwrap();
        assert_eq!(
            jobs[0].dest_path(),
            Some(Path::new("logo_strip_safe_dpi_144.jpg"))
        );

        let ext = Args::try_parse_from(["smashbin", "--format", "webp", "logo.jpg"]).unwrap();
        let jobs = super::jobs(&ext).unwrap();
        assert_eq!(
            jobs[0].dest_path(),
            Some(Path::new("logo_strip_safe.webp"))
        );
    }

    #[test]
    fn a_dot_out_folder_sits_beside_the_input() {
        let source = Path::new("photos/logo.jpg");
        let beside = source.parent().unwrap().join("smashed");
        assert_eq!(
            super::resolve_out(Path::new("./smashed"), Some(source)),
            beside
        );
        assert_eq!(
            super::resolve_out(Path::new(".\\smashed"), Some(source)),
            beside
        );
        assert_eq!(
            super::resolve_out(Path::new("smashed"), Some(source)),
            Path::new("smashed")
        );
    }
}
