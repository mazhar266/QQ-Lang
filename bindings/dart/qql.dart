// QQL — Dart FFI binding.
//
// A thin wrapper over the C ABI in include/qql.h. Not a Flutter plugin: this
// is a plain dart:ffi binding that works in a console app or a Flutter app
// alike.
//
// SPDX-License-Identifier: GPL-3.0-or-later

import 'dart:convert';
import 'dart:ffi';
import 'dart:io' show Platform;

import 'package:ffi/ffi.dart';

// --- C signatures -----------------------------------------------------------

typedef _VersionC = Pointer<Utf8> Function();
typedef _VersionDart = Pointer<Utf8> Function();

typedef _CreateC = Pointer<Void> Function(Pointer<Utf8>);
typedef _CreateDart = Pointer<Void> Function(Pointer<Utf8>);

typedef _ExecuteC = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);
typedef _ExecuteDart = Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>);

typedef _DestroyC = Void Function(Pointer<Void>);
typedef _DestroyDart = void Function(Pointer<Void>);

typedef _FreeC = Void Function(Pointer<Utf8>);
typedef _FreeDart = void Function(Pointer<Utf8>);

// --- Binding ----------------------------------------------------------------

/// Thrown when the query could not be resolved.
class QqlException implements Exception {
  QqlException(this.code, this.message, this.position);

  final String code;
  final String message;
  final int? position;

  @override
  String toString() => position == null
      ? 'QqlException($code): $message'
      : 'QqlException($code) at $position: $message';
}

/// A QQL execution context.
///
/// Holds native memory. Call [dispose] when done — Dart's GC does not know
/// about the Rust allocation behind it.
class Qql {
  Qql._(this._lib, this._ctx);

  final _QqlLib _lib;
  Pointer<Void> _ctx;

  /// Open a context reading data from [dataDirectory].
  ///
  /// [libraryPath] defaults to the platform's usual name next to the
  /// executable; pass an explicit path when bundling.
  factory Qql.open(String dataDirectory, {String? libraryPath}) {
    final lib = _QqlLib(libraryPath ?? _defaultLibraryPath());
    final dir = dataDirectory.toNativeUtf8();
    try {
      final ctx = lib.create(dir);
      if (ctx == nullptr) {
        throw QqlException(
          'QQL_INTERNAL_ERROR',
          'could not create a context for "$dataDirectory"',
          null,
        );
      }
      return Qql._(lib, ctx);
    } finally {
      calloc.free(dir);
    }
  }

  /// Library version, e.g. `0.1.0`.
  String get version => _lib.version().toDartString();

  /// Execute [query] and return the decoded records.
  ///
  /// Throws [QqlException] when the response reports `"ok": false`.
  List<Map<String, dynamic>> execute(String query) {
    final response = executeJson(query);
    final decoded = jsonDecode(response) as Map<String, dynamic>;

    if (decoded['ok'] != true) {
      final error = decoded['error'] as Map<String, dynamic>;
      throw QqlException(
        error['code'] as String,
        error['message'] as String,
        error['position'] as int?,
      );
    }

    return (decoded['results'] as List).cast<Map<String, dynamic>>();
  }

  /// Execute [query] and return the raw JSON response.
  ///
  /// Never throws for a bad query — errors arrive as `"ok": false` JSON.
  String executeJson(String query) {
    if (_ctx == nullptr) {
      throw StateError('context has been disposed');
    }

    final native = query.toNativeUtf8();
    Pointer<Utf8> result = nullptr;
    try {
      result = _lib.execute(_ctx, native);
      // Copy into a Dart string before freeing the native buffer. Rust owns
      // that memory and only qql_free_string may release it.
      return result.toDartString();
    } finally {
      calloc.free(native);
      if (result != nullptr) {
        _lib.freeString(result);
      }
    }
  }

  /// Release the native context. Safe to call more than once.
  void dispose() {
    if (_ctx != nullptr) {
      _lib.destroy(_ctx);
      _ctx = nullptr;
    }
  }

  static String _defaultLibraryPath() {
    if (Platform.isWindows) return 'qql.dll';
    if (Platform.isMacOS || Platform.isIOS) return 'libqql.dylib';
    return 'libqql.so';
  }
}

class _QqlLib {
  _QqlLib(String path) : this._(DynamicLibrary.open(path));

  _QqlLib._(DynamicLibrary lib)
      : version = lib.lookupFunction<_VersionC, _VersionDart>('qql_version'),
        create = lib.lookupFunction<_CreateC, _CreateDart>('qql_context_create'),
        execute =
            lib.lookupFunction<_ExecuteC, _ExecuteDart>('qql_context_execute'),
        destroy =
            lib.lookupFunction<_DestroyC, _DestroyDart>('qql_context_destroy'),
        freeString = lib.lookupFunction<_FreeC, _FreeDart>('qql_free_string');

  final _VersionDart version;
  final _CreateDart create;
  final _ExecuteDart execute;
  final _DestroyDart destroy;
  final _FreeDart freeString;
}
