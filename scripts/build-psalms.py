#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Mazhar Ahmed
"""Build the Psalms (Zabur) dataset QQL reads — the Jewish edition.

`PS` carries the Psalms as the Hebrew text numbers them, from eBible.org's
public-domain USFM exports:

    hbo          Hebrew Masoretic text — the authority for verse numbers  -> he
    engjps       JPS 1917, the Jewish English translation                 -> en
    arb-vd       Smith & Van Dyck Arabic (1865)                           -> ar
    eng-kjv2006  King James Version, 1769 text                            -> kjv
    eng-web      World English Bible                                      -> web

Hebrew numbering counts a psalm's title as a verse of its own: Psalm 3 has 9
verses in Hebrew and 8 in English, and its Hebrew verse 1 is the title alone.
Hebrew has 2527 verses where English Bibles have 2461. The four translations
all arrive in English numbering, so this script re-cuts them:

- **JPS** carries the Hebrew verse numbers inline, `(3-1) A Psalm of David…
  (3-2) LORD, how many…`, so it is split exactly at those markers. Its markers
  define the English -> Hebrew map that the other three follow.
- **Van Dyck, KJV and WEB** mark a psalm's title apart from verse 1 (`\\d`).
  Where Hebrew counts the title as its own verse(s), the title goes there and
  the rest of English verse 1 goes to the next. Where a title spans two
  Hebrew verses (Psalms 51, 52, 54, 60), the title fills the first and the
  second is empty for these three; only JPS can be cut inside it.

Output shape matches what `src/sources/psalms.rs` reads:

    sources/psalms/{psalm}.json

Usage
-----
Fetch and unpack the five USFM zips into one directory, then point at it:

    mkdir /tmp/usfm && cd /tmp/usfm
    for t in hbo engjps arb-vd eng-kjv2006 eng-web; do
        curl -sSLO https://ebible.org/Scriptures/${t}_usfm.zip && unzip -o ${t}_usfm.zip
    done
    python3 scripts/build-psalms.py --from /tmp/usfm
    python3 scripts/build-psalms.py --from /tmp/usfm --check   # verify, write nothing
"""
import argparse
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'sources/psalms')

PSALM_COUNT = 150
HEBREW_VERSES = 2527
ENGLISH_VERSES = 2461

TEXTS = {
    'he': 'hbo',
    'en': 'engjps',
    'ar': 'arb-vd',
    'kjv': 'eng-kjv2006',
    'web': 'eng-web',
}

# Paragraph-level markers whose whole line is a heading, not verse text.
HEADINGS = {'id', 'ide', 'h', 'toc1', 'toc2', 'toc3', 'mt1', 'mt2', 'ms1', 'ms2',
            's1', 's2', 'qa', 'cl', 'rem', 'sp', 'r', 'mr', 'cp', 'qc'}
# Paragraph markers that only break a line inside a verse.
BREAKS = {'p', 'q1', 'q2', 'q3', 'b', 'm', 'pi', 'pi1', 'pi2', 'nb', 'li1', 'li2', 'pmo'}
# Character markers whose content is kept as plain text.
INLINE = {'nd', 'qs', 'tl', 'wh', 'wj', 'sc', 'it', 'bd', 'em', 'pn', 'k'}

# The alphabetic acrostic whose stanza headings some exports mark as titles.
ACROSTIC = 119

# JPS opens five psalms with the Psalter's book headings, which every other
# section heading in the export has had stripped. They are not text.
JPS_BOOK = re.compile(r'^BOOK [IV]+\s+')
JPS_BOOK_PSALMS = {1, 42, 73, 90, 107}

# JPS gives Psalm 70 no Hebrew verse markers, though its Hebrew title is a
# verse of its own. The one cut it needs: the title ends with these words.
JPS_UNMARKED = {70: 'to make memorial.'}

# Words that fall in the same Hebrew verses in every text — the proof that the
# re-cutting is right. Selah sits at the end of a verse; David is mostly in
# titles, so it tests the title placement directly.
#
# The Hebrew is matched as a whole word — `דוד` also sits inside גדוד "a
# troop", `סלה` inside לכסלה "to folly" — allowing the one-letter prefixes
# "to", "and", "in", "as", and the fuller spelling דויד.
ANCHORS = [
    ('Selah', r'סלה', 'سلاه'),
    ('David', r'[ולבכ]{0,2}דוי?ד', 'داود'),
]
MIN_AGREEMENT = 0.95

HEBREW_MARKS = set(range(0x0591, 0x05C8)) - {0x05BE}
ARABIC_MARKS = set(range(0x064B, 0x0653)) | {0x0670, 0x0640}


def find(directory, code):
    suffix = f'PSA{code}.usfm'
    for here, _, files in os.walk(directory):
        for name in files:
            if name.endswith(suffix):
                return os.path.join(here, name)
    raise SystemExit(f'*{suffix} not found under {directory}')


def plain(text, where):
    """USFM character markup -> plain text. Unknown markup is an error."""
    text = re.sub(r'\\f .*?\\f\*', '', text, flags=re.S)
    text = re.sub(r'\\x .*?\\x\*', '', text, flags=re.S)
    # \w word|strong="H1234"\w*  and the nested \+w form
    text = re.sub(r'\\\+?w ([^|\\]*)(?:\|[^\\]*)?\\\+?w\*', r'\1', text)
    # Words the KJV translators supplied: bracketed, as the plain-text export prints them.
    text = re.sub(r'\\\+?add (.*?)\\\+?add\*', r'[\1]', text, flags=re.S)
    for marker in INLINE:
        text = re.sub(rf'\\\+?{marker}\*', '', text)
        text = re.sub(rf'\\\+?{marker} ', '', text)
    if '\\' in text:
        raise SystemExit(f'{where}: unhandled USFM markup in {text!r}')
    return ' '.join(text.split())


def read_usfm(path):
    """-> ({(psalm, verse): text}, {psalm: title})"""
    verses, titles = {}, {}
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
            if marker == 'c':
                flush()
                chapter, current, buffer = int(rest), None, []
            elif marker == 'v':
                flush()
                n, _, text = rest.partition(' ')
                current, buffer = (chapter, int(n)), [text]
            elif marker == 'd' and chapter == ACROSTIC:
                # Psalm 119 has no title; the WEB writes its 22 letter
                # headings (ALEPH, BETH…) with the title marker.
                continue
            elif marker == 'd':
                if current is not None:
                    raise SystemExit(f'{path}:{number}: title after verse text in {chapter}')
                # A long title may run over several \d lines (Van Dyck's 56).
                line_text = plain(rest, f'{path}:{number}')
                titles[chapter] = f'{titles[chapter]} {line_text}' if chapter in titles else line_text
            elif marker in HEADINGS:
                continue
            elif marker in BREAKS:
                if rest:
                    buffer.append(rest)
            else:
                raise SystemExit(f'{path}:{number}: unknown marker \\{marker}')
    flush()
    return verses, titles


def jps_flow(jps, hebrew):
    """Split JPS at its Hebrew markers.

    Returns (segments, targets): segments[h] is the JPS text of Hebrew verse
    h; targets[e] is the ordered list of Hebrew verses English verse e feeds.
    Text runs into the current Hebrew verse until a marker moves it on.
    """
    segments, targets = {}, {}
    removed = 0
    for psalm in range(1, PSALM_COUNT + 1):
        english = sorted(v for p, v in jps if p == psalm)
        texts = {}
        for v in english:
            text = jps[(psalm, v)]
            if v == 1 and psalm in JPS_BOOK_PSALMS:
                text, n = JPS_BOOK.subn('', text)
                removed += n
            texts[v] = text

        marked = any(re.search(r'\(\d+-\d+\)', t) for t in texts.values())
        hebrew_count = sum(1 for p, _ in hebrew if p == psalm)
        if not marked and hebrew_count != len(english):
            cut = JPS_UNMARKED.get(psalm)
            if cut is None:
                raise SystemExit(f'JPS {psalm}: unmarked, but Hebrew has {hebrew_count} verses')
            title, found, rest = texts[1].partition(cut)
            if not found:
                raise SystemExit(f'JPS {psalm}: cut {cut!r} not found')
            texts[1] = f'({psalm}-1) {title}{cut} ({psalm}-2) {rest.strip()}'
            for v in english[1:]:
                texts[v] = f'({psalm}-{v + 1}) {texts[v]}'
            marked = True

        current = None
        for v in english:
            if not marked:
                current = (psalm, v)
                targets[(psalm, v)] = [current]
                segments[current] = texts[v]
                continue
            parts = re.split(r'\((\d+)-(\d+)\)', texts[v])
            fed = []
            lead = parts[0].strip()
            if lead:
                # Text ahead of any marker opens the psalm (81:1 has no
                # "(81-1)") or continues the verse already running (13:6).
                current = current or (psalm, 1)
                fed.append(current)
                segments[current] = (segments.get(current, '') + ' ' + lead).strip()
            for i in range(1, len(parts), 3):
                current = (int(parts[i]), int(parts[i + 1]))
                if current[0] != psalm:
                    raise SystemExit(f'JPS {psalm}:{v}: marker for psalm {current[0]}')
                fed.append(current)
                piece = parts[i + 2].strip()
                segments[current] = (segments.get(current, '') + ' ' + piece).strip()
            targets[(psalm, v)] = list(dict.fromkeys(fed))
    if removed != len(JPS_BOOK_PSALMS):
        raise SystemExit(f'JPS: removed {removed} book headings, expected {len(JPS_BOOK_PSALMS)}')
    return segments, targets


def recut(name, verses, titles, targets, hebrew):
    """Move an English-numbered text onto the Hebrew verses, splitting only at its title."""
    out = {h: [] for h in hebrew}
    empty = set()
    for (psalm, v), fed in sorted(targets.items()):
        text = verses[(psalm, v)]
        title = titles.get(psalm) if v == 1 else None
        if len(fed) == 1:
            # Hebrew counts the title inside verse 1 here, as English does.
            out[fed[0]].append(f'{title} {text}' if title else text)
            continue
        if title is None:
            raise SystemExit(f'{name} {psalm}:{v} spans {fed} but has no title to split at')
        out[fed[0]].append(title)
        out[fed[-1]].append(text)
        empty.update(fed[1:-1])
    return {h: ' '.join(parts) for h, parts in out.items()}, empty


def strip(text, marks):
    return ''.join(' ' if ord(c) == 0x05BE else c for c in text if ord(c) not in marks)


def check(data, empties):
    problems = []
    hebrew = data['he']
    psalms = sorted({p for p, _ in hebrew})
    if psalms != list(range(1, PSALM_COUNT + 1)):
        problems.append(f'psalms run {psalms[0]}..{psalms[-1]}')
    if len(hebrew) != HEBREW_VERSES:
        problems.append(f'Hebrew has {len(hebrew)} verses, expected {HEBREW_VERSES}')
    for psalm in psalms:
        numbers = sorted(v for p, v in hebrew if p == psalm)
        if numbers != list(range(1, len(numbers) + 1)):
            problems.append(f'Hebrew psalm {psalm}: verses are not 1..{len(numbers)}')

    for key, verses in data.items():
        if set(verses) != set(hebrew):
            problems.append(f'{key}: verse set differs from the Hebrew')
        allowed = empties.get(key, set())
        blank = sorted(h for h, t in verses.items() if not t.strip() and h not in allowed)
        if blank:
            problems.append(f'{key}: {len(blank)} unexpected empty verses, first {blank[:3]}')

    # The WEB's Psalm 119 acrostic headings are \qa paragraphs in USFM; none
    # may leak into a verse. It writes no other word in capitals there.
    leaked = [h for h, t in data['web'].items() if h[0] == 119 and re.search(r'\b[A-Z]{2,}\b', t)]
    if leaked:
        problems.append(f'web: Psalm 119 headings in verses {leaked[:3]}')

    agreement = {}
    for en_word, he_word, ar_word in ANCHORS:
        in_he = {
            h for h, t in hebrew.items()
            if any(re.fullmatch(he_word, w) for w in strip(t, HEBREW_MARKS).split())
        }
        for key in ('en', 'ar', 'kjv', 'web'):
            if key == 'ar':
                found = {h for h, t in data[key].items() if ar_word in strip(t, ARABIC_MARKS)}
            else:
                found = {h for h, t in data[key].items() if re.search(rf'\b{en_word}\b', t)}
            # A second title verse this text cannot fill says nothing either way.
            skip = empties.get(key, set())
            union = (in_he | found) - skip
            agree = len((in_he & found) - skip)
            agreement[(key, en_word)] = (agree, len(union))
            if agree < MIN_AGREEMENT * len(union):
                problems.append(f'{key}/{en_word}: anchors agree in {agree}/{len(union)} verses')
    return problems, agreement


def build(data, english):
    os.makedirs(OUT, exist_ok=True)
    for psalm in range(1, PSALM_COUNT + 1):
        numbers = sorted(v for p, v in data['he'] if p == psalm)
        with open(f'{OUT}/{psalm}.json', 'w', encoding='utf-8', newline='\n') as f:
            json.dump({
                'psalm': psalm,
                'verses': [
                    {
                        'verse': v,
                        'he': data['he'][(psalm, v)],
                        'ar': data['ar'][(psalm, v)],
                        'en': data['en'][(psalm, v)],
                        'kjv': data['kjv'][(psalm, v)],
                        'web': data['web'][(psalm, v)],
                        'english': english[(psalm, v)],
                    }
                    for v in numbers
                ],
            }, f, ensure_ascii=False, separators=(',', ':'))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--from', dest='source', required=True,
                    help='directory holding the unzipped hbo, engjps, arb-vd, '
                         'eng-kjv2006 and eng-web USFM exports')
    ap.add_argument('--check', action='store_true', help='verify only')
    args = ap.parse_args()

    raw = {key: read_usfm(find(args.source, code)) for key, code in TEXTS.items()}
    hebrew, _ = raw['he']
    english_count = {key: len(raw[key][0]) for key in ('en', 'ar', 'kjv', 'web')}
    wrong = {k: n for k, n in english_count.items() if n != ENGLISH_VERSES}
    if wrong:
        raise SystemExit(f'English-numbered texts with the wrong verse count: {wrong}')
    for key in ('ar', 'kjv', 'web'):
        if set(raw[key][0]) != set(raw['en'][0]):
            raise SystemExit(f'{key}: English verse references differ from JPS')

    segments, targets = jps_flow(raw['en'][0], hebrew)
    data = {'he': hebrew, 'en': {h: segments.get(h, '') for h in hebrew}}
    empties = {}
    for key in ('ar', 'kjv', 'web'):
        verses, titles = raw[key]
        data[key], empties[key] = recut(key, verses, titles, targets, hebrew)

    english = {h: set() for h in hebrew}
    for (psalm, v), fed in targets.items():
        for h in fed:
            english[h].add(v)
    english = {h: sorted(vs) for h, vs in english.items()}

    problems, agreement = check(data, empties)

    split = sorted(e for e, fed in targets.items() if len(fed) > 1)
    joined = sorted(h for h, vs in english.items() if len(vs) > 1)
    print(f'Hebrew verses        : {len(hebrew)}')
    print(f'English verses       : {english_count}')
    print(f'English verses split : {len(split)} (title cut from verse 1)')
    print(f'Hebrew verses joined : {joined}')
    print(f'empty after re-cut   : {sorted(empties["ar"])} (ar, kjv, web)')
    for (key, word), (agree, total) in agreement.items():
        print(f'pairing {key:<3} / {word:<5}: {agree}/{total} verses agree with the Hebrew')
    if problems:
        print('\nFAILED:')
        for p in problems:
            print(f'  {p}')
        return 1
    print('checks passed')

    if args.check:
        return 0
    build(data, english)
    print(f'wrote {PSALM_COUNT} psalms covering {len(hebrew)} verses to sources/psalms/')
    return 0


if __name__ == '__main__':
    sys.exit(main())
