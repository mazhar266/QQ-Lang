#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Build the Bible dataset QQL reads — the `BI`, `OT` and `NT` sources.

The 66 books of the Protestant canon, in English verse numbering, from
eBible.org's public-domain USFM exports:

    arb-vd       Smith & Van Dyck Arabic (1865)               -> ar
    eng-kjv2006  King James Version, 1769 text                -> kjv
    eng-web      World English Bible                          -> web
    hbo          Hebrew Masoretic text, Old Testament         -> he
    grctr        Greek Textus Receptus, New Testament         -> grc
    engjps       JPS 1917 — read only for its Hebrew verse numbers

A psalm's title opens its verse 1, as English Bibles number them. The Jewish
Torah and Psalms, in Hebrew numbering, are separate sources built by
`build-torah.py` and `build-psalms.py`.

The original languages ride alongside the translations. The Hebrew (with its
Aramaic passages) is Hebrew-numbered, so it is placed on the English verses
through JPS's inline Hebrew numbers: English Genesis 31:55 carries Hebrew
32:1, and English Psalm 3:1 carries Hebrew 3:1 and 3:2. Where Hebrew joins two
English verses into one, the Hebrew stands on the first. The Greek is the
Textus Receptus — the text the KJV and Van Dyck were translated from — and is
numbered as they are.

Output shape matches what `src/sources/bible.rs` reads:

    sources/bible/books.json             66 books: code, testament, Arabic
                                         name, verse count of every chapter
    sources/bible/{BOOK}/{chapter}.json  { verse, ar, kjv, web, he | grc }

Usage
-----
Fetch and unpack the six USFM zips into one directory, then point at it:

    mkdir /tmp/usfm && cd /tmp/usfm
    for t in arb-vd eng-kjv2006 eng-web hbo engjps grctr; do
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
import unicodedata

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hebrew as hebrew_map  # noqa: E402  (English -> Hebrew numbering, from JPS's markers)
import usfm  # noqa: E402  (the shared USFM reader, beside this script)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'sources/bible')

TEXTS = {'ar': 'arb-vd', 'kjv': 'eng-kjv2006', 'web': 'eng-web'}
# The original languages, each for its testament.
ORIGINALS = {'he': 'hbo', 'grc': 'grctr'}

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

# English verses that carry no Hebrew. Where one Hebrew verse spans two
# English verses, it stands on the first: Hebrew Psalm 13:6 is English 13:5-6,
# and Hebrew Isaiah 63:19 is English 63:19 and 64:1. And Nehemiah 7:68 is not
# in the Masoretic text at all (see scripts/hebrew.py).
EMPTY_HEBREW = {('PSA', 13, 6), ('ISA', 64, 1), ('NEH', 7, 68)}

# The Greek has no verse of its own for the two closing greetings the Van Dyck
# numbers separately; it keeps them inside the verse before, as the KJV does.
EMPTY_GREEK = {('1TI', 6, 22), ('3JN', 1, 15)}

# Names that fall in the same verses of the original and the KJV. The Hebrew
# is matched as a whole word with its one-letter prefixes; the Greek by the
# stem every case of the name shares (Ἰησοῦς, Ἰησοῦ, Ἰησοῦν), capitalized so
# that πέτρα "rock" is not Peter.
ORIGINAL_ANCHORS = [
    ('he', 'Moses', r'[ולבכהמ]{0,2}משה'),
    ('he', 'David', r'[ולבכהמ]{0,2}דוי?ד'),
    ('he', 'Abraham', r'[ולבכהמ]{0,2}אברהם'),
    ('grc', 'Jesus', r'Ιησου'),
    ('grc', 'Paul', r'Παυλ'),
    ('grc', 'Peter', r'Πετρ'),
    ('grc', 'Abraham', r'Αβρααμ'),
]
HEBREW_MARKS = set(range(0x0591, 0x05C8)) - {0x05BE}


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


def plain_hebrew(text):
    """Hebrew without points or accents; a maqaf joins words, so a space."""
    return ''.join(' ' if ord(c) == 0x05BE else c for c in text if ord(c) not in HEBREW_MARKS)


def plain_greek(text):
    """Greek without accents or breathings."""
    return ''.join(c for c in unicodedata.normalize('NFD', text)
                   if unicodedata.category(c) != 'Mn')


def read_originals(directory, keys):
    """The Hebrew Old Testament on the English verses, and the Greek New.

    -> `({'he': {(book, chapter, verse): text}, 'grc': {…}}, problems)`
    """
    problems = []
    hbo = usfm.find_books(directory, 'hbo')
    jps = usfm.find_books(directory, 'engjps')
    he = {}
    for code in CANON[:39]:
        hebrew = usfm.read_book(hbo[code]).verses
        english = usfm.read_book(jps[code]).verses
        if set(english) != {(c, v) for b, c, v in keys if b == code}:
            problems.append(f'engjps {code}: English verses differ from the Bible\'s')
            continue
        _, targets = hebrew_map.jps_split(code, english)
        claimed = set()
        for verse in sorted(english):
            parts = []
            for target in targets[verse]:
                if target not in hebrew:
                    problems.append(f'{code} {verse}: Hebrew {target} does not exist')
                elif target not in claimed:
                    claimed.add(target)
                    parts.append(hebrew[target])
            he[(code, *verse)] = ' '.join(parts)
        lost = sorted(set(hebrew) - claimed)
        if lost:
            problems.append(f'hbo {code}: Hebrew verses on no English verse: {lost[:5]}')

    tr = usfm.find_books(directory, 'grctr')
    grc = {}
    for code in CANON[39:]:
        for (chapter, verse), text in usfm.read_book(tr[code], subscriptions=True).verses.items():
            grc[(code, chapter, verse)] = text
    return {'he': he, 'grc': grc}, problems


def check_originals(originals, keys, kjv):
    problems = []
    for key, books, expected in (('he', CANON[:39], EMPTY_HEBREW), ('grc', CANON[39:], EMPTY_GREEK)):
        span = {k for k in keys if k[0] in books}
        texts = originals[key]
        stray = sorted(set(texts) - span)
        if stray:
            problems.append(f'{key}: verses the Bible does not number: {stray[:5]}')
        blank = {k for k in span if not texts.get(k, '').strip()}
        if blank != expected:
            problems.append(f'{key}: empty {sorted(blank - expected)[:5]} unexpected, '
                            f'{sorted(expected - blank)[:5]} expected but filled')

    agreement = {}
    for key, word, pattern in ORIGINAL_ANCHORS:
        if key == 'he':
            found = {k for k, t in originals[key].items()
                     if any(re.fullmatch(pattern, w) for w in plain_hebrew(t).split())}
        else:
            found = {k for k, t in originals[key].items() if re.search(pattern, plain_greek(t))}
        books = CANON[:39] if key == 'he' else CANON[39:]
        in_kjv = {k for k, t in kjv.items() if k[0] in books and re.search(rf'\b{word}\b', t)}
        union = found | in_kjv
        agree = len(found & in_kjv)
        agreement[(key, word)] = (agree, len(union))
        if agree < MIN_AGREEMENT * len(union):
            problems.append(f'{key}/{word}: agrees with the KJV in {agree}/{len(union)} verses')
    return problems, agreement


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


def build(data, keys, names, originals):
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
                original = 'he' if code in OLD_TESTAMENT else 'grc'
                json.dump({'verses': [
                    {
                        'verse': v,
                        'ar': data['ar'].get((code, chapter, v), ''),
                        'kjv': data['kjv'].get((code, chapter, v), ''),
                        'web': data['web'].get((code, chapter, v), ''),
                        original: originals[original].get((code, chapter, v), ''),
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
    originals, read_problems = read_originals(args.source, keys)
    problems += read_problems
    original_problems, original_agreement = check_originals(originals, keys, data['kjv'])
    problems += original_problems
    agreement.update(original_agreement)
    ot = sum(1 for k in keys if k[0] in OLD_TESTAMENT)
    print(f'books               : {len(CANON)}')
    print(f'verses              : {len(keys)} (Old Testament {ot}, New {len(keys) - ot})')
    print(f'per text            : { {k: sum(1 for t in v.values() if t.strip()) for k, v in data.items()} }')
    for (key, word), (agree, total) in agreement.items():
        other = 'KJV' if key in ORIGINALS else 'Arabic'
        print(f'pairing {key:<3} / {word:<8}: {agree}/{total} verses agree with the {other}')
    if problems:
        print('\nFAILED:')
        for p in problems:
            print(f'  {p}')
        return 1
    print('checks passed')

    if args.check:
        return 0
    build(data, keys, names, originals)
    print(f'wrote {len(CANON)} books, {len(keys)} verses to sources/bible/')
    return 0


if __name__ == '__main__':
    sys.exit(main())
