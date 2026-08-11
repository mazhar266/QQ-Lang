# QQL — Dart FFI binding

A plain `dart:ffi` wrapper over the C ABI. Not a Flutter plugin: the same file
works in a console app and in a Flutter app.

## Test

```bash
cargo build --release          # from the repository root
cd bindings/dart
dart pub get
dart test
```

Nine tests covering the whole path:

```text
Dart → qql_context_execute() → Rust → JSON data → JSON result → Dart String → qql_free_string()
```

They pin the things the Rust tests cannot see from this side: Arabic arriving
byte-identical to the data file, query order and within-reference dedup
surviving the boundary, errors becoming `QqlException` with their code and
position intact, and 200 consecutive executions leaving the context healthy.

There is also a console demo:

```bash
dart run example/main.dart "Q:2:255;B:1:1;"
```

## Usage

```dart
final qql = Qql.open('sources', libraryPath: 'target/release/libqql.so');

try {
  for (final record in qql.execute('Q:2:1-5,255')) {
    print(record['ar']);
  }
} on QqlException catch (e) {
  print('${e.code}: ${e.message}');
} finally {
  qql.dispose();
}
```

`executeJson` returns the raw response string instead, and never throws for a
bad query — errors arrive as `{"ok": false, ...}`.

## Memory

The binding owns two kinds of native memory and releases both:

| Allocation | Freed by |
| --- | --- |
| Query strings (`toNativeUtf8`) | `calloc.free` — Dart allocated them |
| Result strings from Rust | `qql_free_string` — **never** `calloc.free` |
| The context | `Qql.dispose()` |

Crossing those wires corrupts the heap. Dart's GC knows nothing about the Rust
allocation, so `dispose()` is not optional; there is no finalizer.

`qql_version()` returns a static string and is never freed.

## Threading

One `Qql` instance must not be used from two isolates at once. Give each
isolate its own — contexts are independent by design, and the cache is
per-context.

## Bundling

| Platform | Artifact | Notes |
| --- | --- | --- |
| Android | `libqql.so` per ABI | `cargo ndk -t arm64-v8a -t armeabi-v7a build --release` |
| iOS | `libqql.a` | Static — the App Store discourages loose dynamic libraries |
| Linux / Windows / macOS | `libqql.so` / `qql.dll` / `libqql.dylib` | |

All of them fall out of the `crate-type` list in the root `Cargo.toml`.

The data directory must also ship with the app; on Android and iOS that means
copying `sources/` into app storage at first launch and passing that path to
`Qql.open`.
