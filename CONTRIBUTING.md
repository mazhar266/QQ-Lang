# Contributing to QQL

Contributions are welcome — bug reports, tests, new source resolvers, data fixes, docs.

## Ground rules

- QQL is licensed under **GPL-3.0-or-later**. By submitting a contribution you agree it is released under that license.
- Keep the layer boundaries intact:

  ```text
  QQL parser knows syntax.
  Source handlers know Islamic-book structure.
  JSON repository knows storage.
  Public API knows FFI.
  ```

  A change that puts source-specific logic in the lexer or parser will be rejected, no matter how small.
- Don't grow the FFI surface without a reason. New public symbols need a justification in the PR.

## Before you open a PR

```bash
cmake -S . -B build -DCMAKE_C_FLAGS="-Wall -Wextra -Wpedantic -Werror -fsanitize=address,undefined"
cmake --build build
ctest --test-dir build
```

- No compiler warnings.
- No sanitizer findings — leaks, use-after-free, and invalid reads/writes all count.
- New behavior comes with a test. Parser and resolver changes need both valid and invalid cases.

## Code style

- C11. `-Wall -Wextra -Wpedantic` clean.
- Small functions, descriptive names, `const` wherever it applies.
- Explicit error returns (`qql_error_t`), never magic values.
- No global mutable state. Everything lives in `qql_context_t`.
- Every allocation has one clear owner; document it in a comment when it isn't obvious.
- Never use `strcpy`, `sprintf`, or `gets`. Bounded or dynamically sized operations only.
- Parser and lexer code must stay bounds-safe and deterministic — it should be fuzzable.

## Adding a source

See [Adding a new source](README.md#adding-a-new-source) in the README. It should touch `src/sources/`, `src/source_registry.c`, `CMakeLists.txt`, `data/`, and nothing else.

## Text data

Arabic is authoritative and passes through unchanged — no normalization of tashkeel, Quranic marks, or punctuation. Data corrections need a source reference (edition, publisher, page) in the PR description.

## Commits and PRs

- One logical change per PR. Match the phase structure in [docs/plan.md](docs/plan.md) where it applies.
- Say what you changed, why, and how you tested it.
- Open an issue first for anything that changes the grammar, the result JSON shape, or the public header.

## Reporting bugs

Include the query string, the output you got, the output you expected, and your platform and compiler. Parser bugs should come with the shortest query that reproduces them.
