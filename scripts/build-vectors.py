#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Build the vector indexes QQL's `*"term"` search reads.

One `.qv` file per source, written to `sources/vectors/{CODE}.qv`. Nothing
here is needed unless the `vector` cargo feature is on.

Embedders
---------
`hashed` (default) needs no model and no asset: text folds to tokens, each
token hashes onto four dimensions with a sign from its own hash, and the sum
is L2-normalized and quantized to int8. The identical function lives in
`src/vector.rs`, so a query embeds the same way at runtime.

That makes it **fuzzy lexical matching, not semantic**. Character trigrams
give it tolerance for diacritics and for Arabic prefixes and suffixes, which
is worth a lot, but it does not know that *charity* and *zakat* are related.

Swapping in real semantic vectors is a build-time change: produce vectors some
other way, write them with a new embedder id, and teach `src/vector.rs` to
embed queries to match. The file format does not care where vectors came from.

Usage
-----
    python3 scripts/build-vectors.py                 # every source, hashed
    python3 scripts/build-vectors.py --source Q      # just the Quran
    python3 scripts/build-vectors.py --dims 512      # fewer collisions, bigger
"""
import argparse
import json
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCES = os.path.join(ROOT, 'sources')
OUT = os.path.join(SOURCES, 'vectors')

MAGIC = b'QQLVEC1\n'
EMBEDDER_HASHED = 1

LANG_AR = 0
LANG_EN = 1

# Must match src/search.rs::fold.
DROP = set(range(0x064B, 0x0653)) | {0x0670, 0x0640} | set(range(0x06D6, 0x06EE))
ALEF = {0x0622: 'ا', 0x0623: 'ا', 0x0625: 'ا', 0x0671: 'ا'}
SWAP = {0x0649: 'ي', 0x0629: 'ه'}


def fold(text):
    out = []
    for ch in text:
        code = ord(ch)
        if code in DROP:
            continue
        if code in ALEF:
            out.append(ALEF[code])
        elif code in SWAP:
            out.append(SWAP[code])
        else:
            out.append(ch.lower())
    return ''.join(out)


def tokens(text):
    """Words plus character trigrams — must match src/vector.rs::tokens."""
    out = []
    word = []
    for ch in fold(text) + ' ':
        if ch.isalnum():
            word.append(ch)
            continue
        if word:
            out.append(''.join(word))
            if len(word) > 3:
                for i in range(len(word) - 2):
                    out.append(''.join(word[i:i + 3]))
            word = []
    return out


def fnv1a(token):
    acc = 0xcbf29ce484222325
    for byte in token.encode('utf-8'):
        acc = ((acc ^ byte) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return acc


def embed(text, dims):
    acc = [0.0] * dims
    for token in tokens(text):
        h = fnv1a(token)
        for _ in range(4):
            acc[h % dims] += -1.0 if h & 0x8000000000000000 else 1.0
            h = ((h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF) ^ (h >> 29)
    return quantize(acc)


def quantize(values):
    norm = sum(v * v for v in values) ** 0.5
    if norm == 0.0:
        return [0] * len(values)
    out = []
    for v in values:
        scaled = round(v / norm * 127.0)
        out.append(max(-127, min(127, int(scaled))))
    return out


# --- corpora ----------------------------------------------------------------
# Each reader yields (primary, number, arabic, english). The numbering must be
# the one QQL resolves with, since hits are looked up as SOURCE:primary:number.

def read_quran():
    base = os.path.join(SOURCES, 'quran/chapters')
    for surah in range(1, 115):
        with open(f'{base}/{surah}.json', encoding='utf-8') as f:
            chapter = json.load(f)
        for verse in chapter['verses']:
            yield surah, verse['id'], verse['text'], verse.get('translation', '')


def read_hadith(directory):
    base = os.path.join(SOURCES, 'hadith', directory)
    if not os.path.isdir(base):
        return
    # Some collections carry an `introduction.json` beside the numbered
    # chapters. QQL addresses chapters by number, so anything unnumbered is
    # unreachable by a query and is left out of the index too.
    chapters = sorted(
        int(name[:-5])
        for name in os.listdir(base)
        if name.endswith('.json') and name[:-5].isdigit()
    )
    for chapter in chapters:
        with open(f'{base}/{chapter}.json', encoding='utf-8') as f:
            payload = json.load(f)
        for hadith in payload['hadiths']:
            english = hadith.get('english') or {}
            yield chapter, hadith['id'], hadith.get('arabic', ''), english.get('text', '')


def read_hisnul():
    path = os.path.join(SOURCES, 'hisnul-muslim/husn_en.json')
    if not os.path.exists(path):
        return
    with open(path, encoding='utf-8-sig') as f:
        book = json.load(f)['English']
    for chapter in book:
        for position, item in enumerate(chapter['TEXT'], start=1):
            arabic = item.get('ARABIC_TEXT') or item.get('Text', '')
            yield chapter['ID'], position, arabic, item.get('TRANSLATED_TEXT', '')


CORPORA = {
    'Q': read_quran,
    'B': lambda: read_hadith('bukhari'),
    'M': lambda: read_hadith('muslim'),
    'AD': lambda: read_hadith('abudawud'),
    'T': lambda: read_hadith('tirmidhi'),
    'N': lambda: read_hadith('nasai'),
    'IM': lambda: read_hadith('ibnmajah'),
    'MA': lambda: read_hadith('malik'),
    'DA': lambda: read_hadith('darimi'),
    'RS': lambda: read_hadith('riyad_assalihin'),
    'BM': lambda: read_hadith('bulugh_almaram'),
    'AM': lambda: read_hadith('aladab_almufrad'),
    'MK': lambda: read_hadith('mishkat_almasabih'),
    'SM': lambda: read_hadith('shamail_muhammadiyah'),
    'NW': lambda: read_hadith('nawawi40'),
    'QD': lambda: read_hadith('qudsi40'),
    'SW': lambda: read_hadith('shahwaliullah40'),
    'HM': read_hisnul,
}


def build(code, dims):
    rows = []
    for primary, number, arabic, english in CORPORA[code]():
        # One vector per language present. A record indexed twice is merged
        # back to one hit at query time, scored by whichever field matched.
        if arabic.strip():
            rows.append((primary, number, LANG_AR, embed(arabic, dims)))
        if english.strip():
            rows.append((primary, number, LANG_EN, embed(english, dims)))

    if not rows:
        return None

    out = bytearray(MAGIC)
    out += struct.pack('<IIII', dims, len(rows), EMBEDDER_HASHED, 0)
    for primary, number, lang, _ in rows:
        out += struct.pack('<III', primary, number, lang)
    for _, _, _, vector in rows:
        out += bytes((v & 0xFF) for v in vector)
    return bytes(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--source', action='append', help='code to build; repeatable')
    ap.add_argument(
        '--dims',
        type=int,
        default=256,
        help='dimensions (default 256; below ~256 hash collisions start '
             'outranking real matches on short queries)',
    )
    args = ap.parse_args()

    if args.dims < 8 or args.dims > 4096:
        print('--dims should be between 8 and 4096', file=sys.stderr)
        return 1

    codes = args.source or list(CORPORA)
    unknown = [c for c in codes if c.upper() not in CORPORA]
    if unknown:
        print(f'unknown source(s): {", ".join(unknown)}', file=sys.stderr)
        return 1

    os.makedirs(OUT, exist_ok=True)
    total = 0
    for code in codes:
        code = code.upper()
        payload = build(code, args.dims)
        if payload is None:
            print(f'{code:>3}: no data, skipped')
            continue
        path = os.path.join(OUT, f'{code}.qv')
        with open(path, 'wb') as f:
            f.write(payload)
        vectors = struct.unpack('<I', payload[12:16])[0]
        total += len(payload)
        print(f'{code:>3}: {vectors:6d} vectors  {len(payload) / 1e6:6.2f} MB  {path}')

    print(f'total {total / 1e6:.2f} MB in {OUT}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
