# smashbin

smashbin makes one image smaller on your own machine. It does not open a network connection. The result is one binary. The Rust libraries it uses are compiled into that file. There is no C code, no header to compile, and no second library to install beside it.

Version 0.1.4 strips metadata and resizes when you pass a width or a height. A PHP module and a Tunes GUI are later. They are not in this version. The license is MIT. The text is `LICENSE`.

The sample logo in `fixtures/logo.jpg` is a 1024 by 1024 JPEG, 82,180 bytes. Running smashbin on it with no size flags does not resize it and does not change its format.

## What this version will not do

Padding a wide picture into a square is later. It is not a flag yet.

## The trip one image takes

The program is a short list of small files. `src/main.rs` calls them in this order and does nothing else.

1. `src/cli.rs` reads what you typed.
2. `src/read.rs` loads the bytes. A missing filename reads stdin.
3. `src/sniff.rs` looks at the first bytes. The filename does not decide the format.
4. `src/strip.rs` removes private notes. GPS, EXIF, comments, and editor data go. The color profile stays, unless you pass `--strip all`.
5. `src/resize.rs` runs when you pass `--width` or `--height`. Leave one of them out and the other side keeps the aspect ratio. The file stays the same format.
6. `src/lossless.rs` writes a PNG again with the pure-Rust `png` crate. A JPEG that was not resized is not decoded here. Decoding and saving again, even at quality 100, would be a second copy and a little more damage. A resized JPEG is already a new file from step 5, at quality 100, unless step 7 replaces it.
7. `src/lossy.rs` runs when you pass `--quality` or `--bytes`. It replaces step 6 with a new JPEG. If `--bytes` is already met, it does not build another JPEG. You cannot pass both flags. PNG and GIF have no quality knob.
8. `src/convert.rs` runs when you pass `--ext` / `-e` and the new container is different. The suffix of the generated name changes. WebP is lossless. A GIF animation becomes its first frame. An ICO must be 256 by 256 or smaller. JPEG output with no `--quality` is quality 100.
9. `src/density.rs` runs when you pass `--dpi`. JPEG and PNG store that many pixels per inch. WebP stores it in an EXIF block. A container with no density field is still written, and stderr names the original image's density. The pixel grid stays the same.
10. `src/write.rs` writes the image. The one-line report goes to stderr, so a pipe stays a pure image.

Exit 0 means the image was written. Exit 1 means the file could not be read, or a flag does not apply.

## Try the logo

From this directory, after `make`:

```bash
./target/release/smashbin fixtures/logo.jpg
```

That writes `fixtures/logo_strip_safe.jpg` beside the original and leaves the original alone. The new name is the original stem, then `_`, then the inputs in a fixed order: `strip`, `width`, `height`, `dpi`, `quality`, `bytes`. stderr looks like:

```text
fixtures/logo.jpg  82180 -> 82180  lossless  1024x1024  JPEG  strip=safe
```

The numbers stay equal on the safe strip because the only metadata in this file is a color profile, and the safe strip keeps it.

Drop the profile:

```bash
./target/release/smashbin --strip all --out /tmp/logo.jpg fixtures/logo.jpg
```

Force a smaller JPEG. This does change pixels:

```bash
./target/release/smashbin --bytes 40kb --out /tmp/logo.jpg fixtures/logo.jpg
```

`--quality 80` picks the quality yourself. `--bytes 40kb` searches for the highest quality that lands at or under 40,000 bytes.

Resize, then let a quality override replace that file:

```bash
./target/release/smashbin --width 500 --quality 80 fixtures/logo.jpg
```

That writes `fixtures/logo_strip_safe_width_500_quality_80.jpg`. The height stays 500 because the logo is square. `--height 250` with no width on a 1024 by 512 picture would write a width of 500.

`--dpi 300` stores 300 pixels per inch. A lower number claims a larger print. A higher number claims a smaller one. The pixels do not move. A GIF cannot store this.

```bash
./target/release/smashbin --dpi 300 fixtures/logo.jpg
```

`--ext png` writes a PNG next to the JPEG. `-e webp` writes a lossless WebP. The other pure-Rust containers are `gif`, `bmp`, `tif`, `tga`, `ico`, `qoi`, `ppm`, `hdr`, and `ff`.

```bash
./target/release/smashbin -e png fixtures/logo.jpg
```

A `smasher.json` or `smasher.yaml` in the same directory as the image runs every job in that file, top to bottom. When both are present, `smasher.json` is the one that runs. Each job writes its own file. `-c` points at a smasher file that lives somewhere else. The readers are part of this program. There is no JSON or YAML library to install.

```json
{
  "jobs": [
    { "width": 512 },
    { "quality": 40 }
  ]
}
```

```yaml
jobs:
  - width: 512
  - quality: 40
```

## Build

`make` compiles `target/release/smashbin` for this Mac. It does not copy the binary onto your PATH. The Intel build is stamped macOS 10.12 so a 2014 Mac can run it. `make` calls `~/.cargo/bin/cargo`, not the Homebrew cargo on PATH. Homebrew's compiler stamps macOS 13.0.

| Command | What it writes | Installs? |
|---|---|---|
| `make` | `target/release/smashbin` | no |
| `make build-silicon` | `target/aarch64-apple-darwin/release/smashbin` | no |
| `make build-windows` | `target/x86_64-pc-windows-gnu/release/smashbin.exe` | no |
| `make deb` | `dist/smashbin-linux-amd64`, `dist/smashbin-linux-arm64`, and a `.deb` for each | no |

`make doctor` prints which cargo and which linker will be used.

Windows needs `brew install mingw-w64`. The linker is `x86_64-w64-mingw32-gcc`.

Linux is built on this Mac. The glibc floor is 2.27, which is Ubuntu 18.04. That same binary runs on Ubuntu 20.04, 22.04, and 24.04, Debian 10 through 13, and Arch, because those ship a newer glibc. Intel and AMD are one build. 64-bit ARM is the other. Docker is not used. The Linux binaries are not copied over the Mac one.

## Install

The Mac install is Homebrew, from the tap at `/Users/developers/Projects/pesty-sh/homebrew-tap`:

```bash
brew install pesty-sh/tap/smashbin
```

That installs the tagged `0.1.4` tarball. `--HEAD` still builds the latest commit on `master`. The formula is `Formula/smashbin.rb` in the tap. This repo does not keep a second copy.

The Linux install is the apt repository at `/Users/developers/Projects/pesty-sh/apt`. From that directory, after `make deb`:

```bash
python3 publish.py \
  /Users/developers/Projects/pesty-sh/rust/smashbin/dist/smashbin_0.1.4-1_amd64.deb \
  /Users/developers/Projects/pesty-sh/rust/smashbin/dist/smashbin_0.1.4-1_arm64.deb
```

That copies the packages into `pool/` and signs `dists/stable`. This repo does not keep a second copy of the apt repository.

There is no update checker in the program. It will not grow one.
