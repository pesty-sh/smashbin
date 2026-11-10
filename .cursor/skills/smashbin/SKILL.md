---
name: smashbin
description: >-
  Build, install, and package the smashbin offline image CLI. Use when
  working in the smashbin repo, or when the user mentions make, make deb,
  Homebrew, apt, Ubuntu 18.04, glibc, or the smashbin command line.
---

# smashbin

Cargo workspace at the repo root. `crates/smashbin-core` is the library. `crates/smashbin` is the binary. The binary name is `smashbin`. Both crates share `target/`. The command-line types live in the library, because the image steps use them. Do not move those types into the binary: the library would then depend on the binary, and Cargo rejects that loop. The root manifest is virtual. `cargo install` and a future Homebrew formula install `crates/smashbin`. The formula in the tap still points at the 0.1.5 tag, which is the old single-package layout. Do not change that formula until a new tag exists.

`smashbin batch --jobs FILE --out DIR IMAGE` decodes once and writes every job. Each job requires `name`, `width`, `height`, `quality`, and `fit` (`cover` or `contain`). `cover` crops the centre to the exact box. `contain` fits the whole picture inside the box and does not pad. Encode the cuts in parallel with `std::thread`. Do not add rayon. A `smasher.json` or `smasher.yaml` next to the image stays one job after another and rejects `name` and `fit`. The checked-in lists are `fixtures/gallery.yaml` and `fixtures/gallery.json` (23 cuts). stderr ends with `N files in S.s`. `--verbose` also prints the version, `smashbin-core`, the pixel cap, the CPU count, and separate read, decode, encode, and write times. It says the PHP module and the GUI are later. Do not put a speed claim in the README until that line has been run on Stuart's 25 MB JPEG. PHP and GD take 50–75 seconds for the template cuts. Phone orientation is applied on the batch path only, then the new JPEG is written without the tag. Refuse a header whose width times height is over 100 million pixels, or whose side is over 10 000 pixels, before decoding. A file over 64 MB is refused while it is being read. Each batch cut is at most 8 million pixels, and one batch file holds at most 64 cuts. A cut name is a single `name.jpg` inside `--out`. The write replaces a symlink in that folder instead of following it. Each written JPEG is checked before it is saved. The only allowed markers are a JFIF header with no thumbnail, the frame, the tables, and the scan. EXIF, XMP, a comment, a color profile, and bytes after the end marker stop that cut.

Version 0.1.6 strips metadata, resizes with `--width`/`-w` and `--height`/`-h`, and losslessly shrinks PNG with the pure-Rust `png` crate. One missing side keeps the aspect ratio. Both sides with a different aspect crop from the centre. `--cv` is `top`, `middle`, `bottom`, or 0–100 (0 is the bottom, 100 is the top). `--ch` is `left`, `middle`, `right`, or 0–100. The slice never leaves the picture. `--dpi` sets the stored pixels per inch and does not move pixels. JPEG and PNG store it directly. WebP stores it in EXIF. A container with no density field is still written, and stderr names the original image's density. `--quality` and `--bytes` cannot be combined. They re-encode JPEG only, after the resize. A `smasher.json` or `smasher.yaml` next to the image runs each job top to bottom. `smasher.json` wins when both are present, then `smasher.yaml`, then `smasher.yml`. `-c` / `--config` selects a different smasher file. Parse those files in the program. Do not add a JSON or YAML crate. `--format` / `-f` writes jpg, png, gif, webp, bmp, tif, tga, ico, qoi, ppm, hdr, or ff. WebP is lossless. Do not add AVIF or lossy WebP: those need a C library or a second large encoder. This version does not pad to a square. Do not add a C library, a `-sys` crate, or a header file. A PHP module and a Tunes GUI are later and are not part of this binary. The license is MIT.

## Builds

Same shape as qrbin. `Makefile` calls `~/.cargo/bin/cargo`. `MACOSX_DEPLOYMENT_TARGET` is `10.12` for the host build and `11.0` for `make build-silicon`. `make build-windows` uses `x86_64-pc-windows-gnu` and `x86_64-w64-mingw32-gcc`. `make` does not install.

`make deb` runs `packaging/build-linux-host.sh`. `LINUX_GLIBC` is `2.27`. Output is `dist/smashbin-linux-amd64`, `dist/smashbin-linux-arm64`, and a `.deb` for each. Zig may be reused from `~/.cache/qrbin` or downloaded into `~/.cache/smashbin`. Do not use Docker. Do not install the Linux binaries over the Mac one.

The Homebrew formula lives only in `/Users/developers/Projects/pesty-sh/homebrew-tap/Formula/smashbin.rb`. The apt repository lives only in `/Users/developers/Projects/pesty-sh/apt`. `python3 publish.py` there copies the `.deb` files into `pool/` and signs `dists/stable`.

## Privacy

No HTTP client, telemetry, crash reporter, update check, or image-content logging. Errors must not print pixel bytes.
