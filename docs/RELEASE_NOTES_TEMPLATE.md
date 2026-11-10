# smashbin release notes

smashbin VERSION strips metadata and resizes an image on your own machine. It does not open a network connection, and it has no update check. The binaries are not signed. There is no warranty. The licence is MIT.

The program writes a new file. It leaves the original image where it is, unless you point `--out` at that same path.

## Minimum systems

These are the floors the binaries are built for. CI does not boot every old release.

| File | Runs on |
|---|---|
| `smashbin-vVERSION-x86_64-apple-darwin.tar.gz` | macOS 10.12 or newer, Intel |
| `smashbin-vVERSION-aarch64-apple-darwin.tar.gz` | macOS 11.0 or newer, Apple silicon |
| `smashbin-universal-apple-darwin.tar.gz` | Both of the above, one file. Not an `.app` |
| `smashbin-vVERSION-x86_64-unknown-linux-musl.tar.gz` and `smashbin_VERSION-1_amd64.deb` | Linux amd64. Static binary. Ubuntu 14.04 / Debian 8 or newer |
| `smashbin-vVERSION-aarch64-unknown-linux-musl.tar.gz` and `smashbin_VERSION-1_arm64.deb` | Linux arm64. Same floor |
| `smashbin-vVERSION-armv7-unknown-linux-musleabihf.tar.gz` and `smashbin_VERSION-1_armhf.deb` | Linux armhf (32-bit ARM). Same floor |
| `smashbin-vVERSION-i686-unknown-linux-musl.tar.gz` and `smashbin_VERSION-1_i386.deb` | Linux i386. Same floor |
| `smashbin-vVERSION-x86_64-pc-windows-msvc.zip` | Windows 10 or Windows Server 2016, 64-bit |

The `.deb` files are gzip-compressed so older dpkg can install them. They do not depend on glibc.

The Windows zip is not signed. Windows SmartScreen warns the first time you open `smashbin.exe`.

## Checksums

Download `SHA256SUMS` next to the archives.

```bash
sha256sum -c SHA256SUMS
```

On Windows, `Get-FileHash file -Algorithm SHA256` and compare the digest to the matching line.

Windows on ARM is not in this release. A 64-bit x86 Windows binary does not run on Windows ARM unless the machine emulates it.

The Mac Homebrew formula builds from the source tarball. It does not install these binaries. The apt repository is published separately, from Stuart's Mac.
