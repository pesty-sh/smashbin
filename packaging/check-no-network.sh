#!/bin/sh
# Fail when a networking crate is in the resolved tree, or when our source
# names the standard networking API. smashbin must not open a socket.
set -eu

cd "$(dirname "$0")/.."

target="${1:-}"
if [ -n "$target" ]; then
    names=$(cargo tree --locked -e normal,build,dev --prefix none --target "$target" | awk '{print $1}' | sort -u)
else
    names=$(cargo tree --locked -e normal,build,dev --prefix none | awk '{print $1}' | sort -u)
fi

banned=$(grep -v '^[[:space:]]*#' packaging/banned-network-crates.txt | grep -v '^[[:space:]]*$' || true)
found=0
for name in $banned; do
    if printf '%s\n' "$names" | grep -qx "$name"; then
        echo "networking crate is not allowed: $name" >&2
        found=1
    fi
done
if [ "$found" -ne 0 ]; then
    exit 1
fi

if grep -RInE --include='*.rs' 'std::net|TcpStream|TcpListener|UdpSocket|ToSocketAddrs' src tests; then
    echo "source names the networking API" >&2
    exit 1
fi

echo "no networking crates and no socket API in src/ or tests/"
