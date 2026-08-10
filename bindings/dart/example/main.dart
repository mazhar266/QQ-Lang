// Console verification of the Dart → C → Rust → JSON → Dart round trip.
//
//   cargo build --release
//   cd bindings/dart && dart pub get
//   dart run example/main.dart "Q:2:255;B:1:1;"
//
// SPDX-License-Identifier: GPL-3.0-or-later

import '../qql.dart';

void main(List<String> args) {
  final query = args.isNotEmpty ? args.first : 'Q:2:255;B:1:1;';

  final qql = Qql.open(
    '../../sources',
    libraryPath: '../../target/release/libqql.so',
  );

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
