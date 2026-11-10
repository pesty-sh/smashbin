#!/bin/sh
# Fail when the build would compile C, or when a new crate name ends in -sys.
# Rust's own standard library still calls the operating system. That is not
# a third-party C library.
#
# linux-raw-sys and windows-sys are already in the dev tree, through tempfile.
# They are Rust bindings. They do not run cc, cmake, or bindgen.
# Do not add another -sys crate.
set -eu

cd "$(dirname "$0")/.."

target="${1:-}"
if [ -n "$target" ]; then
    names=$(cargo tree --locked -e normal,build,dev --prefix none --target "$target" | awk '{print $1}' | sort -u)
else
    names=$(cargo tree --locked -e normal,build,dev --prefix none | awk '{print $1}' | sort -u)
fi

if printf '%s\n' "$names" | grep -Ex 'cc|cmake|bindgen|openssl-sys|libz-sys|onig_sys'; then
    echo "C build crate is not allowed" >&2
    exit 1
fi

# Names already reviewed. A different -sys crate still fails this script.
allowed_sys='^(linux-raw-sys|windows-sys)$'
if printf '%s\n' "$names" | grep -E -- '-sys$' | grep -Ev "$allowed_sys"; then
    echo "a crate name ends in -sys; review it before allowing it" >&2
    exit 1
fi

if find . -name build.rs -not -path './target/*' -not -path './vendor/*' | grep -q .; then
    echo "build.rs is not allowed in smashbin" >&2
    exit 1
fi

if find crates -type f \( -name '*.c' -o -name '*.cc' -o -name '*.cpp' -o -name '*.h' -o -name '*.S' \) | grep -q .; then
    echo "C or C++ source is not allowed" >&2
    exit 1
fi

echo "no C build scripts and no new -sys crates"
