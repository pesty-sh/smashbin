# How a release is built

This page is the map of [`.github/workflows/release.yml`](../.github/workflows/release.yml). Nothing here runs until somebody pushes a tag. Do not push a tag until you want a public GitHub Release.

GitHub Actions is the robot that runs those steps on GitHub's computers. For a **public** repository, GitHub does not bill the standard runner minutes. This repository is public. A full smashbin release uses about ten jobs. Each one is a few minutes. That is normal for a public project. The minutes are still shared machines, so the workflow does not rebuild the same thing twice.

The version in `Cargo.toml` is the latest release number. Today that is `0.1.5`. The tag must be `v0.1.5` for that commit. CI uses the latest stable Rust, because `rust-toolchain.toml` says `stable`. `Cargo.toml` `rust-version` is the floor, `1.88`.

## Cut a release

On `master`, after the green pull request is merged and the version in `Cargo.toml` is the version you want:

```bash
git tag -a v0.1.5 -m "smashbin 0.1.5"
git push origin v0.1.5
```

The tagger is `stuarteske <sdeske@googlemail.com>`, the same identity as the commit author.

`git tag -a v0.1.5` creates an annotated tag that points at the current commit. `git push origin v0.1.5` sends that one tag. `git push --tags` would send every tag in the clone.

The workflow only starts for tags that look like `v0.1.5` (the pattern is `v*`). It checks that the tag, without the `v`, matches `version` in `Cargo.toml`. `v0.1.5` matches `0.1.5`. A mismatch fails the job before it uploads anything.

Pushing the tag creates the GitHub Release when the jobs succeed. There is no separate "publish" button in this repo.

## What happens, in order

### 1. Tag push

GitHub sees the new tag and starts the Release workflow. Pull requests do not start it.

### 2. Matrix builds

A matrix is one job description repeated with different settings. Each setting is one target:

| Rust target | Runner | Archive |
|---|---|---|
| `x86_64-apple-darwin` | optional Intel workflow, not this matrix | See section 4 |
| `aarch64-apple-darwin` | optional Apple silicon workflow, not this matrix | See section 4 |
| `x86_64-unknown-linux-musl` | Ubuntu x86_64 | `.tar.gz` plus `amd64` `.deb` |
| `aarch64-unknown-linux-musl` | Ubuntu arm64 | `.tar.gz` plus `arm64` `.deb` |
| `armv7-unknown-linux-musleabihf` | Ubuntu x86_64, cross-linked | `.tar.gz` plus `armhf` `.deb` |
| `i686-unknown-linux-musl` | Ubuntu x86_64, cross-linked | `.tar.gz` plus `i386` `.deb` |
| `x86_64-pc-windows-msvc` | Windows | `.zip` |

There is one macOS build per CPU family, not one per macOS version. The Intel binary asks for macOS 10.12. The Apple silicon binary asks for 11.0, which is the oldest system that slice can load. Windows on ARM is not built. The hosted Windows image does not reliably include the ARM64 MSVC libraries, so that target is not a job.

Linux binaries from this workflow are static musl. They do not need the system's glibc, which is why the intended floor is Ubuntu 14.04 and Debian 8. CI itself runs on Ubuntu 24.04. armv7 and i686 are linked with `rust-lld` from the Rust toolchain (`packaging/rust-lld.sh`), not with a C cross compiler.

`make deb` on the Mac is a different build. It still produces glibc 2.27 binaries for amd64 and arm64, it does not use Docker, and it does not install those binaries over the Mac one. `make build-windows` still uses the MinGW linker (`x86_64-pc-windows-gnu`), because that is the linker on the Mac. The GitHub Windows job uses MSVC because that is the linker on the Windows runner.

### 3. `.deb` packages

On each Linux leg, after the binary exists, the job installs `cargo-deb` and runs:

```bash
cargo deb --no-build --compress-type gzip --target <triple>
```

`--no-build` packs the binary just produced. It does not compile a second time. `--compress-type gzip` puts `data.tar.gz` inside the `.deb`. Current cargo-deb defaults to xz. Ubuntu 14.04 and Debian 8 dpkg understand gzip. The package metadata is `[package.metadata.deb]` in `Cargo.toml`: maintainer, section, the files to install, and an empty `depends` so the static binary does not require `libc6`. `make deb` does not read that table. It uses `packaging/mkdeb.py`, which still depends on `libc6 (>= 2.27)`.

Debian names from this workflow:

| Rust target | `.deb` architecture |
|---|---|
| `x86_64-unknown-linux-musl` | amd64 |
| `aarch64-unknown-linux-musl` | arm64 |
| `armv7-unknown-linux-musleabihf` | armhf |
| `i686-unknown-linux-musl` | i386 |

`cargo-deb` is a packaging tool. It is not linked into `smashbin`. The workflow installs version 3.6.0 with `--no-default-features` so that tool does not compile liblzma. Gzip compression uses the Rust gzip code in cargo-deb.

There are no maintainer scripts. Installing the package copies `/usr/bin/smashbin`. Removing it deletes that file only.

### 4. Mac builds, optional

The main Release workflow does not build macOS. Both Mac runner pools were blocking pull requests: the arm64 pool is slow to hand out a machine, and an Intel job that shared the CI concurrency group was cancelled while queued because a newer run was waiting. That cancellation is not a code failure. Rerun the Mac job. Do not change the program to chase a runner error.

Two optional workflows build the Mac binaries. Neither runs on a pull request, so neither is a required check.

| Workflow | When it runs | What it uploads on a tag |
|---|---|---|
| [release-apple-silicon.yml](../.github/workflows/release-apple-silicon.yml) | Actions → Run workflow, or a `v*` tag | `smashbin-vX.Y.Z-aarch64-apple-darwin.tar.gz` and a `.sha256` file |
| [release-macos-intel.yml](../.github/workflows/release-macos-intel.yml) | Actions → Run workflow, or a `v*` tag | `smashbin-vX.Y.Z-x86_64-apple-darwin.tar.gz` and a `.sha256` file |

Each has its own concurrency group and `cancel-in-progress: false`. The main CI cannot cancel them. `gh release upload --clobber` adds the files to the release. A tag push uses that tag. A manual run on `master` uploads to `v` plus the version in `Cargo.toml`, and creates that release if it is missing. A manual run used to keep the tarball only on the runner, so the universal job then failed with "No GitHub Release named v0.1.5". `MACOSX_DEPLOYMENT_TARGET` is `10.12` for Intel and `11.0` for Apple silicon.

### 5. Universal Mac binary, also optional

After both tarballs above are on the release, run [release-macos-universal.yml](../.github/workflows/release-macos-universal.yml) by hand. Type the tag, for example `v0.1.5`. It downloads the two archives onto an Intel Mac runner, runs `lipo -create`, and uploads `smashbin-universal-apple-darwin.tar.gz` plus its `.sha256`. It does not run on a tag by itself, because the two Mac jobs may still be queued. It is a CLI binary, not an `.app`.

The job has no checkout, so it sets `GH_REPO` to `${{ github.repository }}`. If a tarball is missing, the log names that file.

### 6. Checksums

One Ubuntu job downloads every archive and every `.deb` from the main workflow. It writes `SHA256SUMS` with `sha256sum`. That file lists the Linux tarballs, the Windows zip, and the four `.deb` files. The Mac files are uploaded by their own workflows and carry a one-line `.sha256` beside them. `SHA256SUMS` does not list itself.

Check a download with:

```bash
sha256sum -c SHA256SUMS
```

### 7. GitHub Release

The same job creates the GitHub Release for the tag and attaches those files plus `SHA256SUMS`. The release text starts from [RELEASE_NOTES_TEMPLATE.md](RELEASE_NOTES_TEMPLATE.md), with `VERSION` replaced by the Cargo version. The notes state the minimum systems and the SmartScreen warning on the unsigned Windows binary.

If any matrix leg fails, the release is not created. A partial set of binaries is not published. Confirm every release workflow succeeded, run the universal workflow, then check the asset names against the table above.

### 8. Homebrew and apt, by hand

The workflow stops at the GitHub Release. It cannot push to `pesty-sh/homebrew-tap` or `pesty-sh/apt`.

Homebrew builds from source only. There is no bottle and no prebuilt binary. The formula lives only in the tap, at `Formula/smashbin.rb`. Its `url` is the tag source tarball. Compute `sha256` from a real download, then check it a second time. The formula uses `depends_on "rust" => :build` and `system "cargo", "install", *std_cargo_args`. The test checks `--version` and one safe run. Run `brew style` and `brew audit --new` before that pull request. This repo does not keep a second copy of the formula.

apt packages are signed and published by `publish.py` on Stuart's Mac. The GPG private key lives only there and must never leave it. A cloud agent cannot publish to apt. Prepare the `.deb` files, then hand them to Stuart. The packages the apt repository has been publishing are the glibc 2.27 amd64 and arm64 files from `make deb`. The four `.deb` files on the GitHub Release are the static musl builds. Do not mix those two sets in one publish.

## What does not happen

- No tag is created by CI.
- crates.io is not updated unless Stuart asks.
- Binaries are not signed. The Windows zip triggers SmartScreen.
- There is no build per macOS version and no `.app`.
- winget and Scoop are later.
