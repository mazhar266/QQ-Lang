# Changelog

QQL's lineage, by what each version taught the language to do:

| Version | Adds | Query |
| --- | --- | --- |
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
