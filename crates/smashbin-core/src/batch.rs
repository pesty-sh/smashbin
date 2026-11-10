//! The batch entrance. One picture, many sizes.
//!
//! The ordinary command reads and decodes the file again for every job.
//! A gallery that asks for 23 sizes then pays for 23 decodes. On a 25 MB
//! JPEG that cost is most of a minute in PHP.
//!
//! This file does the work in seven steps:
//!
//! 1. Read the job list.
//! 2. Read the image once.
//! 3. Refuse a huge pixel count before decoding. The header already knows
//!    the width and height.
//! 4. Decode once. A phone JPEG is turned upright here, and the orientation
//!    tag is not copied into the new files.
//! 5. Fit each size from that one picture. `cover` crops a window out of
//!    the original and scales that window. `contain` scales the whole
//!    picture into the box. A smaller size is not made by shrinking the
//!    previous size.
//! 6. Encode the JPEGs on several threads. Each thread has its own output
//!    buffer. The decoded picture is only read. A cut is kept only when its
//!    markers are picture data: a plain JFIF header, the frame, the tables,
//!    and the scan. EXIF, XMP, comments, and color profiles are refused.
//! 7. Write the files and print how long the seven steps took.
//!
//! A `smasher.json` beside the image does not enter this file.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use image::codecs::jpeg::JpegDecoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, RgbImage};

use crate::config::{self, BatchJob, Fit};
use crate::error::SmashError;
use crate::lossy;
use crate::read;
use crate::resize;
use crate::sniff::{self, Kind};
use crate::write;

/// Stop before decode when the header claims more pixels than this.
pub(crate) use sniff::MAX_PIXELS;

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    /// How many times this thread has decoded a batch picture.
    static DECODES: Cell<u32> = const { Cell::new(0) };
}

fn note_decode() {
    #[cfg(test)]
    DECODES.with(|count| count.set(count.get() + 1));
}

/// The first argument is the word `batch`.
pub(crate) fn requested() -> bool {
    std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("batch"))
}

/// Run `smashbin batch`.
///
/// # Errors
///
/// The job file, the image, or a cut failed. The message is safe to print.
pub(crate) fn run() -> Result<(), SmashError> {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            let code = error.exit_code();
            error.print().map_err(|print_error| {
                SmashError::message(format!("could not print the batch help: {print_error}"))
            })?;
            std::process::exit(code);
        }
    };
    if args.verbose {
        print_support(&args);
    }
    let started = Instant::now();
    let jobs = config::load_batch(&args.jobs)?;
    let cut = cut_file(&args.file, &jobs)?;
    let write_started = Instant::now();
    fs::create_dir_all(&args.out)?;
    for item in &cut.made {
        let path = args.out.join(&item.name);
        write::write_replacing(&path, &item.bytes)?;
        eprintln!(
            "{}  {}x{}  {}  quality={}",
            item.name,
            item.width,
            item.height,
            item.fit.as_str(),
            item.quality
        );
    }
    let write = write_started.elapsed();
    let total = started.elapsed();
    if args.verbose {
        print_bench(&cut, write);
    }
    let noun = if cut.made.len() == 1 { "file" } else { "files" };
    let places = if args.verbose { 3 } else { 1 };
    eprintln!(
        "{} {noun} in {:.places$}s",
        cut.made.len(),
        total.as_secs_f64()
    );
    Ok(())
}

fn print_support(args: &BatchArgs) {
    let version = env!("CARGO_PKG_VERSION");
    let cpu = std::thread::available_parallelism()
        .map_or_else(|_| "unknown".to_owned(), |count| count.get().to_string());
    eprintln!("smashbin {version}");
    eprintln!("library: smashbin-core {version}");
    eprintln!("entry: start");
    eprintln!("command: batch");
    eprintln!("jobs: {}", args.jobs.display());
    eprintln!("image: {}", args.file.display());
    eprintln!("out: {}", args.out.display());
    eprintln!("batch fields: name, width, height, quality, fit");
    eprintln!("fit: cover, contain");
    eprintln!("reads: PNG, JPEG, GIF");
    eprintln!("writes: JPEG");
    eprintln!("pixel cap: {MAX_PIXELS}");
    eprintln!("byte cap: {}", read::MAX_BYTES);
    eprintln!("cpu: {cpu}");
    eprintln!("php module: later");
    eprintln!("smashgui: later");
}

fn print_bench(cut: &BatchCut, write: Duration) {
    let output_bytes: usize = cut.made.iter().map(|item| item.bytes.len()).sum();
    eprintln!("input bytes: {}", cut.input_bytes);
    eprintln!(
        "header: {}x{} {}",
        cut.header_width,
        cut.header_height,
        cut.kind.as_str()
    );
    eprintln!("upright: {}x{}", cut.upright_width, cut.upright_height);
    eprintln!("orientation: {}", cut.orientation);
    eprintln!("read: {:.3}s", cut.read.as_secs_f64());
    eprintln!("decode: {:.3}s", cut.decode.as_secs_f64());
    eprintln!("encode: {:.3}s", cut.encode.as_secs_f64());
    eprintln!("write: {:.3}s", write.as_secs_f64());
    eprintln!("encode threads: {}", cut.made.len());
    eprintln!("output bytes: {output_bytes}");
}

/// Decode `path` once and encode every job.
fn cut_file(path: &Path, jobs: &[BatchJob]) -> Result<BatchCut, SmashError> {
    let read_started = Instant::now();
    let input = read::load(Some(path))?;
    let read = read_started.elapsed();
    let mut cut = cut_bytes(&input.bytes, jobs, true)?;
    cut.input_bytes = u64::try_from(input.bytes.len()).unwrap_or(u64::MAX);
    cut.read = read;
    Ok(cut)
}

fn cut_bytes(bytes: &[u8], jobs: &[BatchJob], parallel: bool) -> Result<BatchCut, SmashError> {
    let picture = sniff::sniff(bytes)?;
    sniff::pixel_budget(picture.width, picture.height)?;
    let decode_started = Instant::now();
    let (image, orientation) = decode_upright(bytes, picture.kind)?;
    let header_pixels = u64::from(picture.width).saturating_mul(u64::from(picture.height));
    let decoded_pixels = u64::from(image.width()).saturating_mul(u64::from(image.height()));
    if decoded_pixels > header_pixels {
        return Err(SmashError::message(
            "this image decoded larger than its header. This version stops.",
        ));
    }
    let decode = decode_started.elapsed();
    let encode_started = Instant::now();
    let made = render_all(&image, jobs, parallel)?;
    let encode = encode_started.elapsed();
    Ok(BatchCut {
        made,
        input_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        header_width: picture.width,
        header_height: picture.height,
        kind: picture.kind,
        upright_width: image.width(),
        upright_height: image.height(),
        orientation,
        read: Duration::ZERO,
        decode,
        encode,
    })
}

fn decode_upright(bytes: &[u8], kind: Kind) -> Result<(RgbImage, &'static str), SmashError> {
    note_decode();
    match kind {
        Kind::Jpeg => {
            let mut decoder =
                JpegDecoder::new(Cursor::new(bytes)).map_err(|error| lossy::image_error(&error))?;
            decoder
                .set_limits(lossy::image_limits())
                .map_err(|error| lossy::image_error(&error))?;
            let (width, height) = decoder.dimensions();
            sniff::pixel_budget(width, height)?;
            lossy::within_alloc(decoder.total_bytes())?;
            let orientation = decoder
                .orientation()
                .map_err(|error| lossy::image_error(&error))?;
            let mut image =
                DynamicImage::from_decoder(decoder).map_err(|error| lossy::image_error(&error))?;
            image.apply_orientation(orientation);
            Ok((image.into_rgb8(), orientation_name(orientation)))
        }
        Kind::Png | Kind::Gif => {
            let image = lossy::decode_limited(bytes)?;
            sniff::pixel_budget(image.width(), image.height())?;
            Ok((image.into_rgb8(), "none"))
        }
    }
}

fn orientation_name(orientation: Orientation) -> &'static str {
    match orientation {
        Orientation::NoTransforms => "none",
        Orientation::Rotate90 => "rotate 90",
        Orientation::Rotate180 => "rotate 180",
        Orientation::Rotate270 => "rotate 270",
        Orientation::FlipHorizontal => "flip horizontal",
        Orientation::FlipVertical => "flip vertical",
        Orientation::Rotate90FlipH => "rotate 90 flip horizontal",
        Orientation::Rotate270FlipH => "rotate 270 flip horizontal",
    }
}

fn render_all(
    image: &RgbImage,
    jobs: &[BatchJob],
    parallel: bool,
) -> Result<Vec<Made>, SmashError> {
    if !parallel || jobs.len() < 2 {
        return jobs.iter().map(|job| render_one(image, job)).collect();
    }
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(jobs.len());
        for job in jobs {
            handles.push(scope.spawn(move || render_one(image, job)));
        }
        handles
            .into_iter()
            .map(|handle| match handle.join() {
                Ok(result) => result,
                Err(_) => Err(SmashError::message("one cut stopped")),
            })
            .collect()
    })
}

/// Refuse a JPEG that still has a place to store user notes.
///
/// The encoder builds the file from pixels. This walk checks that result
/// before it is written. A plain JFIF header may stay, with no thumbnail.
/// The frame, tables, and scan stay. Anything else, including bytes after
/// the end marker, is user metadata and stops the cut.
fn pixels_only(bytes: &[u8]) -> Result<(), SmashError> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err(plain_jpeg());
    }
    let mut index = 2;
    let mut saw_scan = false;
    while index < bytes.len() {
        let marker = next_marker(bytes, &mut index)?;
        if marker == 0xD9 {
            if !saw_scan || index != bytes.len() {
                return Err(plain_jpeg());
            }
            return Ok(());
        }
        if marker == 0x00 || marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            return Err(plain_jpeg());
        }
        let payload = segment_payload(bytes, &mut index)?;
        if marker == 0xDA {
            saw_scan = true;
            index = end_of_scan(bytes, index)?;
            continue;
        }
        if !picture_segment(marker, payload) {
            return Err(SmashError::message(
                "this cut still carries user metadata. Batch writes picture data only.",
            ));
        }
    }
    Err(plain_jpeg())
}

fn plain_jpeg() -> SmashError {
    SmashError::message("this cut is not a plain JPEG")
}

fn next_marker(bytes: &[u8], index: &mut usize) -> Result<u8, SmashError> {
    if *index >= bytes.len() || bytes[*index] != 0xFF {
        return Err(plain_jpeg());
    }
    while *index < bytes.len() && bytes[*index] == 0xFF {
        *index += 1;
    }
    if *index >= bytes.len() {
        return Err(plain_jpeg());
    }
    let marker = bytes[*index];
    *index += 1;
    Ok(marker)
}

fn segment_payload<'a>(bytes: &'a [u8], index: &mut usize) -> Result<&'a [u8], SmashError> {
    if *index + 2 > bytes.len() {
        return Err(plain_jpeg());
    }
    let length = usize::from(u16::from_be_bytes([bytes[*index], bytes[*index + 1]]));
    if length < 2 || *index + length > bytes.len() {
        return Err(plain_jpeg());
    }
    let payload = &bytes[*index + 2..*index + length];
    *index += length;
    Ok(payload)
}

/// Picture segments only. The JFIF header is the encoder's name for the
/// file, and it has no thumbnail bytes where a note could hide.
fn picture_segment(marker: u8, payload: &[u8]) -> bool {
    match marker {
        0xE0 => {
            payload.len() == 14
                && payload.starts_with(b"JFIF\0")
                && payload[12] == 0
                && payload[13] == 0
        }
        0xC0 | 0xC1 | 0xC2 | 0xC4 | 0xDB | 0xDD => true,
        _ => false,
    }
}

/// Leave `index` on the marker that ends this scan. A progressive file has
/// another scan after that marker. Stuffed `FF 00` bytes and restart markers
/// are part of the scan.
fn end_of_scan(bytes: &[u8], mut index: usize) -> Result<usize, SmashError> {
    while index + 1 < bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        if marker == 0x00 || (0xD0..=0xD7).contains(&marker) {
            index += 2;
            continue;
        }
        if marker == 0xFF {
            index += 1;
            continue;
        }
        return Ok(index);
    }
    Err(plain_jpeg())
}

fn render_one(image: &RgbImage, job: &BatchJob) -> Result<Made, SmashError> {
    let fitted = match job.fit {
        Fit::Cover => resize::cover_rgb(image, job.width, job.height)?,
        Fit::Contain => resize::contain_rgb(image, job.width, job.height)?,
    };
    let (width, height) = fitted.dimensions();
    let bytes = lossy::encode_rgb(&fitted, job.quality)?;
    pixels_only(&bytes)?;
    Ok(Made {
        name: job.name.clone(),
        bytes,
        width,
        height,
        fit: job.fit,
        quality: job.quality,
    })
}

/// One decoded picture and every cut made from it.
#[derive(Debug)]
struct BatchCut {
    made: Vec<Made>,
    input_bytes: u64,
    header_width: u32,
    header_height: u32,
    kind: Kind,
    upright_width: u32,
    upright_height: u32,
    orientation: &'static str,
    read: Duration,
    decode: Duration,
    encode: Duration,
}

/// One finished cut, still in memory.
#[derive(Debug)]
pub(crate) struct Made {
    /// The file name from the job.
    pub name: String,
    /// The JPEG bytes.
    pub bytes: Vec<u8>,
    /// Width of those bytes, after `contain` or `cover`.
    pub width: u32,
    /// Height of those bytes.
    pub height: u32,
    /// The fit that produced this size.
    pub fit: Fit,
    /// The quality that was encoded.
    pub quality: u8,
}

impl Fit {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Contain => "contain",
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "smashbin batch",
    about = "Decode one image and write every size in a job file.",
    after_help = "\
The image is read once and decoded once. Each size is scaled from that
picture, then the JPEGs are encoded across the CPU cores.

fit: cover fills the box and crops from the centre.
     contain keeps the whole picture inside the box and does not pad it.

A smasher.json next to the image is a different command. It does not
decode once, and it rejects name and fit."
)]
struct BatchArgs {
    /// JSON or YAML job list. Each job sets name, width, height, quality, and fit.
    #[arg(long, value_name = "FILE")]
    jobs: PathBuf,

    /// Folder for the named files. Missing folders are created.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,

    /// The image to cut.
    file: PathBuf,

    /// Print the version, the picture, and the clock for each step.
    #[arg(long)]
    verbose: bool,
}

fn parse_args() -> Result<BatchArgs, clap::Error> {
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args.len() >= 2 {
        args.remove(1);
    }
    if let Some(program) = args.first_mut() {
        *program = std::ffi::OsString::from("smashbin batch");
    }
    BatchArgs::try_parse_from(args)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use image::ImageEncoder;

    use super::{cut_bytes, pixels_only, DECODES};
    use crate::config::{BatchJob, Fit};
    use crate::lossy;

    #[test]
    fn one_decode_feeds_every_cut_and_threads_match() {
        let png = png_bytes(4, 2);
        let jobs = vec![
            job("icon.jpg", 2, 2, Fit::Cover),
            job("wide.jpg", 4, 2, Fit::Contain),
        ];
        DECODES.with(|count| count.set(0));
        let parallel = cut_bytes(&png, &jobs, true).unwrap();
        assert_eq!(DECODES.with(std::cell::Cell::get), 1);
        let sequential = cut_bytes(&png, &jobs, false).unwrap();
        assert_eq!(DECODES.with(std::cell::Cell::get), 2);
        assert_eq!(parallel.made.len(), 2);
        assert_eq!(parallel.made[0].bytes, sequential.made[0].bytes);
        assert_eq!(parallel.made[1].bytes, sequential.made[1].bytes);
        assert_eq!((parallel.made[0].width, parallel.made[0].height), (2, 2));
        assert_eq!((parallel.made[1].width, parallel.made[1].height), (4, 2));
    }

    #[test]
    fn a_huge_header_is_refused_before_decode() {
        let mut bytes = vec![0u8; 24];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes[8..12].copy_from_slice(&13u32.to_be_bytes());
        bytes[12..16].copy_from_slice(b"IHDR");
        bytes[16..20].copy_from_slice(&50_000u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&50_000u32.to_be_bytes());
        DECODES.with(|count| count.set(0));
        let error = cut_bytes(&bytes, &[job("a.jpg", 8, 8, Fit::Cover)], true).unwrap_err();
        assert!(error.to_string().contains("stops at"));
        assert_eq!(DECODES.with(std::cell::Cell::get), 0);
    }

    #[test]
    fn a_sideways_jpeg_comes_out_upright_without_the_tag() {
        let mut image = image::RgbImage::new(2, 1);
        image.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        image.put_pixel(1, 0, image::Rgb([0, 0, 255]));
        let jpeg = lossy::encode_rgb(&image, 95).unwrap();
        let sideways = insert_orientation(&jpeg, 6);
        let made = cut_bytes(
            &sideways,
            &[job("upright.jpg", 10, 10, Fit::Contain)],
            false,
        )
        .unwrap();
        assert_eq!(made.orientation, "rotate 90");
        assert_eq!((made.made[0].width, made.made[0].height), (5, 10));
        assert!(!made.made[0]
            .bytes
            .windows(4)
            .any(|window| window == b"Exif"));
        pixels_only(&made.made[0].bytes).unwrap();
    }

    #[test]
    fn a_cut_drops_user_metadata_from_jpeg_png_and_gif() {
        let secrets = [
            b"USER-GPS-51.5N-0.1W-SECRET".as_slice(),
            b"USER-XMP-SECRET-DO-NOT-KEEP".as_slice(),
            b"USER-COMMENT-DO-NOT-KEEP".as_slice(),
            b"USER-ICC-PROFILE-SECRET".as_slice(),
            b"USER-PNG-TEXT-SECRET-DO-NOT-KEEP".as_slice(),
            b"USER-PNG-EXIF-SECRET-DO-NOT-KEEP".as_slice(),
            b"USER-GIF-COMMENT-SECRET-DO-NOT-KEEP".as_slice(),
        ];
        let inputs = [dirty_jpeg(), png_with_notes(), gif_with_comment()];
        for source in inputs {
            assert!(secrets.iter().any(|secret| contains(&source, secret)));
            let made = cut_bytes(&source, &[job("clean.jpg", 8, 8, Fit::Cover)], false).unwrap();
            pixels_only(&made.made[0].bytes).unwrap();
            for secret in secrets {
                assert!(!contains(&made.made[0].bytes, secret));
            }
        }
    }

    #[test]
    fn pixels_only_rejects_notes_and_trailing_bytes() {
        let clean = lossy::encode_rgb(&image::RgbImage::new(2, 2), 80).unwrap();
        pixels_only(&clean).unwrap();
        let noted = insert_segment(&clean, 0xFE, b"USER-COMMENT-DO-NOT-KEEP");
        assert!(pixels_only(&noted).is_err());
        let mut trailing = clean.clone();
        trailing.extend_from_slice(b"USER-TRAILING-SECRET");
        assert!(pixels_only(&trailing).is_err());
        let thumb = insert_segment(&clean, 0xE0, b"JFIF\0\x01\x02\x01\x00\x01\x00\x01\x01\x01");
        assert!(pixels_only(&thumb).is_err());
    }

    fn job(name: &str, width: u32, height: u32, fit: Fit) -> BatchJob {
        BatchJob {
            name: name.to_owned(),
            width,
            height,
            quality: 84,
            fit,
        }
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(width, height, image::Rgb([20, 40, 60]));
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                image.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        bytes
    }

    fn insert_orientation(jpeg: &[u8], orientation: u8) -> Vec<u8> {
        let tiff = [
            0x49,
            0x49,
            0x2a,
            0x00,
            0x08,
            0x00,
            0x00,
            0x00,
            0x01,
            0x00,
            0x12,
            0x01,
            0x03,
            0x00,
            0x01,
            0x00,
            0x00,
            0x00,
            orientation,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff);
        insert_segment(jpeg, 0xE1, &payload)
    }

    fn insert_segment(jpeg: &[u8], marker: u8, payload: &[u8]) -> Vec<u8> {
        let length = u16::try_from(2 + payload.len()).unwrap();
        let mut out = Vec::with_capacity(jpeg.len() + payload.len() + 4);
        out.extend_from_slice(&jpeg[..2]);
        out.extend_from_slice(&[0xFF, marker]);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(payload);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    fn dirty_jpeg() -> Vec<u8> {
        let image = image::RgbImage::from_pixel(4, 2, image::Rgb([9, 8, 7]));
        let clean = lossy::encode_rgb(&image, 90).unwrap();
        let gps = b"Exif\0\0USER-GPS-51.5N-0.1W-SECRET".to_vec();
        let xmp =
            b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta>USER-XMP-SECRET-DO-NOT-KEEP</x:xmpmeta>";
        let mut icc = b"ICC_PROFILE\0\x01\x01".to_vec();
        icc.extend_from_slice(b"USER-ICC-PROFILE-SECRET");
        let with_gps = insert_segment(&clean, 0xE1, &gps);
        let with_xmp = insert_segment(&with_gps, 0xE1, xmp);
        let with_icc = insert_segment(&with_xmp, 0xE2, &icc);
        insert_segment(&with_icc, 0xFE, b"USER-COMMENT-DO-NOT-KEEP")
    }

    fn png_with_notes() -> Vec<u8> {
        let raw = png_bytes(4, 2);
        let mut png = img_parts::png::Png::from_bytes(img_parts::Bytes::from(raw)).unwrap();
        let end = png
            .chunks()
            .iter()
            .position(|chunk| chunk.kind() == *b"IEND")
            .unwrap();
        png.chunks_mut().insert(
            end,
            img_parts::png::PngChunk::new(
                *b"tEXt",
                img_parts::Bytes::from(b"Comment\0USER-PNG-TEXT-SECRET-DO-NOT-KEEP".to_vec()),
            ),
        );
        png.chunks_mut().insert(
            end,
            img_parts::png::PngChunk::new(
                *b"eXIf",
                img_parts::Bytes::from(b"USER-PNG-EXIF-SECRET-DO-NOT-KEEP".to_vec()),
            ),
        );
        let mut out = Vec::new();
        for piece in png.encoder() {
            out.extend_from_slice(&piece);
        }
        out
    }

    fn gif_with_comment() -> Vec<u8> {
        let frame = image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]));
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder
                .encode(frame.as_raw(), 2, 2, image::ExtendedColorType::Rgb8)
                .unwrap();
        }
        let secret = b"USER-GIF-COMMENT-SECRET-DO-NOT-KEEP";
        let mut comment = vec![0x21, 0xFE, u8::try_from(secret.len()).unwrap()];
        comment.extend_from_slice(secret);
        comment.push(0x00);
        let end = bytes.iter().rposition(|byte| *byte == 0x3B).unwrap();
        let mut out = bytes[..end].to_vec();
        out.extend_from_slice(&comment);
        out.push(0x3B);
        out
    }

    fn contains(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|window| window == needle)
    }
}
