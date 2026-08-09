# QQL — Quran Query Language

A small, portable C library that parses compact textual references to Islamic texts and resolves them against local JSON data.

```text
Q:2:1-5,255;Q:1;
```

> Surah 2 ayat 1–5 plus ayah 255, then all of Surah 1.

Written in C11, no Flutter coupling. Usable from CLI, C/C++, and any language with a C ABI (Dart FFI, Python, Go, …) on Linux, Windows, macOS, Android NDK, and iOS.

> **Status:** early. See [docs/plan.md](docs/plan.md) for the full design and phase list.

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

| Code | Collection |
| --- | --- |
| `Q` | Quran |
| `B` | Sahih al-Bukhari |
| `M` | Sahih Muslim |
| `AD` | Sunan Abi Dawud |
| `T` | Jami' at-Tirmidhi |
| `N` | Sunan an-Nasa'i |
| `IM` | Sunan Ibn Majah |
| `HM` | Hisnul Muslim |

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
cmake -S . -B build
cmake --build build
```

Produces `libqql.so` / `qql.dll` / `libqql.dylib`, plus `libqql.a` for static linking.

Development builds use `-Wall -Wextra -Wpedantic -Werror`. Run the tests with sanitizers where available:

```bash
cmake -S . -B build -DCMAKE_C_FLAGS="-fsanitize=address,undefined"
cmake --build build
ctest --test-dir build
```

## CLI

```bash
./build/qql "Q:2:255"
./build/qql --data ./data "Q:1;Q:2:255"
```

Output is pretty-printed JSON. The library itself returns compact JSON.

## C usage

```c
#include <stdio.h>
#include <qql/qql.h>

int main(void) {
    qql_context_t *ctx = qql_context_create("./data");
    if (!ctx) return 1;

    char *result = qql_context_execute(ctx, "Q:2:1-5,255;Q:1;");
    printf("%s\n", result);

    qql_free_string(result);
    qql_context_destroy(ctx);
    return 0;
}
```

```bash
cc example.c -Iinclude -Lbuild -lqql -o example
```

`qql_execute(query)` is a convenience wrapper over a default context.

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

All input and output is UTF-8. Arabic passes through byte-for-byte — tashkeel, Quranic marks, and zero-width characters are never normalized.

## FFI ownership

```text
qql_context_execute()  allocates the returned buffer
qql_free_string()      frees it
```

Rules:

- Returned strings are null-terminated UTF-8.
- The **caller** owns the buffer once it is returned, and must release it with `qql_free_string()` — never with the host language's allocator.
- Never call `free()` from another runtime on a QQL pointer.
- `qql_context_t` is opaque. It never crosses the FFI boundary as anything but a pointer.

Dart shape:

```dart
final ptr = qqlContextExecute(ctx, query.toNativeUtf8().cast());
final result = ptr.cast<Utf8>().toDartString();
qqlFreeString(ptr);
```

### Thread safety

Separate `qql_context_t` instances are safe to use from separate threads. A single context is not safe to share without external synchronization. `qql_execute()` uses a shared default context — prefer explicit contexts for anything serious.

## Architecture

```text
query → lexer → parser → AST → validation → source resolver → JSON repository → serializer → UTF-8 string
```

The layers stay independent:

```text
QQL parser knows syntax.
Source handlers know Islamic-book structure.
JSON repository knows storage.
Public API knows FFI.
```

The parser only knows that a reference is `IDENT : INT ( : selectors )`. It has no idea that Surah numbers stop at 114 — that is the Quran resolver's job. So `Q:500:999` and `XYZ:1:2` both parse fine and are rejected later, by the resolver and the registry respectively.

## Adding a new source

Adding Tirmidhi (`T:5:10`) requires **no** change to the lexer or parser.

1. Create `src/sources/tirmidhi.c` / `.h`.
2. Implement two callbacks:

   ```c
   static qql_error_t tirmidhi_validate(qql_context_t *ctx,
                                        const qql_reference_t *ref);

   static qql_error_t tirmidhi_resolve(qql_context_t *ctx,
                                       const qql_reference_t *ref,
                                       qql_result_builder_t *out);
   ```

   `validate` checks the numbers make sense for this collection. `resolve` loads the data and pushes records into the builder, in query order.

3. Expose the handler:

   ```c
   const qql_source_handler_t qql_source_tirmidhi = {
       .code     = "T",
       .name     = "Jami' at-Tirmidhi",
       .validate = tirmidhi_validate,
       .resolve  = tirmidhi_resolve,
   };
   ```

4. Register it in `src/source_registry.c` and add the file to `CMakeLists.txt`.
5. Drop the data under `data/tirmidhi/`.

Aliases (`TIRMIDHI` → `T`) are a registry concern and can be added the same way.

## Data

JSON files live under `data/`, one directory per collection, loaded lazily and cached in the context for its lifetime:

```text
data/
  quran/001.json …
  bukhari/001.json …
  muslim/…
  hisnul_muslim/…
```

Hadith numbering differs between editions. QQL v1 picks one canonical numbering scheme per collection and records it in the data files; `alternate_numbers` is reserved for future edition mapping. The parser is never involved in resolving numbering differences.

## Contributing

Contributions are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) for build, test, and style requirements.

## License

GNU General Public License v3.0 or later — see [LICENSE.md](LICENSE.md).
