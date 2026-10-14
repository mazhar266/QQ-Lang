#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Build the Psalms (Zabur) dataset QQL reads, from eBible.org's VPL exports.

Three public-domain texts, paired verse by verse:

    arb-vd       Smith & Van Dyck Arabic (1865)        -> ar
    eng-kjv2006  King James Version, 1769 text          -> kjv
    eng-web      World English Bible, 2020 stable text  -> web

All three follow the English versification, in which a psalm's superscription
("A Psalm of David") is part of verse 1 rather than a verse of its own. That is
what makes them pair at all; the checks below refuse to write if it ever stops
being true.

Output shape matches what `src/sources/psalms.rs` reads:

    sources/psalms/{psalm}.json

Usage
-----
Fetch and unpack the three VPL zips into one directory, then point at it:

    mkdir /tmp/vpl && cd /tmp/vpl
    for t in arb-vd eng-kjv2006 eng-web; do
        curl -sSLO https://ebible.org/Scriptures/${t}_vpl.zip && unzip -o ${t}_vpl.zip
    done
    python3 scripts/build-psalms.py --from /tmp/vpl
    python3 scripts/build-psalms.py --from /tmp/vpl --check   # verify, write nothing
"""
import argparse
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'sources/psalms')

BOOK = 'PSA'
PSALM_COUNT = 150
VERSE_COUNT = 2461

FILES = {
    'ar': 'arb-vd_vpl.txt',
    'kjv': 'eng-kjv2006_vpl.txt',
    'web': 'eng-web_vpl.txt',
}

# The WEB export keeps Psalm 119's acrostic headings, which every other
# section title in the package has had stripped — and it keeps them in the
# wrong place: ALEPH opens 119:1, but BETH..TAV trail the *last* verse of the
# previous stanza (119:8 ends "Don't utterly forsake me. BETH"). They are not
# translation text, KJV and Van Dyck carry none, so they are removed.
WEB_HEADINGS = [
    'ALEPH', 'BETH', 'GIMEL', 'DALETH', 'HE', 'VAV', 'ZAYIN', 'HETH', 'TETH',
    'YODH', 'KAPF', 'LAMEDH', 'MEM', 'NUN', 'SAMEKH', 'AYIN', 'PE', 'TZADHE',
    'QOPH', 'RESH', 'SIN AND SHIN', 'TAV',
]

# Words that appear in the same verses of every text, used to prove the
# Arabic and English are paired correctly. Measured on the 2026 exports:
# "Selah" agrees in 71/71 verses aligned and 1/141 shifted by one; "David" in
# 86/86 against 2/170. A 95% floor leaves room for a future edit and still
# catches any shift.
ANCHORS = [
    ('Selah', 'سلاه'),
    ('David', 'داود'),
]
MIN_AGREEMENT = 0.95

# Harakat, superscript alef and tatweel: the anchors match without them.
HARAKAT = set(range(0x064B, 0x0653)) | {0x0670, 0x0640}


def find(directory, name):
    for here, _, files in os.walk(directory):
        if name in files:
            return os.path.join(here, name)
    raise SystemExit(f'{name} not found under {directory}')


def read_vpl(path):
    """`PSA 23:1 text` lines → {(psalm, verse): text}, in file order."""
    verses = {}
    with open(path, encoding='utf-8-sig') as f:
        for number, line in enumerate(f, start=1):
            if not line.startswith(BOOK + ' '):
                continue
            try:
                _, ref, text = line.rstrip('\n').split(' ', 2)
                psalm, verse = (int(n) for n in ref.split(':'))
            except ValueError:
                raise SystemExit(f'{path}:{number}: not a VPL line: {line!r}')
            if (psalm, verse) in verses:
                raise SystemExit(f'{path}:{number}: {psalm}:{verse} repeated')
            verses[(psalm, verse)] = text.strip()
    return verses


def strip_web_headings(web):
    """Remove the 22 acrostic headings from WEB Psalm 119. Returns how many."""
    removed = 0
    for index, heading in enumerate(WEB_HEADINGS):
        if index == 0:
            key, pattern = (119, 1), rf'^{heading} '
        else:
            key, pattern = (119, 8 * index), rf' {heading}$'
        text, n = re.subn(pattern, '', web[key])
        web[key] = text
        removed += n
    return removed


def bare(text):
    """Arabic without harakat, alef wasla read as a plain alef."""
    return ''.join(
        chr(0x0627) if ord(ch) == 0x0671 else ch
        for ch in text
        if ord(ch) not in HARAKAT
    )


def check(texts, removed):
    problems = []
    ar = texts['ar']

    for name, verses in texts.items():
        if len(verses) != VERSE_COUNT:
            problems.append(f'{name}: {len(verses)} verses, expected {VERSE_COUNT}')
        if set(verses) != set(ar):
            problems.append(f'{name}: verse references differ from the Arabic')
        empty = [k for k, t in verses.items() if not t]
        if empty:
            problems.append(f'{name}: {len(empty)} empty verses, first {empty[0]}')

    psalms = sorted({p for p, _ in ar})
    if psalms != list(range(1, PSALM_COUNT + 1)):
        problems.append(f'psalms run {psalms[0]}..{psalms[-1]}, expected 1..{PSALM_COUNT}')
    for psalm in psalms:
        numbers = sorted(v for p, v in ar if p == psalm)
        if numbers != list(range(1, len(numbers) + 1)):
            problems.append(f'psalm {psalm}: verses are not 1..{len(numbers)}')

    if removed != len(WEB_HEADINGS):
        problems.append(f'web: removed {removed} Psalm 119 headings, expected {len(WEB_HEADINGS)}')
    # The WEB writes no word in capitals anywhere else in Psalm 119, so any
    # left over is part of a heading.
    left = [
        k for k, t in texts['web'].items()
        if k[0] == 119 and re.search(r'\b[A-Z]{2,}\b', t)
    ]
    if left:
        problems.append(f'web: acrostic headings left at {left}')

    agreement = {}
    for english in ('kjv', 'web'):
        for en_word, ar_word in ANCHORS:
            agree = total = 0
            for key, arabic in ar.items():
                in_en = bool(re.search(rf'\b{en_word}\b', texts[english].get(key, '')))
                in_ar = ar_word in bare(arabic)
                if in_en or in_ar:
                    total += 1
                    agree += in_en == in_ar
            share = agree / max(total, 1)
            agreement[(english, en_word)] = (agree, total)
            if share < MIN_AGREEMENT:
                problems.append(
                    f'{english}/{en_word}: anchors agree in {agree}/{total} verses '
                    f'— the texts are not paired'
                )

    return problems, agreement


def build(texts):
    os.makedirs(OUT, exist_ok=True)
    for psalm in range(1, PSALM_COUNT + 1):
        verses = sorted(v for p, v in texts['ar'] if p == psalm)
        with open(f'{OUT}/{psalm}.json', 'w', encoding='utf-8', newline='\n') as f:
            json.dump({
                'psalm': psalm,
                'verses': [
                    {
                        'verse': verse,
                        'ar': texts['ar'][(psalm, verse)],
                        'kjv': texts['kjv'][(psalm, verse)],
                        'web': texts['web'][(psalm, verse)],
                    }
                    for verse in verses
                ],
            }, f, ensure_ascii=False, separators=(',', ':'))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument(
        '--from',
        dest='source',
        required=True,
        help='directory holding the unzipped arb-vd, eng-kjv2006 and eng-web VPL exports',
    )
    ap.add_argument('--check', action='store_true', help='verify only')
    args = ap.parse_args()

    texts = {key: read_vpl(find(args.source, name)) for key, name in FILES.items()}
    removed = strip_web_headings(texts['web'])
    problems, agreement = check(texts, removed)

    for key, verses in texts.items():
        print(f'{key:<4}: {len(verses)} verses')
    print(f'web : {removed} Psalm 119 headings removed')
    for (english, word), (agree, total) in agreement.items():
        print(f'pairing {english}/{word}: {agree}/{total} verses agree')
    if problems:
        print('\nFAILED:')
        for p in problems:
            print(f'  {p}')
        return 1
    print('checks passed')

    if args.check:
        return 0
    build(texts)
    print(f'wrote {PSALM_COUNT} psalms covering {VERSE_COUNT} verses to sources/psalms/')
    return 0


if __name__ == '__main__':
    sys.exit(main())
