# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

v1 is complete: lexer, parser, AST, error model, source registry, `Repository` cache, Quran resolver, hadith resolvers (B/M/AD/T/N/IM), Hisnul Muslim resolver, `qql` CLI, C ABI (`src/ffi.rs` + `include/qql.h`), Dart binding, fuzz targets, CI. 50 Rust tests plus 9 Dart tests pass. `docs/plan.md` is the spec (41 sections, 11 phases) and remains the authority on design.

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
```

FFI and parser work:

```bash
./scripts/c-smoke.sh                    # C header + link check; run after touching src/ffi.rs
cargo +nightly miri test --test ffi
cargo +nightly fuzz run parse
```

Dart binding:

```bash
cargo build --release                    # the binding loads target/release/libqql.so
cd bindings/dart && dart pub get && dart test
```

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

- The parser knows only `IDENT : INT ( : ranges )`. It has no table of Surah counts and no `match source { "Q" => ... }`. `Q:500:999` and `XYZ:1:2` both parse cleanly; the Quran resolver rejects the first, the registry rejects the second. `tests/parser.rs` must pass without a `data/` directory.
- Adding a collection (`T` = Tirmidhi) means a new `src/sources/*.rs` with `impl Source`, one registry entry, one data directory — and zero lexer or parser edits. If a source change touches the parser, the design is being violated.
- The parser never reads files; the repository never parses queries.
- The AST is plain structs (`Query` / `Reference` / `Range`), never `serde_json::Value`. Deriving `Serialize` on it is fine; *building* it from JSON is not.
- `Reference` has no `select_all` field — empty `ranges` means "all". Expose it as `selects_all()`.
- `Reference::expand(max)` in [src/ast.rs](src/ast.rs) is the one place that does ordering, within-reference dedup, and bounds checking. Resolvers call it; they must not re-implement any of the three.
- `Repository` caches `Arc<dyn Any + Send + Sync>` keyed by path and downcasts on read, so it stays free of source-specific schemas. Schemas live next to their resolver.
- Data is read straight from the `sources/` submodules in their upstream layout — no ETL step, no `data/` copy. Quran: `quran-json-arabic/dist/chapters/en/{surah}.json`. Hadith: `hadith-json/db/by_chapter/the_9_books/{book}/{chapter}.json`. Hisnul Muslim: `Hisn-Muslim-Json/husn_en.json` (one file, all 132 chapters).
- Hadith numbering: `B:C:N` is the N-th hadith *within chapter C*, matching the upstream per-chapter files. That is not the book-global citation number, which lives in `by_book/`. Documented in [src/sources/hadith.rs](src/sources/hadith.rs).
- `HadithCollection` is one `Source` impl instantiated per collection. A new book in the nine is one line in `Registry::with_defaults`, not a new file.
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
