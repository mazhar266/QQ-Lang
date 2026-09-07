# TODO — vector search beyond the hashed embedder

Deferred from the work that shipped in 3.6.0. Tiers 1 and 2 are **done**:
tokens are weighted by inverse document frequency, whole words outweigh their
trigrams three to one, and stopwords are zeroed. That moved the ayah a phrase
describes from rank 11 to rank 2.

What remains is the part that costs an asset. Neither tier below is scheduled;
this file exists so the reasoning is not re-derived from scratch.

## Where it stands

`*"…"` is **fuzzy lexical matching, not semantic**. It is tolerant of
diacritics, prefixes and suffixes — worth a great deal for Arabic — but it
does not know that *charity* and *zakat* are related.

The visible symptom, and the benchmark for anything below:

```text
q:*"quran is easy"~6
  1. 74:10  For the disbelievers - not easy          ← still wrong
  2. 54:17  And We have certainly made the Qur'an easy for remembrance
  3-5. 54:22, 54:32, 54:40
```

74:10 wins because it is short: cosine normalizes by vector length, so one
matching rare word dominates a four-word ayah. BM25 has a length-normalization
knob (`b`) for exactly this; cosine does not. Fixing it properly means
composition, not tuning — which is Tier 3.

Note that `?"quran is easy"` already answers this correctly, ranking the four
al-Qamar ayat first. **Before investing in either tier, decide what job `*`
does that `?` does not.** Since the full-text engine gained folding, stemming,
OR-ranking and alias expansion, the honest options are to make `*` semantic or
to retire it — it currently costs 32 MB of committed indexes and a feature
flag to do a worse job than `?` at the same task.

## Tier 3 — static token embeddings

Ship a vocabulary→vector table and average the tokens of a query. **Not a
transformer at runtime — a table lookup.**

- **Asset**: ~25 MB, restricted to the vocabulary that actually appears in the
  corpus (~100k tokens × 256 dims × `int8`). Comparable to what the vectors
  already cost.
- **Query cost**: a lookup and a mean. No matrix multiply, no ML runtime, no
  new dependency. Still works offline on a phone through the C ABI.
- **Fallback**: a word outside the table hashes as it does today, so coverage
  degrades cleanly instead of failing.
- **Bonus**: aligned cross-lingual vectors (fastText/MUSE-style) would let an
  English query reach the Arabic text directly, which nothing in QQL does now.
- **Limit**: it knows word similarity, not composition. It would relate
  *charity* to *zakat*; it would probably still miss "quran is easy" → 54:17,
  because that needs the meaning of the sentence.

This is the next rung, and the only one that fits the project's constraints as
they stand.

## Tier 4 — a real sentence model

A multilingual sentence encoder, e.g. `multilingual-e5-small`.

- **Asset**: ~470 MB fp32, ~120 MB quantized.
- **Runtime**: candle or ONNX Runtime inside `libqql.so`. The Dart binding and
  the C ABI then carry it onto a phone, which **breaks the
  offline-portable-two-dependencies premise the crate is built on**.
- **Quality caveat**: these models are trained on modern prose. Classical
  Quranic Arabic and translation-register English (*"Verily"*, *"[All] praise
  is [due] to"*) are out of distribution. Expect a large gain over trigrams,
  not the quality these models show on web text. Closing that gap means
  fine-tuning on a Quran/hadith corpus — a research project of its own.

Only worth taking with a **separate desktop/server build profile**, keeping
the hashed embedder as the default for mobile and embedded targets.

## What makes either tier cheap to attempt

The file format was built for this. `Embedder` is an id in the `.qv` header
and `Embedder::from_id` is a match, so a new embedder **coexists** with the
existing ones rather than replacing them — which is exactly how the weighted
embedder was added in 3.6.0. The scan, the scoping, the key layout and the
result shape are all unchanged by a swap.

The invariant to preserve is in `tests/vector.rs`:
`the_build_script_and_the_runtime_embed_alike` re-embeds a stored record at
query time and demands self-similarity > 0.98. Any new embedder must keep that
test passing, because the build script and the runtime are separate
implementations that have to project into the same space.

## One consideration for research use

`?"…"` is deterministic and explainable — you can point at why an ayah
matched. Vector hits are not. For work that gets cited, that asymmetry may
matter more than recall.
