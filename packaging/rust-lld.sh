#!/bin/sh
# Point Cargo at the lld that ships inside the Rust toolchain.
#
# Cross builds of armv7 and i686 musl must not call the host `cc`/`ld`.
# That linker only understands the host architecture, so it rejects the
# other architecture's object files. `rust-lld` is the linker Rust already
# installed. It does not compile C, and it is not a project dependency.
set -eu

sysroot=$(rustc --print sysroot)
host=$(rustc -vV | awk '/^host: / { print $2 }')
exec "$sysroot/lib/rustlib/$host/bin/rust-lld" "$@"
