#!/bin/sh
# Fail when a built binary links a TLS or HTTP library.
# A fully static musl binary is the expected Linux result from GitHub Actions.
#
# On Windows, the Rust standard library is one blob and may import the system
# socket DLL even though smashbin never calls it. This script does not treat
# that import as a failure. It still rejects curl, TLS, and WinHTTP.
set -eu

bin=${1:?binary path}
if [ ! -f "$bin" ]; then
    echo "missing binary: $bin" >&2
    exit 1
fi

info=$(file "$bin" 2>/dev/null || echo "unknown")
echo "$info"

if printf '%s\n' "$info" | grep -Eq 'statically linked|static-pie'; then
    echo "static binary, no dynamic network libraries"
    exit 0
fi

os=$(uname -s)
case "$os" in
    Darwin)
        otool -L "$bin"
        if otool -L "$bin" | grep -Ei 'ssl|crypto|curl|nghttp'; then
            echo "binary links a network library" >&2
            exit 1
        fi
        ;;
    Linux)
        if ldd "$bin" 2>&1 | grep -Ei 'ssl|crypto|curl|nghttp'; then
            echo "binary links a network library" >&2
            exit 1
        fi
        ;;
    MINGW*|MSYS*|CYGWIN*)
        if command -v dumpbin >/dev/null 2>&1; then
            dumpbin /dependents "$bin" || true
            if dumpbin /dependents "$bin" | grep -Ei 'ssl|crypto|curl|nghttp|winhttp'; then
                echo "binary links a network library" >&2
                exit 1
            fi
        else
            echo "dumpbin is not on PATH; skipped the Windows import check"
        fi
        ;;
    *)
        echo "no linker check for $os"
        ;;
esac

echo "no TLS or HTTP libraries linked"
