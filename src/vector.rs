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
//! 16  embedder  u32      1 = hashed
//! 20  reserved  u32
//! 24  keys      count × { u32 primary, u32 number, u32 lang }
//! ..  vectors   count × dims × i8, L2-normalized then scaled by 127
//! ```
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
}

impl Embedder {
    fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Embedder::Hashed),
            _ => None,
        }
    }
}

/// Where one vector sits in its collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// Surah or chapter.
    pub primary: u32,
    /// Ayah or item within that primary.
    pub number: u32,
    /// Which field this vector came from: 0 Arabic, 1 English.
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

        Ok(Index {
            dims,
            embedder,
            keys,
            data,
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
        }
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
pub fn tokens(text: &str) -> Vec<String> {
    let folded = crate::search::fold(text);
    let mut out = Vec::new();

    for word in folded.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        out.push(word.to_string());

        let chars: Vec<char> = word.chars().collect();
        if chars.len() > 3 {
            for window in chars.windows(3) {
                out.push(window.iter().collect());
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
    let mut acc = vec![0f32; dims];

    for token in tokens(text) {
        let mut h = hash(&token);
        for _ in 0..4 {
            let slot = (h % dims as u64) as usize;
            let sign = if h & 0x8000_0000_0000_0000 == 0 {
                1.0
            } else {
                -1.0
            };
            acc[slot] += sign;
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
}
