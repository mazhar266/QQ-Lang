# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

Docs only — no source, no Cargo project, no commits yet. `docs/plan.md` is the full spec (41 sections, 11 phases) and is the authority on design; `README.md` documents the API surface as it is meant to end up. Implementation follows §36 phase order, one reviewable commit per phase, starting with §41: Cargo project, module skeleton, error model, lexer, parser, AST, parser tests, and a `qql-parse` binary that prints the normalized query. Explicitly **not** in that first step: Quran JSON resolution, the source registry, or the FFI layer.

The project was respecified from C to Rust. If anything still reads like C (CMake, manual frees, `qql_error_t` return codes in core logic), it is stale — the plan is the current word.

## Commands

Nothing exists to run yet. Once the crate lands:

```bash
cargo build --release
cargo test
cargo test --test parser                     # single integration test file
cargo test parse_range_inclusive             # single test by name
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run --bin qql -- "Q:2:255"
cargo run --bin qql -- --data ./data "Q:1;Q:2:255"
```

FFI work additionally needs:

```bash
cargo +nightly miri test --test ffi
cargo +nightly fuzz run parse
cbindgen --config cbindgen.toml --output include/qql.h   # committed; CI diffs it
```

Clippy-clean under `-D warnings` and `cargo fmt --check` are part of the build contract, not optional polish.

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

Behavioral contracts that tests exist to pin down:

- Query order is preserved and never sorted — `Q:2:255,1-3` returns 255, 1, 2, 3. No `.sort()`, no `BTreeSet` for item expansion.
- Duplicates are eliminated *within* one reference (`Q:2:1-5,3,4` → 1..=5) via an order-preserving `HashSet` pass, but kept *across* references (`Q:2:255;Q:2:255;` → two items).
- Ranges are inclusive. Source codes normalize with `to_ascii_uppercase`. Whitespace around tokens is legal.
- Every return is valid JSON, including errors: `{"ok":false,"error":{"code":"QQL_...","message":...,"position":N}}`. `position` is a **byte** offset and is omitted when the variant has none. Wire codes come from an exhaustive `match` on `Error` with no wildcard arm.
- Arabic passes through byte-for-byte. No Unicode normalization in v1, and never `from_utf8_lossy` on scripture — reject invalid UTF-8 instead.
- A huge range (`Q:1:1-4294967295`) must not attempt a giant allocation. Bound expansion or resolve lazily.

## Safety and FFI

`#![deny(unsafe_code)]` crate-wide, with a single `#[allow(unsafe_code)]` on `mod ffi`. That module is the entire audit surface.

- Every `extern "C"` function wraps its body in `catch_unwind`. A panic unwinding across the boundary is UB.
- Null/invalid pointers and non-UTF-8 input return error JSON, never a crash.
- `CString::into_raw` out, `qql_free_string` → `CString::from_raw` back. `qql_version()` is the one function returning a static string the caller must *not* free.
- `Context::execute` takes `&mut self`, so the compiler prevents concurrent use. `Context` is `Send`, not forced `Sync`. C callers can share a pointer freely — that must be documented, since the compiler can't help there. `qql_execute()` uses a `OnceLock<Mutex<Context>>` default context and must survive lock poisoning.
- The committed `include/qql.h` is cbindgen output. Drift is an ABI bug.

Data files load lazily into a `Repository` cache owned by the `Context`, freed on drop.

## Style

Rust 2021, stable (nightly only for miri/fuzz). No `unwrap`/`expect` in library code — fine in tests and the CLI. No `as` casts on parsed input; integer overflow is `QQL_EXPECTED_NUMBER`, not a wrap or panic. Lexer tokens borrow `&str` slices from the query rather than allocating. Small functions, no giant `parse`. Exhaustive `match` wherever a new variant should force a review. `#![deny(missing_docs)]`.

Dependencies are deliberately few: `serde`, `serde_json`, `thiserror`. CLI arg parsing is `std::env::args` — no `clap` for two flags.

## License

GPL-3.0-or-later. New files carry that license; contributions are accepted under it (see `CONTRIBUTING.md`).
