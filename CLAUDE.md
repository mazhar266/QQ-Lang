# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

v1 is complete: lexer, parser, AST, error model, source registry, `Repository` cache, Quran resolver, hadith resolvers (B/M/AD/T/N/IM), Hisnul Muslim resolver, user-defined JSON sources, book-wide `B::100` numbering, full-text search, `qql` CLI, C ABI (`src/ffi.rs` + `include/qql.h`), Dart binding, fuzz targets, CI. 106 Rust tests plus 13 Dart tests pass; 130 with `--features vector,fulltext`. `docs/plan.md` is the spec (41 sections, 11 phases) and remains the authority on design.

The project was respecified from C to Rust. Anything that reads like C (CMake, manual frees, `qql_error_t` in core logic) is stale.

Deliberate deviations from the plan, all fine to revisit:

- `Source` has one `resolve` method, not `validate` + `resolve` — every check a dry run would do is the first thing `resolve` does, and nothing needs validation without resolution.
- No `thiserror`. `Error` hand-rolls `Display` alongside the `code()` match it needed anyway, keeping dependencies at `serde` + `serde_json`.
- `include/qql.h` is hand-written, not cbindgen output. `scripts/c-smoke.sh` compiles a C client against it under `-Werror` and links it to the real library, which catches drift harder than diffing generated text.

## Commands

```bash
cargo build --release
cargo test
cargo test --test parser                     # one integration test file
cargo test invalid_queries                   # one test by name
cargo run -- "Q:2:255"
cargo run -- --parse "Q:2:1-5,255;Q:1;"      # parse only, no data access
cargo run -- --data ./sources "B:1:1-3"
cargo run -- --sources
cargo run -- --data tests/fixtures/custom "X:1:2"   # user-defined source
```

Two optional search engines, both off by default. Both index sets are
committed, so a checkout searches without a build step:

```bash
python3 scripts/build-vectors.py                       # sources/vectors/*.qv  (committed, ~90 s, 21 MB)
cargo test --features vector
cargo run --features vector -- 'q:*"worship"~3'

cargo run --features fulltext --bin qql-index          # sources/fulltext/*/   (committed, ~3 s, 16 MB)
cargo test --features fulltext
cargo run --features fulltext -- 'q:?"mercy"~5'
```

Test every combination when touching search: default, `--features vector`,
`--features fulltext`, `--features vector,fulltext`.

FFI and parser work:

```bash
./scripts/c-smoke.sh                    # C header + link check; run after touching src/ffi.rs
cargo +nightly miri test --test ffi
cargo +nightly fuzz run parse
```

Dart binding:

```bash
cargo build --release --features vector,fulltext   # the binding loads target/release/libqql.so
cd bindings/dart && dart pub get && dart test
```

**The feature set is baked into `libqql.so`.** A plain `cargo build --release`
— which `scripts/c-smoke.sh` runs — overwrites it with a featureless one, and
the ranked engines then answer `QQL_UNSUPPORTED` through Dart and C until it
is rebuilt with the flags. The Dart test for ranked search accepts either
outcome for that reason; what it forbids is a silent fallback.

Note: `cargo fmt`, `cargo clippy`, doctests, cargo-fuzz, and Miri do **not** run in this environment — no rustup, clippy is not installed, `rustdoc` fails to load `libLLVM`. Use `cargo test --lib --bins --tests` to skip doctests. They still run in CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) and remain the contract.

`scripts/c-smoke.sh`, `gcc`, and `dart` **do** work here — use them to verify anything FFI-related.

## Architecture

```text
query → lexer → parser → AST → validation → source resolver → repository → serializer → UTF-8 String
```

The one rule that governs every decision:

```text
QQL parser knows syntax.
Source handlers know Islamic-book structure.
Repository knows storage.
FFI module knows the C ABI.
```

Consequences that are easy to get wrong:

- The parser knows only the grammar. It has no table of Surah counts and no `match source { "Q" => ... }`. `Q:500:999` and `XYZ:1:2` both parse cleanly; the Quran resolver rejects the first, the registry rejects the second. `tests/parser.rs` must pass without a `data/` directory.
- **Grouping:** an integer followed by `:` starts a new group, so `q:1:2,3,2:3,4-6` is Surah 1 ayat 2,3 *plus* Surah 2 ayat 3,4-6 — one written reference producing two `Reference` nodes. `Parser::group_follows` is the one-token lookahead that decides this; a range is never a primary, so `Q:1:1-5:3` errors.
- **Optional source:** `Reference::source` is `Option<String>`; `1,2:255` parses with `None`. The parser must not learn that the default is `Q` — `Registry::DEFAULT_CODE` owns that, and `Context::execute` substitutes it so resolvers always see a concrete code.
- **Sticky source:** a stated code carries forward to later references in the same query — `b:1:1;3` is Bukhari twice, `b:1:1;q:3` switches. The parser threads an `inherited: Option<String>` through `query()`, which is still pure syntax since "reuse the previous code" needs no knowledge of the codes. `None` therefore means *no code appeared anywhere earlier in the query*, and only then does the registry default apply.
- `;` separates *sources*, not references; commas already join groups under one source. A trailing `;` is optional.
- **Search:** `Q:1:3~5:"text"` sets `Reference::text`; `primary`/`ranges` then scope *where* to search rather than what to return. `~` (not `-`) marks a search scope, which is what keeps `Q:1:3-5` and `Q:1:3~5:"x"` apart. Execution lives in `Context::search` and is source-agnostic — it resolves the scope as an ordinary reference and filters, so every source gets search from one code path. `Source::total()` supplies the bound for the unscoped `Q:"text"` form; `None` means an unscoped search is refused with `QQL_UNSUPPORTED` rather than silently narrowed.
- A marker before the quote picks the engine — none exact, `?` full text, `*` similarity — and is lexed together with the quote, not as its own token. `*` and `?` only mark when a quote follows, so `Q:2*3` stays an invalid character. A term may be delimited by `"` or `'`, and the lexer closes it only with the quote it opened with — that is what lets `"Allah's"` and `'say "this"'` be written at all. No escapes.
- Matching tries the needle against a record's `ar` *and* `en`, so one term searches both languages. It is plain substring, not stemming: `"mercy"` does not match *Merciful*.
- [src/search.rs](src/search.rs) folds Arabic for comparison only — harakat, superscript alef, Quranic marks, tatweel, alef seats, `ى`/`ة`. Without it search is useless, since the text is fully diacritized and a typed needle would never match. Output text is never folded.
- **Vector search** lives in [src/vector.rs](src/vector.rs) behind the `vector` feature, off by default. `*"term"` sets `MatchKind::Similar`; `~N` after it caps results. Index files are `sources/vectors/{CODE}.qv`, generated by `scripts/build-vectors.py` and **committed** — regenerate and commit after changing text.
- The index is keyed by `(primary, number)`, so a scope filters during the scan and each hit resolves back through the ordinary reference path. That means the index never has to agree with resolvers about record *shape*, only about numbering.
- The embedder is a signed hash projection (words + character trigrams), duplicated in `src/vector.rs` and `scripts/build-vectors.py` — **the two must stay identical or every score is wrong**. Both fold text the same way as `src/search.rs`. It is fuzzy lexical, not semantic; swapping in real embeddings means a new embedder id in the header and a matching branch in `Index::embed`.
- 256 dims is the default for a reason: at 128 hash collisions outranked real matches on short queries (`q:1:` + a three-letter Arabic root). Measured, not guessed.
- Similarity is **the only ranked output in QQL** — results are score-ordered and carry `score` + `"ranked": true`. Weak hits are cut at an absolute floor and at half the top score, so a search may return fewer than its cap.
- Without the feature or without an index, a `*"…"` query returns `QQL_UNSUPPORTED`. It must never fall back to substring matching.
- **Full-text search** is [src/fulltext.rs](src/fulltext.rs) behind the `fulltext` feature (tantivy). `?"term"` sets `MatchKind::FullText`; the `?` is lexed together with the quote, not as its own token. Indexes are `sources/fulltext/{CODE}/`, built by the `qql-index` binary (`required-features`), and **committed** alongside the `.qv` vector indexes, so a checkout searches with no build step. Only tantivy's `.tantivy-*.lock` files are ignored. A rebuild renames every segment (they are UUIDs), so it is a large diff and each one lands in history permanently — batch it with other data changes.
- Three search spellings, three engines, chosen by the query and never by the build flags — `"term"` substring, `?"term"` tantivy, `*"term"` vectors. Two of them rank; `MatchKind::is_ranked()` is the one place that decides which.
- `fulltext::build` enumerates records through `Context::execute("CODE:1")`, `"CODE:2"`… rather than any privileged access, so it cannot disagree with the resolver about numbering: the *n*-th record of a primary is item *n*. It tolerates 5 missing primaries before stopping, so a numbering gap does not silently truncate an index.
- The tantivy index stores Arabic **folded** (`search::fold`) for the same reason vectors do, and English under `en_stem` — that stemmer is why `?"mercy"` finds *Merciful* where `"mercy"` cannot.
- Chapters that are not numbered — `introduction.json`, Nasa'i's `35b.json` — are unreachable by the grammar, so both index builders skip them. Document counts below the collection totals are correct, not truncation.
- `Reference::is_flat()` must stay `primary.is_none() && search.is_none()` — an unscoped search also has no primary but is not the `::` form.
- Adding a collection (`T` = Tirmidhi) means a new `src/sources/*.rs` with `impl Source`, one registry entry, one data directory — and zero lexer or parser edits. If a source change touches the parser, the design is being violated.
- The parser never reads files; the repository never parses queries.
- The AST is plain structs (`Query` / `Reference` / `Range`), never `serde_json::Value`. Deriving `Serialize` on it is fine; *building* it from JSON is not.
- `Reference` has no `select_all` field — empty `ranges` means "all". Expose it as `selects_all()`.
- `Reference::expand(max)` in [src/ast.rs](src/ast.rs) is the one place that does ordering, within-reference dedup, and bounds checking. Resolvers call it; they must not re-implement any of the three.
- `Repository` caches `Arc<dyn Any + Send + Sync>` keyed by path and downcasts on read, so it stays free of source-specific schemas. Schemas live next to their resolver.
- Data is read straight from the `sources/` submodules in their upstream layout — no ETL step, no `data/` copy. Quran: `quran/chapters/{surah}.json` — the one exception, generated by `scripts/build-quran.py` from Tanzil's Uthmani text because the submodule misuses three mark codepoints; see the README. Hadith: `hadith-json/db/by_chapter/the_9_books/{book}/{chapter}.json`. Hisnul Muslim: `Hisn-Muslim-Json/husn_en.json` (one file, all 132 chapters).
- Two addressing modes: `B:C:N` is the N-th hadith *within chapter C* (from `by_chapter/`), and `B::N` is the traditional book-wide number (from `by_book/`). `Reference::primary` is `Option<u32>`; `None` is the `::` form. The parser only records the omission — what it means is each resolver's business. Flat records carry `"numbering": "book"` so a mixed response stays unambiguous.
- **In `by_book/` files, `id` spans all nine books at once** (Bukhari 1..7277, Muslim 7278..14736, …); the per-book citation number is `idInBook`. Using `id` works for Bukhari and silently returns the wrong hadith everywhere else. A test asserts `X::1 == X:1:1` for all six collections.
- Sunan an-Nasa'i has a chapter numbered `35.2`, so chapter ids deserialize as `serde_json::Number`, not `u32`.
- `Q::N` uses `SURAH_VERSES` (114 entries) plus `locate()` in [src/sources/quran.rs](src/sources/quran.rs) to find the ayah's chapter file. There is deliberately no per-ayah directory: it was a second copy of the whole text, 6236 files and 25 MB, that could drift from the chapters. `AYAH_COUNT` is derived from the table by a `const fn`, and a test walks all 114 Surah boundaries against the data.
- `quran-json-arabic` is **not** a submodule — it is needed only to rebuild `sources/quran/`, so `scripts/build-quran.py` takes `--meta <dist/chapters/en>` and the README says how to fetch it. `hadith-json` and `Hisn-Muslim-Json` remain submodules because they are read at runtime.
- `HadithCollection` is one `Source` impl instantiated per collection. A new book in the nine is one line in `Registry::with_defaults`, not a new file.
- [src/sources/json.rs](src/sources/json.rs) adds sources from a `SourceSpec` — a JSON description of paths and field mappings — so users can register a collection without writing Rust. `qql-sources.json` in the data directory is read on the **first query**, not in `Context::new`; that keeps construction infallible and makes custom sources work through the C ABI, which has no way to pass them in. Consequence: explicit `register_spec` calls land *before* the manifest, so the manifest wins unless `load_manifest()` is called first. Registry lookup is newest-first.
- Prefer a real `impl Source` over a spec when the data is irregular enough that a declarative mapping would need escape hatches.
- Hisnul Muslim chapters are stored **out of order** (array position 0 is chapter 27), so [src/sources/hisnul.rs](src/sources/hisnul.rs) looks them up by `ID`. Indexing by position silently returns the wrong supplication; a test pins this.
- The HM file also has a UTF-8 BOM, two objects with duplicate keys, and one misspelled field. The BOM is stripped in [src/repo.rs](src/repo.rs) (storage concern); the rest is absorbed by `Supplication`, which is a `serde_json::Map` newtype with accessors rather than a derived struct — serde's derive rejects duplicate keys outright. That is the one deliberate exception to "no `Value` in schemas", and it is documented in place.

Behavioral contracts that tests exist to pin down:

- Query order is preserved and never sorted — `Q:2:255,1-3` returns 255, 1, 2, 3. No `.sort()`, no `BTreeSet` for item expansion.
- Duplicates are eliminated *within* one reference (`Q:2:1-5,3,4` → 1..=5) via an order-preserving `HashSet` pass, but kept *across* references (`Q:2:255;Q:2:255;` → two items).
- Ranges are inclusive. Source codes normalize with `to_ascii_uppercase`. Whitespace around tokens is legal.
- Every return is valid JSON, including errors: `{"ok":false,"error":{"code":"QQL_...","message":...,"position":N}}`. `position` is a **byte** offset and is omitted when the variant has none. Wire codes come from an exhaustive `match` on `Error` with no wildcard arm.
- Arabic passes through byte-for-byte. No Unicode normalization in v1, and never `from_utf8_lossy` on scripture — reject invalid UTF-8 instead.
- A huge range (`Q:1:1-4294967295`) must not attempt a giant allocation. Bound expansion or resolve lazily.

## Safety and FFI

`#![deny(unsafe_code)]` crate-wide, with a single `#![allow(unsafe_code)]` inside [src/ffi.rs](src/ffi.rs). That module is the entire audit surface — ~230 lines, no query logic.

- Every `extern "C"` function wraps its body in `catch_unwind`. A panic unwinding across the boundary is UB.
- Null/invalid pointers and non-UTF-8 input return error JSON, never a crash.
- `CString::into_raw` out, `qql_free_string` → `CString::from_raw` back. `qql_version()` is the one function returning a static string the caller must *not* free.
- `Context::execute` takes `&mut self`, so the compiler prevents concurrent use. `Context` is `Send` (pinned by a test). C callers can share a pointer freely — hence the doc warning, since the compiler can't help there. `qql_execute()` uses a `OnceLock<Mutex<Context>>` reading `$QQL_DATA` (default `sources`), and recovers from lock poisoning with `unwrap_or_else(|e| e.into_inner())` rather than propagating a panic.
- `qql_context_execute` never returns null and never returns invalid JSON — null ctx, null query, and invalid UTF-8 all serialize into `{"ok":false}`. `tests/ffi.rs` pins each case.

Data files load lazily into a `Repository` cache owned by the `Context`, freed on drop.

## Style

Rust 2021, stable (nightly only for miri/fuzz). No `unwrap`/`expect` in library code — fine in tests and the CLI. No `as` casts on parsed input; integer overflow is `QQL_EXPECTED_NUMBER`, not a wrap or panic. Lexer tokens borrow `&str` slices from the query rather than allocating. Small functions, no giant `parse`. Exhaustive `match` wherever a new variant should force a review. `#![deny(missing_docs)]`.

Dependencies are deliberately few: `serde`, `serde_json`, `thiserror`. CLI arg parsing is `std::env::args` — no `clap` for two flags.

## License

GPL-3.0-or-later. Every source file starts with an `SPDX-License-Identifier: GPL-3.0-or-later` line and a copyright line; new files must too. Contributions are accepted under it (see `CONTRIBUTING.md`).
