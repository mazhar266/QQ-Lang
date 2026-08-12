# QQL — Quran Query Language

A small, portable Rust library that parses compact textual references to Islamic texts and resolves them against local JSON data.

```text
Q:2:1-5,255;Q:1;
```

> Surah 2 ayat 1–5 plus ayah 255, then all of Surah 1.

Usable as an idiomatic Rust crate, or through a C ABI from any language that speaks one — Dart FFI, Python, Go, C/C++ — on Linux, Windows, macOS, Android NDK, and iOS. Not coupled to Flutter.

> **Status:** complete for v1 — parser, source registry, Quran / hadith / Hisnul Muslim resolvers, CLI, C ABI, and a Dart binding, with 65 Rust tests and 9 Dart tests. See [docs/plan.md](docs/plan.md) for the full design.

## Syntax

```text
query      := reference (';' reference)* ';'?
reference  := source ':' primary (':' selector)?
selector   := item (',' item)*
item       := integer | integer '-' integer
source     := [A-Za-z][A-Za-z0-9_]*
```

| Part | Meaning |
| --- | --- |
| **source** | Collection code, normalized to uppercase (`Q`, `B`, `M`, `HM`, …) |
| **primary** | First-level index — Surah for Quran, book for Hadith, chapter for Hisnul Muslim |
| **selector** | Optional list of items within the primary. Omit it to select everything. |
| `-` | Inclusive range: `1-5` → 1, 2, 3, 4, 5 |
| `,` | Joins items: `1-5,255` |
| `;` | Separates references. Trailing `;` is optional. |

Whitespace around tokens is accepted: `Q : 2 : 1-5, 255;`

### Source codes

| Code | Collection | `primary` is |
| --- | --- | --- |
| `Q` | Quran | Surah, 1–114 |
| `B` | Sahih al-Bukhari | chapter (kitab) |
| `M` | Sahih Muslim | chapter |
| `AD` | Sunan Abi Dawud | chapter |
| `T` | Jami' at-Tirmidhi | chapter |
| `N` | Sunan an-Nasa'i | chapter |
| `IM` | Sunan Ibn Majah | chapter |
| `HM` | Hisnul Muslim (alias `HISN`) | chapter, 1–132 |

`qql --sources` prints the registered codes.

### Numbering

The selector always counts **position within the primary**, so `X:C:1` is the first item of `C` for every source. What differs is what `primary` means, and what that numbering is *not*:

- **Quran** — `Q:2:255` is Surah 2, ayah 255. The universal numbering; no ambiguity.
- **Hadith** — `B:1:1` is the first hadith of chapter 1 (Kitab Bad' al-Wahy). This matches the upstream per-chapter files and is QQL's canonical scheme for v1. It is **not** the book-global number most citations use ("Bukhari 6018"); mapping those is a resolver concern and can be added later without touching the grammar.
- **Hisnul Muslim** — `HM:27:1` is the first supplication of chapter 27. Chapter numbers are the book's own, not array positions in the data file, which is stored out of order.

### Examples

```text
Q:1                    all of Surah 1
Q:2:255                Ayat al-Kursi
Q:2:1-5                ayat 1 through 5
Q:2:1-5,10,20-25,255   mixed ranges and singles
Q:1;Q:2:255;Q:112;     three references
B:1:1-10               Bukhari, chapter 1, hadith 1–10
HM:27                  Hisnul Muslim, all of chapter 27 (morning and evening remembrance)
HM:27:1-3              the first three supplications of that chapter
```

### Ordering and duplicates

Query order is preserved — `Q:2:255,1-3` returns 255, 1, 2, 3. Nothing is sorted for you.

Duplicates are removed **within** a single reference (`Q:2:1-5,3,4` → 1, 2, 3, 4, 5) but **across** references they are kept (`Q:2:255;Q:2:255;` returns two items).

## Build

```bash
cargo build --release
cargo test
```

The crate builds as `rlib`, `cdylib`, and `staticlib`, producing `libqql.so` / `qql.dll` / `libqql.dylib` plus `libqql.a`, alongside the `qql` binary.

Lints and the C ABI check are part of the build contract:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
./scripts/c-smoke.sh                    # C header + link check
cargo +nightly miri test --test ffi     # after touching src/ffi.rs
cargo +nightly fuzz run parse           # the parser must never panic
```

## CLI

```bash
cargo run --bin qql -- "Q:2:255"
./target/release/qql "Q:2:1-5,255;Q:1;"
./target/release/qql --data ./sources "B:1:1-3"
./target/release/qql --parse "Q:2:1-5,255;Q:1;"   # parse only, no data access
./target/release/qql --compact "Q:1"
./target/release/qql --sources
```

| Option | Meaning |
| --- | --- |
| `--data <DIR>` | Data directory (default `./sources`) |
| `--source <F>` | Register sources from a manifest, relative to `--data`. Repeatable. |
| `--parse` | Print the parsed query instead of resolving it |
| `--compact` | Compact JSON instead of pretty-printed |
| `--sources` | List registered source codes |

Output is pretty-printed JSON, exit code `0` on success and `1` on error — the JSON goes to stdout either way, so it stays pipeable into `jq`. The library itself returns compact JSON.

## Rust usage

```toml
[dependencies]
qql = "0.1"
```

```rust
use qql::{Context, Error};

fn main() -> Result<(), Error> {
    let mut ctx = Context::new("./sources");

    for record in ctx.execute("Q:2:1-5,255;Q:1;")? {
        println!("{} — {}", record.collection, record.ar);
    }

    Ok(())
}
```

Parsing alone needs no data directory and touches no filesystem:

```rust
let query = qql::parse("Q:2:1-5,255")?;
assert_eq!(query.references.len(), 1);
```

`Context::execute_json` returns a `String` instead, and never fails — errors are serialized into the JSON. `execute_value` is the same thing before serialization. Those are the total functions the FFI layer will wrap.

## Result format

```json
{
  "ok": true,
  "query": "Q:2:255",
  "results": [
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
  ]
}
```

Every record carries `source`, `collection`, `ar`, and `en`. Other fields are source-specific — Hadith records use `book`/`chapter`/`number` instead of `surah`/`ayah`.

Errors are still valid JSON, never a malformed string:

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

Error codes: `QQL_EMPTY_QUERY`, `QQL_INVALID_CHARACTER`, `QQL_EXPECTED_SOURCE`, `QQL_EXPECTED_COLON`, `QQL_EXPECTED_NUMBER`, `QQL_INVALID_RANGE`, `QQL_UNKNOWN_SOURCE`, `QQL_SOURCE_NOT_LOADED`, `QQL_REFERENCE_NOT_FOUND`, `QQL_DATA_FILE_NOT_FOUND`, `QQL_INVALID_DATA_FILE`, `QQL_OUT_OF_MEMORY`, `QQL_INTERNAL_ERROR`.

`position` is a byte offset into the query and is present only for errors that have one.

All input and output is UTF-8. Arabic passes through byte-for-byte — tashkeel, Quranic marks, and zero-width characters are never normalized, and invalid UTF-8 is rejected rather than lossily replaced.

## C / FFI usage

[include/qql.h](include/qql.h) is committed, so C consumers need no Rust toolchain. `scripts/c-smoke.sh` compiles [examples/c/basic.c](examples/c/basic.c) against it and links it to the real library — a signature that drifts from [src/ffi.rs](src/ffi.rs) fails there rather than at a user's link step.

```bash
cargo build --release
./scripts/c-smoke.sh            # builds, links, runs, and checks under ASan when available
```

```c
#include <stdio.h>
#include <qql.h>

int main(void) {
    qql_context_t *ctx = qql_context_create("./sources");
    if (!ctx) return 1;

    char *result = qql_context_execute(ctx, "Q:2:1-5,255;Q:1;");
    printf("%s\n", result);

    qql_free_string(result);
    qql_context_destroy(ctx);
    return 0;
}
```

```bash
cc example.c -Iinclude target/release/libqql.a -lpthread -ldl -lm -o example   # static
cc example.c -Iinclude -Ltarget/release -lqql -o example                        # shared
```

### Ownership

```text
qql_context_execute()  allocates the returned buffer
qql_free_string()      frees it
```

- Returned strings are NUL-terminated UTF-8.
- The **caller** owns the buffer once returned and must release it with `qql_free_string()` — never with the host language's allocator, and never with plain `free()`.
- `qql_version()` is the one exception: it returns a static string that must **not** be freed.
- `qql_context_t` is opaque; only the pointer crosses the boundary.
- Panics never cross the FFI boundary. Every entry point catches them and returns an error JSON string instead.
- `qql_context_execute` never returns null and never returns malformed JSON — a null context, a null query, and invalid UTF-8 all come back as `{"ok":false,...}`.
- Result strings stay valid after `qql_context_destroy`; they are independently owned.

### Dart

A `dart:ffi` binding lives in [bindings/dart/](bindings/dart/) — see its README for testing, bundling, and memory notes.

```bash
cargo build --release
cd bindings/dart && dart pub get && dart test
```

```dart
final qql = Qql.open('sources', libraryPath: 'target/release/libqql.so');
try {
  for (final record in qql.execute('Q:2:1-5,255')) {
    print(record['ar']);
  }
} finally {
  qql.dispose();
}
```

### Thread safety

`Context::execute` takes `&mut self`, so the Rust compiler prevents concurrent use of one context. `Context` is `Send`, so moving one to another thread is fine, and separate contexts on separate threads are safe by construction.

That guarantee does not survive the FFI boundary — **do not use one `qql_context_t` from two threads at once.** `qql_execute()` uses a process-wide default context behind a mutex: thread-safe, but serialized. Prefer explicit contexts.

## Architecture

```text
query → lexer → parser → AST → validation → source resolver → repository → serializer → UTF-8 String
```

The layers stay independent:

```text
QQL parser knows syntax.
Source handlers know Islamic-book structure.
Repository knows storage.
FFI module knows the C ABI.
```

The parser only knows that a reference is `IDENT : INT ( : selectors )`. It has no idea that Surah numbers stop at 114 — that is the Quran resolver's job. So `Q:500:999` and `XYZ:1:2` both parse fine and are rejected later, by the resolver and the registry respectively.

`unsafe` exists in exactly one module, `src/ffi.rs`. The rest of the crate is `#![deny(unsafe_code)]`.

## Adding a source without writing Rust

Point QQL at your own JSON and give it a short code. Drop a `qql-sources.json` in the data directory and it is picked up automatically — no rebuild, and it works through the CLI, the C ABI, and Dart alike.

```json
[
  {
    "code": "X",
    "name": "My Collection",
    "aliases": ["MINE"],
    "path": "mydata/{primary}.json",
    "items": "lines",
    "ar": "arabic",
    "en": "translation",
    "primary_key": "chapter",
    "container_metadata": { "chapter_title": "title" },
    "metadata": { "note": "note" }
  }
]
```

```bash
qql --data ./mydata "X:1:2"
qql --source other-sources.json "X:1:2"    # extra manifest, repeatable
qql --data ./mydata --sources              # X now listed
```

| Key | Meaning |
| --- | --- |
| `code`, `name` | Short code and display name. Codes uppercase automatically. |
| `aliases` | Extra codes that select this source. |
| `path` | Data file, relative to the data directory. `{primary}` → one file per chapter; omit it for a single file. |
| `items` | Dotted path to the array of items. Empty means the file *is* the array. |
| `ar`, `en` | Dotted paths within an item, so `"english.text"` reaches nested fields. |
| `chapters` + `chapter_id` | For single-file books: the chapter array, and the field to match against the primary. |
| `item_id` | Match items by a field instead of by position. |
| `primary_key` | Names the primary in output (`surah`, `chapter`, …). Defaults to `primary`. |
| `metadata` | Extra output fields taken from the item: output key → dotted path. |
| `container_metadata` | Same, taken from the chapter or file. |

Everything else behaves as it does for built-in sources: query order preserved, duplicates dropped within a reference, ranges bounds-checked, errors as JSON.

From Rust, skip the file entirely:

```rust
let mut ctx = qql::Context::new("mydata");
ctx.register_spec(spec);                      // a SourceSpec
ctx.add_sources_from("other-sources.json")?;  // or a whole manifest
```

Sources are searched newest-first, so registering an existing code shadows it. The data-directory manifest loads on the first query, so call `ctx.load_manifest()?` first if you mean to override something it defines.

Reach for a real `impl Source` (below) when the data is too irregular for a mapping to express — the Hisnul Muslim resolver exists because its file has duplicate keys and a misspelled field.

## Adding a new source in Rust

Adding a collection requires **no** change to the lexer or parser.

If it is another book in the nine, it is a single line — `HadithCollection` is one `Source` implementation, instantiated once per collection:

```rust
Box::new(HadithCollection::new("MK", "Muwatta Malik", "malik"))
```

For a collection with a different structure, implement the trait:

```rust
use qql::{Error, Record, Reference, Repository, Source};

pub struct HisnulMuslim;

impl Source for HisnulMuslim {
    fn code(&self) -> &str { "HM" }
    fn name(&self) -> &str { "Hisnul Muslim" }

    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        // Reject impossible numbers, load through `repo`, push records in
        // query order. `reference.expand(total)` handles ordering, dedup,
        // and bounds in one call.
    }
}
```

Then add the module to `src/sources/mod.rs`, one line to `Registry::with_defaults` in [src/registry.rs](src/registry.rs), and the data under `sources/`. Aliases (`TIRMIDHI` → `T`) come from the trait's `aliases()` method — no registry surgery required.

Semantic validation lives inside `resolve` rather than a separate `validate` method: every check a dry run would perform is the first thing `resolve` does anyway.

## Data

Text comes from two open-source datasets, vendored as git submodules under `sources/`. Nothing is transformed or copied at build time — the resolvers read the upstream layout directly, so keeping the data current is a `git submodule update`.

```bash
git submodule update --init
```

```text
sources/
  quran-json-arabic/dist/chapters/en/{1..114}.json     Arabic + English, one file per Surah
  hadith-json/db/by_chapter/the_9_books/{book}/{chapter}.json
  Hisn-Muslim-Json/husn_en.json                        all 132 chapters in one file
```

Files are read on first use and cached in the `Context` until it drops — `Q:2:255` loads roughly 50 KB, not the whole mushaf. There is no eviction policy.

The datasets carry their own licenses; see the submodule directories.

Upstream data is taken as authoritative and is never rewritten, but it is not uniform. The Hisnul Muslim file in particular carries a UTF-8 BOM, stores its chapters out of numerical order, repeats a key in two entries, and misspells one field name. Those are absorbed where they belong — the BOM in [src/repo.rs](src/repo.rs) since it is a storage concern, the rest in [src/sources/hisnul.rs](src/sources/hisnul.rs) — rather than by patching the data.

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) for build, test, and style requirements.

## License

GNU General Public License v3.0 or later — see [LICENSE.md](LICENSE.md).
