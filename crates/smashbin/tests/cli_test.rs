//! The sample logo still decodes, and the binary must stay offline.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use assert_cmd::Command;
use image::{ImageDecoder, ImageEncoder};
use predicates::prelude::*;

fn smashbin() -> Command {
    Command::cargo_bin("smashbin").expect("smashbin test binary")
}

fn logo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/logo.jpg")
}

#[test]
fn the_logo_stays_a_jpeg() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = dir.path().join("logo-out.jpg");
    smashbin()
        .arg(logo())
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("1024x1024"))
        .stderr(predicate::str::contains("lossless"));

    let bytes = std::fs::read(&out).expect("read output");
    assert!(bytes.starts_with(&[0xFF, 0xD8]), "output is a JPEG");
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).expect("jpeg");
    assert_eq!(decoder.dimensions(), (1024, 1024));
}

#[test]
fn strip_all_removes_the_color_profile() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = dir.path().join("logo-out.jpg");
    let before = std::fs::read(logo()).expect("logo");
    smashbin()
        .arg(logo())
        .args(["--strip", "all", "--out"])
        .arg(&out)
        .assert()
        .success();
    let after = std::fs::read(&out).expect("stripped");
    assert!(after.len() < before.len());
    assert!(!after.windows(12).any(|window| window == b"ICC_PROFILE\0"));
    assert!(after.starts_with(&[0xFF, 0xD8]));
}

#[test]
fn quality_and_bytes_cannot_both_be_set() {
    smashbin()
        .args(["--quality", "60", "--bytes", "1mb"])
        .arg(logo())
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn a_byte_cap_rewrites_the_jpeg_under_that_cap() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = dir.path().join("small.jpg");
    smashbin()
        .arg(logo())
        .args(["--bytes", "40kb", "--out"])
        .arg(&out)
        .assert()
        .success();
    let bytes = std::fs::read(&out).expect("small");
    assert!(bytes.len() <= 40_000, "got {} bytes", bytes.len());
    assert!(bytes.starts_with(&[0xFF, 0xD8]));
}

#[test]
fn the_default_file_lands_beside_the_source() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("logo.jpg");
    std::fs::copy(logo(), &source).expect("copy");
    smashbin().arg(&source).assert().success();
    let smashed = dir.path().join("logo_strip_safe.jpg");
    assert!(smashed.is_file(), "missing {}", smashed.display());
    let original = std::fs::read(&source).expect("original still there");
    assert_eq!(original.len(), 82_180);
}

#[test]
fn binary_does_not_phone_home() {
    let bin = assert_cmd::cargo::cargo_bin("smashbin");
    let bytes = std::fs::read(&bin).expect("read smashbin");

    let hosts = embedded_hosts(&bytes);
    // github.com is clap's bug-report string. ns.adobe.com is the XMP
    // marker prefix compiled into img-parts. Neither is a connection.
    assert!(
        hosts.iter().all(|host| {
            host == "github.com"
                || host == "www.w3.org"
                || host == "ns.adobe.com"
                || host == "pesty.sh"
                || host.ends_with(".pesty.sh")
        }),
        "smashbin embedded an unexpected URL host: {hosts:?}"
    );
    for tracker in [
        "sentry.io",
        "bugsnag",
        "crashlytics",
        "posthog",
        "mixpanel",
        "segment.io",
        "google-analytics",
        "googletagmanager",
        "reqwest",
        "ureq",
    ] {
        assert!(
            !contains_marker(&bytes, tracker),
            "smashbin contains tracker marker {tracker}"
        );
    }
    assert_no_network_libraries(&bin);

    let dir = tempfile::tempdir().expect("temp dir");
    let out = dir.path().join("logo-out.jpg");
    let output = run_with_network_denied(
        &bin,
        &[
            logo().to_str().expect("utf8 path"),
            "--out",
            out.to_str().expect("utf8 path"),
        ],
    );
    assert!(
        output.status.success(),
        "network denied run failed: {:?}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.is_file());
}

#[test]
fn width_keeps_the_aspect_ratio() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    let out = dir.path().join("wide-out.jpg");
    smashbin()
        .arg(&source)
        .args(["--width", "2", "--out"])
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("resize"))
        .stderr(predicate::str::contains("2x1"));
    let bytes = std::fs::read(&out).expect("resized");
    assert!(bytes.starts_with(&[0xFF, 0xD8]));
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).expect("jpeg");
    assert_eq!(decoder.dimensions(), (2, 1));
}

#[test]
fn a_1024_by_512_width_of_512_is_256_tall() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 1024, 512);
    smashbin()
        .arg(&source)
        .args(["-w", "512"])
        .assert()
        .success()
        .stderr(predicate::str::contains("resize"))
        .stderr(predicate::str::contains("512x256"));
    let bytes = std::fs::read(dir.path().join("wide_strip_safe_width_512.jpg")).expect("resized");
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).expect("jpeg");
    assert_eq!(decoder.dimensions(), (512, 256));
}

#[test]
fn height_keeps_the_aspect_ratio() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    let out = dir.path().join("wide-out.jpg");
    smashbin()
        .arg(&source)
        .args(["--height", "1", "--out"])
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("2x1"));
    let bytes = std::fs::read(&out).expect("resized");
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).expect("jpeg");
    assert_eq!(decoder.dimensions(), (2, 1));
}

#[test]
fn both_sides_use_the_typed_pixels() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    let out = dir.path().join("square.jpg");
    smashbin()
        .arg(&source)
        .args(["-w", "2", "-h", "2", "--out"])
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("crop"))
        .stderr(predicate::str::contains("2x2"));
    let bytes = std::fs::read(&out).expect("resized");
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).expect("jpeg");
    assert_eq!(decoder.dimensions(), (2, 2));
}

#[test]
fn quality_overrides_the_resized_jpeg() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    let out = dir.path().join("small.jpg");
    smashbin()
        .arg(&source)
        .args(["--width", "2", "--quality", "40", "--out"])
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("quality=40"))
        .stderr(predicate::str::contains("2x1"));
    let bytes = std::fs::read(&out).expect("resized");
    assert!(bytes.starts_with(&[0xFF, 0xD8]));
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).expect("jpeg");
    assert_eq!(decoder.dimensions(), (2, 1));
}

#[test]
fn a_png_resize_stays_a_png() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.png");
    let image = image::RgbImage::from_pixel(4, 2, image::Rgb([1, 2, 3]));
    image.save(&source).expect("png");
    let out = dir.path().join("wide-out.png");
    smashbin()
        .arg(&source)
        .args(["--width", "2", "--out"])
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("resize"))
        .stderr(predicate::str::contains("PNG"));
    let bytes = std::fs::read(&out).expect("resized");
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes)).expect("png");
    assert_eq!(decoder.dimensions(), (2, 1));
}

#[test]
fn a_smasher_json_beside_the_image_runs_every_job() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("logo.jpg");
    std::fs::copy(logo(), &source).expect("copy");
    std::fs::write(
        dir.path().join("smasher.json"),
        r#"{"jobs":[{"width":512},{"quality":40}]}"#,
    )
    .expect("config");
    smashbin().arg(&source).assert().success();
    assert!(dir.path().join("logo_strip_safe_width_512.jpg").is_file());
    assert!(dir.path().join("logo_strip_safe_quality_40.jpg").is_file());
    assert_eq!(std::fs::metadata(&source).expect("source").len(), 82_180);
}

#[test]
fn config_flag_uses_that_file_and_not_the_one_beside_the_image() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("logo.jpg");
    std::fs::copy(logo(), &source).expect("copy");
    std::fs::write(
        dir.path().join("smasher.json"),
        r#"{"jobs":[{"width":10}]}"#,
    )
    .expect("sibling");
    let chosen = dir.path().join("other.yaml");
    std::fs::write(chosen, "jobs:\n  - quality: 30\n").expect("chosen");
    smashbin()
        .arg(&source)
        .args(["-c"])
        .arg(dir.path().join("other.yaml"))
        .assert()
        .success();
    assert!(dir.path().join("logo_strip_safe_quality_30.jpg").is_file());
    assert!(!dir.path().join("logo_strip_safe_width_10.jpg").exists());
}

#[test]
fn a_dot_out_folder_sits_beside_the_input() {
    let dir = tempfile::tempdir().expect("temp dir");
    let photos = dir.path().join("photos");
    std::fs::create_dir(&photos).expect("photos");
    let source = photos.join("wide.jpg");
    write_jpeg(&source, 4, 2);
    let cwd = tempfile::tempdir().expect("cwd");
    smashbin()
        .current_dir(cwd.path())
        .arg(&source)
        .args(["--out", "./smashed"])
        .assert()
        .success();
    assert!(photos.join("smashed").join("wide_strip_safe.jpg").is_file());
    assert!(!cwd.path().join("smashed").exists());
}

#[test]
fn a_missing_output_folder_is_created() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    let out = dir.path().join("icons").join("nested").join("wide.jpg");
    smashbin()
        .arg(&source)
        .arg("--out")
        .arg(&out)
        .assert()
        .success();
    assert!(out.is_file(), "missing {}", out.display());
}

#[test]
fn a_missing_job_folder_is_created() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    let config = dir.path().join("jobs.json");
    std::fs::write(&config, r#"{"jobs":[{"width":2},{"height":1}]}"#).expect("config");
    let out_dir = dir.path().join("set").join("icons");
    smashbin()
        .arg(&source)
        .arg("-c")
        .arg(&config)
        .arg("--out")
        .arg(&out_dir)
        .assert()
        .success();
    assert!(out_dir.join("wide_strip_safe_width_2.jpg").is_file());
    assert!(out_dir.join("wide_strip_safe_height_1.jpg").is_file());
}

#[test]
fn a_mismatched_size_crops_from_the_centre() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("square.png");
    write_png_rgb(&source, 4, 4);
    smashbin()
        .arg(&source)
        .args(["-w", "4", "-h", "2"])
        .assert()
        .success()
        .stderr(predicate::str::contains("crop"))
        .stderr(predicate::str::contains("4x2"));
    let bytes =
        std::fs::read(dir.path().join("square_strip_safe_width_4_height_2.png")).expect("crop");
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes)).expect("png");
    assert_eq!(decoder.dimensions(), (4, 2));
}

#[test]
fn cv_top_sits_at_the_top() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("tall.png");
    write_png_bands(&source);
    smashbin()
        .arg(&source)
        .args(["-h", "1", "--cv", "top"])
        .assert()
        .success()
        .stderr(predicate::str::contains("crop"))
        .stderr(predicate::str::contains("2x1"));
    let bytes =
        std::fs::read(dir.path().join("tall_strip_safe_height_1_cv_top.png")).expect("crop");
    let image = image::load_from_memory(&bytes).expect("png");
    assert_eq!(image.to_rgb8().get_pixel(0, 0), &image::Rgb([255, 0, 0]));
}

#[test]
fn cv_bottom_sits_at_the_bottom() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("tall.png");
    write_png_bands(&source);
    smashbin()
        .arg(&source)
        .args(["-h", "1", "--cv", "0"])
        .assert()
        .success();
    let bytes = std::fs::read(dir.path().join("tall_strip_safe_height_1_cv_0.png")).expect("crop");
    let image = image::load_from_memory(&bytes).expect("png");
    assert_eq!(image.to_rgb8().get_pixel(0, 0), &image::Rgb([0, 0, 255]));
}

#[test]
fn cv_needs_a_height() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    smashbin()
        .arg(&source)
        .args(["--cv", "top"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--cv needs --height"));
}

#[test]
fn dpi_is_stored_and_the_pixels_stay() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    smashbin()
        .arg(&source)
        .args(["--dpi", "300"])
        .assert()
        .success()
        .stderr(predicate::str::contains("dpi=300"))
        .stderr(predicate::str::contains("4x2"));
    let out = dir.path().join("wide_strip_safe_dpi_300.jpg");
    let bytes = std::fs::read(&out).expect("dpi jpeg");
    let start = bytes
        .windows(5)
        .position(|window| window == b"JFIF\0")
        .expect("JFIF");
    let pair = 300u16.to_be_bytes();
    assert_eq!(bytes[start + 7], 1);
    assert_eq!(
        &bytes[start + 8..start + 12],
        &[pair[0], pair[1], pair[0], pair[1]]
    );
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).expect("jpeg");
    assert_eq!(decoder.dimensions(), (4, 2));
}

#[test]
fn a_gif_cannot_store_dpi() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("dot.gif");
    let bytes = [
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x4C, 0x01, 0x00, 0x3B,
    ];
    std::fs::write(&source, bytes).expect("gif");
    smashbin()
        .arg(&source)
        .args(["--dpi", "72"])
        .assert()
        .success()
        .stderr(predicate::str::contains("not stored"))
        .stderr(predicate::str::contains("no display density"));
    assert!(dir.path().join("dot_strip_safe_dpi_72.gif").is_file());
}

#[test]
fn ext_writes_png_and_webp() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 4, 2);
    smashbin()
        .arg(&source)
        .args(["-f", "png"])
        .assert()
        .success()
        .stderr(predicate::str::contains("convert"))
        .stderr(predicate::str::contains("PNG"))
        .stderr(predicate::str::contains("4x2"));
    let png = std::fs::read(dir.path().join("wide_strip_safe.png")).expect("png");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(png)).expect("png");
    assert_eq!(decoder.dimensions(), (4, 2));

    smashbin()
        .arg(&source)
        .args(["--format", "webp"])
        .assert()
        .success()
        .stderr(predicate::str::contains("WEBP"));
    let webp = std::fs::read(dir.path().join("wide_strip_safe.webp")).expect("webp");
    assert!(webp.starts_with(b"RIFF") && webp.get(8..12) == Some(b"WEBP".as_slice()));
}

#[test]
fn quality_does_not_mix_with_a_png_ext() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.jpg");
    write_jpeg(&source, 2, 2);
    smashbin()
        .arg(&source)
        .args(["--format", "png", "--quality", "40"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--format jpg"));
}

#[test]
fn a_text_file_is_not_an_image() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, b"hello").expect("notes");
    smashbin()
        .arg(&path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("PNG, JPG, and GIF"));
}

#[test]
fn a_header_wider_than_the_cap_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("wide.png");
    let mut bytes = vec![0u8; 26];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    bytes[8..12].copy_from_slice(&13u32.to_be_bytes());
    bytes[12..16].copy_from_slice(b"IHDR");
    bytes[16..20].copy_from_slice(&20_000u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&1u32.to_be_bytes());
    std::fs::write(&path, &bytes).expect("header");
    smashbin()
        .arg(&path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("stops at"));
}

#[test]
fn an_empty_file_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("empty.jpg");
    std::fs::write(&path, b"").expect("empty");
    smashbin()
        .arg(&path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("the input is empty"));
}

#[test]
fn empty_stdin_is_refused() {
    smashbin()
        .write_stdin(Vec::<u8>::new())
        .assert()
        .failure()
        .stderr(predicate::str::contains("the input is empty"));
}

#[test]
fn a_missing_file_is_refused() {
    smashbin()
        .arg("no-such-image.jpg")
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not read or write the file"));
}

#[test]
fn ch_needs_a_width() {
    let dir = tempfile::tempdir().expect("temp dir");
    smashbin()
        .arg(logo())
        .arg("--ch")
        .arg("left")
        .arg("--out")
        .arg(dir.path().join("out.jpg"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("--ch needs --width"));
}

#[test]
fn stdin_png_is_written_to_stdout() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("tiny.png");
    write_png_rgb(&source, 2, 2);
    let bytes = std::fs::read(&source).expect("tiny png");
    let output = smashbin()
        .arg("--stdout")
        .write_stdin(bytes)
        .assert()
        .success()
        .stderr(predicate::str::contains("2x2"));
    let written = output.get_output().stdout.clone();
    assert!(written.starts_with(b"\x89PNG"), "stdout is a PNG");
}

#[test]
fn flags_do_not_mix_with_a_smasher_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("logo.jpg");
    std::fs::copy(logo(), &source).expect("copy");
    std::fs::write(dir.path().join("smasher.yaml"), "jobs:\n  - strip: all\n").expect("config");
    smashbin()
        .arg(&source)
        .args(["--width", "10"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("smasher file lists the jobs"));
}

#[test]
fn batch_writes_the_gallery_cuts() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.png");
    write_png_rgb(&source, 4, 2);
    let out = dir.path().join("variants");
    let jobs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gallery.yaml");
    smashbin()
        .arg("batch")
        .arg("--jobs")
        .arg(&jobs)
        .arg("--out")
        .arg(&out)
        .arg(&source)
        .assert()
        .success()
        .stderr(predicate::str::contains("23 files in"))
        .stderr(predicate::str::contains("icon_24x24.png  24x24  cover"));
    let icon = std::fs::read(out.join("icon_24x24.png")).expect("icon");
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(icon)).expect("png");
    assert_eq!(decoder.dimensions(), (24, 24));
    let header = std::fs::read(out.join("header_415x160.png")).expect("header");
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(header)).expect("png");
    assert_eq!(decoder.dimensions(), (415, 160));
    let preview = std::fs::read(out.join("preview_820x860.png")).expect("preview");
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(preview)).expect("png");
    assert_eq!(decoder.dimensions(), (820, 410));
    assert!(out.join("srcset_2400.png").is_file(), "srcset file");
}

#[test]
fn batch_verbose_prints_the_version_and_the_clock() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.png");
    write_png_rgb(&source, 4, 2);
    let jobs = dir.path().join("two.yaml");
    std::fs::write(
        &jobs,
        "jobs:\n  - name: icon.jpg\n    width: 2\n    height: 2\n    quality: 84\n    fit: cover\n  - name: wide.jpg\n    width: 4\n    height: 2\n    quality: 84\n    fit: contain\n",
    )
    .expect("jobs");
    smashbin()
        .arg("batch")
        .arg("--verbose")
        .arg("--jobs")
        .arg(&jobs)
        .arg("--out")
        .arg(dir.path().join("out"))
        .arg(&source)
        .assert()
        .success()
        .stderr(predicate::str::contains("smashbin 0.1.7"))
        .stderr(predicate::str::contains("library: smashbin-core 0.1.7"))
        .stderr(predicate::str::contains("php module: later"))
        .stderr(predicate::str::contains("smashgui: later"))
        .stderr(predicate::str::contains("pixel cap: 100000000"))
        .stderr(predicate::str::contains("decode:"))
        .stderr(predicate::str::contains("encode:"))
        .stderr(predicate::str::contains("2 files in"));
}

#[test]
fn batch_format_flag_writes_webp_and_a_job_can_choose_png() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.png");
    write_png_rgb(&source, 4, 2);
    let jobs = dir.path().join("cuts.yaml");
    std::fs::write(
        &jobs,
        "jobs:\n  - name: icon.jpg\n    width: 4\n    height: 2\n    quality: 84\n    fit: cover\n  - name: tile.png\n    width: 4\n    height: 2\n    quality: 84\n    fit: contain\n    format: png\n",
    )
    .expect("jobs");
    let out = dir.path().join("out");
    smashbin()
        .arg("batch")
        .args(["--format", "webp"])
        .arg("--jobs")
        .arg(&jobs)
        .arg("--out")
        .arg(&out)
        .arg(&source)
        .assert()
        .success()
        .stderr(predicate::str::contains("icon.webp"))
        .stderr(predicate::str::contains("WEBP"))
        .stderr(predicate::str::contains("tile.png"))
        .stderr(predicate::str::contains("PNG"));
    let webp = std::fs::read(out.join("icon.webp")).expect("webp");
    assert_eq!(&webp[..4], b"RIFF");
    assert_eq!(&webp[8..12], b"WEBP");
    let png = std::fs::read(out.join("tile.png")).expect("png");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn a_smasher_file_does_not_accept_batch_fields() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("wide.png");
    write_png_rgb(&source, 4, 2);
    std::fs::write(
        dir.path().join("smasher.json"),
        r#"{"jobs":[{"name":"a.jpg","width":2,"height":2,"quality":84,"fit":"cover"}]}"#,
    )
    .expect("smasher");
    smashbin()
        .arg(&source)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown field"));
}

fn write_png_rgb(path: &std::path::Path, width: u32, height: u32) {
    image::RgbImage::from_pixel(width, height, image::Rgb([10, 20, 30]))
        .save(path)
        .expect("write png");
}

fn write_png_bands(path: &std::path::Path) {
    let mut image = image::RgbImage::new(2, 3);
    for x in 0..2 {
        image.put_pixel(x, 0, image::Rgb([255, 0, 0]));
        image.put_pixel(x, 1, image::Rgb([0, 255, 0]));
        image.put_pixel(x, 2, image::Rgb([0, 0, 255]));
    }
    image.save(path).expect("write png");
}

fn write_jpeg(path: &std::path::Path, width: u32, height: u32) {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([10, 20, 30]));
    let mut bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90);
    encoder
        .write_image(
            image.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .expect("encode jpeg");
    std::fs::write(path, bytes).expect("write jpeg");
}

fn embedded_hosts(bytes: &[u8]) -> Vec<String> {
    let mut hosts = Vec::new();
    for scheme in [b"https://".as_slice(), b"http://".as_slice()] {
        let mut index = 0;
        while let Some(found) = bytes[index..]
            .windows(scheme.len())
            .position(|window| window == scheme)
        {
            let start = index + found + scheme.len();
            let mut end = start;
            while end < bytes.len() && is_host_byte(bytes[end]) {
                end += 1;
            }
            if let Ok(host) = std::str::from_utf8(&bytes[start..end]) {
                let host = host.to_ascii_lowercase();
                if !host.is_empty() && !hosts.contains(&host) {
                    hosts.push(host);
                }
            }
            index = start;
        }
    }
    hosts.sort();
    hosts
}

/// A tracker name, not a piece of a longer word such as `Urequired`.
///
/// A letter or digit on either side does not count. Rust packs the next
/// string straight after `InvalidSignature`, and those bytes contain `ureq`.
/// That is the word "signature", not the ureq crate.
fn contains_marker(bytes: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    if needle.is_empty() || needle.len() > bytes.len() {
        return false;
    }
    bytes
        .windows(needle.len())
        .enumerate()
        .any(|(index, window)| {
            let matched = window
                .iter()
                .zip(needle)
                .all(|(byte, expected)| byte.eq_ignore_ascii_case(expected));
            if !matched {
                return false;
            }
            let before = index.checked_sub(1).map(|at| bytes[at]);
            let after = bytes.get(index + needle.len()).copied();
            !before.is_some_and(|byte| byte.is_ascii_alphanumeric())
                && !after.is_some_and(|byte| byte.is_ascii_alphanumeric())
        })
}

fn is_host_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-'
}

fn assert_no_network_libraries(bin: &std::path::Path) {
    if cfg!(target_os = "macos") {
        let output = std::process::Command::new("otool")
            .args(["-L"])
            .arg(bin)
            .output()
            .expect("otool");
        assert!(output.status.success(), "otool failed");
        let text = String::from_utf8_lossy(&output.stdout);
        let mut libraries = Vec::new();
        for line in text.lines().skip(1) {
            let path = line.split_whitespace().next().unwrap_or("");
            if !path.is_empty() {
                libraries.push(path.to_string());
            }
        }
        assert!(
            libraries.iter().all(|path| {
                path.ends_with("/libSystem.B.dylib")
                    || path.ends_with("/libiconv.2.dylib")
                    || path.ends_with("/libc++.1.dylib")
            }),
            "unexpected linked libraries: {libraries:?}"
        );
    }
}

fn run_with_network_denied(bin: &std::path::Path, args: &[&str]) -> std::process::Output {
    // A fresh network namespace has no routes, so a connect would fail.
    // The program should still write the image.
    //
    // macOS uses sandbox-exec. A desktop Linux can use `unshare` with a
    // user namespace. GitHub's Ubuntu runners reject the write to uid_map,
    // so there the same namespace is opened with passwordless sudo.
    // Windows has neither tool. The string and library checks above are
    // what that runner can prove, and the program is still run so a crash
    // fails the test.
    if cfg!(target_os = "macos") {
        return std::process::Command::new("sandbox-exec")
            .args(["-p", "(version 1)\n(allow default)\n(deny network*)\n"])
            .arg(bin)
            .args(args)
            .output()
            .expect("run smashbin with network denied");
    }

    if cfg!(target_os = "linux") {
        let user_namespace = std::process::Command::new("unshare")
            .args(["--net", "--map-root-user"])
            .arg(bin)
            .args(args)
            .output();
        if let Ok(output) = user_namespace {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let namespace_blocked =
                stderr.contains("uid_map") || stderr.contains("Operation not permitted");
            if !namespace_blocked {
                return output;
            }
        }
        return std::process::Command::new("sudo")
            .args(["-n", "unshare", "--net", "--"])
            .arg(bin)
            .args(args)
            .output()
            .expect("run smashbin with network denied");
    }

    std::process::Command::new(bin)
        .args(args)
        .output()
        .expect("run smashbin")
}
