# Changelog

QQL's lineage, by what each version taught the language to do:

| Version | Adds | Query |
| --- | --- | --- |
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
