#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Build the canonical hadith-number mappings that `B::N` resolves through.

The dataset under sources/hadith numbers hadiths by its own insertion
order, which drifts from the numbering the world actually cites ('Abd al-Baqi
for Bukhari, Dar-us-Salam for Muslim, the sunnah.com reference numbers for the
rest). This script fetches fawazahmed0/hadith-api — public domain, carries the
canonical number *and* an in-book (book, hadith) reference per entry — and
emits one committed map per collection:

    sources/canonical/B.json      {"6403": [80, 98], ...}

meaning: canonical Bukhari 6403 is chapter 80, item 98 — which is exactly how
QQL's `B:80:98` addresses it, because both datasets descend from sunnah.com
and share chapter structure. The resolver then serves `B::6403` through the
ordinary chapter path.

Entries that cannot be addressed are left out, deliberately:
  - book 0 (front matter: Muslim's Muqaddima, Ibn Majah's introduction);
  - non-integer numbers (lettered/sub-numbered variants like 1771.5) — the
    grammar is integers, and they are variants of their base number.

Every emitted pair is validated against the local chapter files, and
canonical 1 must map to chapter 1 item 1, so a wrong or shifted upstream file
fails the build rather than shipping silently-wrong scripture.

Usage:
    python3 scripts/build-canonical.py                 # fetch from jsDelivr
    python3 scripts/build-canonical.py --from DIR      # use cached {book}.json
"""
import argparse
import json
import os
import sys
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHAPTERS = os.path.join(ROOT, 'sources/hadith')
OUT = os.path.join(ROOT, 'sources/canonical')

CDN = 'https://cdn.jsdelivr.net/gh/fawazahmed0/hadith-api@1/editions/ara-{book}.min.json'

BOOKS = [
    ('bukhari', 'B'),
    ('muslim', 'M'),
    ('abudawud', 'AD'),
    ('tirmidhi', 'T'),
    ('nasai', 'N'),
    ('ibnmajah', 'IM'),
]


def canonical_entries(book, cache):
    if cache:
        with open(os.path.join(cache, f'{book}.json'), encoding='utf-8') as f:
            return json.load(f)['hadiths']
    with urllib.request.urlopen(CDN.format(book=book), timeout=120) as f:
        return json.load(f)['hadiths']


def chapter_sizes(book):
    sizes = {}
    directory = os.path.join(CHAPTERS, book)
    for name in os.listdir(directory):
        if name.endswith('.json') and name[:-5].isdigit():
            with open(os.path.join(directory, name), encoding='utf-8') as f:
                sizes[int(name[:-5])] = len(json.load(f)['hadiths'])
    return sizes


def build(book, code, cache):
    sizes = chapter_sizes(book)
    mapping = {}
    skipped_front, skipped_variant, missing_locally = 0, 0, 0

    for entry in canonical_entries(book, cache):
        number = entry['hadithnumber']
        ref = entry['reference']
        chapter, item = ref['book'], ref['hadith']

        if not float(number).is_integer():
            skipped_variant += 1
            continue
        number = int(number)
        if not (isinstance(chapter, int) and chapter in sizes):
            skipped_front += 1
            continue

        if not (isinstance(item, int) and 1 <= item <= sizes[chapter]):
            # The local dataset lacks a handful of entries the canonical
            # edition carries (e.g. Tirmidhi 2089). One or two trailing gaps
            # are data reality; more than a few means the numbering is shifted
            # and every mapping after the shift would be wrong scripture.
            missing_locally += 1
            if missing_locally > 3:
                raise SystemExit(
                    f'{code} {number}: reference {chapter}:{item} is outside '
                    f'the local data (chapter has {sizes.get(chapter)}) and '
                    f'this is the 4th such gap — numbering looks shifted, refusing'
                )
            continue
        if str(number) in mapping:
            raise SystemExit(f'{code}: canonical {number} appears twice — refusing')
        mapping[str(number)] = [chapter, item]

    # Front matter may own the first canonical numbers (Muslim's Muqaddima is
    # 1..92, Ibn Majah's introduction 1..266), so the invariant is that the
    # first *mapped* number lands on chapter 1, item 1 — numbering and chapter
    # order agree where the numbered body starts.
    first = min(mapping, key=int)
    if mapping[first] != [1, 1]:
        raise SystemExit(
            f'{code}: first canonical {first} maps to {mapping[first]}, '
            f'expected [1, 1] — the upstream numbering looks shifted, refusing'
        )

    path = os.path.join(OUT, f'{code}.json')
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(mapping, f, separators=(',', ':'), sort_keys=True)

    top = max(int(k) for k in mapping)
    print(f'{code:>3}: {len(mapping):5d} canonical numbers, 1..{top}  '
          f'(skipped: {skipped_front} front matter, {skipped_variant} variants, '
          f'{missing_locally} missing locally)  {path}')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--from', dest='cache',
                    help='directory of pre-fetched ara-{book} files, named {book}.json')
    args = ap.parse_args()

    os.makedirs(OUT, exist_ok=True)
    for book, code in BOOKS:
        build(book, code, args.cache)
    return 0


if __name__ == '__main__':
    sys.exit(main())
