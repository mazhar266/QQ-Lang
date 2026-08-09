# Build Quran Query Language (QQL) Native C Library

I want you to design and implement a small, portable native C library called **QQL — Quran Query Language**.

The library will parse compact textual references to Islamic texts and resolve those references against local JSON data files.

The first version should support:

- Quran
- Sahih al-Bukhari
- Sahih Muslim
- Several additional Hadith collections later
- Hisnul Muslim
- Easy addition of more sources without rewriting the parser

The library must be written primarily in **portable C**, suitable for compilation using GCC or Clang.

It should eventually be usable through:

- Flutter / Dart FFI
- Linux
- Windows
- macOS
- Android NDK
- iOS native linking
- CLI applications
- Other languages through a C ABI

Do not couple the core library to Flutter.

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

Separate the project into these layers:

```text
Input query
    ↓
Lexer / tokenizer
    ↓
Parser
    ↓
QQL AST / normalized query representation
    ↓
Validation
    ↓
Source resolver
    ↓
JSON data repository
    ↓
Normalized result
    ↓
JSON serializer
    ↓
Returned UTF-8 string
```

Keep these concerns independent.

The parser must not directly read Quran or Hadith JSON files.

The data loader must not contain query parsing logic.

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

Normalize source identifiers to uppercase.

---

# 4. Abstract Syntax Tree

Create an internal AST or normalized representation similar to:

```c
typedef struct {
    uint32_t from;
    uint32_t to;
} qql_range_t;

typedef struct {
    char *source;
    uint32_t primary;

    qql_range_t *ranges;
    size_t range_count;

    bool select_all;
} qql_reference_t;

typedef struct {
    qql_reference_t *references;
    size_t reference_count;
} qql_query_t;
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

Do not use JSON internally as the parser AST.

Use C structs internally.

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
          "ar": "بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ",
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

Prefer a design that does not require loading the entire Quran or all Hadith collections into memory.

For the first implementation, source files may be loaded lazily.

---

# 6. Canonical Returned Result

The public API should return a UTF-8 JSON string.

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

# 7. Simple API

The most important public API should eventually be approximately:

```c
char *qql_execute(const char *query);
```

Example:

```c
char *json = qql_execute("Q:2:255;");
```

Returned value:

```json
{
  "ok": true,
  "results": [
    {
      "source": "Q",
      "primary": 2,
      "number": 255,
      "ar": "...",
      "en": "..."
    }
  ]
}
```

Because this will be called through FFI, memory ownership must be explicit.

Provide:

```c
void qql_free_string(char *ptr);
```

Flutter/Dart should therefore be able to do:

```text
qql_execute(...)
    ↓
Pointer<Utf8>
    ↓
convert to Dart String
    ↓
qql_free_string(...)
```

Never expose internal structures across the public FFI boundary unless necessary.

Keep the ABI simple.

---

# 8. Context-Based API

Also design a better long-running API.

Example:

```c
typedef struct qql_context qql_context_t;

qql_context_t *qql_context_create(const char *data_directory);

char *qql_context_execute(
    qql_context_t *ctx,
    const char *query
);

void qql_context_destroy(qql_context_t *ctx);

void qql_free_string(char *ptr);
```

Usage:

```c
qql_context_t *ctx = qql_context_create("./data");

char *result = qql_context_execute(
    ctx,
    "Q:2:1-5,255;Q:1;"
);

printf("%s\n", result);

qql_free_string(result);
qql_context_destroy(ctx);
```

Prefer this context-based design internally.

The simple `qql_execute()` API may wrap a default context.

---

# 9. Source Resolver Architecture

Create an extensible source registry.

Conceptually:

```c
typedef struct {
    const char *code;
    const char *name;

    qql_error_t (*validate)(
        qql_context_t *,
        const qql_reference_t *
    );

    qql_error_t (*resolve)(
        qql_context_t *,
        const qql_reference_t *,
        qql_result_builder_t *
    );
} qql_source_handler_t;
```

Then register:

```text
Q  → Quran resolver
B  → Bukhari resolver
M  → Muslim resolver
HM → Hisnul Muslim resolver
```

The core parser must not contain code such as:

```c
if (source == "Q") ...
else if (source == "B") ...
```

Source-specific behavior belongs in handlers.

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

rather than automatically sorting them.

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

Implement this distinction clearly.

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

The parser should only know that values are integers.

The Quran resolver knows:

```text
Surah must be 1..114
Ayah must exist in that Surah
```

This separation is important.

---

# 13. Error Result Format

Never return malformed JSON.

Errors should return something like:

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

Possible error codes:

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

Create an enum for internal errors.

Public output can serialize enum values to stable strings.

---

# 14. Parser Diagnostics

Track character positions.

For example:

```text
Q:2:1-,5
      ^
```

should be capable of producing an error position.

Store at least:

```c
size_t offset;
```

Optional future support:

```c
line
column
```

Since normal QQL queries are one line, offset is sufficient for now.

---

# 15. JSON Library

Do not write a full JSON parser yourself.

Choose a small, portable C JSON library.

Good characteristics:

- easy to vendor
- supports UTF-8
- works with GCC / Clang
- works on Android/iOS/Linux/macOS/Windows
- minimal dependencies

Candidates may include:

```text
yyjson
cJSON
jansson
```

Choose one and explain why.

Prefer performance, portability, and simple vendoring.

Use the selected library for:

1. reading source JSON files
2. creating returned JSON

Keep JSON-specific implementation isolated under something like:

```text
src/json/
```

---

# 16. Unicode

All input and output strings must use UTF-8.

Arabic must pass through unchanged.

Do not attempt to normalize Arabic Unicode in version 1.

Do not modify:

- tashkeel
- Quranic marks
- Arabic punctuation
- zero-width characters

Treat the source JSON text as authoritative.

---

# 17. Directory Structure

Create a maintainable project layout similar to:

```text
qql/
├── CMakeLists.txt
├── README.md
├── LICENSE
├── include/
│   └── qql/
│       └── qql.h
├── src/
│   ├── qql.c
│   ├── context.c
│   ├── context.h
│   ├── lexer.c
│   ├── lexer.h
│   ├── parser.c
│   ├── parser.h
│   ├── ast.c
│   ├── ast.h
│   ├── error.c
│   ├── error.h
│   ├── result.c
│   ├── result.h
│   ├── source_registry.c
│   ├── source_registry.h
│   ├── sources/
│   │   ├── quran.c
│   │   ├── quran.h
│   │   ├── bukhari.c
│   │   ├── bukhari.h
│   │   ├── muslim.c
│   │   ├── muslim.h
│   │   ├── hisnul_muslim.c
│   │   └── hisnul_muslim.h
│   └── json/
│       ├── json_reader.c
│       └── json_reader.h
├── third_party/
├── tests/
│   ├── test_lexer.c
│   ├── test_parser.c
│   ├── test_quran.c
│   ├── test_errors.c
│   └── fixtures/
├── data/
│   ├── quran/
│   ├── bukhari/
│   ├── muslim/
│   └── hisnul_muslim/
├── examples/
│   ├── basic.c
│   └── cli.c
└── bindings/
    └── dart/
        └── README.md
```

Adjust this structure when there is a strong technical reason.

---

# 18. Build System

Use CMake.

Support at least:

```bash
cmake -S . -B build
cmake --build build
```

Produce:

Linux:

```text
libqql.so
```

Windows:

```text
qql.dll
```

macOS:

```text
libqql.dylib
```

Also support static builds:

```text
libqql.a
```

Use symbol export macros in the public header.

Example:

```c
#ifdef _WIN32
#ifdef QQL_BUILD
#define QQL_API __declspec(dllexport)
#else
#define QQL_API __declspec(dllimport)
#endif
#else
#define QQL_API
#endif
```

Apply `QQL_API` to public functions.

---

# 19. C ABI

The public header must compile as C and also work when included from C++.

Use:

```c
#ifdef __cplusplus
extern "C" {
#endif

...

#ifdef __cplusplus
}
#endif
```

Do not expose compiler-specific C++ ABI.

---

# 20. Thread Safety

Design the library so separate contexts can safely be used by separate threads.

Avoid mutable global state.

If a default context exists for `qql_execute()`, document its thread-safety characteristics.

Prefer:

```text
qql_context_t
```

for serious use.

---

# 21. Caching

Do not prematurely build a complicated cache.

However, design the context so parsed JSON files can later be cached.

A reasonable first implementation:

```text
first query for Surah 2
    ↓
load data/quran/002.json
    ↓
parse
    ↓
keep parsed representation in qql_context_t
```

Future queries reuse it.

Free everything when:

```c
qql_context_destroy()
```

is called.

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

For QQL v1, choose one internal canonical numbering scheme.

Document that numbering scheme.

Do not attempt to solve all edition-numbering differences inside the parser.

---

# 23. Hisnul Muslim

Hisnul Muslim may have a simpler indexing model.

Example:

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

Again, keep this outside the parser.

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

and allow source-specific metadata.

---

# 25. Public API Version

Expose version information:

```c
const char *qql_version(void);
```

Example:

```text
0.1.0
```

Follow semantic versioning.

---

# 26. Proposed Public Header

Aim for something roughly like:

```c
#ifndef QQL_H
#define QQL_H

#include <stddef.h>

#ifdef _WIN32
#ifdef QQL_BUILD
#define QQL_API __declspec(dllexport)
#else
#define QQL_API __declspec(dllimport)
#endif
#else
#define QQL_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct qql_context qql_context_t;

QQL_API const char *qql_version(void);

QQL_API qql_context_t *
qql_context_create(const char *data_directory);

QQL_API void
qql_context_destroy(qql_context_t *ctx);

QQL_API char *
qql_context_execute(
    qql_context_t *ctx,
    const char *query
);

QQL_API char *
qql_execute(const char *query);

QQL_API void
qql_free_string(char *ptr);

#ifdef __cplusplus
}
#endif

#endif
```

Modify it if needed, while preserving a very small FFI surface.

---

# 27. CLI

Create a simple command-line application for development.

Example:

```bash
./qql "Q:2:255"
```

Output:

```json
{
  "ok": true,
  "results": [...]
}
```

Also support:

```bash
./qql --data ./data "Q:1;Q:2:255"
```

Pretty-print JSON in the CLI if practical.

The native library itself should return compact JSON unless configured otherwise.

---

# 28. Tests

Tests are important.

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

Invalid queries:

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

UTF-8 tests must verify that Arabic survives:

```text
JSON file → native parser → returned JSON
```

without corruption.

---

# 29. Memory Tests

Compile and run tests with sanitizers when available.

For GCC/Clang:

```bash
-fsanitize=address,undefined
```

Run queries repeatedly and verify:

- no leaks
- no use-after-free
- no double-free
- no invalid reads
- no invalid writes

Every allocation must have clear ownership.

Document ownership rules in code comments where necessary.

---

# 30. Fuzz-Friendly Parser

Keep lexer/parser code deterministic and bounds-safe.

Never read beyond the provided null-terminated string.

Avoid unsafe functions such as:

```c
strcpy
sprintf
gets
```

Prefer bounded or dynamically sized operations.

The parser should be suitable for fuzzing later with libFuzzer/AFL++.

---

# 31. Performance Goals

QQL queries are generally tiny.

Parser performance does not need exotic optimization.

Prioritize:

1. correctness
2. memory safety
3. predictable errors
4. portability
5. extensibility

The potentially expensive part is loading large Islamic-text datasets.

Use lazy loading and caching where reasonable.

---

# 32. Flutter/Dart FFI Compatibility

Do not implement the Flutter plugin yet.

Make sure the native API can easily be called from Dart FFI.

Expected Dart shape later:

```dart
final ptr = qqlExecute(
  query.toNativeUtf8().cast()
);

final result = ptr.cast<Utf8>().toDartString();

qqlFreeString(ptr);
```

Therefore:

- return null-terminated UTF-8
- library owns result before return
- caller owns returned buffer
- caller releases it using `qql_free_string`
- never require Dart to free native memory with Dart's allocator

---

# 33. Future Syntax

Do not implement these yet unless they naturally fit the architecture.

The language may later support options such as:

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

Do not prematurely build these features.

Just make the lexer/parser architecture extensible enough that adding tokens later is reasonable.

---

# 34. Source Aliases

Potential future aliases:

```text
BUKHARI
B

MUSLIM
M

HISN
HM
```

For version 1, only canonical short codes are required.

The source registry should make aliases easy to add later.

---

# 35. Parser vs Resolver Contract

Maintain this distinction strictly.

Parser:

```text
Q:500:999
```

may be syntactically valid.

Quran resolver then rejects it semantically.

Similarly:

```text
XYZ:1:2
```

is syntactically valid.

The source registry rejects:

```text
XYZ
```

as an unknown source.

This separation must be reflected in the code architecture and tests.

---

# 36. Development Phases

Implement the project incrementally.

Do not generate everything in one huge unreviewable commit.

## Phase 1 — Foundation

Create:

- directory structure
- CMake project
- public header
- error infrastructure
- basic tests
- CLI skeleton

Make sure everything compiles.

## Phase 2 — Lexer

Implement tokenization.

Create tests.

Possible token types:

```text
TOKEN_IDENTIFIER
TOKEN_INTEGER
TOKEN_COLON
TOKEN_SEMICOLON
TOKEN_COMMA
TOKEN_DASH
TOKEN_EOF
TOKEN_INVALID
```

Track offsets.

## Phase 3 — Parser

Implement:

```text
query
reference
selector
range
```

Build the C AST.

Implement complete AST cleanup.

Add parser tests.

## Phase 4 — JSON Integration

Vendor/select the JSON library.

Create JSON reader and writer abstractions.

Test Arabic UTF-8.

## Phase 5 — Source Registry

Implement generic source registration and lookup.

Register Quran.

Unknown sources should return structured errors.

## Phase 6 — Quran Resolver

Implement:

```text
Q:surah
Q:surah:ayah
Q:surah:start-end
Q:surah:a,b,c
Q:surah:a-b,c,d-e
```

Use test fixture Quran data before importing an entire production dataset.

## Phase 7 — Result Serialization

Return stable JSON.

Implement:

```c
qql_context_execute()
qql_free_string()
```

Ensure all errors also return JSON.

## Phase 8 — CLI

Complete CLI execution.

Examples:

```bash
qql "Q:1"
qql "Q:2:255"
qql "Q:2:1-5,255"
```

## Phase 9 — Hadith Resolver

Add Bukhari first.

Use the same parser.

Only the resolver should be new.

Then add Muslim.

## Phase 10 — Hisnul Muslim

Add HM source handler.

Verify that sources with different logical structures still work through the same generic QQL parser.

## Phase 11 — FFI Verification

Compile shared library.

Create a tiny Dart console test using `dart:ffi`.

Do not build a full Flutter UI.

Verify:

```text
Dart
→ qql_context_execute()
→ C
→ JSON data
→ C JSON result
→ Dart String
```

---

# 37. Coding Style

Use C11 unless there is a strong reason otherwise.

Compile with strict warnings:

```text
-Wall
-Wextra
-Wpedantic
```

Prefer additionally during development:

```text
-Werror
```

Keep functions small.

Avoid giant parser functions.

Use `const` aggressively.

Do not hide ownership.

Avoid global mutable variables.

Use descriptive names.

Prefer explicit error return values over magic values.

---

# 38. Documentation

README should explain:

## What QQL is

Example:

```text
Q:2:1-5,255;Q:1;
```

## Syntax

Explain source, primary selector, ranges, commas, and semicolons.

## Build

```bash
cmake -S . -B build
cmake --build build
```

## CLI

```bash
./build/qql "Q:2:255"
```

## C usage

Provide a complete example.

## FFI ownership

Clearly explain:

```text
qql_context_execute() allocates
qql_free_string() frees
```

## Adding a new source

Document exactly how a future developer adds:

```text
T = Tirmidhi
```

without modifying parser logic.

---

# 39. Example End-to-End Behavior

Given:

```text
Q:2:1-3,255;Q:1:1;
```

the library should:

1. tokenize the input
2. parse two references
3. normalize their selectors
4. locate source handler `Q`
5. validate Surah/ayah numbers
6. lazily load required Quran JSON files
7. resolve requested ayat
8. preserve requested ordering
9. serialize them as UTF-8 JSON
10. return an allocated `char *`

Conceptual response:

```json
{
  "ok": true,
  "results": [
    {
      "source": "Q",
      "collection": "Quran",
      "surah": 2,
      "ayah": 1,
      "ar": "...",
      "en": "..."
    },
    {
      "source": "Q",
      "collection": "Quran",
      "surah": 2,
      "ayah": 2,
      "ar": "...",
      "en": "..."
    },
    {
      "source": "Q",
      "collection": "Quran",
      "surah": 2,
      "ayah": 3,
      "ar": "...",
      "en": "..."
    },
    {
      "source": "Q",
      "collection": "Quran",
      "surah": 2,
      "ayah": 255,
      "ar": "...",
      "en": "..."
    },
    {
      "source": "Q",
      "collection": "Quran",
      "surah": 1,
      "ayah": 1,
      "ar": "...",
      "en": "..."
    }
  ]
}
```

---

# 40. Important Design Principle

The most important architectural rule is:

```text
QQL parser knows syntax.
Source handlers know Islamic-book structure.
JSON repository knows storage.
Public API knows FFI.
```

Do not mix these responsibilities.

I want to eventually be able to add a source such as:

```text
T:5:10
```

by implementing and registering a Tirmidhi resolver without changing the lexer or parser.

---

# 41. First Task

Start by inspecting the current repository.

If it is empty, initialize the structure described above.

Then implement only:

1. CMake project
2. public API skeleton
3. error model
4. lexer
5. parser
6. AST
7. parser unit tests
8. simple CLI that can parse and display a normalized query

Do **not** implement Quran JSON resolution in the first step.

For example:

```bash
./qql-parse "Q:2:1-5,255;Q:1;Q:3:2;"
```

should initially print something equivalent to:

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

Run the tests.

Fix compiler warnings.

Then show me:

- files created
- architecture decisions
- public types
- parser behavior
- test results
- any design concerns

Stop there so the next implementation phase can be reviewed separately.