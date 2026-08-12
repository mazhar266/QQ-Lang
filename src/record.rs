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
    pub ar: String,
    /// English text.
    pub en: String,
}
