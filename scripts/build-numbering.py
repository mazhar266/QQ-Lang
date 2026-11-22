#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Build the six collection numbering maps from a pinned Sunnah.com capture.

Pass the Sunnah directory of CheeseWithSauce/HadithsJSONFormat at commit
1ef8f97ac41cd04845081de79529d879966c6fbc. Only citation labels are copied.
Each output chapter lists one comma-separated set of labels per local item.
Rows follow the original by_book idInBook order, including introductions.
"""
import argparse
import json
import re
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / 'sources'
BOOKS = dict(B='bukhari', M='muslim', AD='abudawud', T='tirmidhi',
             N='nasai', IM='ibnmajah')


def letters(text):
    return ''.join(c for c in unicodedata.normalize('NFKC', text).lower()
                   if c.isalpha() and c != 'ـ')


def labels(text):
    """Expand printed combinations such as 1809a, b or 6173-6175."""
    out = []
    for part in re.split('[,/]', text.lower().replace(' ', '')):
        if not part:
            continue
        if re.fullmatch('[a-z]+', part):
            part = re.match(r'\d+', out[-1])[0] + part
        if '-' in part:
            first, last = part.split('-')
            if last.isdecimal():
                out.extend(str(i) for i in range(int(first), int(last) + 1))
            else:
                base, start = re.fullmatch(r'(\d+)([a-z])', first).groups()
                out.extend(base + chr(i) for i in range(ord(start), ord(last) + 1))
        else:
            assert re.fullmatch(r'\d+[a-z]*', part), part
            out.append(part)
    return ','.join(out)


def build(code, book, source, by_book):
    local = {p.stem: json.loads(p.read_bytes())['hadiths']
             for p in (ROOT / 'hadith' / book).glob('*.json')}
    chapters = {key: [None] * len(rows) for key, rows in local.items()}
    for path in sorted((source / book).glob('*.json')):
        for row in json.loads(path.read_bytes()):
            printed, ref = row['reference'].split('In-book reference : ', 1)
            m = re.match(r'(?:Book (\d+b?)|Introduction), (?:Hadith|Narration) (\d+)', ref)
            chapter, item = m[1] or 'introduction', int(m[2])
            # Bukhari 3633/3634 trade positions in the local chapter.
            if code == 'B' and chapter == '61' and item in (137, 138):
                item = {137: 138, 138: 137}[item]
            record = local[chapter][item - 1]
            assert record['id'] == item
            ar, en = letters(row['arabic']), letters(row['english'])
            local_ar = letters(record['arabic'])
            local_en = letters(record['english']['narrator'] + record['english']['text'])
            # A few editions add an editorial ending or vary a word. Verify
            # both lead texts at the printed coordinate for these records.
            exact = (ar and ar == local_ar) or (en and en == local_en)
            if not exact and not (len(ar) >= 80 and ar[:80] == local_ar[:80]
                                  and len(en) >= 50 and en[:50] == local_en[:50]):
                raise ValueError(f'{code} {chapter}:{item} text mismatch')
            number = re.search(r'\d.*', printed.strip())
            assert chapters[chapter][item - 1] is None, (code, chapter, item)
            # Muslim's introduction uses its own overlapping citation numbers.
            separate_intro = code == 'M' and chapter == 'introduction'
            if separate_intro:
                citation = ''
            elif number:
                citation = labels(number[0])
            else:
                # The capture omitted these two labels. The publisher gives
                # both coordinates at https://sunnah.com/nasai:512 and
                # https://sunnah.com/nasai:570.
                citation = {('N', '6', 19): '512', ('N', '6', 77): '570'}[code, chapter, item]
            chapters[chapter][item - 1] = citation
    edition = json.loads((by_book / f'{book}.json').read_bytes())['hadiths']
    # Read actual chapter order from the edition. 35b carries chapter ID 35.2.
    keys = {rows[0]['chapterId']: key for key, rows in local.items()}
    order = list(dict.fromkeys(keys[row['chapterId']] for row in edition))
    ordered = [(key, row) for key in order for row in local[key]]
    if len(ordered) != len(edition):
        raise ValueError(f'{code}: book and chapter editions differ in size')
    for number, ((key, row), original) in enumerate(zip(ordered, edition), 1):
        if (number != original['idInBook'] or row['arabic'] != original['arabic']
                or row['english'] != original['english'] or row['chapterId'] != original['chapterId']):
            raise ValueError(f'{code}: book ID {number} differs in chapter {key}')
    blocks = []
    for key in order:
        if None in chapters[key]:
            raise ValueError(f'{code}: missing a local record in {key}')
        blocks.append([key, chapters[key]])
    output = ROOT / 'numbering' / f'{code}.json'
    output.parent.mkdir(exist_ok=True)
    output.write_text('[\n' + ',\n'.join(json.dumps(ch, separators=(',', ':')) for ch in blocks) + '\n]\n')
    print(f'{code}: {len(ordered)} records')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('--by-book', type=Path,
                        default=ROOT / 'hadith-json/db/by_book/the_9_books')
    args = parser.parse_args()
    for code, book in BOOKS.items():
        build(code, book, args.source, args.by_book)
