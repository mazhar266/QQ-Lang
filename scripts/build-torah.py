#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Build the Torah (Tawrat) dataset QQL reads — the Jewish edition, `TO`.

The five books of Moses as the Hebrew text numbers them, from eBible.org's
public-domain USFM exports:

    hbo          Hebrew Masoretic text — the authority for verse numbers  -> he
    engjps       JPS 1917, the Jewish English translation                 -> en
    arb-vd       Smith & Van Dyck Arabic (1865)                           -> ar
    eng-kjv2006  King James Version, 1769 text                            -> kjv
    eng-web      World English Bible                                      -> web

The same books sit inside the Bible (`OT:GEN`…`OT:DEU`) in English
numbering. The two disagree in ten places, where a verse or two moves across
a chapter boundary — Hebrew Genesis 32:1 is English 31:55 — and once inside a
verse: English Numbers 26:1 is Hebrew 25:19 and 26:1. The translations
arrive in English numbering and are re-cut:

- **JPS** carries the Hebrew verse numbers inline, "(32-1) And early in the
  morning…", so it is divided exactly there. Its markers give the
  English -> Hebrew map that the other three follow.
- **Van Dyck, KJV and WEB** move whole verses, and Numbers 26:1 is divided
  by an explicit rule, checked word for word.
- **The Ten Commandments** (Exodus 20, Deuteronomy 5) are the exception.
  JPS numbers them by the "lower" accents, joining commandments 6-9 into one
  Hebrew verse; the Hebrew text here divides them as English Bibles do, so in
  those two chapters the English numbers stand and JPS's markers are set aside.

Output shape matches what `src/sources/torah.rs` reads:

    sources/torah/books.json             5 books: code, Hebrew and Arabic
                                         names, verse count of every chapter
    sources/torah/{BOOK}/{chapter}.json  { verse, he, ar, en, kjv, web, english }

Usage
-----
Fetch and unpack the five USFM zips into one directory, then point at it:

    mkdir /tmp/usfm && cd /tmp/usfm
    for t in hbo engjps arb-vd eng-kjv2006 eng-web; do
        curl -sSLO https://ebible.org/Scriptures/${t}_usfm.zip && unzip -o ${t}_usfm.zip
    done
    python3 scripts/build-torah.py --from /tmp/usfm
    python3 scripts/build-torah.py --from /tmp/usfm --check   # verify, write nothing
"""
import argparse
import json
import os
import re
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import usfm  # noqa: E402  (the shared USFM reader, beside this script)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'sources/torah')

TORAH = ['GEN', 'EXO', 'LEV', 'NUM', 'DEU']
HEBREW_NAMES = {'GEN': 'Bereshit', 'EXO': 'Shemot', 'LEV': 'Vayikra', 'NUM': 'Bamidbar',
                'DEU': 'Devarim'}

TEXTS = {
    'he': 'hbo',
    'en': 'engjps',
    'ar': 'arb-vd',
    'kjv': 'eng-kjv2006',
    'web': 'eng-web',
}

# The ten places the chapter boundaries move, as the `(book, chapter)` pairs
# whose Hebrew and English verse counts differ. Numbers 26 keeps its count —
# only half its first verse moves to 25:19 — so the ten places touch 19
# chapters. Anything else is a failed build.
MOVES = {
    ('GEN', 31), ('GEN', 32), ('EXO', 7), ('EXO', 8), ('EXO', 21), ('EXO', 22),
    ('LEV', 5), ('LEV', 6), ('NUM', 16), ('NUM', 17), ('NUM', 25),
    ('NUM', 29), ('NUM', 30), ('DEU', 12), ('DEU', 13), ('DEU', 22), ('DEU', 23),
    ('DEU', 28), ('DEU', 29),
}
# Where JPS's markers follow the lower accents and the Hebrew text does not.
DECALOGUE = {('EXO', 20), ('DEU', 5)}

# English Numbers 26:1 is Hebrew 25:19 ("And it came to pass after the
# plague") and 26:1. The words before the cut, in each translation that cannot
# be divided at a marker; the build checks each against the text.
SPLITS = {
    ('NUM', 26, 1): {
        'ar': 'ثم بعد الوبإ',
        'kjv': 'And it came to pass after the plague,',
        'web': 'After the plague,',
    },
}

# Names that fall in the same Hebrew verses in every text. Matched in the
# Hebrew as whole words with the one-letter prefixes "and", "to", "in", "as",
# "the", "from" — `משה` also sits inside חמשה "five".
ANCHORS = [
    ('Moses', r'[ולבכהמ]{0,2}משה', 'موسى'),
    ('Aaron', r'[ולבכהמ]{0,2}אהרן', 'هارون'),
    ('Pharaoh', r'[ולבכהמ]{0,2}פרעה', 'فرعون'),
    ('Jacob', r'[ולבכהמ]{0,2}יעקב', 'يعقوب'),
]
MIN_AGREEMENT = 0.95

HEBREW_MARKS = set(range(0x0591, 0x05C8)) - {0x05BE}
ARABIC_MARKS = set(range(0x064B, 0x0653)) | {0x0670, 0x0640}
MARKER = re.compile(r'\((\d+)-(\d+)\)')


def strip(text, marks):
    """Drop marks; a maqaf joins Hebrew words, so it becomes a space; alef
    wasla reads as a plain alef."""
    out = []
    for c in text:
        if ord(c) == 0x05BE:
            out.append(' ')
        elif ord(c) == 0x0671:
            out.append('ا')
        elif ord(c) not in marks:
            out.append(c)
    return ''.join(out)


def jps_flow(code, jps):
    """Divide one JPS book at its Hebrew markers.

    -> (segments, targets): segments[h] is the JPS text of Hebrew verse h;
    targets[e] the Hebrew verses English verse e feeds, in order. Text before
    any marker stays at its English number, which is the Hebrew one wherever
    JPS marks nothing.
    """
    segments, targets = {}, {}
    for (chapter, verse), text in sorted(jps.items()):
        if (code, chapter) in DECALOGUE:
            text = MARKER.sub('', text)
        parts = MARKER.split(text)
        fed = []
        pieces = [((chapter, verse), parts[0])]
        pieces += [((int(parts[i]), int(parts[i + 1])), parts[i + 2]) for i in range(1, len(parts), 3)]
        for target, piece in pieces:
            piece = piece.strip()
            if not piece and target == (chapter, verse) and len(pieces) > 1:
                continue  # the empty text ahead of a leading marker
            fed.append(target)
            segments[target] = (segments.get(target, '') + ' ' + piece).strip()
        targets[(chapter, verse)] = list(dict.fromkeys(fed))
    return segments, targets


def recut(key, code, verses, targets):
    """Move an English-numbered text onto the Hebrew verses."""
    out = {}
    for english, fed in sorted(targets.items()):
        text = verses[english]
        if len(fed) == 1:
            out[fed[0]] = (out.get(fed[0], '') + ' ' + text).strip()
            continue
        rule = SPLITS.get((code, *english), {}).get(key)
        if rule is None or len(fed) != 2:
            raise SystemExit(f'{key} {code} {english} feeds {fed} and has no cut rule')
        words = text.split()
        size = len(rule.split())
        head, tail = ' '.join(words[:size]), ' '.join(words[size:])
        if strip(head, ARABIC_MARKS) != rule:
            raise SystemExit(f'{key} {code} {english}: cut is {head!r}, expected {rule!r}')
        out[fed[0]] = (out.get(fed[0], '') + ' ' + head).strip()
        out[fed[1]] = (out.get(fed[1], '') + ' ' + tail).strip()
    return out


def check(data, english, cut):
    problems = []
    hebrew = data['he']
    for code in TORAH:
        chapters = sorted({c for b, c, _ in hebrew if b == code})
        if chapters != list(range(1, len(chapters) + 1)):
            problems.append(f'{code}: chapters are not 1..{len(chapters)}')
        for chapter in chapters:
            numbers = sorted(v for b, c, v in hebrew if b == code and c == chapter)
            if numbers != list(range(1, len(numbers) + 1)):
                problems.append(f'{code} {chapter}: verses are not 1..{len(numbers)}')

    moved = {(b, c) for (b, c), n in cut.items() if n}
    if moved != MOVES:
        problems.append(f'chapters whose counts differ: {sorted(moved ^ MOVES)} unexpected')

    for key, verses in data.items():
        if set(verses) != set(hebrew):
            problems.append(f'{key}: {len(set(hebrew) - set(verses))} Hebrew verses not filled, '
                            f'first {sorted(set(hebrew) - set(verses))[:3]}')
        blank = sorted(h for h, t in verses.items() if not t.strip())
        if blank:
            problems.append(f'{key}: empty verses {blank[:5]}')

    joined = sorted(h for h, refs in english.items() if len(refs) > 1)
    if joined:
        problems.append(f'Hebrew verses joining several English ones: {joined[:5]}')

    agreement = {}
    for en_word, he_word, ar_word in ANCHORS:
        in_he = {h for h, t in hebrew.items()
                 if any(re.fullmatch(he_word, w) for w in strip(t, HEBREW_MARKS).split())}
        for key in ('en', 'ar', 'kjv', 'web'):
            if key == 'ar':
                found = {h for h, t in data[key].items() if ar_word in strip(t, ARABIC_MARKS)}
            else:
                found = {h for h, t in data[key].items() if re.search(rf'\b{en_word}\b', t)}
            union = in_he | found
            agree = len(in_he & found)
            agreement[(key, en_word)] = (agree, len(union))
            if agree < MIN_AGREEMENT * len(union):
                problems.append(f'{key}/{en_word}: agrees with the Hebrew in {agree}/{len(union)} verses')
    return problems, agreement


def build(data, english, names):
    if os.path.isdir(OUT):
        for entry in os.listdir(OUT):
            if entry in TORAH:
                shutil.rmtree(os.path.join(OUT, entry))
    os.makedirs(OUT, exist_ok=True)
    books = []
    for code in TORAH:
        chapters = sorted({c for b, c, _ in data['he'] if b == code})
        counts = []
        os.makedirs(os.path.join(OUT, code), exist_ok=True)
        for chapter in chapters:
            numbers = sorted(v for b, c, v in data['he'] if b == code and c == chapter)
            counts.append(len(numbers))
            with open(os.path.join(OUT, code, f'{chapter}.json'), 'w', encoding='utf-8',
                      newline='\n') as f:
                json.dump({'verses': [
                    {
                        'verse': v,
                        'he': data['he'][(code, chapter, v)],
                        'ar': data['ar'][(code, chapter, v)],
                        'en': data['en'][(code, chapter, v)],
                        'kjv': data['kjv'][(code, chapter, v)],
                        'web': data['web'][(code, chapter, v)],
                        'english': [list(ref) for ref in english[(code, chapter, v)]],
                    }
                    for v in numbers
                ]}, f, ensure_ascii=False, separators=(',', ':'))
        books.append({'code': code, 'name_he': HEBREW_NAMES[code], 'name_ar': names[code],
                      'chapters': counts})
    with open(os.path.join(OUT, 'books.json'), 'w', encoding='utf-8', newline='\n') as f:
        lines = ',\n'.join(json.dumps(book, ensure_ascii=False) for book in books)
        f.write('{"books": [\n' + lines + '\n]}\n')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--from', dest='source', required=True,
                    help='directory holding the unzipped hbo, engjps, arb-vd, '
                         'eng-kjv2006 and eng-web USFM exports')
    ap.add_argument('--check', action='store_true', help='verify only')
    args = ap.parse_args()

    raw, names = {}, {}
    for key, edition in TEXTS.items():
        books = usfm.find_books(args.source, edition)
        raw[key] = {code: usfm.read_book(books[code]).verses for code in TORAH}
        if key == 'ar':
            for code in TORAH:
                with open(books[code], encoding='utf-8-sig') as f:
                    toc = re.search(r'^\\toc2 (.+)$', f.read(), re.M)
                names[code] = toc.group(1).strip() if toc else ''

    data = {key: {} for key in TEXTS}
    english = {}
    cut = {}
    for code in TORAH:
        for (c, v), text in raw['he'][code].items():
            data['he'][(code, c, v)] = text
        for key in ('ar', 'kjv', 'web'):
            if set(raw[key][code]) != set(raw['en'][code]):
                raise SystemExit(f'{key} {code}: English verse references differ from JPS')
        segments, targets = jps_flow(code, raw['en'][code])
        for h, text in segments.items():
            data['en'][(code, *h)] = text
        for key in ('ar', 'kjv', 'web'):
            for h, text in recut(key, code, raw[key][code], targets).items():
                data[key][(code, *h)] = text
        for e, fed in targets.items():
            for h in fed:
                english.setdefault((code, *h), []).append(e)
        hebrew_counts = {}
        for (c, _v) in raw['he'][code]:
            hebrew_counts[c] = hebrew_counts.get(c, 0) + 1
        for (c, _v) in raw['en'][code]:
            hebrew_counts[c] = hebrew_counts.get(c, 0) - 1
        for c, n in hebrew_counts.items():
            cut[(code, c)] = n
    english = {h: sorted(set(refs)) for h, refs in english.items()}

    problems, agreement = check(data, english, cut)
    moved = sum(1 for h, refs in english.items() if refs != [h[1:]])
    print(f'Hebrew verses        : {len(data["he"])}')
    print(f"numbered differently : {moved} verses, in {len(MOVES)} chapters")
    for (key, word), (agree, total) in agreement.items():
        print(f'pairing {key:<3} / {word:<8}: {agree}/{total} verses agree with the Hebrew')
    if problems:
        print('\nFAILED:')
        for p in problems:
            print(f'  {p}')
        return 1
    print('checks passed')

    if args.check:
        return 0
    build(data, english, names)
    print(f'wrote {len(TORAH)} books, {len(data["he"])} verses to sources/torah/')
    return 0


if __name__ == '__main__':
    sys.exit(main())
