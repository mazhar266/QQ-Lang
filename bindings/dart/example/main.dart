// Console verification of the Dart → C → Rust → JSON → Dart round trip.
//
//   cargo build --release
//   cd bindings/dart && dart pub get
//   dart run example/main.dart "Q:2:255;B:1:1;"
//
// SPDX-License-Identifier: GPL-3.0-or-later

import 'dart:io' show Platform;

import '../qql.dart';

/// The build output lives outside the package during development, so point at
/// it explicitly rather than relying on the default lookup.
String get _library {
  if (Platform.isWindows) return '../../target/release/qql.dll';
  if (Platform.isMacOS) return '../../target/release/libqql.dylib';
  return '../../target/release/libqql.so';
}

void main(List<String> args) {
  final query = args.isNotEmpty ? args.first : 'Q:2:255;B:1:1;';

  final qql = Qql.open('../../sources', libraryPath: _library);

  try {
    print('qql ${qql.version}\n');

    for (final record in qql.execute(query)) {
      print('${record['collection']} [${record['source']}]');
      print('  ${record['ar']}');
      print('  ${record['en']}\n');
    }

    // Errors surface as exceptions, not as silent empties.
    try {
      qql.execute('Q:2:5-1');
      print('BUG: an invalid range should have thrown');
    } on QqlException catch (e) {
      print('expected failure: $e');
    }
  } finally {
    qql.dispose();
  }
}
