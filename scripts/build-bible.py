#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Build the Bible dataset QQL reads — the `BI`, `OT` and `NT` sources.

The 66 books of the Protestant canon, in English verse numbering, from
eBible.org's public-domain USFM exports:

    arb-vd       Smith & Van Dyck Arabic (1865)   -> ar
    eng-kjv2006  King James Version, 1769 text    -> kjv
    eng-web      World English Bible              -> web

A psalm's title opens its verse 1, as English Bibles number them. The Jewish
Torah and Psalms, in Hebrew numbering, are separate sources built by
`build-torah.py` and `build-psalms.py`.

Output shape matches what `src/sources/bible.rs` reads:

    sources/bible/books.json             66 books: code, testament, Arabic
                                         name, verse count of every chapter
    sources/bible/{BOOK}/{chapter}.json  { verse, ar, kjv, web }

Usage
-----
Fetch and unpack the three USFM zips into one directory, then point at it:

    mkdir /tmp/usfm && cd /tmp/usfm
    for t in arb-vd eng-kjv2006 eng-web; do
        curl -sSLO https://ebible.org/Scriptures/${t}_usfm.zip && unzip -o ${t}_usfm.zip
    done
    python3 scripts/build-bible.py --from /tmp/usfm
    python3 scripts/build-bible.py --from /tmp/usfm --check   # verify, write nothing
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
OUT = os.path.join(ROOT, 'sources/bible')

TEXTS = {'ar': 'arb-vd', 'kjv': 'eng-kjv2006', 'web': 'eng-web'}

# The Protestant canon, in its order. The first 39 are the Old Testament.
CANON = [
    'GEN', 'EXO', 'LEV', 'NUM', 'DEU', 'JOS', 'JDG', 'RUT', '1SA', '2SA', '1KI', '2KI',
    '1CH', '2CH', 'EZR', 'NEH', 'EST', 'JOB', 'PSA', 'PRO', 'ECC', 'SNG', 'ISA', 'JER',
    'LAM', 'EZK', 'DAN', 'HOS', 'JOL', 'AMO', 'OBA', 'JON', 'MIC', 'NAM', 'HAB', 'ZEP',
    'HAG', 'ZEC', 'MAL',
    'MAT', 'MRK', 'LUK', 'JHN', 'ACT', 'ROM', '1CO', '2CO', 'GAL', 'EPH', 'PHP', 'COL',
    '1TH', '2TH', '1TI', '2TI', 'TIT', 'PHM', 'HEB', 'JAS', '1PE', '2PE', '1JN', '2JN',
    '3JN', 'JUD', 'REV',
]
OLD_TESTAMENT = set(CANON[:39])

# Where the three texts are known to differ. The build refuses to write unless
# what it finds is exactly this — no more, no fewer.
#
# Van Dyck numbers two closing greetings as verses of their own, which the KJV
# and WEB keep inside the verse before.
ONLY_IN = {
    ('1TI', 6, 22): {'ar'},
    ('3JN', 1, 15): {'ar'},
}
# The WEB follows the critical text and leaves these verses out.
WEB_OMITS = {('LUK', 17, 36), ('ACT', 8, 37), ('ACT', 15, 34), ('ACT', 24, 7)}
# The WEB places Romans' closing doxology after 14:23, where the Majority Text
# has it. KJV and Van Dyck have it at 16:25–27; it moves there, word for word.
WEB_DOXOLOGY = ([('ROM', 14, 24), ('ROM', 14, 25), ('ROM', 14, 26)],
                [('ROM', 16, 25), ('ROM', 16, 26), ('ROM', 16, 27)])
# Psalm 119 has no title; the WEB writes its acrostic letters with the title
# marker.
ACROSTIC = 119

# Names that should fall in the same verses of the Arabic and both English
# texts. A text shifted by a verse anywhere would break them.
ANCHORS = [('Moses', 'موسى'), ('David', 'داود'), ('Jesus', 'يسوع'), ('Abraham', 'إبراهيم')]
MIN_AGREEMENT = 0.95
ARABIC_MARKS = set(range(0x064B, 0x0653)) | {0x0670, 0x0640}


def read_edition(directory, edition):
    """-> ({(book, chapter, verse): text}, {book: Arabic or English name})"""
    books = usfm.find_books(directory, edition)
    missing = [code for code in CANON if code not in books]
    if missing:
        raise SystemExit(f'{edition}: books missing: {missing}')
    verses, names = {}, {}
    for code in CANON:
        skip = {ACROSTIC} if code == 'PSA' else ()
        book = usfm.read_book(books[code], skip_titles_in=skip)
        for (chapter, verse), text in book.merged().items():
            verses[(code, chapter, verse)] = text
        with open(books[code], encoding='utf-8-sig') as f:
            toc = re.search(r'^\\toc2 (.+)$', f.read(), re.M)
        names[code] = toc.group(1).strip() if toc else ''
    return verses, names


def strip(text):
    return ''.join(c for c in text if ord(c) not in ARABIC_MARKS)


def check(data, keys):
    problems = []

    # Every chapter numbered 1..n without a gap, the same chapters everywhere.
    for code in CANON:
        chapters = sorted({c for b, c, _ in keys if b == code})
        if chapters != list(range(1, len(chapters) + 1)):
            problems.append(f'{code}: chapters are not 1..{len(chapters)}')
        for chapter in chapters:
            numbers = sorted(v for b, c, v in keys if b == code and c == chapter)
            if numbers != list(range(1, len(numbers) + 1)):
                problems.append(f'{code} {chapter}: verses are not 1..{len(numbers)}')

    # Exactly the known differences.
    for key, verses in data.items():
        blank = {k for k in keys if not verses.get(k, '').strip()}
        expected = {k for k, owners in ONLY_IN.items() if key not in owners}
        if key == 'web':
            expected |= WEB_OMITS
        if blank != expected:
            problems.append(f'{key}: empty verses {sorted(blank - expected)[:5]} unexpected, '
                            f'{sorted(expected - blank)[:5]} expected but filled')

    agreement = {}
    for en_word, ar_word in ANCHORS:
        in_ar = {k for k, t in data['ar'].items() if ar_word in strip(t)}
        for key in ('kjv', 'web'):
            found = {k for k, t in data[key].items() if re.search(rf'\b{en_word}\b', t)}
            union = in_ar | found
            agree = len(in_ar & found)
            agreement[(key, en_word)] = (agree, len(union))
            if agree < MIN_AGREEMENT * len(union):
                problems.append(f'{key}/{en_word}: agrees with the Arabic in {agree}/{len(union)} verses')
    return problems, agreement


def build(data, keys, names):
    if os.path.isdir(OUT):
        for entry in os.listdir(OUT):
            if entry in CANON:
                shutil.rmtree(os.path.join(OUT, entry))
    os.makedirs(OUT, exist_ok=True)

    books = []
    for code in CANON:
        chapters = sorted({c for b, c, _ in keys if b == code})
        counts = []
        os.makedirs(os.path.join(OUT, code), exist_ok=True)
        for chapter in chapters:
            numbers = sorted(v for b, c, v in keys if b == code and c == chapter)
            counts.append(len(numbers))
            with open(os.path.join(OUT, code, f'{chapter}.json'), 'w', encoding='utf-8',
                      newline='\n') as f:
                json.dump({'verses': [
                    {
                        'verse': v,
                        'ar': data['ar'].get((code, chapter, v), ''),
                        'kjv': data['kjv'].get((code, chapter, v), ''),
                        'web': data['web'].get((code, chapter, v), ''),
                    }
                    for v in numbers
                ]}, f, ensure_ascii=False, separators=(',', ':'))
        books.append({
            'code': code,
            'testament': 'OT' if code in OLD_TESTAMENT else 'NT',
            'name_ar': names[code],
            'chapters': counts,
        })
    # One book per line: readable, and a rebuild diffs by book.
    with open(os.path.join(OUT, 'books.json'), 'w', encoding='utf-8', newline='\n') as f:
        lines = ',\n'.join(json.dumps(book, ensure_ascii=False) for book in books)
        f.write('{"books": [\n' + lines + '\n]}\n')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--from', dest='source', required=True,
                    help='directory holding the unzipped arb-vd, eng-kjv2006 and eng-web USFM exports')
    ap.add_argument('--check', action='store_true', help='verify only')
    args = ap.parse_args()

    data, names = {}, {}
    for key, edition in TEXTS.items():
        data[key], edition_names = read_edition(args.source, edition)
        if key == 'ar':
            names = edition_names

    # The WEB's doxology goes where the other two have it.
    web = data['web']
    found, target = WEB_DOXOLOGY
    if not all(web.get(k, '').strip() for k in found) or any(web.get(k, '').strip() for k in target):
        raise SystemExit("web: Romans' doxology is not where it was expected")
    for src, dst in zip(found, target):
        web[dst] = web.pop(src)

    keys = set(data['ar']) | set(data['kjv']) | set(data['web'])
    for key, verses in data.items():
        stray = set(verses) - set(data['kjv']) - set(ONLY_IN)
        if stray:
            raise SystemExit(f'{key}: verses no other text numbers: {sorted(stray)[:5]}')

    problems, agreement = check(data, keys)
    ot = sum(1 for k in keys if k[0] in OLD_TESTAMENT)
    print(f'books               : {len(CANON)}')
    print(f'verses              : {len(keys)} (Old Testament {ot}, New {len(keys) - ot})')
    print(f'per text            : { {k: sum(1 for t in v.values() if t.strip()) for k, v in data.items()} }')
    for (key, word), (agree, total) in agreement.items():
        print(f'pairing {key:<3} / {word:<8}: {agree}/{total} verses agree with the Arabic')
    if problems:
        print('\nFAILED:')
        for p in problems:
            print(f'  {p}')
        return 1
    print('checks passed')

    if args.check:
        return 0
    build(data, keys, names)
    print(f'wrote {len(CANON)} books, {len(keys)} verses to sources/bible/')
    return 0


if __name__ == '__main__':
    sys.exit(main())
