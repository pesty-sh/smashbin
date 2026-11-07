//! The sample logo must fit a GitHub avatar, and the binary must stay offline.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use assert_cmd::Command;
use image::{ImageDecoder, ImageEncoder};
use predicates::prelude::*;

fn smashbin() -> Command {
    Command::cargo_bin("smashbin").expect("smashbin test binary")
}

fn logo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/github-avatar.jpg")
}

#[test]
fn the_logo_fits_a_github_avatar() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = dir.path().join("avatar.jpg");
    smashbin()
        .arg(logo())
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("avatar=ok"))
        .stderr(predicate::str::contains("1024x1024"))
        .stderr(predicate::str::contains("lossless"));

    let bytes = std::fs::read(&out).expect("read output");
    assert!(bytes.starts_with(&[0xFF, 0xD8]), "output is a JPEG");
    assert!(bytes.len() < 1_000_000, "GitHub allows less than 1 MB");
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).expect("jpeg");
    assert_eq!(decoder.dimensions(), (1024, 1024));
}

#[test]
fn strip_all_removes_the_color_profile() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = dir.path().join("avatar.jpg");
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
        .success()
        .stderr(predicate::str::contains("avatar=ok"));
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
            !contains_ascii_ignore_case(&bytes, tracker),
            "smashbin contains tracker marker {tracker}"
        );
    }
    assert_no_network_libraries(&bin);

    let dir = tempfile::tempdir().expect("temp dir");
    let out = dir.path().join("avatar.jpg");
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

fn contains_ascii_ignore_case(bytes: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    bytes.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(byte, expected)| byte.eq_ignore_ascii_case(expected))
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
    let mut command = std::process::Command::new("sandbox-exec");
    command.args(["-p", "(version 1)\n(allow default)\n(deny network*)\n"]);
    command.arg(bin);
    command
        .args(args)
        .output()
        .expect("run smashbin with network denied")
}
