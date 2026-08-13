// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Data-driven sources: add a collection with a JSON description instead of
//! Rust code.
//!
//! A [`SourceSpec`] says where the data lives and which fields hold the text.
//! That is enough to serve queries for a new short code — `X:1:2` — without
//! touching the lexer, the parser, or this crate.
//!
//! The spec is deliberately small. It covers the two file layouts the built-in
//! sources already use:
//!
//! **One file per primary**, with the items at the top level:
//!
//! ```json
//! {
//!   "code": "X",
//!   "name": "My Collection",
//!   "path": "mydata/{primary}.json",
//!   "items": "verses",
//!   "ar": "text",
//!   "en": "translation"
//! }
//! ```
//!
//! **One file for everything**, with chapters selected by an id field:
//!
//! ```json
//! {
//!   "code": "X",
//!   "name": "My Collection",
//!   "path": "mydata/book.json",
//!   "chapters": "English",
//!   "chapter_id": "ID",
//!   "items": "TEXT",
//!   "ar": "ARABIC_TEXT",
//!   "en": "TRANSLATED_TEXT"
//! }
//! ```
//!
//! Field paths are dotted, so `"english.text"` reaches into a nested object.
//! An empty path means "this value itself", which is how you point at a file
//! that is a bare array.
//!
//! Anything more irregular than this wants a real `impl Source` — the Hisnul
//! Muslim resolver exists because its data has duplicate keys and a misspelled
//! field, which no declarative mapping should try to express.

use crate::ast::Reference;
use crate::error::Error;
use crate::record::Record;
use crate::repo::Repository;
use crate::sources::Source;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Declarative description of a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    /// Short code, e.g. `"X"`. Normalized to uppercase on load.
    pub code: String,
    /// Display name, used as `collection` in results.
    pub name: String,
    /// Alternate codes that also select this source.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Path to the data file, relative to the data directory.
    ///
    /// `{primary}` is replaced with the reference's primary number. Including
    /// it means one file per chapter; leaving it out means a single file.
    pub path: String,
    /// Dotted path to the array of chapters, for single-file layouts.
    #[serde(default)]
    pub chapters: Option<String>,
    /// Field within a chapter holding its number. Required with `chapters`.
    #[serde(default)]
    pub chapter_id: Option<String>,
    /// Dotted path to the array of items. Empty means the container itself.
    pub items: String,
    /// Dotted path within an item to the Arabic text.
    pub ar: String,
    /// Dotted path within an item to the translation.
    pub en: String,
    /// Field within an item holding its number. Without it, items are
    /// selected by position, which is usually what you want.
    #[serde(default)]
    pub item_id: Option<String>,
    /// Name for the primary in results. Defaults to `"primary"`; set it to
    /// `"surah"` or `"chapter"` to match the vocabulary of your collection.
    #[serde(default)]
    pub primary_key: Option<String>,
    /// Extra result fields taken from the item: output key → dotted path.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// Extra result fields taken from the chapter or file: output key → path.
    #[serde(default)]
    pub container_metadata: BTreeMap<String, String>,
    /// Where to find the collection numbered straight through, for `X::100`.
    ///
    /// Without it, a flat reference to this source is a "not found" error
    /// rather than a silent fallback to something that might be wrong.
    #[serde(default)]
    pub flat: Option<FlatSpec>,
}

/// The collection laid out as one continuous run of items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatSpec {
    /// Data file, relative to the data directory. No `{primary}` here.
    pub path: String,
    /// Dotted path to the array of items. Empty means the file is the array.
    #[serde(default)]
    pub items: String,
    /// Field holding each item's number. Without it, items are positional.
    #[serde(default)]
    pub item_id: Option<String>,
}

/// A [`Source`] driven entirely by a [`SourceSpec`].
#[derive(Debug, Clone)]
pub struct JsonSource {
    spec: SourceSpec,
}

impl JsonSource {
    /// Build a source from a spec, normalizing its codes to uppercase so it
    /// matches what the parser produces.
    pub fn new(mut spec: SourceSpec) -> Self {
        spec.code = spec.code.to_ascii_uppercase();
        for alias in &mut spec.aliases {
            *alias = alias.to_ascii_uppercase();
        }
        JsonSource { spec }
    }

    /// The spec this source was built from.
    pub fn spec(&self) -> &SourceSpec {
        &self.spec
    }

    fn primary_key(&self) -> &str {
        self.spec.primary_key.as_deref().unwrap_or("primary")
    }
}

/// Walk a dotted path. An empty path yields the value itself.
fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for key in path.split('.') {
        current = current.get(key)?;
    }
    Some(current)
}

/// A path resolved to text. Missing or non-string yields `""` rather than
/// failing the whole query — a spec that names a field some records lack is a
/// normal situation, not a corrupt file.
fn text(value: &Value, path: &str) -> String {
    at(value, path)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl Source for JsonSource {
    fn code(&self) -> &str {
        &self.spec.code
    }

    fn name(&self) -> &str {
        &self.spec.name
    }

    fn aliases(&self) -> Vec<&str> {
        self.spec.aliases.iter().map(String::as_str).collect()
    }

    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let Some(primary) = reference.primary else {
            return self.resolve_flat(repo, reference, out);
        };

        let templated = self.spec.path.contains("{primary}");
        let path = self.spec.path.replace("{primary}", &primary.to_string());

        let root: std::sync::Arc<Value> = match repo.load(&path) {
            Ok(root) => root,
            // With one file per chapter, a missing file means the chapter does
            // not exist — a semantic error, not a storage failure. With a
            // fixed path, a missing file really is a missing file.
            Err(Error::DataFileNotFound { .. }) if templated => {
                return Err(Error::ReferenceNotFound {
                    detail: format!("{}:{primary} (no such chapter)", self.spec.code),
                })
            }
            Err(other) => return Err(other),
        };

        let container = match (&self.spec.chapters, &self.spec.chapter_id) {
            (Some(chapters), Some(id_field)) => {
                let list = at(&root, chapters)
                    .and_then(Value::as_array)
                    .ok_or_else(|| Error::InvalidDataFile {
                        path: path.clone(),
                        detail: format!("no array at '{chapters}'"),
                    })?;
                list.iter()
                    .find(|c| {
                        at(c, id_field).and_then(Value::as_u64) == Some(u64::from(primary))
                    })
                    .ok_or_else(|| Error::ReferenceNotFound {
                        detail: format!("{}:{primary} (no such chapter)", self.spec.code),
                    })?
            }
            (Some(_), None) => {
                return Err(Error::InvalidDataFile {
                    path: path.clone(),
                    detail: "spec sets 'chapters' without 'chapter_id'".into(),
                })
            }
            _ => &root,
        };

        let items = at(container, &self.spec.items)
            .and_then(Value::as_array)
            .ok_or_else(|| Error::InvalidDataFile {
                path: path.clone(),
                detail: format!("no array at '{}'", self.spec.items),
            })?;

        let total = u32::try_from(items.len()).map_err(|_| Error::InvalidDataFile {
            path: path.clone(),
            detail: "implausible item count".into(),
        })?;

        for number in reference.expand(total)? {
            let item = match &self.spec.item_id {
                Some(field) => items
                    .iter()
                    .find(|i| at(i, field).and_then(Value::as_u64) == Some(u64::from(number))),
                None => items.get((number - 1) as usize),
            }
            .ok_or_else(|| Error::ReferenceNotFound {
                detail: format!("{}:{primary}:{number}", self.spec.code),
            })?;

            let mut extra: BTreeMap<String, Value> = BTreeMap::new();
            extra.insert(self.primary_key().to_string(), primary.into());
            extra.insert("number".to_string(), number.into());

            for (key, path) in &self.spec.container_metadata {
                if let Some(value) = at(container, path) {
                    extra.insert(key.clone(), value.clone());
                }
            }
            for (key, path) in &self.spec.metadata {
                if let Some(value) = at(item, path) {
                    extra.insert(key.clone(), value.clone());
                }
            }

            out.push(Record {
                source: self.spec.code.clone(),
                collection: self.spec.name.clone(),
                extra,
                ar: text(item, &self.spec.ar),
                en: text(item, &self.spec.en),
            });
        }

        Ok(())
    }
}

impl JsonSource {
    /// `X::N` — the whole collection numbered straight through, which only
    /// works if the spec says where that lives.
    fn resolve_flat(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let Some(flat) = &self.spec.flat else {
            return Err(Error::ReferenceNotFound {
                detail: format!(
                    "{}:: (this source defines no book-wide numbering; add a \"flat\" block to its spec)",
                    self.spec.code
                ),
            });
        };

        let root: std::sync::Arc<Value> = repo.load(&flat.path)?;
        let items = at(&root, &flat.items)
            .and_then(Value::as_array)
            .ok_or_else(|| Error::InvalidDataFile {
                path: flat.path.clone(),
                detail: format!("no array at '{}'", flat.items),
            })?;

        let total = u32::try_from(items.len()).map_err(|_| Error::InvalidDataFile {
            path: flat.path.clone(),
            detail: "implausible item count".into(),
        })?;

        for number in reference.expand(total)? {
            let item = match &flat.item_id {
                Some(field) => items
                    .iter()
                    .find(|i| at(i, field).and_then(Value::as_u64) == Some(u64::from(number))),
                None => items.get((number - 1) as usize),
            }
            .ok_or_else(|| Error::ReferenceNotFound {
                detail: format!("{}::{number}", self.spec.code),
            })?;

            let mut extra: BTreeMap<String, Value> = BTreeMap::new();
            extra.insert("number".to_string(), number.into());
            extra.insert("numbering".to_string(), "book".into());
            for (key, path) in &self.spec.metadata {
                if let Some(value) = at(item, path) {
                    extra.insert(key.clone(), value.clone());
                }
            }

            out.push(Record {
                source: self.spec.code.clone(),
                collection: self.spec.name.clone(),
                extra,
                ar: text(item, &self.spec.ar),
                en: text(item, &self.spec.en),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_paths_reach_into_nested_objects() {
        let value = serde_json::json!({"english": {"text": "hello"}, "n": 3});
        assert_eq!(at(&value, "english.text").unwrap(), "hello");
        assert_eq!(at(&value, "n").unwrap(), 3);
        assert!(at(&value, "english.missing").is_none());
        assert!(at(&value, "nope.deeper").is_none());
    }

    #[test]
    fn an_empty_path_is_the_value_itself() {
        let value = serde_json::json!([1, 2, 3]);
        assert!(at(&value, "").unwrap().is_array());
    }

    #[test]
    fn missing_text_fields_yield_empty_not_an_error() {
        let value = serde_json::json!({"a": 1});
        assert_eq!(text(&value, "missing"), "");
        // A number is not text; do not stringify it into scripture.
        assert_eq!(text(&value, "a"), "");
    }

    #[test]
    fn codes_and_aliases_normalize_to_uppercase() {
        let source = JsonSource::new(SourceSpec {
            code: "x".into(),
            name: "Test".into(),
            aliases: vec!["mine".into()],
            path: "x.json".into(),
            chapters: None,
            chapter_id: None,
            items: "items".into(),
            ar: "ar".into(),
            en: "en".into(),
            item_id: None,
            primary_key: None,
            metadata: BTreeMap::new(),
            container_metadata: BTreeMap::new(),
            flat: None,
        });
        assert_eq!(source.code(), "X");
        assert_eq!(source.aliases(), ["MINE"]);
    }
}
