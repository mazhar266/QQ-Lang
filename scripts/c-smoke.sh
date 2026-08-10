#!/usr/bin/env bash
#
# Compile examples/c/basic.c against include/qql.h and link it to the real
# library. This is what keeps the hand-written header honest: a signature that
# drifts from src/ffi.rs fails to compile or link here.
#
# Runs under ASan/LSan when available, which checks the CString::into_raw /
# qql_free_string pairing from the C side.
#
# Usage: scripts/c-smoke.sh [QUERY]

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

query="${1:-Q:2:255;B:1:1;}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT

echo "==> cargo build --release"
cargo build --release

# Static linking avoids LD_LIBRARY_PATH and proves the staticlib works too.
# libqql.a needs the usual Rust runtime dependencies.
echo "==> cc (static)"
cc examples/c/basic.c \
    -Iinclude \
    -Wall -Wextra -Werror \
    target/release/libqql.a \
    -lpthread -ldl -lm \
    -o "$out/basic-static"

echo "==> cc (shared, ASan)"
if cc -fsanitize=address -xc /dev/null -o /dev/null 2>/dev/null; then
    cc examples/c/basic.c \
        -Iinclude \
        -Wall -Wextra -Werror \
        -fsanitize=address \
        -L target/release -lqql \
        -o "$out/basic-shared"
    sanitized=1
else
    echo "    (ASan unavailable, skipping)"
    sanitized=0
fi

echo "==> run static"
"$out/basic-static" "$query"

if [ "$sanitized" = 1 ]; then
    echo "==> run shared under ASan"
    LD_LIBRARY_PATH="$root/target/release" \
        ASAN_OPTIONS=detect_leaks=1 \
        "$out/basic-shared" "$query"
fi

echo "==> ok"
