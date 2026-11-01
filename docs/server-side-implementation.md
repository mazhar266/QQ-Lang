# Server-side semantic search — implementation plan

Status: proposed, not yet scheduled. Written 2026-11-01 after investigating
the options (Elasticsearch, MongoDB, Postgres+pgvector) against the
architecture constraints in [todo.md](todo.md). This document extends that
file's Tier 4 into a concrete design.

## Why this exists

The current `*"term"` similarity engine (`src/vector.rs`) is a hashed
**lexical** projection scanned brute-force. It is tolerant of diacritics and
affixes but cannot relate *charity* to *zakat*, and the documented failure
`q:*"quran is easy"` still ranks 74:10 first (todo.md, "Where it stands").
Tier 3 (static token tables) stays inside the offline premise but only knows
word similarity, not composition. This plan takes **Tier 4 — a real sentence
model** — in the only shape todo.md considers acceptable for it: a **separate
server profile**, leaving the core crate, the CLI, and the Dart bindings
untouched.

## Backend choice: Postgres + pgvector

The three candidates considered:

- **Elasticsearch — rejected.** A JVM cluster with GB-scale memory and real
  operational overhead, to obtain BM25 (QQL already ships BM25 locally via
  tantivy) plus a vector index that, at ~90k records, behaves no better than
  pgvector's.
- **MongoDB — rejected.** Vector search is an Atlas (cloud) / Atlas-local
  feature, not part of the self-hosted community server; vendor-tied and a
  poor fit next to a Rust service.
- **Postgres + pgvector — chosen.** One proven server, an HNSW index, SQL
  scope filters that map naturally onto QQL's source/book/range scoping, and
  trivial local development through docker.

Scale honesty: at ~87k records × 384 dims the ANN index is not a performance
necessity — a flat scan is milliseconds. Postgres is chosen for the
**deployment** (persistence, multi-user service, ops familiarity), not speed.

## Architecture

A new `server/` directory = its own Cargo workspace (precedent: `fuzz/`
isolates itself with `[workspace]`), one crate `qql-server`. The core crate is
untouched: no lexer/parser/AST/FFI/repository changes, the two-dependency
premise holds, `git clone` remains a complete offline installation.

```text
client ──HTTP──► qql-server (axum)
                   │ 1. embed query text ── ONNX Runtime (ort) +
                   │                        multilingual-e5-small int8
                   │                        (~120 MB on disk, NOT in git)
                   │ 2. ANN search ──────── Postgres + pgvector HNSW over
                   │                        semantic_chunks(code, primary,
                   │                        number, lang, body, vector(384))
                   │ 3. resolve hits ────── qql::Context::execute — the
                   │                        ordinary reference path
                   ▼
              same JSON envelope as local engines:
              records + "score" + "ranked": true
```

### Single embedding implementation, Rust only

Indexing and querying embed through one module (`server/src/embed.rs`, ort +
HF `tokenizers` loading the model's own `tokenizer.json`). This deliberately
avoids the cross-language drift class that
`tests/vector.rs::the_build_script_and_the_runtime_embed_alike` exists to
catch — no Python in the pipeline, so no new invariant to babysit. e5 requires
`"query: "` / `"passage: "` prefixes; `embed.rs` applies them internally so
callers cannot forget.

### Endpoints

- `POST /query {"q": "q:*\"mercy\"~5"}` — full QQL syntax. The server parses
  with the same public parse API the CLI's `--parse` flag uses
  (`src/bin/qql.rs:104-113`) and routes AST nodes like `src/context.rs:162-177`:
  exact/range references resolve locally through `Context::execute`;
  `MatchKind::Similar` nodes go embed → filtered ANN → resolve. Scopes
  (`Q:1:*"x"`, `Q:1:3~5:*"x"`, `OT:GEN:*"x"`) become SQL predicates; `~N`
  becomes LIMIT. Errors reuse qql's JSON error envelope (`QQL_UNSUPPORTED`
  and friends).
- `POST /search {"text": "...", "source": "Q"?, "limit"?}` — plain-text sugar
  that synthesizes whole-source Similar queries, for app clients that just
  have a sentence.

### Indexer: `qql-embed`

A second binary in the server crate, mirroring `src/bin/qql_index.rs`: iterate
`ctx.sources()`, skip `IndexKeys::Flat` windows that alias another code
(`qql_index.rs:67-73`), enumerate records per primary through
`Context::execute("CODE:{primary}")`, embed every text field (mirroring the
`(primary, number, lang)` keying of the existing `.qv` format), and upsert
into pgvector. `--source` flags allow partial builds — Quran-only first
(≈18.7k chunks, minutes on CPU); all 21 sources ≈ 190k chunks as a background
job.

### pgvector details

HNSW (m=16, ef_construction=64), query-time `ef_search` ≈ 64. Scope-filtered
ANN via overfetch-then-filter, with iterative refinement when a scoped search
underfills. Best-field-per-record merge at the service layer, like
`src/vector.rs:364-372` already does across langs.

## File layout

New, all under `server/`:

| Path | Purpose |
|---|---|
| `Cargo.toml` | axum, tokio, ort, tokenizers, tokio-postgres, serde/serde_json, `qql` path dep (default features) |
| `src/main.rs` | routes, config (DATABASE_URL, MODEL_PATH, QQL_DATA, listen addr) |
| `src/embed.rs` | ort + tokenizers; batch embed; e5 prefixes; int8 ONNX |
| `src/store.rs` | schema DDL, upserts, ANN search with scope predicates |
| `src/engine.rs` | AST walk, dispatch, score merge |
| `src/bin/qql_embed.rs` | the indexer binary |
| `Dockerfile`, `docker-compose.yml` | pgvector/pgvector:pg16 + server mounting `./sources` |
| `scripts/fetch-model.sh` | downloads quantized ONNX with SHA-256 verification |
| `tests/` | API parity + golden-set eval |
| `README.md` | deployment guide |

Modified, small: `.gitignore` (server target dir, model files), `moon.yml`
(`server:build`, `server:test`, `eval` tasks), `docs/todo.md` (Tier 4 marked
as taken by this document), `README.md` (a server section). The core crate
changes only if the parse entrypoint used by `--parse` is not already public —
then a one-line additive `pub` export in `src/lib.rs`.

Reused as-is: `qql::Context` (`execute`, `execute_json`, `sources`,
`index_keys`), the qql AST types, the resolution pattern of
`Context::ranked_hit` (`src/context.rs:338-376`), the indexer skeleton of
`src/bin/qql_index.rs`, and the skip-gracefully-if-asset-absent test pattern
(`tests/vector.rs:76-94`, `tests/fulltext.rs:84-102`).

## Verification

1. `docker compose up` → postgres healthy; `qql-embed --source Q` indexes the
   Quran; `POST /query {"q":"q:1:*\"worship\"~3"}` returns the same hit
   records the local engine returns; `POST /search {"text":"quran is easy"}`
   ranks the four al-Qamar ayat (54:17/22/32/40) at top — the documented
   failure, fixed.
2. Golden-set eval: ~20 queries with expected relevant records; print
   recall@10 for the hashed `.qv` baseline vs BM25 (`?"…"`) vs e5 server.
   This is the acceptance evidence that Tier 4 earns its cost — the benchmark
   todo.md explicitly demands.
3. API parity tests: exact references over HTTP byte-match the CLI JSON;
   QQL error envelopes reproduce (`QQL_UNSUPPORTED` without model or index);
   scope narrowing over SQL matches local `q:1:3~5:*"help"` results.
4. Embed-module unit tests skip gracefully when the model file is absent
   (precedent pattern). moon task `server:test` (compose-backed) stays out of
   the default `test` path.
5. Perf sanity: p95 query latency under ~100 ms on CPU (embed ≈15 ms + ANN
   <10 ms + resolve).

## Known risks

- Classical Quranic Arabic and translation-register English are
  out-of-distribution for e5 — expect a large gain over trigrams, not
  web-text quality. Fine-tuning on a Quran/hadith corpus remains a future
  research track (todo.md, Tier 4).
- ONNX Runtime adds a native ~50 MB dependency **to the server image only**;
  the core crate and released CLI bundles stay exactly as they are.
- The e5 query/passage prefix convention must never leak inconsistently;
  confining it to `embed.rs` is the guard.
