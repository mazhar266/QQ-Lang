# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Read eBible.org's USFM exports into plain verse text.

Shared by `build-psalms.py`, `build-bible.py` and `build-torah.py`, so every
scripture QQL carries is read the same way. USFM rather than eBible's
plain-text (VPL) export because it keeps what the plain text loses: a psalm's
title apart from its first verse, and JPS's inline Hebrew verse numbers.

The converter is strict: a marker it does not know is an error, never
silently dropped text. Measured against the plain-text export, it reproduces
the Van Dyck Arabic and the KJV word for word, all 31,104 and 31,102 verses
(psalm titles aside, which the plain text merges into verse 1).
"""
import os
import re

# Paragraph-level markers whose whole line is a heading or an introduction,
# not verse text: section headings, book titles, the Psalter's "BOOK 1",
# speakers in the Song of Songs, the KJV's subscriptions to the epistles.
HEADINGS = {
    'id', 'ide', 'h', 'toc1', 'toc2', 'toc3', 'mt1', 'mt2', 'mt3', 'ms1', 'ms2',
    's1', 's2', 'qa', 'cl', 'cp', 'rem', 'sp', 'r', 'mr', 'ip', 'is1', 'ili',
}
# Paragraph markers that only break a line inside a verse.
BREAKS = {'p', 'q1', 'q2', 'q3', 'qc', 'b', 'm', 'mi', 'pi', 'pi1', 'pi2', 'pc',
          'nb', 'li1', 'li2', 'pmo'}
# Character markers whose content is kept as plain text.
INLINE = {'nd', 'qs', 'tl', 'wh', 'wj', 'sc', 'it', 'bd', 'em', 'pn', 'k', 'bk'}


class Book:
    """One book: `{(chapter, verse): text}` and `{chapter: title}`."""

    def __init__(self, code, verses, titles):
        self.code = code
        self.verses = verses
        self.titles = titles

    def merged(self):
        """Verses with each chapter's title opening its verse 1, as English
        Bibles number them and eBible's plain-text export prints them."""
        out = dict(self.verses)
        for chapter, title in self.titles.items():
            out[(chapter, 1)] = f'{title} {out[(chapter, 1)]}'
        return out


def plain(text, where):
    """USFM character markup -> plain text. Unknown markup is an error."""
    text = re.sub(r'\\f .*?\\f\*', '', text, flags=re.S)
    text = re.sub(r'\\x .*?\\x\*', '', text, flags=re.S)
    # \w word|strong="H1234"\w*  and the nested \+w form
    text = re.sub(r'\\\+?w ([^|\\]*)(?:\|[^\\]*)?\\\+?w\*', r'\1', text)
    # Words the KJV translators supplied: bracketed, as eBible's plain-text
    # export prints them and as the Quran translation already does.
    text = re.sub(r'\\\+?add (.*?)\\\+?add\*', r'[\1]', text, flags=re.S)
    for marker in INLINE:
        text = re.sub(rf'\\\+?{marker}\*', '', text)
        text = re.sub(rf'\\\+?{marker} ', '', text)
    if '\\' in text:
        raise SystemExit(f'{where}: unhandled USFM markup in {text!r}')
    return ' '.join(text.split())


def read_book(path, skip_titles_in=()):
    """Read one USFM book.

    A `\\d` title before a chapter's first verse is that chapter's title. One
    after verse text is an error, except in the chapters `skip_titles_in`
    names: the WEB marks Psalm 119's acrostic letters with the title marker.
    """
    verses, titles = {}, {}
    code = None
    chapter, current, buffer = 0, None, []

    def flush():
        if current is not None:
            verses[current] = plain(' '.join(buffer), f'{path} {current}')

    with open(path, encoding='utf-8-sig') as f:
        for number, line in enumerate(f, start=1):
            line = line.strip()
            if not line:
                continue
            m = re.match(r'\\([a-z]+[0-9]*)\s*(.*)', line)
            if not m:
                buffer.append(line)
                continue
            marker, rest = m.groups()
            if marker == 'id':
                code = rest.split()[0]
            elif marker == 'c':
                flush()
                chapter, current, buffer = int(rest.split()[0]), None, []
            elif marker == 'v':
                flush()
                n, _, text = rest.partition(' ')
                # `\v 15-16`: one sentence numbered as two verses. The text
                # goes to the first; the others exist, empty.
                first, _, last = n.partition('-')
                current, buffer = (chapter, int(first)), [text]
                for joined in range(int(first) + 1, int(last or first) + 1):
                    verses[(chapter, joined)] = ''
            elif marker == 'd':
                if chapter in skip_titles_in:
                    continue
                if current is not None:
                    raise SystemExit(f'{path}:{number}: title after verse text in {chapter}')
                # A long title may run over several \d lines (Van Dyck's 56).
                text = plain(rest, f'{path}:{number}')
                titles[chapter] = f'{titles[chapter]} {text}' if chapter in titles else text
            elif marker in HEADINGS:
                continue
            elif marker in BREAKS:
                if rest:
                    buffer.append(rest)
            else:
                raise SystemExit(f'{path}:{number}: unknown marker \\{marker}')
    flush()
    if code is None:
        raise SystemExit(f'{path}: no \\id line')
    return Book(code, verses, titles)


def find_books(directory, edition):
    """`{book code: path}` for every `*{edition}.usfm` under `directory`.

    eBible names its files `02-GENarb-vd.usfm`, so several editions unzipped
    into one directory stay apart by their suffix; the code is read from the
    file's own `\\id` line.
    """
    found = {}
    suffix = f'{edition}.usfm'
    for here, _, files in os.walk(directory):
        for name in files:
            if not name.endswith(suffix):
                continue
            path = os.path.join(here, name)
            with open(path, encoding='utf-8-sig') as f:
                for line in f:
                    if line.startswith('\\id '):
                        found[line.split()[1]] = path
                        break
    if not found:
        raise SystemExit(f'no *{suffix} files under {directory}')
    return found
