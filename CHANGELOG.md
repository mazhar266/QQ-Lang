# Changelog

QQL's lineage, by what each version taught the language to do:

| Version | Adds | Query |
| --- | --- | --- |
| **[4.0](#400--the-bible-the-torah-and-the-psalms)** | the Bible, the Torah and the Psalms | `NT:JHN:3:16` |
| **[3.6](#360--spelling)** | search that survives spelling | `q:?"quran is easy"` |
| **[3.2](#320--ten-more-collections)** | ten more hadith collections | `RS:1:1` |
| **[3.0](#300--indexed-search)** | indexed full-text search | `q:1:?"mercy"` |
| **[2.0](#200--vector-search)** | vector similarity search | `q:1:*"worship"` |
| **[1.1](#110--text-search)** | plain text search | `q:1:"الحمد"` |
| **[1.0](#100--referencing)** | referencing | `Q:2:255` |

Versions follow semantic versioning. **The C ABI is part of the public API**
for that purpose, so a change to `include/qql.h` is a major bump.

> These are recorded after the fact: the work happened in sequence but no tags
> were cut at the time, so 3.0.0 is the first tagged release. Everything below
> ships in it.

---

## 4.0.0 — The Bible, the Torah and the Psalms

The scriptures before the Quran: the Torah, the Psalms and the rest of the
Bible, each with the text it was written in. A query names a book now, so the
grammar grew a path — which is the only reason this is a major version. No
query changes meaning and the C ABI is untouched; the Rust `Reference` struct
gained a field (see *Compatibility*).

### Added

- **Book names in the grammar.** A path of names may follow a stated code —
  the book in `OT:GEN:1:1`, the testament and book in `BI:OT:GEN:1:1` — and a
  name may start with digits, as in `NT:1JN:4:8`. Names are legal only right
  after a code, where an identifier was always a syntax error, so no query
  without one changes meaning. The parser still knows no names; each source
  says what its names mean.
- **`BI`, `OT`, `NT` — the Bible**, the 66 books of the Protestant canon in
  English numbering:
  - `ar` is the Smith & Van Dyck Arabic, `en` the KJV, `en_web` the WEB;
  - `OT:GEN:1:1`, `NT:JHN:3:16`, `BI:NT:JHN:3:16`, or `BI:JHN:3:16` without
    the testament;
  - `OT:GEN` is a whole book, `BI:OT` a whole testament, and `OT:GEN::32` and
    `NT::1` count book-wide;
  - books are named by USFM code, eBible's variant or English name.
- **`TO` — the Torah (Tawrat), the Jewish edition.** The five books of Moses
  in Hebrew numbering, with the Hebrew as `he`, JPS 1917 as `en`, and the Van
  Dyck, KJV and WEB re-cut to fit. The Torah's books also answer to their
  Hebrew names: `TO:BERESHIT:1:1`.
- **The original languages.** Every Bible verse carries the original it was
  translated from:
  - Old Testament verses: `he`, the Hebrew Masoretic text, with its Aramaic
    passages;
  - New Testament verses: `grc`, the Greek Textus Receptus behind the KJV.

  The Hebrew is placed on the English verses through the same JPS-derived map
  the Torah uses (`scripts/hebrew.py`), so `OT:GEN:31:55` carries Hebrew 32:1.
- **`bible_ref`** on every `TO` and `PS` record: a query for the same passage
  in `OT`, since the numberings differ. `TO:GEN:32:1` carries
  `OT:GEN:31:55`, and `PS:3:2` carries `OT:PSA:3:1`.
- **Ranked search across books.** Bible and Torah indexes are keyed by the
  book-wide verse number (`IndexKeys::Flat`), so a testament, a book or a
  chapter is one run of it. `OT` and `NT` search the Bible's index as windows
  onto it, so `OT:?"covenant"` and `BI:OT:?"covenant"` rank exactly alike.
- `scripts/build-bible.py` and `scripts/build-torah.py`, sharing a USFM
  converter, `scripts/usfm.py`, with `build-psalms.py`. Each refuses to write
  unless its texts pair verse for verse.
- Vector and full-text indexes for `BI` and `TO`: 17 MB and 5.2 MB, and
  3.3 MB and 1 MB. No other index was rebuilt.
- **`PS`, the Psalms — the Zabur** (aliases `ZABUR`, `PSALMS`), in the
  Jewish edition. `PS:23:1` is Psalm 23, verse 1; `PS::N` counts the book's
  2527 verses end to end, so an unscoped `PS:"..."` searches all of it.
- **Five public-domain texts per verse:**
  - the Hebrew Masoretic text as `he`;
  - JPS 1917 as `en`;
  - the Smith & Van Dyck Arabic as `ar`;
  - the King James Version and the World English Bible as `en_kjv` and
    `en_web`.

  Search covers `ar` and `en`.
- `scripts/build-psalms.py` generates `sources/psalms/` from eBible.org's
  USFM exports. It refuses to write unless every Hebrew verse is filled and
  *Selah* and *David* fall in the same verses in all five texts.
- Vector and full-text indexes for `PS`. No other index was rebuilt.

### Notes

- **Verses follow the Hebrew numbering**, where a psalm's title is a verse of
  its own: `PS:3:1` is Psalm 3's title and its prayer begins at `PS:3:2`.
  English Bibles number the Psalter's 2527 verses as 2461. Every record's
  `english_verses` gives their number(s), so either citation can be followed.
- The translations are English-numbered upstream and are re-cut to the
  Hebrew verses: JPS at the Hebrew numbers it carries inline, the others at
  the psalm title. Where a title spans two Hebrew verses (Psalms 51, 52, 54,
  60), the second is empty in `ar`, `en_kjv` and `en_web`.
- **The Torah and the Psalms each exist twice, on purpose.** `OT:GEN`…`OT:DEU`
  and `OT:PSA` are the Christian edition in English numbering; `TO` and `PS`
  are the Jewish edition in Hebrew numbering. They differ in ten places in
  the Torah and in most titled psalms — `bible_ref` links each verse across.
- **JPS's Ten Commandments.** JPS numbers Exodus 20 and Deuteronomy 5 by the
  "lower" accents, joining commandments 6–9 into one verse; the Hebrew text
  `TO` follows keeps them apart, and so does `TO`.

### Compatibility

- Every query that worked before returns exactly what it did.
- Every query that failed still fails with the same error code at the same
  byte. A name on a source without books — `Q:A`, `Q:1SA` — reports what the
  parser reported before names existed, even when a later syntax error
  follows. This was checked against the previous release on 292,284 generated
  queries, with no difference.
- `qql::parse` itself, and so `qql --parse`, now accepts a name after any code
  (`Q:GEN` parses, and the Quran refuses it at execution). Executing the query
  fails as before.
- `qql --parse` prints the names as `path` (`"path":["OT","GEN"]`). A query
  without names prints exactly what it did.
- **Rust API:** `Reference` gains a public `path` field, so code that builds a
  `Reference` with a struct literal must add `path: Vec::new()`. `Source`
  gains only default methods (`has_books`, `index_keys`, `flat_offset`,
  `flat_scope`), so custom sources compile unchanged.
- The C ABI, `include/qql.h`, the Dart binding and the wire format are
  unchanged.

---

## 3.6.0 — Spelling

Search stopped depending on the reader spelling a word the way the corpus
does. Every fix below is one a query could not work around.

### Added

- **Emlaei alongside Uthmani.** Each ayah now carries `emlaei`, the simplified
  spelling, beside the mushaf text in `ar`. The two differ in the consonantal
  skeleton and not merely in marks — `ٱلصَّلَوٰةَ` against `الصلاة`, `ٱلْكِتَٰب` against
  `الكتاب` — so no amount of folding bridges them, and a search typed in modern
  orthography returned **nothing** for some of the commonest words in the
  text. `q:"الكتاب"` now finds 162 ayat where it found none. Merged in by
  `scripts/build-quran.py` from the committed `sources/hafs_smart_v8.json`,
  which refuses to write unless every ayah's spelling aligns with its Uthmani
  one. `ar` is unchanged, byte for byte.
- **A transliteration alias table**, expanded at query time: `q:"koran"`
  reaches `Qur'an`, `q:"ibrahim"` reaches *Abraham*. About thirty curated
  groups, deliberately not general — collision-prone pairs (`lut`/`lot`) are
  left out.
- **Token weighting in the vector index** (embedder id 2). Tokens are scaled
  by inverse document frequency, a whole word outweighs one of its own
  trigrams three to one, and stopwords are zeroed. Previously *is* weighed
  what *quran* weighed. The weight table ships inside the `.qv`, keyed by
  token hash so no vocabulary is needed — about 0.3 MB per source.

### Fixed

- **Apostrophes no longer split a word in two.** Every tokenizer here breaks
  on non-alphanumerics, so `Qur'an` indexed as `qur` + `an` and `?"quran"`
  matched neither half. The corpus is not even self-consistent: it writes
  `qur'an` 199 times and `qur’an` 50, `rak'ahs` beside `rak’ahs`, and `` `Asr ``
  with a backtick. `fold` now drops the whole class.
- **English is indexed folded**, like every other field. Folding only the
  query would not have helped — the mismatch was on both sides.
- **Full-text terms are combined with `OR`, not `AND`.** One word the corpus
  spelled differently used to answer a whole phrase with zero results. BM25
  ranks partial matches instead; an explicit `AND` in the term still means
  `AND`.

Together these turn `q:?"quran is easy"` from **no results at all** into
54:17, 54:22 and 54:40 — the ayat that say it.

### Changed

- A *plain* term — no phrase, boolean or field syntax — has its stopwords
  dropped and its aliases expanded before parsing. Terms carrying syntax pass
  through untouched, so `?'"the straight path"'` still means what it says.
- Both index sets were rebuilt: vectors 32 MB, full-text 23 MB.

### Notes

- Vector search remains **fuzzy lexical, not semantic**. Tiers 3 and 4 of that
  work — static token embeddings, and a real sentence model — are recorded in
  [docs/todo.md](docs/todo.md) rather than attempted.
- Embedder id 1 files still parse and query exactly as before.
- 3.3 through 3.5 were never cut; this release carries that work.

---

## 3.2.0 — Ten more collections

### Added

- **Ten hadith collections**, taking the total to sixteen: `MA` Muwatta Malik,
  `DA` Sunan ad-Darimi, `RS` Riyad as-Salihin, `BM` Bulugh al-Maram, `AM`
  Al-Adab Al-Mufrad, `MK` Mishkat al-Masabih, `SM` Ash-Shama'il, and the three
  forties `NW`, `QD`, `SW`.
- Canonical numbering maps for the Muwatta and the three forties, so `MA::1858`
  and `NW::42` resolve like `B::6403`.

### Changed

- **Six collections are addressable by chapter only.** `DA`, `RS`, `BM`, `AM`,
  `MK` and `SM` have no citation numbering QQL can source: the data carries a
  position within the chapter and nothing more, and that position is *not*
  what these works are cited by. `CODE::N` is refused with `QQL_UNSUPPORTED`
  rather than answered from a number that would return the wrong hadith.
- Musnad Ahmad is deliberately **not** carried — upstream has 8 of its musnads
  and 1,374 of roughly 27,000 hadiths.
- `DA` has no English translation upstream; all 2,757 records have an empty
  `en`.

---

## 3.1.0 — Canonical hadith numbering

`B::N` now resolves the numbers the world actually cites.

### Fixed

- **`B::6403` returns what every hadith site calls Bukhari 6403.** The flat
  form previously exposed the dataset's private sequential numbering, which
  drifts from the canonical editions by up to ~300 — silently returning a
  *neighboring, wrong* hadith for any number pasted from sunnah.com or a
  printed edition. It now resolves through committed maps
  (`sources/canonical/*.json`) built from the public-domain
  fawazahmed0/hadith-api dataset: 'Abd al-Baqi numbering for Bukhari,
  Dar-us-Salam for Muslim, sunnah.com reference numbers for the rest. Every
  mapping is validated against the local text at build time.

### Changed

- The canonical space has holes — front matter (Muslim's Muqaddima owns
  canonical 1–92) and lettered variants (1771.5). Alone they error with a
  clear message; inside a range they are skipped.
- The 60 MB `by_book/` files are no longer read, and release bundles shrink
  by that much (212 → 161 MB staged).
- **No more git submodules.** The six hadith collections and the Hisnul
  Muslim file are committed directly (`sources/hadith/`,
  `sources/hisnul-muslim/`, with attribution), so a plain `git clone` is a
  complete working installation — no `--recurse-submodules`, no network at
  build time.

---

## 3.0.0 — Indexed search

Ranked full-text search over a real inverted index, behind the `fulltext`
feature.

### Added

- **`?"term"`** — ranked full-text search backed by
  [tantivy](https://github.com/quickwit-oss/tantivy), off by default.
  - English stemming, so `?"mercy"` finds *Merciful* where `"mercy"` cannot.
  - BM25 ranking, best first.
  - The engine's own syntax inside the term: `?"prayer AND charity"`,
    `?"prayer -charity"`, `?'"straight path"'`.
  - Arabic indexed folded, English under `en_stem`.
- `qql-index` binary, which builds the indexes.
- Both index sets are committed, so a checkout searches with no build step.
- Vector indexes for all eight sources, not just the Quran.

### Changed

- **Breaking: similarity is now `*"term"`, not `` `term` ``.** Backticks are
  command substitution in bash, which made the old spelling hazardous to type
  from a shell. The marker now matches the full-text one — a prefix before an
  ordinary quote — and either quote works after it: `*'term'` too.
- A bare marked term now starts a reference, so `?"mercy"` and `*"mercy"`
  default to the Quran the way `"mercy"` always did.

### Errors

`QQL_UNSUPPORTED` now also covers a missing `fulltext` feature or index.

---

## 2.0.0 — Vector search

Similarity search: find text that reads like the query rather than text that
contains it. Behind the `vector` feature, off by default.

### Added

- **`*"term"`** (written `` `term` `` at the time) — ranked by vector
  similarity.
- `~N` caps the results; the default is 20.
- Ranked hits carry `score` and `"ranked": true`. **This is the first QQL
  output ordered by relevance rather than position**, and it is marked as such
  so a mixed response stays readable.
- Weak hits are dropped, at an absolute floor and at half the top score, so a
  ranked search can return fewer than its cap or nothing at all.
- `scripts/build-vectors.py`, and `Source::total()` so an unscoped search
  knows how far the collection runs.

### Design

The embedder is a signed hash projection of folded tokens — words plus
character trigrams — needing no model and no asset beyond the index. That
makes it fuzzy lexical rather than semantic, which is the honest description:
it tolerates diacritics and Arabic affixes but does not know that *charity*
and *zakat* are related. Real embeddings are a build-time swap.

There is no approximate-nearest-neighbour index. At ~40,000 records a flat
`int8` scan is fast enough and cannot drift from the text.

### Errors

- `QQL_UNSUPPORTED` — the feature or index is missing. A ranked query is
  refused rather than quietly falling back to substring matching.

---

## 1.1.0 — Text search

Searching the text, not just addressing it.

### Added

- **`"term"`** and **`'term'`** — folded substring search over `ar` and `en`
  together, so one term searches both languages.
  - Either quote delimits a term, and each carries the other verbatim:
    `"Allah's"`, `'say "this"'`. No escapes.
- Scoping: `q:"term"` the whole collection, `q:1:"term"` one primary,
  `q:1:3~5:"term"` a range within it. `~` rather than `-` keeps a search scope
  apart from an ordinary selector.
- **Arabic folding for comparison only** — harakat, sukun, superscript alef,
  Quranic marks, tatweel, alef seats, `ى`/`ة`, ASCII case. Without it search
  would be useless on a fully diacritized corpus. Returned text keeps every
  mark.
- Search is source-agnostic: the scope resolves as an ordinary reference and
  the records are filtered, so every source gained search from one code path.

### Errors

- `QQL_EXPECTED_TEXT`, `QQL_UNTERMINATED_TEXT`.

---

## 1.0.0 — Referencing

The language for addressing Islamic texts, and everything needed to use it.

### The query language

- `SOURCE:PRIMARY:SELECTOR` — `Q:2:255`, `Q:1`, `B:1:1-10`, `HM:27`.
- Selectors: singles, inclusive ranges, and lists — `Q:2:1-5,10,20-25,255`.
- **Order preserved, never sorted.** Duplicates dropped *within* a reference,
  kept *across* references.
- **Groups** — `q:1:2,3,2:3,4-6` addresses two chapters in one reference. An
  integer followed by `:` starts a new group.
- **Omitted source** — `1`, `2:255`, `1,2:255` mean the Quran.
- **Sticky source** — a stated code carries forward: `b:1:1;3` is Bukhari
  twice, `b:1:1;q:3` switches.
- **Book-wide numbering** — `B::100`, `Q::100`, `HM::75`, the numbering
  citations use. Tagged `"numbering": "book"` so it cannot be confused with
  per-chapter numbering.
- Whitespace around tokens is legal; source codes are case-insensitive.

### Sources

Quran, Bukhari, Muslim, Abu Dawud, Tirmidhi, Nasa'i, Ibn Majah, Hisnul
Muslim — plus **user-defined sources** from a JSON spec, needing no Rust.

Text is read from `sources/` in each project's own layout: no ETL step, no
second copy. The Quran text is generated from Tanzil's Uthmani text, because
the alternative package spells three combining marks with codepoints that mean
something else.

### Interfaces

- `qql` CLI, JSON on stdout, exit code from `ok`.
- C ABI — six functions, `include/qql.h`, panics caught at the boundary,
  never null and never malformed JSON.
- Dart FFI binding.
- Rust crate: `Context`, `parse`, `Record`, `Source`.

### Guarantees

- Every response is valid JSON, errors included.
- Arabic passes through byte-for-byte. No Unicode normalization; invalid UTF-8
  is rejected rather than lossily replaced.
- `#![deny(unsafe_code)]` everywhere except the FFI module.
