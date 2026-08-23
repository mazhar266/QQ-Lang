# QQL — Quran Query Language

A small, portable Rust library that parses compact textual references to Islamic texts and resolves them against local JSON data.

```text
Q:2:1-5,255;Q:1;
```

> Surah 2 ayat 1–5 plus ayah 255, then all of Surah 1.

Usable as an idiomatic Rust crate, or through a C ABI from any language that speaks one — Dart FFI, Python, Go, C/C++ — on Linux, Windows, macOS, Android NDK, and iOS. Not coupled to Flutter.

> **Status:** complete for v1 — parser, source registry, Quran / hadith / Hisnul Muslim resolvers, CLI, C ABI, and a Dart binding, with 104 Rust tests (128 with both optional features) and 9 Dart tests. See [docs/plan.md](docs/plan.md) for the full design.

## Syntax

```text
query      := reference (';' reference)* ';'?
reference  := (source ':')? body
body       := text                          // Q:"..."       search
            | ':' selector                  // B::100
            | group (',' group)*
group      := primary ':' text              // Q:1:"..."     search
            | primary ':' scope ':' text    // Q:1:3~5:"..." search
            | primary (':' selector)?
scope      := integer '~' integer
selector   := item (',' item)*
item       := integer | integer '-' integer
text       := quoted                          exact substring
            | '`' ... '`' ('~' integer)?     similarity, optionally capped
            | '?' quoted ('~' integer)?      full text, optionally capped
quoted     := '"' ... '"' | "'" ... "'"
source     := [A-Za-z][A-Za-z0-9_]*
```

| Part | Meaning |
| --- | --- |
| **source** | Collection code, normalized to uppercase (`Q`, `B`, `M`, `HM`, …). **Optional — omitted means the Quran.** |
| **primary** | First-level index — Surah for Quran, chapter for Hadith and Hisnul Muslim |
| **selector** | Optional list of items within the primary. Omit it to select everything. |
| `-` | Inclusive range: `1-5` → 1, 2, 3, 4, 5 |
| `,` | Joins items, and joins groups: `1:2,3,2:5` |
| `;` | Separates references. Only needed to switch collection or start a new primary; a trailing one is optional. |
| `::` | Skips the primary: `B::100` numbers across the whole collection. |
| `"…"` / `'…'` | Full-text search within whatever the reference scopes. |
| `~` | Scopes a search to an item range: `Q:1:3~5:"…"`, or caps results: ``Q:`…`~5``. |
| `` `…` `` | Similarity search, ranked by score. Needs the `vector` feature. |
| `?"…"` | Ranked full-text search with stemming. Needs the `fulltext` feature. |

Whitespace around tokens is accepted: `Q : 2 : 1-5, 255;`

### Groups

One source can address several chapters at once. The rule: **an integer followed by `:` starts a new group**, so it is a primary rather than another selector item.

```text
q:1:2,3,2:3,4-6
```

> Surah 1 ayat 2 and 3, then Surah 2 ayat 3 and 4–6.

```text
Q:1,2:255        all of Surah 1, then Surah 2 ayah 255
Q:1,2,3          three whole Surahs
B:1:1,2:5        Bukhari chapter 1 hadith 1, then chapter 2 hadith 5
```

A range is never a primary, so `Q:1:1-5:3` is a syntax error rather than a second reading. Both things it could have meant are writable, and they differ:

```text
Q:1:1-5;3     Surah 1 ayat 1–5, then all of Surah 3
Q:1:1-5,3     Surah 1 ayat 1–5, plus ayah 3 (already in the range, so deduped)
```

### Omitting the source

Leave the code out and the query is Quran:

```text
1                the whole of Surah 1
2:255            Ayat al-Kursi
1,2:255          all of Surah 1, then Surah 2 ayah 255
1:2,3,2:3,4-6    groups work the same
```

`;` is only needed to change collection, and the last one can always be dropped:

```text
1:1;b:1:1        Quran 1:1, then Bukhari chapter 1 hadith 1
q:1;b:1          same as q:1;b:1;
```

### A stated source carries forward

Once a query names a collection, everything after it belongs to that collection until another code says otherwise:

```text
b:1:1;3          Bukhari 1:1, then Bukhari chapter 3
b:1:1;q:3        Bukhari 1:1, then Surah 3
b:1:1;3;q:1;2    Bukhari 1:1 and 3, then Surah 1 and Surah 2
```

The Quran default applies only when nothing has been named yet — `1,2:255` and the leading `1:1` in `1:1;b:1:1`.

The parser handles the carry-forward, which stays pure syntax: "reuse the previous code" needs no idea what the codes mean. It never learns that `Q` is the default either — it records that no code was stated, and the registry substitutes one when resolving.

### Search

A quoted term searches the text instead of addressing it. The reference in
front of it says *where* to look:

```text
"text"            search the Quran — the default source
q:"text"          search the whole Quran
q:1:"text"        search Surah 1
q:1:3~5:"text"    search ayat 3–5 of Surah 1
b:1:"text"        search Bukhari chapter 1
b:"text"          search all of Bukhari
```

Either quote delimits a term, identically — so a term containing one can
always be written with the other:

```text
q:1:'الحمد'       same as q:1:"الحمد"
b:1:"Allah's"     an apostrophe needs the double quotes
q:'say "this"'    and the reverse
```

There are no escapes: the first matching close ends the term.

**Arabic and English are searched together.** Every record in scope is tried
against both its `ar` and its `en`, so `q:2:"prayer"` and `q:2:"الصلاة"` both
work without saying which language you mean.

Results are ordinary records, in the order they appear in the text. A search
that matches nothing is an empty `results`, not an error.

`~` rather than `-` marks a search scope, which keeps it apart from the `1-5`
of an ordinary selector — `Q:1:3-5` returns three ayat, `Q:1:3~5:"x"` searches
them.

**Arabic is matched with the marks folded away.** The Quran text is fully
diacritized, so a typed `الحمد` shares no substring with the stored
`ٱلْحَمْدُ`. For comparison only, QQL drops harakat, sukun, the superscript
alef and the Quranic annotation marks, folds the alef seats (`أ إ آ ٱ` → `ا`),
`ى` → `ي` and `ة` → `ه`, and lowercases ASCII. Records come back with their
text exactly as stored — nothing rewrites scripture.

Matching is plain substring, not word or stem matching: `"mercy"` does not
find *Merciful*, and `"pray"` does find *prayer*. It covers `ar` and `en` but
not metadata, so `"Al-Fatihah"` finds nothing — Surah names are not part of
the verse text.

The scan is linear over the scope — at 6236 ayat a whole-Quran search takes
about 100 ms cold, including reading every chapter file, and there is no index
to fall out of step with the text. Sources that declare no book-wide axis
(a custom source without a `flat` block) refuse an unscoped search with
`QQL_UNSUPPORTED` rather than quietly searching less than you asked for.

### Three ways to search

| Form | Matches | Order | Needs |
| --- | --- | --- | --- |
| `"term"` `'term'` | folded substring | positional | nothing |
| `?"term"` | words, stemmed, BM25 | **ranked** | `fulltext` feature + index |
| `` `term` `` | vector similarity | **ranked** | `vector` feature + index |

The spelling picks the engine, so a build flag never changes what a query
means. Both optional engines are refused with `QQL_UNSUPPORTED` when their
feature or index is missing, rather than quietly falling back to substring
matching.

### Full-text search — optional

Backed by [tantivy](https://github.com/quickwit-oss/tantivy). A large
dependency, so it is a feature, **off by default**:

```toml
qql = { version = "0.1", features = ["fulltext"] }
```

```bash
cargo run --features fulltext --bin qql-index          # every source
cargo run --features fulltext --bin qql-index -- --source Q
cargo run --features fulltext -- 'q:?"mercy"~5'
```

What it brings over `"term"`:

```text
q:1:"mercy"        0 hits  — "Merciful" does not contain "mercy"
q:1:?"mercy"       2 hits  — stemming reaches it
```

The term carries tantivy's own query syntax, so boolean and phrase queries
work. A phrase needs inner quotes, which is what the other delimiter is for:

```text
q:?"mercy OR forgiveness"~3
q:?"prayer AND charity"~3
q:?"prayer -charity"~3
q:?'"straight path"'~3        phrase
```

Arabic is indexed **folded** — the corpus is fully diacritized, and an index
over raw tokens would only match a query reproducing every mark. English is
indexed with tantivy's `en_stem` tokenizer.

Indexing all eight sources takes about three seconds and 16 MB. Unlike the
vector indexes these are **not** committed — a tantivy index is a directory of
binary segments whose names change on every rebuild, so it is built locally.

### Similarity search — optional

Backticks ask for *similar* rather than *contains*. It is a cargo feature,
**off by default**, so the core crate stays at two dependencies and needs no
extra assets:

```toml
qql = { version = "0.1", features = ["vector"] }
```

```bash
python3 scripts/build-vectors.py --source Q    # generates sources/vectors/Q.qv
cargo run --features vector -- 'q:`worship`~3'
```

```text
q:`worship`~3      top 3 across the Quran   → 109:2, 109:3, 109:4
q:1:`worship`      within Surah 1           → 1:5
q:1:3~5:`help`     within ayat 3–5          → 1:5
q:1:`حمد`          undiacritized, no article → 1:2
```

Hits are ordinary records with two fields added: `score` and `"ranked": true`.
**This is the one place in QQL where output is ordered by relevance rather
than position** — everything else preserves the order written. `~N` caps the
result count (default 20), and weak matches are dropped, so a search can
return fewer than the cap or nothing at all.

Without the feature, or without an index for that source, a backtick query is
**refused** with `QQL_UNSUPPORTED` naming the fix. It never silently falls
back to substring matching — answering a different question than the one asked
is worse than saying no.

#### What it actually does

The shipped embedder is a **signed hash projection of folded tokens** — whole
words plus character trigrams, hashed onto 256 dimensions, normalized and
quantized to `int8`. No model, no weights, no asset beyond the index itself,
and query embedding is a handful of hashes rather than a transformer.

That makes it **fuzzy lexical matching, not semantic**. It tolerates
diacritics, prefixes and suffixes — which is worth a great deal for Arabic —
but it does not know that *charity* and *zakat* are related, and a nonsense
query returns weak noise rather than nothing.

Real semantic vectors are a build-time swap: emit an index with a different
embedder id and teach [src/vector.rs](src/vector.rs) to embed queries the same
way. The file format, the scan, the scoping and the result shape are all
unchanged by that.

#### Cost

| | |
| --- | --- |
| Index size | ~3.3 MB Quran, ~21 MB all eight sources |
| Query | hash the term, then a flat `int8` dot-product scan |
| Structure | none — no ANN index, nothing to fall out of sync |

At ~40,000 records corpus-wide a flat scan is a few million integer
multiply-accumulates; an approximate-nearest-neighbour index would add a large
dependency and a second artifact that can drift, to beat something already
fast enough on a weak core. Scoping narrows the scan further.

All eight indexes are committed, so the feature works without a build step.
Rebuild after changing text:

```bash
python3 scripts/build-vectors.py              # all sources, ~90 s, 21 MB
python3 scripts/build-vectors.py --source Q   # just one
```

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

There are two ways to address an item, and every source supports both.

**Within a chapter** — `SOURCE:primary:n` counts from 1 inside `primary`:

- **Quran** — `Q:2:255` is Surah 2, ayah 255.
- **Hadith** — `B:1:1` is the first hadith of chapter 1 (Kitab Bad' al-Wahy), matching the upstream per-chapter files.
- **Hisnul Muslim** — `HM:27:1` is the first supplication of chapter 27.

**Across the whole book** — `SOURCE::n` skips the chapter and uses traditional continuous numbering, which is what citations normally mean:

```text
B::100        hadith 100 of Sahih al-Bukhari
Q::100        the 100th ayah of the mushaf (Surah 2, ayah 93)
HM::75        the 75th supplication of Hisnul Muslim
B::1-10,255   ranges and lists work the same
```

Records from the flat form carry `"numbering": "book"`, so a single query mixing both forms — `B:1:1;B::100;` — stays unambiguous. They also still report the chapter they belong to.

Bounds are the whole collection: 1–6236 for the Quran, 1–7277 for Bukhari, 1–267 for Hisnul Muslim.

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
B::100                 Bukhari hadith 100, traditional book-wide numbering
Q::1-7;B::1;           flat form mixes freely with the rest
1                      no source code — the whole of Surah 1
1,2:255                all of Surah 1, then Surah 2 ayah 255
q:1:2,3,2:3,4-6        two groups under one source
b:1:1;3                the source carries forward — Bukhari twice
b:1:1;q:3              …until another code switches it
q:1:"الحمد"            search Surah 1 in Arabic
q:2:"prayer"           search Surah 2 in English
q:1:3~5:"You"          search ayat 3–5
'mercy'                either quote works
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

Error codes: `QQL_EMPTY_QUERY`, `QQL_INVALID_CHARACTER`, `QQL_EXPECTED_SOURCE`, `QQL_EXPECTED_COLON`, `QQL_EXPECTED_NUMBER`, `QQL_EXPECTED_TEXT`, `QQL_UNTERMINATED_TEXT`, `QQL_INVALID_RANGE`, `QQL_UNKNOWN_SOURCE`, `QQL_UNSUPPORTED`, `QQL_REFERENCE_NOT_FOUND`, `QQL_DATA_FILE_NOT_FOUND`, `QQL_INVALID_DATA_FILE`, `QQL_INTERNAL_ERROR`.

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
| `flat` | Enables `X::100`: `{ "path", "items", "item_id" }` pointing at the collection numbered straight through. Without it, a flat reference is an error. |
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
  quran/chapters/{1..114}.json                         generated — see below
  quran/verses/{1..6236}.json                          generated — mushaf order
  hadith-json/db/by_chapter/the_9_books/{book}/{chapter}.json
  Hisn-Muslim-Json/husn_en.json                        all 132 chapters in one file
```

### The Quran directory is generated, not vendored

`sources/quran/` is built by [scripts/build-quran.py](scripts/build-quran.py) and committed:

```bash
python3 scripts/build-quran.py            # fetch Tanzil, rebuild
python3 scripts/build-quran.py --check    # verify only, write nothing
```

The Arabic comes from **Tanzil's Uthmani text**; names, the English translation and
the per-ayah transliteration still come from the `quran-json-arabic` submodule.

The split exists because that submodule spells three marks with codepoints that mean
something else in Unicode — `U+0657 INVERTED DAMMA` for an open fathatan, `U+065E` for
a dammatan, `U+0656` for a kasratan. A font that follows Unicode draws them literally,
so 2:286's `إِصْرًا` gains a damma above the reh and reads *isru* rather than *isran*.
Tanzil uses the standard marks and carries the pause, sajdah and silence signs the
submodule omits.

The generator refuses to write unless the two texts agree: same ayah counts per surah,
no misused codepoint in the output, the basmalah stripped from exactly the 112 surahs
that carry it, and the consonant skeletons matching except for the handful of ayat
where the two disagree about hamza spelling (`ئ` versus `ي` plus a combining hamza).

`quran/verses/` carries only the English translation. The submodule ships ten languages
per ayah, which made the equivalent directory 14 MB rather than 2.6 MB, and QQL reads
only the one.

Files are read on first use and cached in the `Context` until it drops — `Q:2:255` loads roughly 50 KB, not the whole mushaf. There is no eviction policy.

The datasets carry their own licenses; see the submodule directories, and
`sources/quran/TANZIL-LICENSE.txt` for the Quran text.

Upstream data is taken as authoritative and is never rewritten, but it is not uniform. The Hisnul Muslim file in particular carries a UTF-8 BOM, stores its chapters out of numerical order, repeats a key in two entries, and misspells one field name. Those are absorbed where they belong — the BOM in [src/repo.rs](src/repo.rs) since it is a storage concern, the rest in [src/sources/hisnul.rs](src/sources/hisnul.rs) — rather than by patching the data.

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) for build, test, and style requirements.

## License

GNU General Public License v3.0 or later — see [LICENSE.md](LICENSE.md).
