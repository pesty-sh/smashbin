#!/bin/sh
# Fail when a Mach-O binary asks for a newer macOS than the build allows.
# Usage: packaging/check-macos-minos.sh BINARY OLDEST_ALLOWED
set -eu

bin=$1
oldest=$2

if [ ! -x "$bin" ]; then
    echo "missing binary: $bin" >&2
    exit 1
fi

stamp=$(mktemp)
trap 'rm -f "$stamp"' EXIT
vtool -show-build "$bin" >"$stamp"

got=$(awk '
    /minos / { print $2; found = 1; exit }
    /version / { print $2; found = 1; exit }
    END { if (!found) exit 1 }
' "$stamp") || {
    echo "vtool did not report a macOS version for $bin" >&2
    exit 1
}

echo "$bin requires macOS $got. This build allows $oldest or older."

awk -v got="$got" -v oldest="$oldest" 'BEGIN {
    split(got, g, ".")
    split(oldest, o, ".")
    for (i = 1; i <= 3; i++) {
        gv = g[i] + 0
        ov = o[i] + 0
        if (gv > ov) exit 1
        if (gv < ov) exit 0
    }
    exit 0
}' || {
    echo "Refusing this binary. It needs macOS $got, which is newer than $oldest." >&2
    echo "A 2014 Intel Mac can run 10.12. Homebrew's rustc stamps 13.0. make must call ~/.cargo/bin/cargo." >&2
    exit 1
}
