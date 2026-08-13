// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Hadith resolver, shared by every collection in the nine books.
//!
//! Reads `hadith-json/db/by_chapter/the_9_books/{book}/{chapter}.json`. One
//! implementation covers Bukhari, Muslim, Abu Dawud, Tirmidhi, Nasa'i, and Ibn
//! Majah — they differ only in a code, a name, and a directory, so they are
//! instances rather than separate types.
//!
//! # Numbering
//!
//! Two addressing modes, both served here:
//!
//! - `B:C:N` — **chapter C, the N-th hadith within that chapter**, matching
//!   the `by_chapter` files, which renumber from 1 in every chapter.
//! - `B::N` — the **book-global** number most citations use ("Bukhari 6018"),
//!   read from the `by_book` files, which number 1..n across the collection.
//!
//! Records from the flat form carry `"numbering": "book"` so the two cannot be
//! confused when a single query mixes them.
//!
//! The `by_book` files are large — Bukhari is 12 MB — so they load only when a
//! flat reference asks for one, and then stay cached for the life of the
//! context.

use crate::ast::Reference;
use crate::error::Error;
use crate::record::Record;
use crate::repo::Repository;
use crate::sources::Source;
use serde::Deserialize;

const DIR: &str = "hadith-json/db/by_chapter/the_9_books";
const BOOK_DIR: &str = "hadith-json/db/by_book/the_9_books";

/// One hadith collection, identified by its data directory.
#[derive(Debug, Clone, Copy)]
pub struct HadithCollection {
    code: &'static str,
    name: &'static str,
    dir: &'static str,
}

impl HadithCollection {
    /// Define a collection: QQL code, display name, and directory under
    /// `hadith-json/db/by_chapter/the_9_books`.
    pub const fn new(code: &'static str, name: &'static str, dir: &'static str) -> Self {
        HadithCollection { code, name, dir }
    }
}

#[derive(Debug, Deserialize)]
struct ChapterFile {
    hadiths: Vec<Hadith>,
    chapter: Option<ChapterInfo>,
}

#[derive(Debug, Deserialize)]
struct ChapterInfo {
    arabic: String,
    english: String,
}

/// The whole collection in one file, numbered 1..n.
#[derive(Debug, Deserialize)]
struct BookFile {
    chapters: Vec<Chapter>,
    hadiths: Vec<Hadith>,
}

#[derive(Debug, Deserialize)]
struct Chapter {
    /// Not always an integer — Sunan an-Nasa'i has a chapter `35.2` — so this
    /// keeps whatever number upstream used rather than forcing `u32` and
    /// failing the whole file.
    id: serde_json::Number,
    arabic: String,
    english: String,
}

#[derive(Debug, Deserialize)]
struct Hadith {
    /// Position within the enclosing file. In `by_chapter` this restarts at 1
    /// in every chapter; in `by_book` it is an identifier spanning *all nine
    /// books* (Bukhari 1..7277, Muslim 7278..14736, …), which is why the flat
    /// form must not use it.
    id: u32,
    /// The per-book number, 1..n — what citations mean by "Bukhari 6018".
    #[serde(rename = "idInBook")]
    id_in_book: u32,
    /// Matches [`Chapter::id`], so it carries the same caveat.
    #[serde(rename = "chapterId")]
    chapter_id: serde_json::Number,
    arabic: String,
    english: English,
}

#[derive(Debug, Deserialize)]
struct English {
    narrator: String,
    text: String,
}

impl Source for HadithCollection {
    fn code(&self) -> &str {
        self.code
    }

    fn name(&self) -> &str {
        self.name
    }

    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        // `B::100` — number across the whole book instead of within a chapter.
        let Some(chapter_id) = reference.primary else {
            return self.resolve_flat(repo, reference, out);
        };

        if chapter_id == 0 {
            return Err(Error::ReferenceNotFound {
                detail: format!("{}:0 (chapters start at 1)", self.code),
            });
        }

        let relative = format!("{DIR}/{}/{chapter_id}.json", self.dir);
        // A chapter past the end of the collection shows up as a missing file.
        // Translate it into a semantic error rather than leaking storage
        // layout to the caller.
        let file: std::sync::Arc<ChapterFile> = match repo.load(&relative) {
            Ok(file) => file,
            Err(Error::DataFileNotFound { .. }) => {
                return Err(Error::ReferenceNotFound {
                    detail: format!("{}:{chapter_id} (no such chapter)", self.code),
                })
            }
            Err(other) => return Err(other),
        };

        let total = u32::try_from(file.hadiths.len()).map_err(|_| Error::InvalidDataFile {
            path: relative.clone(),
            detail: "implausible hadith count".into(),
        })?;

        let (chapter_ar, chapter_en) = match &file.chapter {
            Some(info) => (info.arabic.as_str(), info.english.as_str()),
            None => ("", ""),
        };

        for number in reference.expand(total)? {
            let hadith = file
                .hadiths
                .iter()
                .find(|h| h.id == number)
                .ok_or_else(|| Error::ReferenceNotFound {
                    detail: format!("{}:{chapter_id}:{number}", self.code),
                })?;

            out.push(Record {
                source: self.code.to_string(),
                collection: self.name.to_string(),
                extra: [
                    ("chapter".to_string(), chapter_id.into()),
                    ("chapter_name_ar".to_string(), chapter_ar.into()),
                    ("chapter_name_en".to_string(), chapter_en.into()),
                    ("number".to_string(), number.into()),
                    ("narrator".to_string(), hadith.english.narrator.clone().into()),
                ]
                .into_iter()
                .collect(),
                ar: hadith.arabic.clone(),
                en: hadith.english.text.clone(),
            });
        }

        Ok(())
    }
}

impl HadithCollection {
    /// `B::N` — the N-th hadith of the collection in traditional numbering.
    fn resolve_flat(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let relative = format!("{BOOK_DIR}/{}.json", self.dir);
        let book: std::sync::Arc<BookFile> = repo.load(&relative)?;

        let total = u32::try_from(book.hadiths.len()).map_err(|_| Error::InvalidDataFile {
            path: relative.clone(),
            detail: "implausible hadith count".into(),
        })?;

        for number in reference.expand(total)? {
            // `idInBook`, never `id` — see the field docs. Using `id` happens
            // to work for Bukhari and silently returns the wrong hadith for
            // every other collection.
            let hadith = book
                .hadiths
                .iter()
                .find(|h| h.id_in_book == number)
                .ok_or_else(|| Error::ReferenceNotFound {
                    detail: format!("{}::{number}", self.code),
                })?;

            let chapter = book.chapters.iter().find(|c| c.id == hadith.chapter_id);
            let (chapter_ar, chapter_en) = match chapter {
                Some(c) => (c.arabic.as_str(), c.english.as_str()),
                None => ("", ""),
            };

            out.push(Record {
                source: self.code.to_string(),
                collection: self.name.to_string(),
                extra: [
                    ("chapter".to_string(), hadith.chapter_id.clone().into()),
                    ("chapter_name_ar".to_string(), chapter_ar.into()),
                    ("chapter_name_en".to_string(), chapter_en.into()),
                    ("number".to_string(), number.into()),
                    ("numbering".to_string(), "book".into()),
                    ("narrator".to_string(), hadith.english.narrator.clone().into()),
                ]
                .into_iter()
                .collect(),
                ar: hadith.arabic.clone(),
                en: hadith.english.text.clone(),
            });
        }

        Ok(())
    }
}
