#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Stage a self-contained QQL bundle: CLI + libraries + header + the data the
# resolvers actually read. Run after `cargo build --release --features
# vector,fulltext`. Used by .github/workflows/release.yml and testable locally:
#
#     scripts/package.sh qql-v3.0.0-x86_64-linux
#
# The bundle works in place: `cd <name> && ./qql 'Q:1:1'` — the binary's
# default data directory is ./sources.

set -euo pipefail

name="${1:?usage: package.sh <bundle-name>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

out="dist/$name"
rm -rf "$out"
mkdir -p "$out/lib" "$out/include"

# --- binaries and libraries (copy whichever this platform produced) ---------
release=target/release
for bin in qql qql-index qql.exe qql-index.exe; do
    [ -f "$release/$bin" ] && cp "$release/$bin" "$out/"
done
for lib in libqql.so libqql.dylib libqql.a qql.dll qql.dll.lib qql.lib; do
    [ -f "$release/$lib" ] && cp "$release/$lib" "$out/lib/"
done
cp include/qql.h "$out/include/"

# --- data: exactly what the resolvers read, nothing else --------------------
mkdir -p "$out/sources/quran" \
         "$out/sources/hadith-json/db/by_chapter/the_9_books" \
         "$out/sources/hadith-json/db/by_book/the_9_books" \
         "$out/sources/Hisn-Muslim-Json"

cp -r sources/quran/chapters "$out/sources/quran/"
cp sources/quran/TANZIL-LICENSE.txt "$out/sources/quran/"
for book in bukhari muslim abudawud tirmidhi nasai ibnmajah; do
    cp -r "sources/hadith-json/db/by_chapter/the_9_books/$book" \
          "$out/sources/hadith-json/db/by_chapter/the_9_books/"
    cp "sources/hadith-json/db/by_book/the_9_books/$book.json" \
       "$out/sources/hadith-json/db/by_book/the_9_books/"
done
cp sources/Hisn-Muslim-Json/husn_en.json "$out/sources/Hisn-Muslim-Json/"
cp -r sources/vectors "$out/sources/"
cp -r sources/fulltext "$out/sources/"
# Tantivy lock files are runtime artifacts, recreated on demand.
rm -f "$out"/sources/fulltext/*/.tantivy-*.lock

cp LICENSE.md README.md "$out/"

# --- smoke test: the staged bundle must answer with its own data ------------
(
    cd "$out"
    bin=./qql; [ -f qql.exe ] && bin=./qql.exe
    "$bin" --compact 'Q:1:1'           | grep -q '"ok":true'
    "$bin" --compact 'b::100'          | grep -q '"numbering":"book"'
    "$bin" --compact 'q:1:?"mercy"~1'  | grep -q '"ranked":true'
    "$bin" --compact 'q:1:*"worship"'  | grep -q '"ranked":true'
    "$bin" --compact 'hm:1:1'          | grep -q '"ok":true'
)
echo "bundle ok: $out ($(du -sh "$out" | cut -f1))"
