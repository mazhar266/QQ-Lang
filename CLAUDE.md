# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

Docs only — no source, no CMake, no commits yet. `docs/plan.md` is the full spec (41 sections, 11 phases) and is the authority on design; `README.md` documents the API surface as it is meant to end up. Implementation follows §36 phase order, one reviewable commit per phase, starting with §41: CMake, public header, error model, lexer, parser, AST, parser tests, and a `qql-parse` CLI that prints the normalized query. Explicitly **not** in that first step: Quran JSON resolution.

## Commands

Nothing exists to run yet. Once the CMake project lands:

```bash
cmake -S . -B build
cmake --build build
ctest --test-dir build
ctest --test-dir build -R test_parser        # single test
./build/qql "Q:2:255"
./build/qql --data ./data "Q:1;Q:2:255"
```

Development builds must be warning-free under `-Wall -Wextra -Wpedantic -Werror`, and tests must be sanitizer-clean:

```bash
cmake -S . -B build -DCMAKE_C_FLAGS="-Wall -Wextra -Wpedantic -Werror -fsanitize=address,undefined"
```

## Architecture

```text
query → lexer → parser → AST → validation → source resolver → JSON repository → serializer → UTF-8 string
```

The one rule that governs every decision:

```text
QQL parser knows syntax.
Source handlers know Islamic-book structure.
JSON repository knows storage.
Public API knows FFI.
```

Consequences that are easy to get wrong:

- The parser knows only `IDENT : INT ( : ranges )`. It has no table of Surah counts and no `if (source == "Q")`. `Q:500:999` and `XYZ:1:2` both parse cleanly; the Quran resolver rejects the first, the source registry rejects the second.
- Adding a collection (`T` = Tirmidhi) means a new `src/sources/*.c` implementing `validate`/`resolve`, one registry entry, one CMake line — and zero lexer or parser edits. If a source change touches the parser, the design is being violated.
- The parser never reads data files; the loader never parses queries.
- AST is C structs (`qql_query_t` / `qql_reference_t` / `qql_range_t`), never JSON. JSON is only I/O, isolated under `src/json/`.

Behavioral contracts that tests exist to pin down:

- Query order is preserved and never sorted — `Q:2:255,1-3` returns 255, 1, 2, 3.
- Duplicates are eliminated *within* one reference (`Q:2:1-5,3,4` → 1..5) but kept *across* references (`Q:2:255;Q:2:255;` → two items).
- Ranges are inclusive. Source codes normalize to uppercase. Whitespace around tokens is legal.
- Every return is valid JSON, including errors: `{"ok":false,"error":{"code":"QQL_...","message":...,"position":N}}`. Error codes are an internal enum serialized to stable strings.
- Arabic passes through byte-for-byte. No Unicode normalization in v1 — not tashkeel, Quranic marks, or zero-width characters.

## Memory and FFI

`qql_context_execute()` allocates; the caller frees with `qql_free_string()`, never the host allocator. `qql_context_t` is opaque and is the only thing that holds mutable state — no globals. Separate contexts are thread-safe; one context shared across threads is not. `qql_execute()` wraps a shared default context.

Data files load lazily and stay cached in the context until `qql_context_destroy()`.

## Style

C11. Small functions, no giant parser function. `const` aggressively. Explicit `qql_error_t` returns over magic values. Never `strcpy`/`sprintf`/`gets` — the lexer and parser must stay bounds-safe and fuzzable, and must never read past the null terminator. Ownership is documented in comments where it isn't obvious.

## License

GPL-3.0-or-later. New files carry that license; contributions are accepted under it (see `CONTRIBUTING.md`).
