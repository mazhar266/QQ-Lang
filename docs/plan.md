# Build Quran Query Language (QQL) — Rust Library

I want you to design and implement a small, portable Rust library called **QQL — Quran Query Language**.

The library will parse compact textual references to Islamic texts and resolve those references against local JSON data files.

The first version should support:

- Quran
- Sahih al-Bukhari
- Sahih Muslim
- Several additional Hadith collections later
- Hisnul Muslim
- Easy addition of more sources without rewriting the parser

The library must be written in **safe, portable Rust**, building on stable toolchains.

It should be usable both as:

- an idiomatic Rust crate (`qql = "0.1"`), and
- a C ABI shared/static library for everything else

through:

- Flutter / Dart FFI
- Linux
- Windows
- macOS
- Android NDK
- iOS native linking
- CLI applications
- Other languages through a C ABI

Do not couple the core library to Flutter, and do not let FFI concerns leak into the core types.

---

# 1. Core Idea

QQL allows references such as:

```text
Q:2:1-5,255;Q:1;Q:3:2;
```

Meaning:

```text
Q:2:1-5,255
Surah 2, ayat 1 through 5, plus ayah 255

Q:1
Entire Surah 1

Q:3:2
Surah 3, ayah 2
```

Future examples:

```text
B:1:1-10;
M:5:20;
HM:27;
```

Possible source identifiers:

```text
Q   = Quran
B   = Sahih al-Bukhari
M   = Sahih Muslim
AD  = Sunan Abi Dawud
T   = Jami' at-Tirmidhi
N   = Sunan an-Nasa'i
IM  = Sunan Ibn Majah
HM  = Hisnul Muslim
```

Do not hardcode parsing logic specifically for these sources.

The parser should understand generic source identifiers and numeric selectors.

A separate resolver should understand the meaning of each source.

---

# 2. Overall Architecture

Separate the crate into these layers:

```text
Input query (&str)
    ↓
Lexer / tokenizer
    ↓
Parser
    ↓
QQL AST / normalized query representation
    ↓
Validation
    ↓
Source resolver (trait object)
    ↓
JSON data repository
    ↓
Normalized result
    ↓
JSON serializer (serde_json)
    ↓
Returned UTF-8 String
```

Keep these concerns in separate modules.

The parser module must not touch the filesystem or `serde_json`.

The data loader must not contain query parsing logic.

Module visibility should enforce this: everything is private to the crate except the public API surface re-exported from `lib.rs`.

---

# 3. Query Grammar

Start with this conceptual grammar:

```text
query        := reference (';' reference)* ';'?

reference    := source ':' primary (':' selector)?

source       := identifier

primary      := integer

selector     := item (',' item)*

item         := integer
              | integer '-' integer

identifier   := [A-Za-z][A-Za-z0-9_]*

integer      := [0-9]+
```

> **As built:** the grammar grew three conveniences after v1, none of which
> needed a source handler to change:
>
> - `reference := (source ':')? body` — the source is optional. A stated code
>   carries forward to later references in the same query, so `b:1:1;3` is
>   Bukhari twice; only when no code appears anywhere earlier does the Quran
>   default apply. The parser does the carry-forward (pure syntax) while
>   `Registry` owns the default code and `Context` substitutes it.
> - `body := group (',' group)*` with `group := primary (':' selector)?` — one
>   source can address several chapters, as in `q:1:2,3,2:3,4-6`. The rule that
>   makes it unambiguous is that an integer followed by `:` is a primary, not a
>   selector item, decided by one token of lookahead.
> - `body := ':' selector` — `B::100`, book-wide numbering (§22).
>
> `;` therefore separates *sources* rather than references, since commas
> already join groups under one source.

Examples:

```text
Q:1
Q:2:255
Q:2:1-5
Q:2:1-5,255
Q:2:1-5,10,20-25,255
Q:1;Q:2:255;Q:112;
B:1:1-10
M:3:15
HM:27
```

Whitespace should be accepted around tokens where reasonable:

```text
Q:2:1-5, 255;
Q : 2 : 255;
```

Normalize source identifiers to uppercase (ASCII-only uppercasing — `str::to_ascii_uppercase`, never locale-dependent).

---

# 4. Abstract Syntax Tree

Create an internal AST similar to:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub source: String,
    pub primary: u32,
    pub ranges: Vec<Range>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub references: Vec<Reference>,
}
```

Note that `select_all` from a C design is not a separate field in Rust — an empty `ranges` vector already means "everything". Provide it as a method instead:

```rust
impl Reference {
    pub fn selects_all(&self) -> bool {
        self.ranges.is_empty()
    }
}
```

For:

```text
Q:2:1-5,255;Q:1;Q:3:2;
```

the logical representation should be equivalent to:

```json
[
  {
    "source": "Q",
    "primary": 2,
    "selectAll": false,
    "ranges": [
      {"from": 1, "to": 5},
      {"from": 255, "to": 255}
    ]
  },
  {
    "source": "Q",
    "primary": 1,
    "selectAll": true,
    "ranges": []
  },
  {
    "source": "Q",
    "primary": 3,
    "selectAll": false,
    "ranges": [
      {"from": 2, "to": 2}
    ]
  }
]
```

Do not use `serde_json::Value` as the parser AST.

Use plain Rust structs internally. Deriving `Serialize` on the AST is acceptable for debugging and for the `qql-parse` output, but the AST must never be *built* from JSON.

---

# 5. Data Model

Islamic texts will be stored locally as JSON files.

Design the loader so that file layout can evolve.

For Quran, a possible JSON representation could be:

```json
{
  "source": "Q",
  "surahs": [
    {
      "number": 1,
      "name_ar": "الفاتحة",
      "name_en": "Al-Fatihah",
      "ayat": [
        {
          "number": 1,
          "ar": "بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ",
          "en": "In the name of Allah..."
        }
      ]
    }
  ]
}
```

It is also acceptable to use separate files such as:

```text
data/
  quran/
    001.json
    002.json
    ...
  bukhari/
    001.json
    ...
  muslim/
    ...
  hisnul_muslim/
    ...
```

Define these shapes as `#[derive(Deserialize)]` structs, not as `serde_json::Value` trees — a malformed data file should fail at deserialization with a clear error, not deep inside the resolver.

> **As built:** one exception, in the Hisnul Muslim resolver. Two objects in
> that file repeat a key with different values, which serde's derive rejects as
> a duplicate field, so a derived struct cannot load the file at all. Only the
> innermost type is a `serde_json::Map` newtype with accessors; the levels
> above it stay typed, and the reason is documented at the definition.

Prefer a design that does not require loading the entire Quran or all Hadith collections into memory.

For the first implementation, source files may be loaded lazily.

---

# 6. Canonical Returned Result

The public API should return a UTF-8 JSON `String`.

At minimum, each resolved text item should provide:

```json
{
  "ar": "...",
  "en": "..."
}
```

However, design the actual response so useful metadata is preserved.

Preferred structure:

```json
{
  "ok": true,
  "query": "Q:2:1-5,255",
  "results": [
    {
      "source": "Q",
      "collection": "Quran",
      "primary": 2,
      "number": 1,
      "ar": "...",
      "en": "..."
    },
    {
      "source": "Q",
      "collection": "Quran",
      "primary": 2,
      "number": 2,
      "ar": "...",
      "en": "..."
    }
  ]
}
```

For multiple references:

```text
Q:1;Q:2:255;HM:27;
```

return all resolved items in the order requested.

Do not reorder references unless explicitly requested.

---

# 7. Rust API

The primary API is the Rust one. FFI wraps it, not the other way round.

```rust
pub struct Context { /* ... */ }

impl Context {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self;

    /// Parse only. No filesystem access.
    pub fn parse(query: &str) -> Result<Query, Error>;

    /// Parse, validate, resolve, and return structured records.
    pub fn execute(&mut self, query: &str) -> Result<Vec<Record>, Error>;

    /// Same as `execute`, but always returns JSON — including for errors.
    pub fn execute_json(&mut self, query: &str) -> String;
}
```

`execute` returns `Result` because that is what Rust callers want. `execute_json` is the total function the FFI layer needs: it never fails, it serializes the error instead.

Rust callers never see raw pointers and never free anything manually.

---

# 8. C ABI / FFI Layer

Everything `unsafe` lives in one module: `src/ffi.rs`. The rest of the crate is `#![deny(unsafe_code)]`.

```rust
#[repr(C)]
pub struct QqlContext {
    _private: [u8; 0],
}

#[no_mangle]
pub extern "C" fn qql_version() -> *const c_char;

#[no_mangle]
pub unsafe extern "C" fn qql_context_create(
    data_directory: *const c_char,
) -> *mut QqlContext;

#[no_mangle]
pub unsafe extern "C" fn qql_context_execute(
    ctx: *mut QqlContext,
    query: *const c_char,
) -> *mut c_char;

#[no_mangle]
pub unsafe extern "C" fn qql_context_destroy(ctx: *mut QqlContext);

#[no_mangle]
pub extern "C" fn qql_execute(query: *const c_char) -> *mut c_char;

#[no_mangle]
pub unsafe extern "C" fn qql_free_string(ptr: *mut c_char);
```

Rules for this layer:

- Every `extern "C"` function wraps its body in `std::panic::catch_unwind`. **A panic must never unwind across the FFI boundary** — that is undefined behavior. On catch, return a null pointer or a serialized `QQL_INTERNAL_ERROR` JSON string.
- Null and invalid pointers are handled defensively: a null `ctx` or `query` returns an error JSON string, never a segfault.
- Non-UTF-8 input from C returns `QQL_INVALID_CHARACTER` rather than panicking. Use `CStr::to_str()` and handle the `Err`.
- Strings handed out are created with `CString::into_raw` and must come back through `qql_free_string`, which calls `CString::from_raw`. Never hand out a pointer into a Rust `String`'s buffer.
- `qql_context_create` returns `Box::into_raw(Box::new(Context::new(..))) as *mut QqlContext`; `qql_context_destroy` reverses it with `Box::from_raw`. Destroying null is a no-op, not a crash.

Usage from C:

```c
qql_context_t *ctx = qql_context_create("./data");
char *result = qql_context_execute(ctx, "Q:2:1-5,255;Q:1;");
printf("%s\n", result);
qql_free_string(result);
qql_context_destroy(ctx);
```

The C header `include/qql.h` is committed so C consumers do not need a Rust toolchain to get it.

> **As built:** the header is hand-written rather than cbindgen-generated. It is six functions and one opaque type; the generator was more moving parts than the artifact. `scripts/c-smoke.sh` keeps it honest by compiling a C client against it under `-Wall -Wextra -Werror` and linking it to the real library, so drift fails to build. That catches more than diffing generated text would — a wrong parameter type fails to compile, not merely to match.

---

# 9. Source Resolver Architecture

Use a trait, not function pointers.

```rust
pub trait Source: Send + Sync {
    /// Canonical code, uppercase, e.g. "Q".
    fn code(&self) -> &'static str;

    /// Display name, e.g. "Quran".
    fn name(&self) -> &'static str;

    /// Optional alternate codes accepted for this source.
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    /// Semantic validation. Numbers exist? Ranges in bounds?
    fn validate(&self, repo: &mut Repository, reference: &Reference) -> Result<(), Error>;

    /// Load and append records, preserving request order.
    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error>;
}
```

The registry is a `HashMap<&'static str, Box<dyn Source>>` (or a static slice plus linear scan — there are fewer than twenty sources, so either is fine) built once when the `Context` is created.

Then register:

```text
Q  → Quran resolver
B  → Bukhari resolver
M  → Muslim resolver
HM → Hisnul Muslim resolver
```

The core parser must not contain code such as:

```rust
match source {
    "Q" => ...,
    "B" => ...,
}
```

Source-specific behavior belongs in `impl Source for ...`.

`Repository` is the thing that owns the lazy-loaded, cached JSON. Handlers receive it; they never open files themselves.

---

# 10. Quran Resolver Rules

Implement Quran first.

Rules:

```text
Q:2
```

means all ayat of Surah 2.

```text
Q:2:255
```

means Surah 2, ayah 255.

```text
Q:2:1-5
```

means Surah 2, ayat:

```text
1
2
3
4
5
```

```text
Q:2:1-5,255
```

means:

```text
1
2
3
4
5
255
```

Ranges are inclusive.

Preserve query order.

For:

```text
Q:2:255,1-3
```

prefer returning:

```text
255
1
2
3
```

rather than automatically sorting them. Resist the temptation to reach for `.sort()` or a `BTreeSet` here — both would silently break this contract.

---

# 11. Duplicate Handling

Define deterministic behavior.

For:

```text
Q:2:1-5,3,4
```

I prefer duplicates to be eliminated inside one reference.

Expected output:

```text
1
2
3
4
5
```

For separate references:

```text
Q:2:255;Q:2:255;
```

preserve both references unless a future deduplication option is explicitly enabled.

Implement this with an order-preserving pass: iterate the expanded item numbers, keep a `HashSet<u32>` of what has been emitted, and skip repeats. Do not collect into a set and iterate it — that destroys order.

---

# 12. Validation

Validation should happen after parsing.

Examples of syntax errors:

```text
Q::
Q:abc
Q:2:
Q:2:1-
Q:2:-5
Q:2:1,,5
Q:2:5-1
```

Examples of semantic errors:

```text
Q:0
Q:115
Q:2:999
UNKNOWN:1
```

The parser should only know that values are integers. Integer overflow is a syntax error, not a panic: parse with `str::parse::<u32>()` and map the `Err` to `QQL_EXPECTED_NUMBER`. Never use `as` casts on untrusted input.

The Quran resolver knows:

```text
Surah must be 1..=114
Ayah must exist in that Surah
```

This separation is important.

---

# 13. Error Model

Never return malformed JSON.

Define one error enum for the whole crate:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("query is empty")]
    EmptyQuery,
    #[error("invalid character")]
    InvalidCharacter { position: usize },
    #[error("expected a source identifier")]
    ExpectedSource { position: usize },
    #[error("expected ':'")]
    ExpectedColon { position: usize },
    #[error("expected a number")]
    ExpectedNumber { position: usize },
    #[error("range start cannot be greater than range end")]
    InvalidRange { position: usize },
    #[error("unknown source '{code}'")]
    UnknownSource { code: String },
    #[error("reference not found")]
    ReferenceNotFound,
    #[error("data file not found: {path}")]
    DataFileNotFound { path: String },
    #[error("invalid data file: {path}")]
    InvalidDataFile { path: String, source: serde_json::Error },
    #[error("internal error")]
    Internal,
}
```

Errors serialize to a stable wire format:

```json
{
  "ok": false,
  "error": {
    "code": "QQL_INVALID_RANGE",
    "message": "Range start cannot be greater than range end",
    "position": 7
  }
}
```

The stable strings are produced by an explicit `fn code(&self) -> &'static str` on `Error` — a match with no wildcard arm, so adding a variant fails to compile until its wire code is chosen:

```text
QQL_OK

QQL_EMPTY_QUERY
QQL_INVALID_CHARACTER
QQL_EXPECTED_SOURCE
QQL_EXPECTED_COLON
QQL_EXPECTED_NUMBER
QQL_INVALID_RANGE
QQL_UNKNOWN_SOURCE
QQL_SOURCE_NOT_LOADED
QQL_REFERENCE_NOT_FOUND
QQL_DATA_FILE_NOT_FOUND
QQL_INVALID_DATA_FILE
QQL_OUT_OF_MEMORY
QQL_INTERNAL_ERROR
```

`QQL_OUT_OF_MEMORY` exists for C-ABI parity; in Rust, allocation failure aborts, so it is only reachable from the FFI layer's `catch_unwind`.

`position` is omitted from the JSON for variants that do not carry one — do not emit `"position": 0` as a placeholder.

---

# 14. Parser Diagnostics

Track byte offsets into the input.

For example:

```text
Q:2:1-,5
      ^
```

should be capable of producing an error position.

Store at least:

```rust
position: usize   // byte offset from the start of the query
```

Since the grammar is ASCII, byte offsets and character offsets agree for all *valid* queries. They can diverge if a query contains multi-byte garbage — document that `position` is a **byte** offset, and never slice a `&str` at an arbitrary offset without `is_char_boundary` or `get()`.

Optional future support: line and column. Normal QQL queries are one line, so offset is sufficient for now.

---

# 15. JSON Library

Use **serde** and **serde_json**.

Reasoning:

- de facto standard in the Rust ecosystem, so no vendoring or build glue
- `#[derive(Serialize, Deserialize)]` removes the entire hand-written mapping layer that a C implementation would need
- UTF-8 correct by construction — Rust `String` is UTF-8, and `serde_json` escapes on output without touching the underlying scalar values
- pure Rust, no C toolchain, so Android NDK / iOS / Windows cross-compilation stays trivial
- streaming reader available (`serde_json::from_reader`) if data files grow

Do not pull in `simd-json` or similar for v1. Query workloads are tiny; the data files are the only large input and they are cached after first load.

Keep JSON-specific code (file layout structs, reading, writing) isolated under:

```text
src/repo/
```

The rest of the crate should be able to compile in a world without `serde_json`, conceptually.

---

# 16. Unicode

All input and output strings are UTF-8. Rust's `String`/`&str` enforce this for free — the remaining risk is at the FFI boundary and in file reads, both of which must **reject** invalid UTF-8 rather than lossily convert it.

Do not use `String::from_utf8_lossy` on Quran or Hadith text. Replacement characters silently corrupt scripture.

Arabic must pass through unchanged.

Do not attempt to normalize Arabic Unicode in version 1.

Do not modify:

- tashkeel
- Quranic marks
- Arabic punctuation
- zero-width characters

Treat the source JSON text as authoritative.

Note that `char` indexing, `len()`, and `to_uppercase()` are all wrong tools for Arabic. Uppercasing applies only to ASCII source codes; use `to_ascii_uppercase`.

---

# 17. Directory Structure

Create a maintainable crate layout similar to:

```text
qql/
├── Cargo.toml
├── README.md
├── CONTRIBUTING.md
├── LICENSE.md
├── cbindgen.toml
├── include/
│   └── qql.h                  # generated by cbindgen, committed
├── src/
│   ├── lib.rs                 # public Rust API, re-exports
│   ├── error.rs
│   ├── lexer.rs
│   ├── parser.rs
│   ├── ast.rs
│   ├── context.rs
│   ├── record.rs              # Record + serialization
│   ├── registry.rs
│   ├── ffi.rs                 # the only unsafe module
│   ├── sources/
│   │   ├── mod.rs             # the Source trait
│   │   ├── quran.rs
│   │   ├── bukhari.rs
│   │   ├── muslim.rs
│   │   └── hisnul_muslim.rs
│   ├── repo/
│   │   ├── mod.rs             # Repository: lazy load + cache
│   │   └── schema.rs          # Deserialize structs for data files
│   └── bin/
│       ├── qql.rs             # CLI
│       └── qql_parse.rs       # phase-1 parse-only CLI
├── tests/
│   ├── parser.rs
│   ├── quran.rs
│   ├── errors.rs
│   ├── ffi.rs
│   └── fixtures/
├── fuzz/
│   └── fuzz_targets/
│       └── parse.rs
├── benches/
├── data/
│   ├── quran/
│   ├── bukhari/
│   ├── muslim/
│   └── hisnul_muslim/
├── examples/
│   └── basic.rs
└── bindings/
    └── dart/
        └── README.md
```

A single crate is preferred over a workspace for v1. Split out a `qql-ffi` crate only if the FFI surface starts pulling dependencies the core does not want.

Adjust this structure when there is a strong technical reason.

---

# 18. Build System

Use Cargo.

```toml
[package]
name = "qql"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "GPL-3.0-or-later"

[lib]
crate-type = ["rlib", "cdylib", "staticlib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
```

```bash
cargo build --release
cargo test
```

Produces:

| Platform | Shared | Static |
| --- | --- | --- |
| Linux | `libqql.so` | `libqql.a` |
| Windows | `qql.dll` | `qql.lib` |
| macOS | `libqql.dylib` | `libqql.a` |

`rlib` keeps the crate usable as a normal Rust dependency; `cdylib` and `staticlib` serve FFI consumers. Note that building all three is slower — that is an accepted cost, not a reason to drop `rlib`.

No symbol-export macros are needed. `#[no_mangle] pub extern "C"` exports from a `cdylib` on every platform.

Cross-compilation targets to verify: `aarch64-linux-android`, `aarch64-apple-ios`, `x86_64-pc-windows-msvc`. Since there is no C code in the dependency tree, `cargo build --target ...` should be sufficient.

---

# 19. C ABI Compatibility

The generated header must compile as C and also work when included from C++. cbindgen emits the `extern "C"` guard automatically; verify it rather than assume it.

The opaque context type is emitted as an incomplete struct, so consumers cannot depend on its layout:

```c
typedef struct qql_context qql_context_t;
```

Configure cbindgen (`cbindgen.toml`) with:

- `language = "C"`
- an include guard
- `[export] prefix` rules so all symbols keep the `qql_` prefix
- no auto-generated types for anything not in the FFI surface

Regenerate and diff the header in CI. A drifted header is an ABI bug that ships silently.

---

# 20. Thread Safety

Rust encodes this in the type system rather than in documentation, so use it.

- `Context` holds the resolver registry and the mutable `Repository` cache. `execute` takes `&mut self`, so the compiler prevents concurrent use of a single context.
- `Context` should be `Send`. Making it `Sync` is not required and should not be forced with interior mutability in v1.
- Individual `Source` implementations are stateless and are `Send + Sync` (see §9).
- Separate contexts on separate threads are safe by construction.

At the FFI boundary this guarantee is lost — C callers can share a pointer freely. Document it explicitly: **one `qql_context_t` must not be used from two threads simultaneously.**

`qql_execute()` uses a process-wide default context behind a `Mutex` (`static DEFAULT: OnceLock<Mutex<Context>>`). It is therefore thread-safe but serialized, and it must recover from lock poisoning rather than propagate a panic across the boundary. Prefer explicit contexts for serious use.

Avoid `static mut` and any global mutable state that is not behind a lock.

---

# 21. Caching

Do not prematurely build a complicated cache.

A reasonable first implementation lives in `Repository`:

```text
first query for Surah 2
    ↓
load data/quran/002.json
    ↓
deserialize into schema structs
    ↓
insert into HashMap<(SourceCode, u32), Arc<SurahData>>
```

Future queries reuse it. The cache is owned by the `Context`, so it is freed when the context is dropped — no explicit teardown code, and no leak to test for.

No eviction policy in v1. If memory becomes a concern, an LRU is a later change confined to `Repository`.

---

# 22. Hadith Data Model

Do not assume that Hadith numbering is universally consistent.

Design Hadith records so they can preserve multiple identifiers.

Example:

```json
{
  "id": "bukhari:1",
  "collection": "bukhari",
  "book": 1,
  "chapter": 1,
  "number": 1,
  "canonical_number": 1,
  "alternate_numbers": {
    "edition_x": 1,
    "edition_y": 1
  },
  "ar": "...",
  "en": "..."
}
```

In Rust this is a `Deserialize` struct with `alternate_numbers: HashMap<String, u32>` and `#[serde(default)]` so older files without the field still load.

For QQL v1, choose one internal canonical numbering scheme and document it.

Do not attempt to solve all edition-numbering differences inside the parser.

---

# 23. Hisnul Muslim

Hisnul Muslim may have a simpler indexing model.

```text
HM:27
```

could represent Hisnul Muslim item/chapter 27.

Its JSON could look like:

```json
{
  "number": 27,
  "title_ar": "...",
  "title_en": "...",
  "items": [
    {
      "ar": "...",
      "en": "..."
    }
  ]
}
```

The source-specific resolver decides how `HM:27` is interpreted.

This source is the proof that the trait boundary works: it has no second-level numbering comparable to ayat, yet it must need zero parser changes.

> **As built:** it did need zero parser changes — the resolver, one registry
> line, and the module export were the entire diff. The real data
> (`Hisn-Muslim-Json/husn_en.json`) turned out messier than this sketch:
>
> - all 132 chapters live in one file under an `"English"` key, so the whole
>   book loads once instead of per chapter;
> - chapters are stored **out of numerical order** (position 0 is chapter 27),
>   so lookups go by the `ID` field — indexing by position would return the
>   wrong supplication silently;
> - supplication IDs are global across the book (75, 76, … 267) rather than
>   per-chapter, so the selector counts position within the chapter, keeping
>   `HM:27:1` consistent with `Q:2:1` and `B:1:1`;
> - the file carries a UTF-8 BOM, two objects repeat a key with different
>   values, and one entry spells `ARABIC_TEXT` as `Text`.
>
> The BOM is stripped in `Repository` (storage), the rest is absorbed by the
> resolver's accessors (schema). None of it reached the parser, which is the
> point §40 was making.

---

# 24. Result Metadata

Design returned records with enough metadata for UI usage.

For Quran:

```json
{
  "source": "Q",
  "collection": "Quran",
  "surah": 2,
  "surah_name_ar": "البقرة",
  "surah_name_en": "Al-Baqarah",
  "ayah": 255,
  "ar": "...",
  "en": "..."
}
```

For Hadith:

```json
{
  "source": "B",
  "collection": "Sahih al-Bukhari",
  "book": 1,
  "chapter": 1,
  "number": 1,
  "ar": "...",
  "en": "..."
}
```

Do not force every source into exactly the same metadata fields.

Require only:

```text
source
collection
ar
en
```

and allow source-specific metadata. In Rust:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub source: String,
    pub collection: String,
    pub ar: String,
    pub en: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
```

`#[serde(flatten)]` puts source-specific keys at the top level of each record, matching the JSON above. `BTreeMap` rather than `HashMap` so key order in the output is deterministic and snapshot tests are stable.

An enum of per-source record types is the alternative. It is more type-safe but forces every new source to edit a shared enum, which fights §40. Prefer the map for v1.

---

# 25. Versioning

Expose version information from a single source of truth:

```rust
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

and over FFI:

```rust
#[no_mangle]
pub extern "C" fn qql_version() -> *const c_char;
```

returning a `'static` NUL-terminated string that the caller must **not** free. Document that asymmetry — it is the one function whose return value does not go to `qql_free_string`.

Follow semantic versioning. The C ABI is part of the public API for semver purposes.

---

# 26. Public Rust API Surface

Aim for something roughly like `lib.rs`:

```rust
#![deny(unsafe_code)]
#![deny(missing_docs)]

mod ast;
mod context;
mod error;
mod ffi;
mod lexer;
mod parser;
mod record;
mod registry;
mod repo;
mod sources;

pub use ast::{Query, Range, Reference};
pub use context::Context;
pub use error::Error;
pub use record::Record;
pub use sources::Source;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parse a query without touching the filesystem.
pub fn parse(query: &str) -> Result<Query, Error>;
```

`#![deny(unsafe_code)]` at the crate root with a single `#[allow(unsafe_code)]` on `mod ffi` makes the unsafe boundary reviewable in one place and visible in every diff.

Keep the FFI surface very small. Modify this shape if needed, but adding a public FFI function requires justification.

---

# 27. CLI

Create a command-line application for development.

```bash
cargo run --bin qql -- "Q:2:255"
./target/release/qql "Q:2:255"
./target/release/qql --data ./data "Q:1;Q:2:255"
```

Output:

```json
{
  "ok": true,
  "results": [...]
}
```

Pretty-print with `serde_json::to_string_pretty` in the CLI. The library returns compact JSON unless configured otherwise.

Argument parsing: use `std::env::args` for v1. There are two flags. Do not add `clap` until the flag count justifies it.

Exit codes: `0` on `"ok": true`, `1` on `"ok": false`. The JSON goes to stdout either way so it stays pipeable into `jq`.

---

# 28. Tests

Tests are important.

Unit tests live in `#[cfg(test)] mod tests` next to the code they test (lexer, parser internals). Integration tests in `tests/` exercise only the public API.

Create parser tests for:

```text
Q:1
Q:2:255
Q:2:1-5
Q:2:1-5,255
Q:2:1,3,5
Q:1;Q:2:255;Q:112;
B:1:1-10
HM:27
```

Whitespace:

```text
Q : 2 : 255
Q:2:1-5, 255
```

Invalid queries — assert the specific `Error` variant *and* its position, not merely `is_err()`:

```text
""
";"
"Q"
"Q:"
"Q::"
":2"
"Q:A"
"Q:2:"
"Q:2:-5"
"Q:2:1-"
"Q:2:5-1"
"Q:2:1,,5"
"Q:99999999999999999999"
```

Semantic Quran validation:

```text
Q:0
Q:115
Q:1:8
Q:2:999
```

Boundary tests:

```text
Q:1:1
Q:1:1-7
Q:114
```

Order and duplicate contracts (§10, §11) get dedicated tests — they are the behaviors most likely to be "fixed" into breakage by a well-meaning refactor.

UTF-8 tests must verify Arabic survives:

```text
JSON file → deserialize → Record → serialize → returned JSON
```

byte-for-byte, including tashkeel and any zero-width characters in the fixture.

---

# 29. Memory and Safety Testing

Safe Rust removes the leak/use-after-free/double-free test burden for the core. What remains is the `unsafe` in `src/ffi.rs`, and it must be tested directly.

- Run FFI tests under **Miri**: `cargo +nightly miri test --test ffi`.
- Test the abuse cases explicitly: null pointers, freeing twice (documented as UB but must not be reachable through the safe API), non-UTF-8 input, empty strings, destroying a context while a returned string is still alive (must be fine — the string is independently owned).
- Verify no panic escapes: add a test with input engineered to panic internally and assert the FFI call returns an error JSON instead of aborting.
- Optionally run the C-side example under ASan/LeakSanitizer to prove the `CString::into_raw` / `qql_free_string` pairing is balanced.

Ownership rules are enforced by the borrow checker inside the crate; document them only at the FFI boundary, where the compiler cannot help.

---

# 30. Fuzzing

The parser is a natural fuzz target and Rust makes this cheap.

```bash
cargo +nightly fuzz run parse
```

```rust
// fuzz/fuzz_targets/parse.rs
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = qql::parse(s);
    }
});
```

The invariant: `parse` never panics and never loops forever, for any input. Memory safety is already guaranteed, so the fuzzer is hunting panics — arithmetic overflow in debug, slicing on a non-char-boundary, unwrap on a malformed token, and unbounded allocation from something like `Q:1:1-4294967295`.

That last case deserves thought before it is found by a fuzzer: expanding a huge range must not attempt a multi-gigabyte allocation. Bound the expansion or resolve ranges lazily.

Also fuzz the FFI entry point with arbitrary bytes, since it accepts input that is not valid UTF-8.

---

# 31. Performance Goals

QQL queries are tiny. Parser performance does not need exotic optimization.

Prioritize:

1. correctness
2. predictable errors
3. portability
4. extensibility
5. speed

The expensive part is loading large datasets. Use lazy loading and caching.

Avoid allocation in the lexer — tokens should borrow `&str` slices from the input rather than copy. This is free in Rust and removes an entire class of C-style ownership work.

Add `benches/` only when there is a number worth defending. Do not optimize against a guess.

---

# 32. Flutter / Dart FFI Compatibility

Do not implement the Flutter plugin yet.

Make sure the native API can easily be called from Dart FFI.

Expected Dart shape:

```dart
final ptr = qqlContextExecute(ctx, query.toNativeUtf8().cast());
final result = ptr.cast<Utf8>().toDartString();
qqlFreeString(ptr);
```

Therefore:

- return NUL-terminated UTF-8
- the library owns the buffer before return
- the caller owns the returned buffer
- the caller releases it with `qql_free_string`
- never require Dart to free Rust memory with Dart's allocator, or vice versa

Build notes for later: `cdylib` for Android (`.so` per ABI via `cargo-ndk`), and a static library for iOS since App Store rules discourage loose dynamic libraries. Both fall out of the `crate-type` list in §18.

---

# 33. Future Syntax

Do not implement these yet unless they naturally fit the architecture.

```text
Q:2:255@en
Q:2:255@ar
Q:2:255@ar,en
Q:2:255+tafsir
```

or aliases:

```text
Q:Baqarah:255
```

Possibly:

```text
Q:2:255|translation=sahih
```

Do not prematurely build these.

Keep the `Token` enum and the parser structured so adding a variant is a compile-error-driven change: exhaustive `match` with no `_` arm means the compiler lists every site that must be updated. That property is worth more than any amount of speculative extension machinery.

---

# 34. Source Aliases

Potential future aliases:

```text
BUKHARI → B
MUSLIM  → M
HISN    → HM
```

For version 1, only canonical short codes are required. The `aliases()` method on the `Source` trait (§9) already reserves the mechanism; the registry indexes both the code and every alias at construction time.

---

# 35. Parser vs Resolver Contract

Maintain this distinction strictly.

```text
Q:500:999
```

is syntactically valid. The Quran resolver then rejects it semantically.

```text
XYZ:1:2
```

is syntactically valid. The registry rejects `XYZ` as an unknown source.

This separation must be reflected in the module structure and in the tests: `tests/parser.rs` must not need a `data/` directory to run, and must pass with the `sources` module effectively unused.

---

# 36. Development Phases

Implement incrementally. Do not generate everything in one huge unreviewable commit.

> **Status:** phases 1–11 are done. The Dart round trip in phase 11 is
> verified — `dart test` in `bindings/dart` runs it against the built library.
>
> Deviations, all deliberate and recorded where they apply: `Source` has one
> `resolve` method instead of `validate` + `resolve` (§9); `Error` hand-rolls
> `Display` instead of pulling in `thiserror` (§13); the C header is
> hand-written and verified by compiling against it (§8, §19); data is read
> from the upstream submodule layout with no `data/` copy (§5, §17).

## Phase 1 — Foundation

- Cargo project, `crate-type` set, dependencies pinned
- module skeleton
- error enum with wire codes and JSON serialization
- CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`

Everything compiles and CI is green.

## Phase 2 — Lexer

Implement tokenization with borrowed slices and byte offsets.

```rust
enum Token<'a> {
    Ident(&'a str),
    Integer(&'a str),
    Colon,
    Semicolon,
    Comma,
    Dash,
    Eof,
}
```

Each token carries its start offset. Unit tests included.

## Phase 3 — Parser

Implement `query`, `reference`, `selector`, `range`. Build the AST.

No cleanup code needed — `Drop` handles it, which is the single largest simplification over the C design.

Add parser tests and the `qql-parse` binary.

## Phase 4 — JSON Integration

Add serde. Define data-file schema structs and the `Record` output type. Test Arabic round-trips byte-for-byte.

## Phase 5 — Source Registry

Implement the `Source` trait and the registry. Register Quran. Unknown sources return `QQL_UNKNOWN_SOURCE`.

## Phase 6 — Quran Resolver

Implement:

```text
Q:surah
Q:surah:ayah
Q:surah:start-end
Q:surah:a,b,c
Q:surah:a-b,c,d-e
```

Use fixture data before importing a production dataset.

## Phase 7 — Result Serialization

Stable JSON output. `Context::execute` and `execute_json`. All errors return JSON.

## Phase 8 — CLI

Complete `qql` execution with `--data`, pretty printing, and exit codes.

## Phase 9 — FFI Layer

`src/ffi.rs`, cbindgen config, committed `include/qql.h`, a C smoke-test example, and Miri tests. This lands **before** the remaining sources so the ABI is exercised early rather than bolted on at the end.

## Phase 10 — Hadith Resolvers

Bukhari, then Muslim. Same parser, new `impl Source` only. If either phase touches `lexer.rs` or `parser.rs`, stop and reconsider the design.

## Phase 11 — Hisnul Muslim and Dart Verification

Add the HM handler — a source with a different logical structure through the same parser.

Then a tiny Dart console test using `dart:ffi`. Not a Flutter UI.

Verify:

```text
Dart
→ qql_context_execute()
→ Rust
→ JSON data
→ JSON result
→ Dart String
→ qql_free_string()
```

---

# 37. Coding Style

Rust 2021 edition, stable toolchain. Nightly only for `miri` and `cargo-fuzz`.

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Rules:

- `#![deny(unsafe_code)]` crate-wide, allowed only on `mod ffi`.
- No `unwrap()` or `expect()` in library code. They are fine in tests and acceptable in the CLI binary.
- No `panic!` reachable from a public API. Errors are `Result`.
- No `as` casts on parsed input; use `TryFrom` / `parse` and handle the error.
- Prefer borrowing over cloning. The lexer borrows the query; the parser owns only what it must.
- Keep functions small. Avoid a giant `parse` function.
- Exhaustive `match` over `_` wildcards wherever adding a variant should force a review.
- Public items are documented; `#![deny(missing_docs)]` enforces it.

---

# 38. Documentation

README should explain:

- what QQL is, with `Q:2:1-5,255;Q:1;`
- syntax: source, primary selector, ranges, commas, semicolons
- build: `cargo build --release`
- CLI usage
- Rust usage, with a complete example
- C / FFI usage and the ownership rule: `qql_context_execute` allocates, `qql_free_string` frees
- how a future developer adds `T = Tirmidhi` without modifying parser logic

Rustdoc carries the API detail; the README should not duplicate it. `cargo doc --open` is the reference.

---

# 39. Example End-to-End Behavior

Given:

```text
Q:2:1-3,255;Q:1:1;
```

the library should:

1. tokenize the input
2. parse two references
3. normalize their selectors (dedupe within, preserve order)
4. look up the `Q` handler in the registry
5. validate Surah/ayah numbers
6. lazily load required Quran JSON files through `Repository`
7. resolve requested ayat
8. preserve requested ordering
9. serialize as UTF-8 JSON
10. return a `String` (Rust) or an owned `*mut c_char` (FFI)

Conceptual response:

```json
{
  "ok": true,
  "results": [
    { "source": "Q", "collection": "Quran", "surah": 2, "ayah": 1,   "ar": "...", "en": "..." },
    { "source": "Q", "collection": "Quran", "surah": 2, "ayah": 2,   "ar": "...", "en": "..." },
    { "source": "Q", "collection": "Quran", "surah": 2, "ayah": 3,   "ar": "...", "en": "..." },
    { "source": "Q", "collection": "Quran", "surah": 2, "ayah": 255, "ar": "...", "en": "..." },
    { "source": "Q", "collection": "Quran", "surah": 1, "ayah": 1,   "ar": "...", "en": "..." }
  ]
}
```

---

# 40. Important Design Principle

The most important architectural rule is:

```text
QQL parser knows syntax.
Source handlers know Islamic-book structure.
Repository knows storage.
FFI module knows the C ABI.
```

Do not mix these responsibilities.

I want to eventually be able to add a source such as:

```text
T:5:10
```

by writing `impl Source for Tirmidhi` and registering it, without changing the lexer or parser.

---

# 41. First Task

Start by inspecting the current repository.

If it has no source, initialize the crate described above.

Then implement only:

1. Cargo project with `crate-type = ["rlib", "cdylib", "staticlib"]`
2. module skeleton and public API surface
3. error model with wire codes
4. lexer
5. parser
6. AST
7. parser unit and integration tests
8. a `qql-parse` binary that prints the normalized query

Do **not** implement Quran JSON resolution, the source registry, or the FFI layer in the first step.

For example:

```bash
cargo run --bin qql-parse -- "Q:2:1-5,255;Q:1;Q:3:2;"
```

should print something equivalent to:

```json
{
  "references": [
    {
      "source": "Q",
      "primary": 2,
      "all": false,
      "ranges": [
        [1, 5],
        [255, 255]
      ]
    },
    {
      "source": "Q",
      "primary": 1,
      "all": true,
      "ranges": []
    },
    {
      "source": "Q",
      "primary": 3,
      "all": false,
      "ranges": [
        [2, 2]
      ]
    }
  ]
}
```

Run `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt --check`. Fix everything.

Then show me:

- files created
- architecture decisions
- public types
- parser behavior
- test results
- any design concerns

Stop there so the next implementation phase can be reviewed separately.
