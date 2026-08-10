/*
 * Minimal C consumer of libqql.
 *
 * Build and run with scripts/c-smoke.sh, or by hand:
 *
 *   cargo build --release
 *   cc examples/c/basic.c -Iinclude -Ltarget/release -lqql -o /tmp/qql-basic
 *   LD_LIBRARY_PATH=target/release /tmp/qql-basic "Q:2:255"
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <qql.h>

int main(int argc, char **argv) {
    const char *query = argc > 1 ? argv[1] : "Q:2:255;B:1:1;";
    const char *env = getenv("QQL_DATA");
    const char *data = env ? env : "sources";

    printf("qql %s\n", qql_version());

    qql_context_t *ctx = qql_context_create(data);
    if (!ctx) {
        fprintf(stderr, "failed to create context for '%s'\n", data);
        return 1;
    }

    char *result = qql_context_execute(ctx, query);
    printf("%s\n", result);

    /* Deliberately exercise the documented edge cases. */
    int ok = strstr(result, "\"ok\":true") != NULL;
    qql_free_string(result);

    char *from_null = qql_context_execute(ctx, NULL);
    if (!from_null || strstr(from_null, "\"ok\":false") == NULL) {
        fprintf(stderr, "a NULL query should still return error JSON\n");
        qql_free_string(from_null);
        qql_context_destroy(ctx);
        return 1;
    }
    qql_free_string(from_null);

    qql_context_destroy(ctx);
    qql_context_destroy(NULL); /* no-op */
    qql_free_string(NULL);     /* no-op */

    return ok ? 0 : 1;
}
