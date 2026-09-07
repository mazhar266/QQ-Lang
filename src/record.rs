// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! A single resolved text item.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// One resolved ayah, hadith, or supplication.
///
/// Only `source`, `collection`, `ar`, and `en` are required of every source.
/// Anything else — `surah`/`ayah` for Quran, `chapter`/`number`/`narrator` for
/// hadith — goes in `extra` and is flattened to the top level of the JSON, so
/// sources are not forced into a shared shape.
///
/// `emlaei` is the one addition to that rule: it is a *text* field rather than
/// metadata, and [`Context`](crate::Context)'s source-agnostic search matches
/// it alongside `ar` and `en`. Putting it in `extra` would force the search
/// path to know a source-specific key name, which is exactly the coupling the
/// rest of this design avoids.
///
/// `BTreeMap` rather than `HashMap` so key order in the output is
/// deterministic across runs.
#[derive(Debug, Clone, Serialize)]
pub struct Record {
    /// Source code, e.g. `"Q"`.
    pub source: String,
    /// Human-readable collection name, e.g. `"Quran"`.
    pub collection: String,
    /// Source-specific metadata, flattened into the record.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
    /// Arabic text, byte-for-byte as stored.
    ///
    /// For the Quran this is Tanzil's **Uthmani** text, in mushaf orthography.
    pub ar: String,
    /// Simplified (Emlaei) Arabic spelling, where the source has one.
    ///
    /// Uthmani and Emlaei differ in the consonantal skeleton, not just in
    /// marks: `ٱلصَّلَوٰةَ` against `الصلاة`, `ٱلْكِتَٰب` against `الكتاب`. Folding
    /// cannot bridge that — it drops marks, and a waw stays a waw — so a
    /// reader typing modern spelling would silently miss those words. Carrying
    /// both means the mushaf text stays exact and search still answers.
    ///
    /// Empty for every source that has no second spelling, and omitted from
    /// the JSON when empty, so those records are unchanged on the wire.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub emlaei: String,
    /// English text.
    pub en: String,
}
