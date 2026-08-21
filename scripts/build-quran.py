#!/usr/bin/env python3
"""Build the Quran dataset QQL reads, from Tanzil's Uthmani text.

Why this exists
---------------
The `quran-json-arabic` submodule is a fine package, but its Arabic spells
three marks with codepoints that mean something else in Unicode:

    U+0657 INVERTED DAMMA      used as an open fathatan
    U+065E FATHA WITH TWO DOTS used as an open dammatan
    U+0656 SUBSCRIPT ALEF      used as an open kasratan

A font that follows Unicode draws them literally, so 2:286's `إِصۡرٗا` gets a
damma above the reh and reads *isru* rather than *isran*. It also omits
several pause and silence marks.

Tanzil's Uthmani text uses only standard codepoints and carries the full mark
set, so the Arabic comes from there. Everything else — surah names, the
English translation, the per-ayah transliteration — still comes from the
submodule, which is good at those.

Output shape matches what `src/sources/quran.rs` reads:

    sources/quran/chapters/{surah}.json
    sources/quran/verses/{n}.json          (mushaf order, 1..=6236)

Usage
-----
    python3 scripts/build-quran.py                  # fetch Tanzil, then build
    python3 scripts/build-quran.py --tanzil FILE    # use a local copy
    python3 scripts/build-quran.py --check          # verify, write nothing
"""
import argparse
import json
import os
import re
import sys
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SUBMODULE = os.path.join(ROOT, 'sources/quran-json-arabic/dist/chapters/en')
OUT = os.path.join(ROOT, 'sources/quran')

# Uthmani, with the pause, sajdah and rub-el-hizb marks kept.
TANZIL_URL = (
    'https://tanzil.net/pub/download/index.php'
    '?quranType=uthmani&outType=txt-2&agree=true'
    '&marks=true&sajdah=true&rub=true'
)

SURAH_COUNT = 114
AYAH_COUNT = 6236

# The codepoints the submodule misuses. None may appear in the output.
BAD_MARKS = {0x0656, 0x0657, 0x065E}


def fetch_tanzil(path=None):
    if path:
        with open(path, encoding='utf-8') as f:
            return f.read()
    with urllib.request.urlopen(TANZIL_URL, timeout=120) as r:
        return r.read().decode('utf-8')


def _is_mark(cp):
    return (0x0610 <= cp <= 0x061A or 0x064B <= cp <= 0x065F
            or cp == 0x0670 or 0x06D6 <= cp <= 0x06ED or cp == 0x0640)


def parse_tanzil(raw):
    """`sura|aya|text` lines into {(surah, ayah): text}, plus the licence."""
    verses, notice = {}, []
    for line in raw.splitlines():
        line = line.strip('﻿').rstrip()
        if not line:
            continue
        if line.startswith('#'):
            notice.append(line)
            continue
        surah, ayah, text = line.split('|', 2)
        verses[(int(surah), int(ayah))] = text.strip()

    # Tanzil prints the basmalah at the head of ayah 1 of every surah but the
    # ninth. It is not counted as part of that ayah — only Al-Fatihah's first
    # ayah *is* the basmalah — and the submodule leaves it out, so strip it to
    # keep ayah 1 meaning the same thing it did before.
    #
    # Matched on the mark-stripped skeleton rather than literally: surahs 95
    # and 97 write the ba with a shadda (بِّسْمِ), and any other mark variant
    # would slip past an exact comparison and leave the basmalah embedded.
    def bare(text):
        return ''.join(c for c in text if not _is_mark(ord(c)))

    basmalah_words = verses[(1, 1)].split()
    basmalah_bare = [bare(w) for w in basmalah_words]
    stripped = 0
    for surah in range(2, SURAH_COUNT + 1):
        words = verses[(surah, 1)].split()
        head = [bare(w) for w in words[:len(basmalah_words)]]
        if head != basmalah_bare:
            continue
        rest = ' '.join(words[len(basmalah_words):]).strip()
        if not rest:
            raise SystemExit(f'Surah {surah}: ayah 1 is only the basmalah')
        verses[(surah, 1)] = rest
        stripped += 1
    # Surahs 2..114 carry it, except the ninth: 112 in all. Any other number
    # means the match drifted and ayat are being truncated or left alone.
    if stripped != 112:
        raise SystemExit(
            f'stripped the basmalah from {stripped} surahs, expected 112')

    return verses, '\n'.join(notice)


def load_submodule():
    """{surah: chapter json} from quran-json-arabic."""
    chapters = {}
    for surah in range(1, SURAH_COUNT + 1):
        with open(f'{SUBMODULE}/{surah}.json', encoding='utf-8') as f:
            chapters[surah] = json.load(f)
    return chapters


def check(verses, chapters):
    """Fail loudly rather than write a subtly wrong mushaf."""
    problems = []
    if len(verses) != AYAH_COUNT:
        problems.append(f'expected {AYAH_COUNT} ayat, Tanzil gave {len(verses)}')

    for surah, chapter in chapters.items():
        expected = chapter['total_verses']
        got = sum(1 for (s, _) in verses if s == surah)
        if got != expected:
            problems.append(f'Surah {surah}: {got} ayat, submodule says {expected}')

    for key, text in verses.items():
        bad = {c for c in text if ord(c) in BAD_MARKS}
        if bad:
            problems.append(f'{key}: Tanzil text carries {bad!r}')

    # The submodule keeps the basmalah out of ayah 1; Tanzil should too, or
    # every surah after the first would gain words.
    if verses[(2, 1)].startswith('بِسْمِ'):
        problems.append('Tanzil prepends the basmalah to 2:1')

    # Same words, different marks: compare consonant skeletons.
    def skeleton(text):
        text = ''.join(
            c for c in text
            if not (0x0610 <= ord(c) <= 0x061A or 0x064B <= ord(c) <= 0x065F
                    or ord(c) == 0x0670 or 0x06D6 <= ord(c) <= 0x06ED
                    or ord(c) == 0x0640))
        text = re.sub('[آأإاٱ]', '', text).replace('ى', 'ي')
        return re.sub(r'\s+', '', text)

    drifted = 0
    for surah, chapter in chapters.items():
        for verse in chapter['verses']:
            key = (surah, verse['id'])
            if skeleton(verses[key]) != skeleton(verse['text']):
                drifted += 1
    # Hamza spelling differs by convention (ءا vs آ) in a few hundred ayat;
    # anything beyond that means the two texts are not the same mushaf.
    if drifted > 400:
        problems.append(f'{drifted} ayat differ beyond mark spelling')
    return problems, drifted


def build(verses, chapters, notice):
    os.makedirs(f'{OUT}/chapters', exist_ok=True)
    os.makedirs(f'{OUT}/verses', exist_ok=True)

    flat = 0
    for surah in range(1, SURAH_COUNT + 1):
        source = chapters[surah]
        out_verses = []
        for verse in source['verses']:
            ayah = verse['id']
            out_verses.append({
                'id': ayah,
                'text': verses[(surah, ayah)],
                'translation': verse['translation'],
                'transliteration': verse['transliteration'],
            })

            flat += 1
            with open(f'{OUT}/verses/{flat}.json', 'w', encoding='utf-8') as f:
                json.dump({
                    'number': ayah,
                    'text': verses[(surah, ayah)],
                    # Only `en`: it is the one QQL reads, and carrying the
                    # other nine languages made this directory 27 MB.
                    'translations': {'en': verse['translation']},
                    'chapter': {
                        'id': surah,
                        'name': source['name'],
                        'transliteration': source['transliteration'],
                    },
                }, f, ensure_ascii=False, separators=(',', ':'))

        with open(f'{OUT}/chapters/{surah}.json', 'w', encoding='utf-8') as f:
            json.dump({
                'id': surah,
                'name': source['name'],
                'transliteration': source['transliteration'],
                'translation': source['translation'],
                'type': source['type'],
                'total_verses': source['total_verses'],
                'verses': out_verses,
            }, f, ensure_ascii=False, separators=(',', ':'))

    with open(f'{OUT}/TANZIL-LICENSE.txt', 'w', encoding='utf-8') as f:
        f.write(notice + '\n')
    return flat


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--tanzil', help='local copy of the Tanzil txt-2 export')
    ap.add_argument('--check', action='store_true', help='verify only')
    args = ap.parse_args()

    verses, notice = parse_tanzil(fetch_tanzil(args.tanzil))
    chapters = load_submodule()
    problems, drifted = check(verses, chapters)

    print(f'Tanzil ayat        : {len(verses)}')
    print(f'submodule chapters : {len(chapters)}')
    print(f'hamza-spelling only: {drifted} ayat differ in skeleton')
    if problems:
        print('\nFAILED:')
        for p in problems:
            print(f'  {p}')
        return 1
    print('checks passed')

    if args.check:
        return 0
    written = build(verses, chapters, notice)
    print(f'wrote {written} verses + {SURAH_COUNT} chapters to sources/quran/')
    return 0


if __name__ == '__main__':
    sys.exit(main())
