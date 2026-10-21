# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Map English verse numbers to Hebrew ones, from JPS 1917's inline markers.

The Hebrew Bible and English Bibles number many verses differently: a psalm's
title is a verse of its own in Hebrew, and in a few dozen places elsewhere a
verse or two moves across a chapter boundary (Hebrew Genesis 32:1 is English
31:55; Hebrew Malachi 3:19-24 is English 4:1-6). eBible's JPS 1917 is
English-numbered but carries the Hebrew numbers inline — "(32-1) And early in
the morning…" — so it gives the map for every book of the Old Testament.

Shared by `build-psalms.py` and `build-torah.py`, which re-cut English-numbered
translations onto the Hebrew verses, and `build-bible.py`, which goes the
other way and places the Hebrew text on the English verses.
"""
import re

MARKER = re.compile(r'\((\d+)-(\d+)\)')

# JPS numbers the Ten Commandments by the "lower" accents, joining
# commandments 6-9 into one verse. The Hebrew text QQL carries divides them as
# English Bibles do, so in these chapters the English numbers stand.
DECALOGUE = {('EXO', 20), ('DEU', 5)}

# JPS opens five psalms with the Psalter's book headings, which every other
# section heading in the export has had stripped. They are not text.
BOOK_HEADING = re.compile(r'^BOOK [IV]+\s+')
BOOK_HEADING_PSALMS = {1, 42, 73, 90, 107}

# JPS gives Psalm 70 no Hebrew markers, though its Hebrew title is a verse of
# its own. The one cut it needs: the title ends with these words.
UNMARKED = {('PSA', 70): 'to make memorial.'}

# English verses the Masoretic text does not have. Nehemiah 7:68 ("Their
# horses … their mules") is missing from most Hebrew manuscripts; English
# Bibles supply it from the parallel list in Ezra 2:66, and JPS includes it
# without marking the shift. It has no Hebrew, and the chapter's later English
# verses are the Hebrew's one lower: English 7:73 is Hebrew 7:72.
ABSENT = {('NEH', 7, 68)}


def jps_split(code, jps):
    """Divide one JPS book at its Hebrew markers.

    `jps` is `{(chapter, verse): text}` in English numbering. Returns
    `(segments, targets)`: `segments[h]` is the JPS text of Hebrew verse `h`,
    and `targets[e]` the Hebrew verses English verse `e` feeds, in order.
    Text ahead of any marker keeps its English number, which is the Hebrew one
    wherever JPS marks nothing — and where it does not, the build's own checks
    (every Hebrew verse filled, nothing joined unexpectedly) catch it.
    """
    texts = dict(jps)

    if code == 'PSA':
        removed = 0
        for psalm in BOOK_HEADING_PSALMS:
            texts[(psalm, 1)], n = BOOK_HEADING.subn('', texts[(psalm, 1)])
            removed += n
        if removed != len(BOOK_HEADING_PSALMS):
            raise SystemExit(f'JPS: removed {removed} book headings, expected {len(BOOK_HEADING_PSALMS)}')

    for (book, chapter), cut in UNMARKED.items():
        if book != code:
            continue
        if any(MARKER.search(t) for (c, _), t in texts.items() if c == chapter):
            raise SystemExit(f'JPS {code} {chapter}: expected no markers, found some')
        title, found, rest = texts[(chapter, 1)].partition(cut)
        if not found:
            raise SystemExit(f'JPS {code} {chapter}: cut {cut!r} not found')
        texts[(chapter, 1)] = f'({chapter}-1) {title}{cut} ({chapter}-2) {rest.strip()}'
        for (c, v) in sorted(texts):
            if c == chapter and v > 1:
                texts[(c, v)] = f'({chapter}-{v + 1}) {texts[(c, v)]}'

    def unmarked(chapter, verse):
        """Where unmarked text goes: its own number, less any absent verse
        before it in the chapter; nowhere, if it is absent itself."""
        if (code, chapter, verse) in ABSENT:
            return None
        lower = sum(1 for b, c, v in ABSENT if (b, c) == (code, chapter) and v < verse)
        return (chapter, verse - lower)

    segments, targets = {}, {}
    for (chapter, verse) in sorted(texts):
        text = texts[(chapter, verse)]
        if (code, chapter) in DECALOGUE:
            text = MARKER.sub('', text)
        parts = MARKER.split(text)
        home = unmarked(chapter, verse)
        if home is None:
            if len(parts) > 1:
                raise SystemExit(f'JPS {code} {chapter}:{verse}: absent from the Hebrew, yet marked')
            targets[(chapter, verse)] = []
            continue
        pieces = [(home, parts[0])]
        pieces += [((int(parts[i]), int(parts[i + 1])), parts[i + 2])
                   for i in range(1, len(parts), 3)]
        fed = []
        for target, piece in pieces:
            piece = piece.strip()
            if not piece and target == home and len(pieces) > 1:
                continue  # the empty text ahead of a leading marker
            fed.append(target)
            segments[target] = (segments.get(target, '') + ' ' + piece).strip()
        targets[(chapter, verse)] = list(dict.fromkeys(fed))
    return segments, targets
