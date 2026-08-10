//! Hadith resolver, shared by every collection in the nine books.
//!
//! Reads `hadith-json/db/by_chapter/the_9_books/{book}/{chapter}.json`. One
//! implementation covers Bukhari, Muslim, Abu Dawud, Tirmidhi, Nasa'i, and Ibn
//! Majah — they differ only in a code, a name, and a directory, so they are
//! instances rather than separate types.
//!
//! # Numbering
//!
//! `B:C:N` means **chapter C, the N-th hadith within that chapter**. The
//! upstream `by_chapter` files renumber from 1 in every chapter, and QQL takes
//! that as its canonical scheme for v1. It is *not* the book-global number
//! most citations use ("Bukhari 6018"), which lives in the `by_book` files.
//! Mapping global numbers is deliberately out of scope here — it is a resolver
//! concern, not a parser one, so it can be added later without touching the
//! grammar.

use crate::ast::Reference;
use crate::error::Error;
use crate::record::Record;
use crate::repo::Repository;
use crate::sources::Source;
use serde::Deserialize;

const DIR: &str = "hadith-json/db/by_chapter/the_9_books";

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

#[derive(Debug, Deserialize)]
struct Hadith {
    id: u32,
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
        let chapter_id = reference.primary;
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
