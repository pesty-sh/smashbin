# smashbin

smashbin makes one image smaller on your own machine. It does not open a network connection. The result is one binary. The Rust libraries it uses are compiled into that file. There is no C code, no header to compile, and no second library to install beside it.

Version 0.1.7 strips metadata, resizes when you pass a width or a height, and cuts one picture into every size in a batch file. A PHP module and a Tunes GUI are later. They are not in this version. The license is MIT. The text is `LICENSE`.

The repository is a Cargo workspace. That is one project with two crates, and they share the `target/` folder.

- `crates/smashbin-core` is the library. It reads, strips, resizes, and writes. It has no `main`, so you cannot run it by itself.
- `crates/smashbin` is the program you type. Its `main` calls the library and returns the exit code.

The command is still `smashbin`. The flags did not change. The command-line types live in the library, because strip, resize, and write use the job those types describe. If the types lived in the program crate, the library would have to depend on the program. Cargo rejects that loop.

The picture of the two crates, and of the ordinary path beside batch, is [docs/tutorial.md](docs/tutorial.md).

The sample logo in `fixtures/logo.jpg` is a 1024 by 1024 JPEG, 82,180 bytes. Running smashbin on it with no size flags does not resize it and does not change its format.

## What this version will not do

Padding a wide picture into a square is later. It is not a flag yet.

## The trip one image takes

The library is a short list of small files. `crates/smashbin-core/src/lib.rs` calls them in this order and does nothing else.

1. `crates/smashbin-core/src/cli.rs` reads what you typed.
2. `crates/smashbin-core/src/read.rs` loads the bytes. A missing filename reads stdin.
3. `crates/smashbin-core/src/sniff.rs` looks at the first bytes. The filename does not decide the format.
4. `crates/smashbin-core/src/strip.rs` removes private notes. GPS, EXIF, comments, and editor data go. The color profile stays, unless you pass `--strip all`.
5. `crates/smashbin-core/src/resize.rs` runs when you pass `--width` or `--height`. Leave one of them out and the other side keeps the aspect ratio. Both sides with a different aspect crop from the centre. `--cv` and `--ch` place the slice. The file stays the same format.
6. `crates/smashbin-core/src/lossless.rs` writes a PNG again with the pure-Rust `png` crate. A JPEG that was not resized is not decoded here. Decoding and saving again, even at quality 100, would be a second copy and a little more damage. A resized JPEG is already a new file from step 5, at quality 100, unless step 7 replaces it.
7. `crates/smashbin-core/src/lossy.rs` runs when you pass `--quality` or `--bytes`. It replaces step 6 with a new JPEG. If `--bytes` is already met, it does not build another JPEG. You cannot pass both flags. PNG and GIF have no quality knob.
8. `crates/smashbin-core/src/convert.rs` runs when you pass `--format` / `-f` and the new container is different. The suffix of the generated name changes. WebP is lossless. A GIF animation becomes its first frame. An ICO must be 256 by 256 or smaller. JPEG output with no `--quality` is quality 100.
9. `crates/smashbin-core/src/density.rs` runs when you pass `--dpi`. JPEG and PNG store that many pixels per inch. WebP stores it in an EXIF block. A container with no density field is still written, and stderr names the original image's density. The pixel grid stays the same.
10. `crates/smashbin-core/src/write.rs` writes the image. The one-line report goes to stderr, so a pipe stays a pure image.

`crates/smashbin-core/src/lib.rs` calls those steps. `crates/smashbin/src/main.rs` calls `lib.rs` and nothing else.

Exit 0 means the image was written. Exit 1 means the file could not be read, or a flag does not apply.

## Try the logo

From this directory, after `make`:

```bash
./target/release/smashbin fixtures/logo.jpg
```

That writes `fixtures/logo_strip_safe.jpg` beside the original and leaves the original alone. The new name is the original stem, then `_`, then the inputs in a fixed order: `strip`, `width`, `height`, `cv`, `ch`, `dpi`, `quality`, `bytes`. stderr looks like:

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

`--out ./smashed` writes into a `smashed` folder next to the input file. `.\smashed` is the same on Windows. A path without that `./` prefix stays relative to the working directory.

Resize, then let a quality override replace that file:

```bash
./target/release/smashbin --width 500 --quality 80 fixtures/logo.jpg
```

That writes `fixtures/logo_strip_safe_width_500_quality_80.jpg`. The height stays 500 because the logo is square. `--height 250` with no width on a 1024 by 512 picture would write a width of 500.

On a 1024 by 1024 picture, `-w 1024 -h 256` crops a 1024 by 256 slice from the vertical centre. It does not stretch. `--cv top`, `--cv middle`, and `--cv bottom` pick the slice, and so do `--cv 0` (bottom) through `--cv 100` (top). `--ch` is the same left to right: `left` / `0`, `middle` / `50`, `right` / `100`. The slice never leaves the picture. `-c` is the smasher file, so these flags are `--cv` and `--ch`.

`--dpi 300` stores 300 pixels per inch. A lower number claims a larger print. A higher number claims a smaller one. The pixels do not move. A GIF cannot store this.

```bash
./target/release/smashbin --dpi 300 fixtures/logo.jpg
```

`--format png` writes a PNG next to the JPEG. `-f webp` writes a lossless WebP. The other pure-Rust containers are `gif`, `bmp`, `tif`, `tga`, `ico`, `qoi`, `ppm`, `hdr`, and `ff`.

```bash
./target/release/smashbin -f png fixtures/logo.jpg
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

## Batch

`smashbin batch` is the gallery command. It reads the image once, decodes it once, and writes every size in a job file. The ordinary command above still reads the file again for each job in a `smasher.json`. Leaving `gallery.yaml` beside a photo does not turn that photo into a batch. `name` and `fit` are batch fields, and a smasher file rejects them.

```bash
./target/release/smashbin batch --jobs fixtures/gallery.yaml --out ./variants fixtures/logo.jpg
```

`--verbose` prints the version, the library name, the pixel cap, the CPU count, and the clock split into read, decode, encode, and write. It also says the PHP module and the GUI are later. Those lines are the facts a later program can rely on. The last line is still `N files in` and the seconds.

`-f` / `--format` is the same list as the ordinary command: jpg, png, gif, webp, bmp, tif, tga, ico, qoi, ppm, hdr, or ff. It overrides the container for every cut that does not set `format`. A job's `format` still wins over the flag. With neither, the cut matches the input. A PNG stays a PNG, a JPEG stays a JPEG, and a GIF stays a GIF. The file name's suffix changes to that container. JPEG uses `quality`. WebP is lossless. The other containers do not use `quality`.

```bash
./target/release/smashbin batch --format webp --jobs fixtures/gallery.yaml --out ./variants fixtures/logo.jpg
```

`fixtures/gallery.yaml` and `fixtures/gallery.json` are the same 23 cuts. Six icons and four headers use `fit: cover`, so the file is exactly that wide and that tall, cropped from the centre. Six previews and seven srcset sizes use `fit: contain`, so the whole picture stays inside the box. One side matches the box. The other side is equal or shorter. The file is not padded.

stderr ends with the clock for the whole run: `23 files in` and the seconds. That number is the report. PHP and GD take 50–75 seconds for the template cuts on a 25 MB JPEG, inside a worker capped at 10% of one core and 2 MB/s of reads. The caps are written in [docs/bench.md](docs/bench.md). This command times all 23 files, templates plus srcset. Write the seconds down after you run your file. A phone JPEG is turned upright before the cuts, and the new files do not keep the orientation tag. Each cut is checked before it is written. JPEG, PNG, GIF, and WebP must be picture data. EXIF, XMP, a comment, a color profile, and any bytes after a JPEG's end marker are refused. A file over 64 MB is refused while it is read. A side over 10 000 pixels, or a picture over 100 million pixels, is refused before decode. One cut is at most 8 million pixels, and one batch file holds at most 64 cuts. A cut name is one file name in `--out`. The suffix follows the container: the input format, `--format`, or the job's `format`. A symlink at that name is replaced, and the link target is left alone.

PHP can call the same command with `proc_open` until an extension exists.

## Install

### Homebrew

The formula lives in [pesty-sh/homebrew-tap](https://github.com/pesty-sh/homebrew-tap). Trust is stored on the Mac. Run the tap and trust commands once.

```bash
brew tap pesty-sh/tap
brew trust --tap pesty-sh/tap
HOMEBREW_NO_AUTO_UPDATE=1 brew install pesty-sh/tap/smashbin
smashbin --version
```

The formula in the tap still installs the tagged `0.1.5` tarball. This checkout is 0.1.7. The tap stays on 0.1.5 until that formula is pointed at a new tag. The first install compiles with Homebrew's Rust and needs a network connection. `--HEAD` still builds the latest commit on `master`. `HOMEBREW_NO_AUTO_UPDATE=1` stops Homebrew from fetching every tap before this one formula.

### apt

The package lives in [pesty-sh/apt](https://github.com/pesty-sh/apt) and is served from `https://pesty-sh.github.io/apt`. Ubuntu 18.04 and newer, and Debian 10 and newer, can install it. Apt picks `amd64` or `arm64`. Run the key and source-list commands once.

The signing-key fingerprint is `2564 D17C 2F8B EC6A 9E59 6BB0 2B0D 9ACE 648D D6FF`.

```bash
sudo mkdir -p /usr/share/keyrings
curl -fsSL https://pesty-sh.github.io/apt/pesty-archive-keyring.gpg \
  | sudo tee /usr/share/keyrings/pesty-archive-keyring.gpg >/dev/null
```

```bash
echo "deb [signed-by=/usr/share/keyrings/pesty-archive-keyring.gpg] https://pesty-sh.github.io/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/pesty.list
```

```bash
sudo apt-get update
sudo apt-get install smashbin
smashbin --version
```

`stable` is the channel. `main` is the section. Later upgrades are `sudo apt-get update` then `sudo apt-get install smashbin`.

A tagged GitHub Release is built by the workflows in `.github/workflows/`. The steps are in `docs/RELEASING.md`. The Windows zip is not signed, so Windows SmartScreen warns the first time you open `smashbin.exe`. winget and Scoop are later.

There is no update checker in the program. It will not grow one.

## Build

Install [rustup](https://rustup.rs) so `~/.cargo/bin/cargo` and `~/.cargo/bin/rustc` exist. `Cargo.toml` asks for Rust 1.88. `make` calls that rustup cargo by path. It does not copy a binary onto `PATH`. `make doctor` prints the cargo, rustc, Windows linker, and Linux glibc floor the Makefile will use.

| Command | What it writes | Linker | Installs? |
|---|---|---|---|
| `make` | `target/release/smashbin` | the host Apple linker, stamped macOS 10.12 on Intel | no |
| `make build-silicon` | `target/aarch64-apple-darwin/release/smashbin` | the Apple linker, stamped macOS 11.0 | no |
| `make build-windows` | `target/x86_64-pc-windows-gnu/release/smashbin.exe` | `x86_64-w64-mingw32-gcc` from mingw-w64 | no |
| `make deb` | `dist/smashbin-linux-amd64`, `dist/smashbin-linux-arm64`, and a `.deb` for each | Zig through `cargo-zigbuild`, glibc 2.27 | no |

Host macOS needs Xcode's command-line tools so `rustc` can invoke `ld`. Apple silicon from an Intel host needs `rustup target add aarch64-apple-darwin`.

Windows is the `x86_64-pc-windows-gnu` target. Install the MinGW linker once (`mingw-w64` on macOS Homebrew, or the matching `gcc-mingw-w64` package on Linux), then:

```bash
rustup target add x86_64-pc-windows-gnu
make build-windows
```

Linux does not use Docker. `make deb` installs `cargo-zigbuild` if it is missing and may download Zig into `~/.cache/smashbin`. The glibc floor is 2.27 (Ubuntu 18.04). That binary also runs where glibc is newer: Ubuntu 20.04, 22.04, and 24.04, Debian 10 through 13, and Arch. Intel and AMD are one build. 64-bit ARM is the other. Those files stay in `dist/` and are not copied over the host binary.
