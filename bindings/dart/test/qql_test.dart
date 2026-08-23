// Verifies the Dart → C → Rust → JSON → Dart round trip.
//
//   cargo build --release      # from the repository root
//   cd bindings/dart && dart test
//
// SPDX-License-Identifier: GPL-3.0-or-later

import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import '../qql.dart';

/// Paths are relative to the package root, which is where `dart test` runs.
const _data = '../../sources';
String get _library {
  if (Platform.isWindows) return '../../target/release/qql.dll';
  if (Platform.isMacOS) return '../../target/release/libqql.dylib';
  return '../../target/release/libqql.so';
}

void main() {
  if (!File(_library).existsSync()) {
    // A missing library means `cargo build --release` has not run — that is a
    // setup problem, not a passing test suite.
    throw StateError('$_library not found; run `cargo build --release` first');
  }

  late Qql qql;

  setUp(() => qql = Qql.open(_data, libraryPath: _library));
  tearDown(() => qql.dispose());

  test('reports the library version', () {
    expect(qql.version, matches(RegExp(r'^\d+\.\d+\.\d+')));
  });

  test('resolves an ayah with its metadata', () {
    final results = qql.execute('Q:2:255');

    expect(results, hasLength(1));
    expect(results.first['source'], 'Q');
    expect(results.first['collection'], 'Quran');
    expect(results.first['surah'], 2);
    expect(results.first['ayah'], 255);
    expect(results.first['surah_name_en'], 'Al-Baqarah');
    expect(results.first['ar'], isNotEmpty);
  });

  test('Arabic crosses the FFI boundary without corruption', () {
    final ar = qql.execute('Q:1:1').first['ar'] as String;

    final file = jsonDecode(
      File('$_data/quran/chapters/1.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    final expected = (file['verses'] as List).first['text'] as String;

    expect(ar, expected);
    expect(utf8.encode(ar), utf8.encode(expected));
    expect(ar, isNot(contains('�')));
  });

  test('preserves query order and dedupes within a reference', () {
    final ayat = qql
        .execute('Q:2:255,1-3')
        .map((r) => r['ayah'] as int)
        .toList();
    expect(ayat, [255, 1, 2, 3]);

    expect(
      qql.execute('Q:2:1-5,3,4').map((r) => r['ayah']).toList(),
      [1, 2, 3, 4, 5],
    );
  });

  test('resolves hadith through the same binding', () {
    final results = qql.execute('B:1:1-3');
    expect(results, hasLength(3));
    expect(results.first['collection'], 'Sahih al-Bukhari');
    expect(results.first['chapter'], 1);
    expect(results.first['number'], 1);
  });

  test('surfaces errors as exceptions with code and position', () {
    expect(
      () => qql.execute('Q:2:5-1'),
      throwsA(
        isA<QqlException>()
            .having((e) => e.code, 'code', 'QQL_INVALID_RANGE')
            .having((e) => e.position, 'position', 4),
      ),
    );

    expect(
      () => qql.execute('XYZ:1'),
      throwsA(
        isA<QqlException>().having((e) => e.code, 'code', 'QQL_UNKNOWN_SOURCE'),
      ),
    );
  });

  test('executeJson always returns valid JSON, never throws', () {
    for (final query in ['Q:2:255', '', 'Q:2:5-1', 'XYZ:1', '!!!']) {
      final decoded = jsonDecode(qql.executeJson(query)) as Map<String, dynamic>;
      expect(decoded['ok'], isA<bool>());
      expect(decoded['query'], query);
    }
  });

  test('repeated execution does not leak or corrupt the context', () {
    for (var i = 0; i < 200; i++) {
      expect(qql.execute('Q:2:1-10'), hasLength(10));
    }
  });

  test('dispose is idempotent and use-after-dispose is caught', () {
    qql.dispose();
    qql.dispose();
    expect(() => qql.executeJson('Q:1'), throwsStateError);
  });
}
