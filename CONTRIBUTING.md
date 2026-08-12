# Contributing to QQL

Contributions are welcome — bug reports, tests, new source resolvers, data fixes, docs.

## Ground rules

- QQL is licensed under **GPL-3.0-or-later**. By submitting a contribution you agree it is released under that license. New source files start with:

  ```rust
  // SPDX-License-Identifier: GPL-3.0-or-later
  // Copyright (C) 2026 Mazhar Ahmed
  ```
- Keep the layer boundaries intact:

  ```text
  QQL parser knows syntax.
  Source handlers know Islamic-book structure.
  Repository knows storage.
  FFI module knows the C ABI.
  ```

  A change that puts source-specific logic in the lexer or parser will be rejected, no matter how small.
- Don't grow the FFI surface without a reason. New `extern "C"` symbols need a justification in the PR, and they change the ABI — which is public API for semver purposes.

## Before you open a PR

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

All three must be clean. If you touched `src/ffi.rs`:

```bash
cargo +nightly miri test --test ffi
./scripts/c-smoke.sh
```

`include/qql.h` is hand-written and updated by hand. `scripts/c-smoke.sh` is what keeps it honest — it compiles a C client against the header under `-Wall -Wextra -Werror` and links it to the real library, so a drifted signature fails to build rather than shipping silently. That is a stronger check than diffing generated text, which is why there is no cbindgen step.

New behavior comes with a test. Parser and resolver changes need both valid and invalid cases, and error tests assert the specific `Error` variant and its position — not just `is_err()`.

## Code style

- Rust 2021, stable toolchain. Nightly is only for `miri` and `cargo-fuzz`.
- `#![deny(unsafe_code)]` is crate-wide. `unsafe` is allowed **only** in `src/ffi.rs`, and every block there needs a comment saying why it is sound.
- No `unwrap()` or `expect()` in library code. Fine in tests and in the CLI binary.
- No `panic!` reachable from a public API — and none that can reach the FFI boundary. Every `extern "C"` function catches unwinds.
- No `as` casts on parsed input. Use `parse` / `TryFrom` and handle the error; integer overflow is a `QQL_EXPECTED_NUMBER`, not a wrap or a panic.
- Prefer borrowing over cloning. The lexer borrows slices of the query; don't make it allocate.
- Prefer exhaustive `match` over `_` wildcards where adding a variant should force a review — the error-code mapping especially.
- Public items are documented (`#![deny(missing_docs)]`). Rustdoc is the API reference; don't duplicate it in the README.
- Small functions. No giant `parse`.

## Behavioral contracts

These are easy to "fix" into breakage. Tests pin them; don't work around a failing one without reading [docs/plan.md](docs/plan.md) first.

- Query order is preserved. No `.sort()`, no `BTreeSet` for item expansion.
- Duplicates are removed *within* a reference, kept *across* references.
- Every public entry point returns valid JSON, including on error.
- Arabic is byte-for-byte identical from data file to output. Never `from_utf8_lossy` on scripture.

## Adding a source

First check whether it needs code at all: a collection whose JSON maps cleanly onto paths and field names is a `SourceSpec` entry in a manifest, no Rust required — see [Adding a source without writing Rust](README.md#adding-a-source-without-writing-rust).

Write an `impl Source` when the data is irregular enough that a mapping would need escape hatches. It should touch `src/sources/`, `src/registry.rs`, and nothing else. If your diff touches `lexer.rs` or `parser.rs`, something is wrong with the approach.

## Fuzzing

The parser must never panic or hang on any input:

```bash
cargo +nightly fuzz run parse
```

If you change the lexer or parser, run it for a few minutes before opening the PR. Crashes found by the fuzzer are welcome as issues with the reproducing input attached.

## Text data

Arabic is authoritative and passes through unchanged — no normalization of tashkeel, Quranic marks, or punctuation. Data corrections need a source reference (edition, publisher, page) in the PR description.

## Commits and PRs

- One logical change per PR. Match the phase structure in [docs/plan.md](docs/plan.md) where it applies.
- Say what you changed, why, and how you tested it.
- Open an issue first for anything that changes the grammar, the result JSON shape, the `Source` trait, or the C ABI.

## Reporting bugs

Include the query string, the output you got, the output you expected, and your platform and Rust version (`rustc -V`). Parser bugs should come with the shortest query that reproduces them.
