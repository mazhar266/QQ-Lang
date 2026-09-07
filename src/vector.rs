// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Vector similarity search — the `*"term"` form.
//!
//! Behind the `vector` cargo feature, off by default, so the core crate keeps
//! its two dependencies and needs no vector assets to build or run.
//!
//! # Why there is no index structure
//!
//! The whole corpus is about 40,000 records. A flat scan of 128-dimensional
//! `i8` vectors is a few million integer multiply-accumulates — single-digit
//! milliseconds on a weak ARM core, and less once a scope narrows it. An
//! approximate-nearest-neighbour index would add a large dependency and a
//! second artifact that can drift from the text, to beat a scan that is
//! already fast enough. So: no HNSW, no graph, no clustering. Just a scan.
//!
//! # Why the query needs no model on the device
//!
//! Embedding the *query* is the part that usually drags a transformer runtime
//! onto the phone. [`Embedder::Hashed`] avoids it: text folds to tokens, each
//! token hashes to a handful of dimensions, and the signed sum is normalized.
//! No weights, no matrix multiply, no asset to ship — the same function runs
//! at build time over the corpus and at query time over the needle.
//!
//! # Weighting
//!
//! Every token used to count the same, so *is* weighed what *quran* weighed
//! and a five-letter word contributed four times — once as itself, three
//! times as its trigrams. Embedder 2 fixes both: each token is scaled by its
//! inverse document frequency, whole words count [`WORD_WEIGHT`] against a
//! trigram's 1, and stopwords are zeroed outright.
//!
//! The weights ride in the index. Because the embedder hashes tokens anyway,
//! the table is keyed by hash rather than by word — no vocabulary to ship —
//! and tokens seen in a single document are left out, since they all share
//! the same maximum IDF, which the `default` field carries. That is most of
//! the vocabulary, so the table costs a few hundred KB per source.
//!
//! **That makes it fuzzy lexical matching, not semantic.** It is tolerant of
//! diacritics, prefixes and suffixes — which is worth a lot for Arabic — but
//! it does not know that *charity* and *zakat* are related. Real semantic
//! vectors are a build-time swap: emit an index with a different
//! [`Embedder`], and the runtime learns to embed queries the same way.
//!
//! # File format
//!
//! One `.qv` file per source, little-endian:
//!
//! ```text
//! 0   magic     8 bytes  "QQLVEC1\n"
//! 8   dims      u32
//! 12  count     u32
//! 16  embedder  u32      1 = hashed, 2 = hashed + IDF weights
//! 20  weights   u32      entries in the weight table; 0 for embedder 1
//! 24  keys      count × { u32 primary, u32 number, u32 lang }
//!                         lang: 0 Arabic, 1 English, 2 Emlaei
//! ..  vectors   count × dims × i8, L2-normalized then scaled by 127
//! ..  table     f32 default, then weights × { u64 hash, f32 idf },
//!               sorted by hash                       (embedder 2 only)
//! ```
//!
//! Field 20 was reserved and always written as zero, so an embedder-1 file
//! stays valid unchanged. The other direction is safe too: an older build
//! meeting embedder 2 refuses the file by name rather than misreading it.
//!
//! Keys carry each vector's address, so a scope filters during the scan and a
//! hit resolves back through the ordinary `SOURCE:primary:number` path. The
//! index therefore never has to agree with the resolver about record shapes —
//! it only has to agree about numbering.

use crate::error::Error;

/// Magic at the head of every index file.
const MAGIC: &[u8; 8] = b"QQLVEC1\n";

/// Bytes before the key table.
const HEADER: usize = 24;

/// Bytes per key entry.
const KEY: usize = 12;

/// Results returned when a similarity query gives no cap of its own.
pub const DEFAULT_LIMIT: u32 = 20;

/// Scores at or below this are noise — every record in scope has *some*
/// cosine with the query, including negative ones, and without a floor a
/// scoped search would return its whole scope in ranked order.
const MIN_SCORE: f32 = 0.05;

/// Hits must also reach this fraction of the best score. A relative cut
/// adapts to the query: a sharp match keeps only its neighbours, while a
/// broad one keeps a broad field.
const RELATIVE_CUTOFF: f32 = 0.5;

/// How vectors in an index were produced.
///
/// The runtime must embed queries the same way the index was built, so this
/// is recorded in the file rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Embedder {
    /// Signed hash projection of folded tokens. Needs no model asset.
    Hashed,
    /// The same projection, with each token scaled by its inverse document
    /// frequency and whole words outweighing their trigrams. Carries a
    /// weight table in the file; still needs no model asset.
    HashedIdf,
}

impl Embedder {
    fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Embedder::Hashed),
            2 => Some(Embedder::HashedIdf),
            _ => None,
        }
    }
}

/// How much a whole word outweighs one of its character trigrams.
///
/// Trigrams exist for morphology — they let an Arabic query reach a prefixed
/// form of the same root — but they collide across unrelated roots, and a long
/// word emits several of them. The exact match should dominate.
pub const WORD_WEIGHT: f32 = 3.0;

/// Per-token weights, keyed by token hash.
///
/// Sorted, so a lookup is a binary search over a few tens of thousands of
/// entries. Anything absent was seen in one document only and takes
/// `default`, the maximum IDF for the corpus.
#[derive(Debug, Default)]
pub struct Weights {
    default: f32,
    table: Vec<(u64, f32)>,
}

impl Weights {
    /// The IDF of a token, or the default for one the corpus barely carries.
    fn idf(&self, hash: u64) -> f32 {
        match self.table.binary_search_by(|(h, _)| h.cmp(&hash)) {
            Ok(at) => self.table[at].1,
            Err(_) => self.default,
        }
    }

    /// Entries held, excluding the default.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

/// Where one vector sits in its collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// Surah or chapter.
    pub primary: u32,
    /// Ayah or item within that primary.
    pub number: u32,
    /// Which field this vector came from: 0 Arabic, 1 English, 2 the
    /// simplified (Emlaei) Arabic spelling.
    ///
    /// Informational — the scan keeps one hit per `(primary, number)`
    /// whichever field scored best, so a new field id needs no code change
    /// here, only vectors that carry it.
    pub lang: u32,
}

/// A loaded vector index.
#[derive(Debug)]
pub struct Index {
    dims: usize,
    embedder: Embedder,
    keys: Vec<Key>,
    /// `count × dims` values, row-major.
    data: Vec<i8>,
    /// Empty for embedder 1, which weighs every token the same.
    weights: Weights,
}

impl Index {
    /// Parse an index from its file bytes.
    ///
    /// Every length is checked against the buffer: a truncated or forged file
    /// is a data error, never a panic or an out-of-bounds read.
    pub fn parse(path: &str, bytes: &[u8]) -> Result<Self, Error> {
        let bad = |detail: &str| Error::InvalidDataFile {
            path: path.to_string(),
            detail: detail.to_string(),
        };

        if bytes.len() < HEADER || &bytes[..8] != MAGIC {
            return Err(bad("not a QQL vector index"));
        }

        let word = |at: usize| -> u32 {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&bytes[at..at + 4]);
            u32::from_le_bytes(buf)
        };

        let dims = word(8) as usize;
        let count = word(12) as usize;
        let embedder = Embedder::from_id(word(16))
            .ok_or_else(|| bad("built by an embedder this build does not know"))?;

        if dims == 0 || dims > 4096 {
            return Err(bad("implausible dimension count"));
        }

        let keys_end = HEADER
            .checked_add(
                count
                    .checked_mul(KEY)
                    .ok_or_else(|| bad("index too large"))?,
            )
            .ok_or_else(|| bad("index too large"))?;
        let vectors_end = keys_end
            .checked_add(
                count
                    .checked_mul(dims)
                    .ok_or_else(|| bad("index too large"))?,
            )
            .ok_or_else(|| bad("index too large"))?;

        if bytes.len() < vectors_end {
            return Err(bad("truncated: fewer bytes than the header promises"));
        }

        let mut keys = Vec::with_capacity(count);
        for i in 0..count {
            let at = HEADER + i * KEY;
            keys.push(Key {
                primary: word(at),
                number: word(at + 4),
                lang: word(at + 8),
            });
        }

        let data = bytes[keys_end..vectors_end]
            .iter()
            .map(|&b| b as i8)
            .collect();

        // Weight table, embedder 2 only: `f32 default` then sorted
        // `(u64 hash, f32 idf)` pairs. Every length is checked, like the rest.
        let entries = word(20) as usize;
        let mut weights = Weights::default();
        if entries > 0 {
            let table_end = vectors_end
                .checked_add(4)
                .and_then(|at| entries.checked_mul(12).and_then(|n| at.checked_add(n)))
                .ok_or_else(|| bad("index too large"))?;
            if bytes.len() < table_end {
                return Err(bad("truncated: the weight table is shorter than promised"));
            }

            let float = |at: usize| -> f32 {
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&bytes[at..at + 4]);
                f32::from_le_bytes(buf)
            };
            let long = |at: usize| -> u64 {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(&bytes[at..at + 8]);
                u64::from_le_bytes(buf)
            };

            weights.default = float(vectors_end);
            weights.table = Vec::with_capacity(entries);
            for i in 0..entries {
                let at = vectors_end + 4 + i * 12;
                weights.table.push((long(at), float(at + 8)));
            }
            // Binary search depends on it, and the file is not trusted.
            if weights.table.windows(2).any(|w| w[0].0 >= w[1].0) {
                return Err(bad("weight table is not sorted by hash"));
            }
        }

        Ok(Index {
            dims,
            embedder,
            keys,
            data,
            weights,
        })
    }

    /// How queries against this index must be embedded.
    pub fn embedder(&self) -> Embedder {
        self.embedder
    }

    /// Dimensions per vector.
    pub fn dims(&self) -> usize {
        self.dims
    }

    /// Vectors held.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the index holds nothing.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Embed `text` for this index.
    pub fn embed(&self, text: &str) -> Vec<i8> {
        match self.embedder {
            Embedder::Hashed => hashed_embed(text, self.dims),
            Embedder::HashedIdf => weighted_embed(text, self.dims, &self.weights),
        }
    }

    /// The token weights this index carries. Empty for embedder 1.
    pub fn weights(&self) -> &Weights {
        &self.weights
    }

    /// The vector at `position`, if it exists.
    pub fn vector(&self, position: usize) -> Option<&[i8]> {
        let start = position.checked_mul(self.dims)?;
        self.data.get(start..start.checked_add(self.dims)?)
    }

    /// Rank vectors against `query`, keeping only those `accept` allows.
    ///
    /// Returns at most `limit` hits, best first, one per `(primary, number)` —
    /// a record indexed in both languages scores as whichever field matched
    /// better, rather than appearing twice.
    ///
    /// Weak hits are dropped: everything in scope has some cosine with the
    /// query, so results are cut at [`MIN_SCORE`] and at [`RELATIVE_CUTOFF`]
    /// of the best score. A similarity search can therefore return fewer than
    /// `limit`, or nothing at all.
    pub fn nearest(
        &self,
        query: &[i8],
        limit: usize,
        accept: impl Fn(&Key) -> bool,
    ) -> Vec<(Key, f32)> {
        if query.len() != self.dims || limit == 0 {
            return Vec::new();
        }

        let mut best: Vec<(Key, f32)> = Vec::new();
        for (position, key) in self.keys.iter().enumerate() {
            if !accept(key) {
                continue;
            }
            let Some(vector) = self.vector(position) else {
                continue;
            };

            let score = dot(query, vector);
            // Keep one entry per record, whichever language scored higher.
            match best
                .iter_mut()
                .find(|(k, _)| k.primary == key.primary && k.number == key.number)
            {
                Some(existing) if existing.1 >= score => {}
                Some(existing) => *existing = (*key, score),
                None => best.push((*key, score)),
            }
        }

        // Ranked, not positional — the one place in QQL where output order is
        // by relevance. Ties keep their corpus order, so results are stable.
        best.sort_by(|a, b| b.1.total_cmp(&a.1));

        let floor = match best.first() {
            Some((_, top)) => (top * RELATIVE_CUTOFF).max(MIN_SCORE),
            None => return Vec::new(),
        };
        best.retain(|(_, score)| *score >= floor);
        best.truncate(limit);
        best
    }
}

/// Cosine of two normalized, `i8`-quantized vectors.
///
/// Both sides were scaled by 127 at build time, so the integer dot product
/// divided by `127²` lands back on roughly the original cosine. Accumulating
/// in `i32` cannot overflow: 4096 dimensions of `127 × 127` is well inside it.
fn dot(a: &[i8], b: &[i8]) -> f32 {
    let mut sum: i32 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        sum += i32::from(*x) * i32::from(*y);
    }
    sum as f32 / (127.0 * 127.0)
}

/// Split text into the tokens the hashed embedder projects.
///
/// Whole words carry meaning; character trigrams carry morphology, which is
/// what lets an Arabic query match a prefixed or suffixed form of the same
/// root. Folding first means diacritics never reach the hash.
pub fn tokens(text: &str) -> Vec<(String, bool)> {
    let folded = crate::search::fold(text);
    let mut out = Vec::new();

    for word in folded.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        out.push((word.to_string(), true));

        let chars: Vec<char> = word.chars().collect();
        if chars.len() > 3 {
            for window in chars.windows(3) {
                out.push((window.iter().collect(), false));
            }
        }
    }

    out
}

/// Deterministic 64-bit hash (FNV-1a), so the build script and the runtime
/// agree without sharing code.
fn hash(token: &str) -> u64 {
    let mut acc: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in token.as_bytes() {
        acc ^= u64::from(*byte);
        acc = acc.wrapping_mul(0x0000_0100_0000_01b3);
    }
    acc
}

/// Project folded tokens into `dims` dimensions and quantize.
///
/// Each token lands on four dimensions with a sign drawn from its own hash —
/// a signed random projection of the bag of tokens. Cheap, allocation-light,
/// and identical on both sides of the build.
pub fn hashed_embed(text: &str, dims: usize) -> Vec<i8> {
    project(text, dims, |_, _, _| 1.0)
}

/// The same projection, weighted: IDF from the index, whole words scaled by
/// [`WORD_WEIGHT`], stopwords zeroed.
///
/// Zeroing rather than filtering keeps [`tokens`] a pure tokenizer, so
/// embedder 1 stays bit-identical to what it always produced.
pub fn weighted_embed(text: &str, dims: usize, weights: &Weights) -> Vec<i8> {
    project(text, dims, |token, is_word, hash| {
        if crate::search::is_stopword(token) {
            return 0.0;
        }
        weights.idf(hash) * if is_word { WORD_WEIGHT } else { 1.0 }
    })
}

/// Shared projection: each token lands on four dimensions with a sign drawn
/// from its own hash, scaled by whatever `weight` returns.
fn project(text: &str, dims: usize, weight: impl Fn(&str, bool, u64) -> f32) -> Vec<i8> {
    let mut acc = vec![0f32; dims];

    for (token, is_word) in tokens(text) {
        let start = hash(&token);
        let scale = weight(&token, is_word, start);
        if scale == 0.0 {
            continue;
        }

        let mut h = start;
        for _ in 0..4 {
            let slot = (h % dims as u64) as usize;
            let sign = if h & 0x8000_0000_0000_0000 == 0 {
                1.0
            } else {
                -1.0
            };
            acc[slot] += sign * scale;
            h = h.wrapping_mul(0x0000_0100_0000_01b3) ^ (h >> 29);
        }
    }

    quantize(&acc)
}

/// L2-normalize, then scale to `i8`. An all-zero input stays all-zero, which
/// simply scores 0 against everything rather than producing a NaN.
pub fn quantize(values: &[f32]) -> Vec<i8> {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm == 0.0 {
        return vec![0; values.len()];
    }
    values
        .iter()
        .map(|v| {
            let scaled = (v / norm * 127.0).round();
            scaled.clamp(-127.0, 127.0) as i8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(dims: usize, rows: &[(Key, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(dims as u32).to_le_bytes());
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for (key, _) in rows {
            out.extend_from_slice(&key.primary.to_le_bytes());
            out.extend_from_slice(&key.number.to_le_bytes());
            out.extend_from_slice(&key.lang.to_le_bytes());
        }
        for (_, text) in rows {
            for value in hashed_embed(text, dims) {
                out.push(value as u8);
            }
        }
        out
    }

    /// An embedder-2 file: vectors weighted by a table the reader must parse.
    fn build_weighted(dims: usize, rows: &[(Key, &str)], weights: &Weights) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(dims as u32).to_le_bytes());
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(weights.table.len() as u32).to_le_bytes());
        for (key, _) in rows {
            out.extend_from_slice(&key.primary.to_le_bytes());
            out.extend_from_slice(&key.number.to_le_bytes());
            out.extend_from_slice(&key.lang.to_le_bytes());
        }
        for (_, text) in rows {
            for value in weighted_embed(text, dims, weights) {
                out.push(value as u8);
            }
        }
        out.extend_from_slice(&weights.default.to_le_bytes());
        for (hash, idf) in &weights.table {
            out.extend_from_slice(&hash.to_le_bytes());
            out.extend_from_slice(&idf.to_le_bytes());
        }
        out
    }

    fn key(primary: u32, number: u32) -> Key {
        Key {
            primary,
            number,
            lang: 0,
        }
    }

    #[test]
    fn a_term_is_nearest_to_the_record_that_contains_it() {
        let rows = [
            (key(1, 1), "in the name of Allah the entirely merciful"),
            (key(1, 2), "all praise is due to Allah lord of the worlds"),
            (key(1, 5), "it is You we worship and You we ask for help"),
        ];
        let bytes = build(64, &rows);
        let index = Index::parse("test.qv", &bytes).unwrap();

        let hits = index.nearest(&index.embed("worship help"), 3, |_| true);
        assert_eq!(hits[0].0.number, 5, "expected 1:5 first, got {hits:?}");
        // The unrelated ayat fall under the cutoff rather than trailing along.
        assert!(hits.len() < rows.len(), "got {hits:?}");
    }

    #[test]
    fn arabic_matches_across_diacritics_and_affixes() {
        let rows = [
            (key(1, 2), "ٱلْحَمْدُ لِلَّهِ رَبِّ ٱلْعَٰلَمِينَ"),
            (key(1, 5), "إِيَّاكَ نَعْبُدُ وَإِيَّاكَ نَسْتَعِينُ"),
        ];
        let index = Index::parse("test.qv", &build(64, &rows)).unwrap();

        // Undiacritized, and without the definite article.
        let hits = index.nearest(&index.embed("حمد"), 2, |_| true);
        assert_eq!(hits[0].0.number, 2, "got {hits:?}");
    }

    #[test]
    fn weak_and_negative_hits_are_dropped() {
        let rows = [
            (key(1, 1), "mercy and compassion"),
            (key(1, 2), "a completely unrelated sentence about camels"),
            (key(1, 3), "nothing in common at all here"),
        ];
        let index = Index::parse("test.qv", &build(64, &rows)).unwrap();

        // Without a cutoff this would return all three, ranked.
        let hits = index.nearest(&index.embed("mercy"), 10, |_| true);
        assert_eq!(hits.len(), 1, "expected only the real match: {hits:?}");
        assert_eq!(hits[0].0.number, 1);
        assert!(hits.iter().all(|(_, score)| *score >= MIN_SCORE));
    }

    #[test]
    fn a_query_matching_nothing_returns_nothing() {
        let rows = [(key(1, 1), "mercy"), (key(1, 2), "light")];
        let index = Index::parse("test.qv", &build(64, &rows)).unwrap();
        assert!(index
            .nearest(&index.embed("xyzzy quuxbaz"), 10, |_| true)
            .is_empty());
    }

    #[test]
    fn a_scope_filter_keeps_the_scan_inside_it() {
        let rows = [
            (key(1, 1), "mercy"),
            (key(2, 1), "mercy"),
            (key(2, 2), "mercy"),
        ];
        let index = Index::parse("test.qv", &build(32, &rows)).unwrap();

        let hits = index.nearest(&index.embed("mercy"), 10, |k| k.primary == 2);
        assert_eq!(hits.len(), 2, "both 2:1 and 2:2 match exactly: {hits:?}");
        assert!(hits.iter().all(|(k, _)| k.primary == 2));
    }

    #[test]
    fn a_record_indexed_twice_is_reported_once() {
        let rows = [
            (
                Key {
                    primary: 1,
                    number: 1,
                    lang: 0,
                },
                "بسم الله",
            ),
            (
                Key {
                    primary: 1,
                    number: 1,
                    lang: 1,
                },
                "in the name of Allah",
            ),
        ];
        let index = Index::parse("test.qv", &build(32, &rows)).unwrap();

        let hits = index.nearest(&index.embed("name of Allah"), 10, |_| true);
        assert_eq!(hits.len(), 1, "the two languages should merge: {hits:?}");
        assert_eq!(hits[0].0.lang, 1, "the better-scoring field should win");
    }

    #[test]
    fn the_limit_is_honoured() {
        let rows: Vec<_> = (1..=10).map(|n| (key(1, n), "mercy and light")).collect();
        let borrowed: Vec<_> = rows.iter().map(|(k, t)| (*k, *t)).collect();
        let index = Index::parse("test.qv", &build(32, &borrowed)).unwrap();

        assert_eq!(index.nearest(&index.embed("mercy"), 3, |_| true).len(), 3);
        assert_eq!(index.nearest(&index.embed("mercy"), 0, |_| true).len(), 0);
    }

    #[test]
    fn malformed_files_are_data_errors_not_panics() {
        assert!(Index::parse("x.qv", b"").is_err());
        assert!(Index::parse("x.qv", b"NOTMAGIC").is_err());

        // Header promises more vectors than the file holds.
        let mut bytes = build(32, &[(key(1, 1), "a")]);
        bytes[12] = 200;
        let error = Index::parse("x.qv", &bytes).unwrap_err();
        assert_eq!(error.code(), "QQL_INVALID_DATA_FILE");

        // An embedder this build cannot reproduce.
        let mut bytes = build(32, &[(key(1, 1), "a")]);
        bytes[16] = 99;
        assert!(Index::parse("x.qv", &bytes).is_err());
    }

    /// A term with no tokens embeds to all zeros. That must score 0 rather
    /// than dividing by a zero norm and producing NaN, which would poison the
    /// sort and could rank junk first.
    #[test]
    fn an_unembeddable_query_scores_zero_rather_than_nan() {
        let index = Index::parse("x.qv", &build(32, &[(key(1, 1), "text")])).unwrap();
        let query = index.embed("!!!");
        assert!(query.iter().all(|v| *v == 0));

        let raw = dot(&query, index.vector(0).unwrap());
        assert_eq!(raw, 0.0);
        assert!(!raw.is_nan());

        // Scoring zero, it falls under the floor and is not reported.
        assert!(index.nearest(&query, 5, |_| true).is_empty());
    }

    /// A stopword must not move the vector at all, however often it appears.
    #[test]
    fn weighting_zeroes_stopwords() {
        let weights = Weights {
            default: 1.0,
            table: Vec::new(),
        };
        assert_eq!(
            weighted_embed("mercy", 64, &weights),
            weighted_embed("the mercy of the and is", 64, &weights),
            "stopwords still reach the projection"
        );
    }

    /// A whole word outweighs its own trigrams, so an exact match dominates
    /// a morphological one.
    #[test]
    fn a_whole_word_outweighs_its_trigrams() {
        let weights = Weights {
            default: 1.0,
            table: Vec::new(),
        };
        // `charity` and `charitable` share the trigrams of `charit`; only the
        // first shares the whole word.
        let query = weighted_embed("charity", 256, &weights);
        let exact = weighted_embed("charity", 256, &weights);
        let kin = weighted_embed("charitable", 256, &weights);
        assert!(
            dot(&query, &exact) > dot(&query, &kin),
            "the exact word should win"
        );
        assert!(dot(&query, &kin) > 0.0, "but trigrams should still carry");
    }

    /// The table rides in the file, so a round trip must preserve both the
    /// weights and the vectors they produced.
    #[test]
    fn a_weighted_index_round_trips() {
        let weights = Weights {
            default: 4.0,
            // Sorted by hash, as the format requires.
            table: {
                let mut t = vec![(hash("mercy"), 0.5f32), (hash("praise"), 2.0f32)];
                t.sort_by_key(|(h, _)| *h);
                t
            },
        };
        let rows = [(key(1, 1), "mercy and praise"), (key(1, 2), "guidance")];
        let bytes = build_weighted(64, &rows, &weights);

        let index = Index::parse("test.qv", &bytes).unwrap();
        assert_eq!(index.embedder(), Embedder::HashedIdf);
        assert_eq!(index.weights().len(), 2);
        assert!(!index.weights().is_empty());
        // A document embedded through the parsed index matches what was
        // written — the weights survived the round trip.
        assert_eq!(index.embed("mercy and praise"), index.vector(0).unwrap());
    }

    /// An unsorted table would break the binary search, so it is refused.
    #[test]
    fn an_unsorted_weight_table_is_a_data_error() {
        let weights = Weights {
            default: 1.0,
            table: vec![(9_000, 1.0), (10, 2.0)],
        };
        let bytes = build_weighted(64, &[(key(1, 1), "mercy")], &weights);
        let error = Index::parse("test.qv", &bytes).unwrap_err();
        assert_eq!(error.code(), "QQL_INVALID_DATA_FILE");
    }

    /// Embedder 1 files predate the table and must keep working untouched.
    #[test]
    fn an_unweighted_index_still_parses() {
        let bytes = build(64, &[(key(1, 1), "mercy")]);
        let index = Index::parse("test.qv", &bytes).unwrap();
        assert_eq!(index.embedder(), Embedder::Hashed);
        assert!(index.weights().is_empty());
        assert_eq!(index.embed("mercy"), hashed_embed("mercy", 64));
    }
}
