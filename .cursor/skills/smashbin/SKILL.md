---
name: smashbin
description: >-
  Build, install, and package the smashbin offline image CLI. Use when
  working in the smashbin repo, or when the user mentions make, make deb,
  Homebrew, apt, Ubuntu 18.04, glibc, or the smashbin command line.
---

# smashbin

Single Rust package at the repo root. Binary and package name are `smashbin`.

Version 0.1.3 strips metadata, resizes with `--width`/`-w` and `--height`/`-h`, losslessly shrinks PNG with the pure-Rust `png` crate, and checks the GitHub avatar rule: PNG, JPG, or GIF, under 1,000,000 bytes, edges smaller than 3000. One missing side keeps the aspect ratio. `--quality` and `--bytes` cannot be combined. They re-encode JPEG only, after the resize. A `smasher.json` or `smasher.yaml` next to the image runs each job top to bottom. `-c` / `--config` selects a different smasher file. Parse those files in the program. Do not add a JSON or YAML crate. This version does not change format or pad to a square. Do not add a C library, a `-sys` crate, or a header file. A PHP module and a Tunes GUI are later and are not part of this binary. The license is MIT.

## Builds

Same shape as qrbin. `Makefile` calls `~/.cargo/bin/cargo`. `MACOSX_DEPLOYMENT_TARGET` is `10.12` for the host build and `11.0` for `make build-silicon`. `make build-windows` uses `x86_64-pc-windows-gnu` and `x86_64-w64-mingw32-gcc`. `make` does not install.

`make deb` runs `packaging/build-linux-host.sh`. `LINUX_GLIBC` is `2.27`. Output is `dist/smashbin-linux-amd64`, `dist/smashbin-linux-arm64`, and a `.deb` for each. Zig may be reused from `~/.cache/qrbin` or downloaded into `~/.cache/smashbin`. Do not use Docker. Do not install the Linux binaries over the Mac one.

The Homebrew formula lives only in `/Users/developers/Projects/pesty-sh/homebrew-tap/Formula/smashbin.rb`. The apt repository lives only in `/Users/developers/Projects/pesty-sh/apt`. `python3 publish.py` there copies the `.deb` files into `pool/` and signs `dists/stable`.

## Privacy

No HTTP client, telemetry, crash reporter, update check, or image-content logging. Errors must not print pixel bytes.
