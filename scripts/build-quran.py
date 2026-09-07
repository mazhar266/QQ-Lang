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

Each verse also carries the simplified **Emlaei** spelling, read from the
committed `sources/hafs_smart_v8.json`. Uthmani and Emlaei differ in the
consonantal skeleton, not only in marks (`ٱلصَّلَوٰةَ` / `الصلاة`), so a search typed
in modern orthography cannot reach the mushaf text however it is folded.
Carrying both keeps `text` exact and lets search answer.

Output shape matches what `src/sources/quran.rs` reads:

    sources/quran/chapters/{surah}.json

Usage
-----
    python3 scripts/build-quran.py --meta DIR        # fetch Tanzil, then build
    python3 scripts/build-quran.py --meta DIR --tanzil FILE
    python3 scripts/build-quran.py --meta DIR --check   # verify, write nothing

`--meta` is a checkout of quran-json-arabic's `dist/chapters/en`, which supplies
the Surah names, translation and transliteration. It is only needed to rebuild,
so it is not a submodule — grab it when you need it:

    git clone --depth 1 https://github.com/asim/quran-json-arabic /tmp/qja
    python3 scripts/build-quran.py --meta /tmp/qja/dist/chapters/en
"""
import argparse
import json
import os
import re
import sys
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'sources/quran')

# Uthmani, with the pause, sajdah and rub-el-hizb marks kept.
TANZIL_URL = (
    'https://tanzil.net/pub/download/index.php'
    '?quranType=uthmani&outType=txt-2&agree=true'
    '&marks=true&sajdah=true&rub=true'
)

SURAH_COUNT = 114
AYAH_COUNT = 6236

# Simplified (Emlaei) spelling, one entry per ayah. Committed rather than
# fetched: it is small, and pairing scripture with the wrong ayah is exactly
# the kind of thing a network fetch should not be able to change silently.
EMLAEI = os.path.join(ROOT, 'sources/hafs_smart_v8.json')

# Uthmani and Emlaei disagree only about alef, waw and ya, so a skeleton with
# those removed is nearly identical for a correctly paired ayah. Measured:
# 1.3% differ (ta maftuha vs marbuta, mostly), against 100% for an off-by-one
# pairing. 5% is a wide margin that still catches any real misalignment.
MAX_EMLAEI_DRIFT = 0.05

# The codepoints quran-json-arabic misuses. None may appear in the output.
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


def load_emlaei():
    """{(surah, ayah): emlaei text} from the committed hafs_smart_v8 export."""
    with open(EMLAEI, encoding='utf-8') as f:
        rows = json.load(f)
    return {(r['sura_no'], r['aya_no']): r['aya_text_emlaey'].strip() for r in rows}


def _emlaei_skeleton(text):
    """Consonants only, with the letters the two rasms disagree about removed.

    Also drops bidi marks, which the Emlaei export sprinkles into a few ayat,
    and folds ta maftuha onto ta marbuta — Uthmani writes `نِعْمَتَ` where modern
    spelling writes `نعمة`.
    """
    text = ''.join(
        c for c in text
        if not (_is_mark(ord(c)) or ord(c) in (0x200C, 0x200D, 0x200E, 0x200F, 0x061C)))
    text = text.replace('ة', 'ه').replace('ت', 'ه')
    return re.sub(r'\s+', '', ''.join(c for c in text if c not in 'اآأإٱوؤيئىء'))


def load_metadata(meta_dir):
    """{surah: chapter json} from a quran-json-arabic dist/chapters/en."""
    chapters = {}
    for surah in range(1, SURAH_COUNT + 1):
        with open(f'{meta_dir}/{surah}.json', encoding='utf-8') as f:
            chapters[surah] = json.load(f)
    return chapters


def check(verses, chapters, emlaei):
    """Fail loudly rather than write a subtly wrong mushaf."""
    problems = []
    if len(verses) != AYAH_COUNT:
        problems.append(f'expected {AYAH_COUNT} ayat, Tanzil gave {len(verses)}')

    # The Emlaei export must address exactly the same ayat, or a verse would
    # be paired with a neighbour's spelling and search would answer with the
    # wrong ayah.
    if len(emlaei) != AYAH_COUNT:
        problems.append(f'expected {AYAH_COUNT} Emlaei ayat, got {len(emlaei)}')
    missing = set(verses) - set(emlaei)
    if missing:
        problems.append(f'{len(missing)} ayat have no Emlaei spelling, e.g. {sorted(missing)[:3]}')
    blank = [k for k, v in emlaei.items() if not v]
    if blank:
        problems.append(f'{len(blank)} Emlaei ayat are empty, e.g. {blank[:3]}')

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

    # Alignment, not orthography: a shifted pairing shows up as ~100% drift.
    shared = set(verses) & set(emlaei)
    off = sum(1 for k in shared
              if _emlaei_skeleton(verses[k]) != _emlaei_skeleton(emlaei[k]))
    if shared and off / len(shared) > MAX_EMLAEI_DRIFT:
        problems.append(
            f'{off} of {len(shared)} ayat disagree with their Emlaei spelling '
            f'({100 * off / len(shared):.1f}%) — the pairing looks shifted')
    return problems, drifted, off


def build(verses, chapters, emlaei, notice):
    os.makedirs(f'{OUT}/chapters', exist_ok=True)
    written = 0

    for surah in range(1, SURAH_COUNT + 1):
        source = chapters[surah]
        out_verses = []
        for verse in source['verses']:
            ayah = verse['id']
            out_verses.append({
                'id': ayah,
                'text': verses[(surah, ayah)],
                'emlaei': emlaei[(surah, ayah)],
                'translation': verse['translation'],
                'transliteration': verse['transliteration'],
            })
            written += 1

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
    return written


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--tanzil', help='local copy of the Tanzil txt-2 export')
    ap.add_argument(
        '--meta',
        required=True,
        help="quran-json-arabic dist/chapters/en, for names and translation",
    )
    ap.add_argument('--check', action='store_true', help='verify only')
    args = ap.parse_args()

    verses, notice = parse_tanzil(fetch_tanzil(args.tanzil))
    chapters = load_metadata(args.meta)
    emlaei = load_emlaei()
    problems, drifted, off = check(verses, chapters, emlaei)

    print(f'Tanzil ayat        : {len(verses)}')
    print(f'metadata chapters : {len(chapters)}')
    print(f'Emlaei ayat        : {len(emlaei)}')
    print(f'hamza-spelling only: {drifted} ayat differ in skeleton')
    print(f'Emlaei pairing     : {off} ayat differ in rasm skeleton '
          f'({100 * off / max(len(verses), 1):.2f}%, limit '
          f'{100 * MAX_EMLAEI_DRIFT:.0f}%)')
    if problems:
        print('\nFAILED:')
        for p in problems:
            print(f'  {p}')
        return 1
    print('checks passed')

    if args.check:
        return 0
    written = build(verses, chapters, emlaei, notice)
    print(f'wrote {SURAH_COUNT} chapters covering {written} ayat to sources/quran/')
    return 0


if __name__ == '__main__':
    sys.exit(main())
