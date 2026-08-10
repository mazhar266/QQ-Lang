/*
 * QQL — Quran Query Language
 * C ABI for the Rust library in src/ffi.rs.
 *
 * Copyright (C) 2026 Mazhar Ahmed
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Kept in sync with src/ffi.rs by scripts/c-smoke.sh, which compiles a C
 * program against this header and links it to the real library — a signature
 * that drifts fails to link.
 */

#ifndef QQL_H
#define QQL_H

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Opaque execution context. The layout is not part of the ABI; only the
 * pointer ever crosses the boundary.
 */
typedef struct qql_context_t qql_context_t;

/*
 * Library version, e.g. "0.1.0".
 *
 * Owned by the library. Do NOT pass it to qql_free_string().
 */
const char *qql_version(void);

/*
 * Create a context reading data from `data_directory`.
 *
 * Returns NULL if `data_directory` is NULL or is not valid UTF-8.
 * Release with qql_context_destroy().
 */
qql_context_t *qql_context_create(const char *data_directory);

/*
 * Execute `query` and return an allocated, NUL-terminated UTF-8 JSON string.
 *
 * Never returns NULL and never returns malformed JSON — failures, including a
 * NULL context or query, are serialized into the response as
 * {"ok":false,"error":{...}}.
 *
 * The caller owns the result and must release it with qql_free_string().
 * Results stay valid after the context is destroyed.
 *
 * One context must not be used from two threads at once.
 */
char *qql_context_execute(qql_context_t *ctx, const char *query);

/*
 * Destroy a context. NULL is a no-op. Strings already returned stay valid.
 */
void qql_context_destroy(qql_context_t *ctx);

/*
 * Execute `query` against a process-wide default context reading from
 * $QQL_DATA (default "./sources").
 *
 * Thread-safe but serialized behind a mutex. Prefer qql_context_execute()
 * with your own context for concurrent work.
 */
char *qql_execute(const char *query);

/*
 * Free a string returned by qql_context_execute() or qql_execute().
 *
 * NULL is a no-op. Never pass the result of qql_version(), a pointer this
 * library did not produce, or the same pointer twice.
 */
void qql_free_string(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* QQL_H */
