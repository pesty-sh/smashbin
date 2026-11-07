# smashbin

smashbin makes one image smaller on your own machine. It does not open a network connection. The result is one binary. The Rust libraries it uses are compiled into that file. There is no C code, no header to compile, and no second library to install beside it.

Version 0.1.2 strips metadata, resizes when you pass a width or a height, and checks that a PNG, JPG, or GIF can be a GitHub profile picture. A PHP module and a Tunes GUI are later. They are not in this version. The license is MIT. The text is `LICENSE`.

GitHub's rule, from their profile page:

* PNG, JPG, or GIF
* under 1 MB (this program uses 1,000,000 bytes)
* smaller than 3000 by 3000 pixels

The sample logo in `fixtures/github-avatar.jpg` is already inside that rule. It is a 1024 by 1024 JPEG, 82,180 bytes. Running smashbin on it with no size flags does not resize it and does not change its format. The command checks the rule and prints `avatar=ok`.

## What this version will not do

These are later. They are not flags yet.

* a different output format
* padding a wide picture into a square

## The trip one image takes

The program is nine small files. `src/main.rs` calls them in this order and does nothing else.

1. `src/cli.rs` reads what you typed.
2. `src/read.rs` loads the bytes. A missing filename reads stdin.
3. `src/sniff.rs` looks at the first bytes. The filename does not decide the format.
4. `src/strip.rs` removes private notes. GPS, EXIF, comments, and editor data go. The color profile stays, unless you pass `--strip all`.
5. `src/resize.rs` runs when you pass `--width` or `--height`. Leave one of them out and the other side keeps the aspect ratio. The file stays the same format.
6. `src/lossless.rs` writes a PNG again with the pure-Rust `png` crate. A JPEG that was not resized is not decoded here. Decoding and saving again, even at quality 100, would be a second copy and a little more damage. A resized JPEG is already a new file from step 5, at quality 100, unless step 7 replaces it.
7. `src/lossy.rs` runs when you pass `--quality` or `--bytes`. It replaces step 6 with a new JPEG. If `--bytes` is already met, it does not build another JPEG. You cannot pass both flags. PNG and GIF have no quality knob.
8. `src/avatar.rs` checks the GitHub rule on the size after a resize.
9. `src/write.rs` writes the image. The one-line report goes to stderr, so a pipe stays a pure image.

Exit 0 means the written file fits the rule. Exit 2 means the file was written and GitHub would still refuse it. Exit 1 means the file could not be read, or a flag does not apply.

## Try the logo

From this directory, after `make`:

```bash
./target/release/smashbin fixtures/github-avatar.jpg
```

That writes `fixtures/github-avatar_strip_safe.jpg` beside the original and leaves the original alone. The new name is the original stem, then `_`, then the inputs in a fixed order: `strip`, `width`, `height`, `quality`, `bytes`. stderr looks like:

```text
fixtures/github-avatar.jpg  82180 -> 82180  lossless  1024x1024  JPEG  strip=safe  avatar=ok
```

The numbers stay equal on the safe strip because the only metadata in this file is a color profile, and the safe strip keeps it.

Drop the profile:

```bash
./target/release/smashbin --strip all --out /tmp/avatar.jpg fixtures/github-avatar.jpg
```

Force a smaller JPEG. This does change pixels:

```bash
./target/release/smashbin --bytes 40kb --out /tmp/avatar.jpg fixtures/github-avatar.jpg
```

`--quality 80` picks the quality yourself. `--bytes 40kb` searches for the highest quality that lands at or under 40,000 bytes.

Resize, then let a quality override replace that file:

```bash
./target/release/smashbin --width 500 --quality 80 fixtures/github-avatar.jpg
```

That writes `fixtures/github-avatar_strip_safe_width_500_quality_80.jpg`. The height stays 500 because the logo is square. `--height 250` with no width on a 1024 by 512 picture would write a width of 500.

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
brew install --HEAD pesty-sh/tap/smashbin
```

`--HEAD` builds the current git commit. A numbered install (`brew install pesty-sh/tap/smashbin` with no `--HEAD`) waits until version 0.1.2 is tagged on GitHub and the formula has that tarball's checksum. The formula is `Formula/smashbin.rb` in the tap. This repo does not keep a second copy.

The Linux install is the apt repository at `/Users/developers/Projects/pesty-sh/apt`. From that directory, after `make deb`:

```bash
python3 publish.py \
  /Users/developers/Projects/pesty-sh/smashbin/dist/smashbin_0.1.2-1_amd64.deb \
  /Users/developers/Projects/pesty-sh/smashbin/dist/smashbin_0.1.2-1_arm64.deb
```

That copies the packages into `pool/` and signs `dists/stable`. This repo does not keep a second copy of the apt repository.

There is no update checker in the program. It will not grow one.
