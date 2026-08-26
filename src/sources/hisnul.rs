// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Hisnul Muslim resolver.
//!
//! Reads `hisnul-muslim/husn_en.json` — a single ~280 KB file holding all
//! 132 chapters, loaded once and cached.
//!
//! # Numbering
//!
//! `HM:27` is chapter **27**, and `HM:27:1-3` selects the first three
//! supplications within it.
//!
//! Two things about the upstream file drive this implementation:
//!
//! - Chapters are **not stored in ID order** — the array begins 27, 28, 1, 6,
//!   … — so chapters are looked up by their `ID` field. Indexing by array
//!   position would silently return the wrong supplication.
//! - Supplication `ID`s are global across the whole book (75, 76, … 267), not
//!   per-chapter, so the selector counts *position within the chapter*
//!   instead. That keeps `HM:27:1` meaning "the first supplication of chapter
//!   27", consistent with how `Q` and the hadith collections read.
//!
//! This is the source that proves the layering works: it has a different file
//! layout, a different numbering scheme, and a different record shape from
//! every other source, and it needed no lexer or parser change.

use crate::ast::Reference;
use crate::error::Error;
use crate::record::Record;
use crate::repo::Repository;
use crate::sources::Source;
use serde::Deserialize;
use serde_json::Value;

const FILE: &str = "hisnul-muslim/husn_en.json";

/// `HM` — Hisnul Muslim (Fortress of the Muslim).
#[derive(Debug, Default)]
pub struct HisnulMuslim;

#[derive(Debug, Deserialize)]
struct Book {
    #[serde(rename = "English")]
    chapters: Vec<Chapter>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct Chapter {
    id: u32,
    title: String,
    text: Vec<Supplication>,
}

/// A supplication, held as a raw map rather than a typed struct.
///
/// This is the one place in the crate that deliberately keeps JSON around
/// instead of deserializing into named fields, because the upstream file is
/// not uniform enough for a derived `Deserialize` to accept it at all:
///
/// - two entries repeat a key (`TRANSLATED_TEXT`, `LANGUAGE_ARABIC_TRANSLATED_TEXT`)
///   with different values, which serde's derive rejects outright as a
///   duplicate field. `Map` takes the last one, matching every other JSON
///   parser's behavior;
/// - one entry (ID 267) spells the Arabic key `Text` instead of `ARABIC_TEXT`;
/// - three entries omit a field entirely.
///
/// The accessors below absorb all of that in one place, so a data quirk cannot
/// take down the whole file — a missing field yields an empty string rather
/// than a failed load. `Book` and `Chapter` stay typed; only this level is
/// messy enough to need it.
#[derive(Debug, Deserialize)]
struct Supplication(serde_json::Map<String, Value>);

impl Supplication {
    fn field(&self, key: &str) -> &str {
        self.0.get(key).and_then(Value::as_str).unwrap_or("")
    }

    fn arabic(&self) -> &str {
        match self.field("ARABIC_TEXT") {
            "" => self.field("Text"),
            text => text,
        }
    }

    fn english(&self) -> &str {
        self.field("TRANSLATED_TEXT")
    }

    /// Usually a transliteration of the Arabic, but sometimes a recitation
    /// instruction ("Recite Ayat-Al-Kursiy (Al-Baqarah :255)") — upstream
    /// stores both under one key, so this is exposed as a neutral `note`
    /// rather than mislabelled as a transliteration.
    fn note(&self) -> &str {
        self.field("LANGUAGE_ARABIC_TRANSLATED_TEXT")
    }

    /// How many times to say it — 1, 3, 10, 33, or 100 in this data.
    fn repeat(&self) -> u32 {
        self.0
            .get("REPEAT")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(1)
            .max(1)
    }

    fn audio(&self) -> &str {
        self.field("AUDIO")
    }

    /// Book-global identifier, 1..267 in this data.
    fn id(&self) -> Option<u32> {
        self.0
            .get("ID")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    }
}

impl Source for HisnulMuslim {
    fn code(&self) -> &str {
        "HM"
    }

    fn name(&self) -> &str {
        "Hisnul Muslim"
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["HISN"]
    }

    fn total(&self, repo: &mut Repository) -> Result<Option<u32>, Error> {
        let book: std::sync::Arc<Book> = repo.load(FILE)?;
        u32::try_from(book.chapters.iter().map(|c| c.text.len()).sum::<usize>())
            .map(Some)
            .map_err(|_| Error::InvalidDataFile {
                path: FILE.to_string(),
                detail: "implausible supplication count".into(),
            })
    }

    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let book: std::sync::Arc<Book> = repo.load(FILE)?;

        // `HM::75` — supplication IDs run 1..267 across the whole book.
        let Some(wanted) = reference.primary else {
            return self.resolve_flat(&book, reference, out);
        };

        // By ID, never by position — see the module docs.
        let chapter = book
            .chapters
            .iter()
            .find(|c| c.id == wanted)
            .ok_or_else(|| Error::ReferenceNotFound {
                detail: format!("HM:{wanted} (no such chapter)"),
            })?;

        let total = u32::try_from(chapter.text.len()).map_err(|_| Error::InvalidDataFile {
            path: FILE.to_string(),
            detail: "implausible supplication count".into(),
        })?;

        for number in reference.expand(total)? {
            let item = chapter.text.get((number - 1) as usize).ok_or_else(|| {
                Error::ReferenceNotFound {
                    detail: format!("HM:{wanted}:{number}"),
                }
            })?;

            let mut extra: std::collections::BTreeMap<String, Value> = [
                ("chapter".to_string(), wanted.into()),
                ("chapter_title".to_string(), chapter.title.clone().into()),
                ("number".to_string(), number.into()),
                ("repeat".to_string(), item.repeat().into()),
            ]
            .into_iter()
            .collect();

            // Present on most but not all entries; omit rather than emit "".
            if !item.note().trim().is_empty() {
                extra.insert("note".to_string(), item.note().into());
            }
            if !item.audio().trim().is_empty() {
                extra.insert("audio".to_string(), item.audio().into());
            }

            out.push(Record {
                source: self.code().to_string(),
                collection: self.name().to_string(),
                extra,
                ar: item.arabic().to_string(),
                en: item.english().to_string(),
            });
        }

        Ok(())
    }
}

impl HisnulMuslim {
    /// `HM::N` — the N-th supplication of the book, across every chapter.
    fn resolve_flat(
        &self,
        book: &Book,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let total = u32::try_from(book.chapters.iter().map(|c| c.text.len()).sum::<usize>())
            .map_err(|_| Error::InvalidDataFile {
                path: FILE.to_string(),
                detail: "implausible supplication count".into(),
            })?;

        for number in reference.expand(total)? {
            // Items carry a book-global ID, so find it rather than counting.
            let (chapter, item) = book
                .chapters
                .iter()
                .find_map(|c| {
                    c.text
                        .iter()
                        .find(|t| t.id() == Some(number))
                        .map(|t| (c, t))
                })
                .ok_or_else(|| Error::ReferenceNotFound {
                    detail: format!("HM::{number}"),
                })?;

            let mut extra: std::collections::BTreeMap<String, Value> = [
                ("chapter".to_string(), chapter.id.into()),
                ("chapter_title".to_string(), chapter.title.clone().into()),
                ("number".to_string(), number.into()),
                ("numbering".to_string(), "book".into()),
                ("repeat".to_string(), item.repeat().into()),
            ]
            .into_iter()
            .collect();

            if !item.note().trim().is_empty() {
                extra.insert("note".to_string(), item.note().into());
            }
            if !item.audio().trim().is_empty() {
                extra.insert("audio".to_string(), item.audio().into());
            }

            out.push(Record {
                source: self.code().to_string(),
                collection: self.name().to_string(),
                extra,
                ar: item.arabic().to_string(),
                en: item.english().to_string(),
            });
        }

        Ok(())
    }
}
