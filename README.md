# QQL — Quran Query Language

A small, portable Rust library that parses compact textual references to Islamic texts and resolves them against local JSON data.

```text
Q:2:1-5,255;Q:1;
```

> Surah 2 ayat 1–5 plus ayah 255, then all of Surah 1.

Usable as an idiomatic Rust crate, or through a C ABI from any language that speaks one — Dart FFI, Python, Go, C/C++ — on Linux, Windows, macOS, Android NDK, and iOS. Not coupled to Flutter.

> **Status:** the lexer, parser, source registry, Quran resolver, hadith resolvers, and CLI work today. The C ABI layer is designed but not yet written, so the crate currently builds as an `rlib` only. See [docs/plan.md](docs/plan.md) for the full design and phase list.

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

| Code | Collection | |
| --- | --- | --- |
| `Q` | Quran | |
| `B` | Sahih al-Bukhari | |
| `M` | Sahih Muslim | |
| `AD` | Sunan Abi Dawud | |
| `T` | Jami' at-Tirmidhi | |
| `N` | Sunan an-Nasa'i | |
| `IM` | Sunan Ibn Majah | |
| `HM` | Hisnul Muslim | *no data source yet — not registered* |

`qql --sources` prints the registered codes.

### Hadith numbering

For hadith, `B:C:N` means **chapter C, the N-th hadith within that chapter** — `B:1:1` is the first hadith of Kitab Bad' al-Wahy. That is the numbering the upstream per-chapter data files use, and it is QQL's canonical scheme for v1.

It is **not** the book-global number most citations use ("Bukhari 6018"). Mapping global numbers is a resolver concern and can be added later without touching the grammar.

### Examples

```text
Q:1                    all of Surah 1
Q:2:255                Ayat al-Kursi
Q:2:1-5                ayat 1 through 5
Q:2:1-5,10,20-25,255   mixed ranges and singles
Q:1;Q:2:255;Q:112;     three references
B:1:1-10               Bukhari, book 1, hadith 1–10
HM:27                  Hisnul Muslim, item 27
```

### Ordering and duplicates

Query order is preserved — `Q:2:255,1-3` returns 255, 1, 2, 3. Nothing is sorted for you.

Duplicates are removed **within** a single reference (`Q:2:1-5,3,4` → 1, 2, 3, 4, 5) but **across** references they are kept (`Q:2:255;Q:2:255;` returns two items).

## Build

```bash
cargo build --release
cargo test
```

The crate currently builds as an `rlib` plus the `qql` binary. `cdylib` and `staticlib` are added along with the FFI layer — building them before there is anything to export would only slow the build down.

Lints are part of the build contract:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
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

> **Not implemented yet.** This section is the contract the FFI layer will honor; see [docs/plan.md §8](docs/plan.md).

`include/qql.h` will be generated by [cbindgen](https://github.com/mozilla/cbindgen) and committed, so C consumers need no Rust toolchain to build against the library.

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
cc example.c -Iinclude -Ltarget/release -lqql -o example
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

Dart shape:

```dart
final ptr = qqlContextExecute(ctx, query.toNativeUtf8().cast());
final result = ptr.cast<Utf8>().toDartString();
qqlFreeString(ptr);
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

## Adding a new source

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
```

Files are read on first use and cached in the `Context` until it drops — `Q:2:255` loads roughly 50 KB, not the whole mushaf. There is no eviction policy.

Both datasets carry their own licenses; see the submodule directories.

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) for build, test, and style requirements.

## License

GNU General Public License v3.0 or later — see [LICENSE.md](LICENSE.md).
