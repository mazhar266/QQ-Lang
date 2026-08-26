// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Hadith resolver, shared by every collection in the nine books.
//!
//! Reads `hadith/{book}/{chapter}.json`. One
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
//! - `B::N` — the **canonical citation number**: 'Abd al-Baqi's numbering for
//!   Bukhari (1..7563), Dar-us-Salam's for Muslim, the sunnah.com reference
//!   numbers for the rest. `B::6403` returns what the world cites as
//!   Bukhari 6403.
//!
//! The dataset's own sequential numbering drifts from those citations (its
//! Bukhari has 7277 entries against the canonical 7563 — repetitions and
//! variants counted differently), so the flat form resolves through a small
//! committed map, `canonical/{CODE}.json`, of canonical number →
//! `(chapter, item)`. Built and validated by `scripts/build-canonical.py`.
//!
//! The canonical space has holes: front matter (Muslim's Muqaddima owns
//! canonical 1..92) and lettered variants (`1771.5`) are not addressable.
//! Asking for such a number alone is an error; a range simply skips them, so
//! `B::1-7563` and an unscoped search walk the whole book without tripping.
//!
//! Records from the flat form carry `"numbering": "book"` and the canonical
//! number, so the two modes cannot be confused when a single query mixes
//! them.

use crate::ast::{Range, Reference};
use crate::error::Error;
use crate::record::Record;
use crate::repo::Repository;
use crate::sources::Source;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Canonical citation number (as a JSON key) → `(chapter, item)`.
type CanonicalMap = HashMap<String, (u32, u32)>;

const DIR: &str = "hadith";
const CANONICAL_DIR: &str = "canonical";

/// One hadith collection, identified by its data directory.
#[derive(Debug, Clone, Copy)]
pub struct HadithCollection {
    code: &'static str,
    name: &'static str,
    dir: &'static str,
}

impl HadithCollection {
    /// Define a collection: QQL code, display name, and directory under
    /// `sources/hadith`.
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
    /// Position within the chapter, restarting at 1 in every chapter file.
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

    fn total(&self, repo: &mut Repository) -> Result<Option<u32>, Error> {
        let map = self.canonical(repo)?;
        Ok(Some(
            map.keys()
                .filter_map(|k| k.parse::<u32>().ok())
                .max()
                .unwrap_or(0),
        ))
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
                    (
                        "narrator".to_string(),
                        hadith.english.narrator.clone().into(),
                    ),
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
    /// The committed canonical-number map for this collection:
    /// canonical citation number → `(chapter, item)`.
    fn canonical(&self, repo: &mut Repository) -> Result<Arc<CanonicalMap>, Error> {
        match repo.load(format!("{CANONICAL_DIR}/{}.json", self.code)) {
            Err(Error::DataFileNotFound { .. }) => Err(Error::Unsupported {
                detail: format!(
                    "no canonical numbering map for {}; build it with scripts/build-canonical.py",
                    self.code
                ),
            }),
            other => other,
        }
    }

    /// `B::N` — the canonical citation number, resolved through the map and
    /// then the ordinary chapter path, so a hit is byte-identical to its
    /// `B:chapter:item` form.
    ///
    /// The canonical space has holes (front matter, lettered variants). A
    /// number asked for *by itself* must exist; a range skips the holes, so
    /// `B::1-7563` and an unscoped search walk the whole book cleanly.
    fn resolve_flat(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let map = self.canonical(repo)?;
        let total = map
            .keys()
            .filter_map(|k| k.parse::<u32>().ok())
            .max()
            .unwrap_or(0);

        let singles: HashSet<u32> = reference
            .ranges
            .iter()
            .filter(|r| r.from == r.to)
            .map(|r| r.from)
            .collect();

        for number in reference.expand(total)? {
            let Some(&(chapter, item)) = map.get(number.to_string().as_str()) else {
                if singles.contains(&number) {
                    return Err(Error::ReferenceNotFound {
                        detail: format!(
                            "{}::{number} (front matter or a lettered variant — \
                             not an addressable canonical number)",
                            self.code
                        ),
                    });
                }
                continue;
            };

            let mut found = Vec::new();
            self.resolve(
                repo,
                &Reference {
                    source: reference.source.clone(),
                    primary: Some(chapter),
                    ranges: vec![Range {
                        from: item,
                        to: item,
                    }],
                    search: None,
                },
                &mut found,
            )?;

            for mut record in found {
                record.extra.insert("number".to_string(), number.into());
                record.extra.insert("numbering".to_string(), "book".into());
                out.push(record);
            }
        }

        Ok(())
    }
}
