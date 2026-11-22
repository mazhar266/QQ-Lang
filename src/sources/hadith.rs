// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Hadith resolver, shared by every hadith collection QQL carries.
//!
//! Reads `hadith/{book}/{chapter}.json`. One implementation covers all of
//! them — the six canonical Sunni collections, the Muwatta, ad-Darimi, the
//! topical works (Riyad as-Salihin, Bulugh al-Maram, al-Adab al-Mufrad,
//! Mishkat al-Masabih, ash-Shama'il) and the three forties. They differ only
//! in a code, a name, a directory, and whether they have a citation
//! numbering, so they are instances rather than separate types.
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
//!
//! Not every collection has a citation numbering QQL can source. Those are
//! built with [`HadithCollection::chaptered`] and serve `CODE:chapter:number`
//! only; see that constructor for why inventing one would be worse than
//! refusing.

mod numbering;

use crate::ast::Reference;
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
    /// Whether a `canonical/{CODE}.json` map exists, i.e. whether `CODE::N`
    /// means anything for this collection. See [`HadithCollection::chaptered`].
    numbered: bool,
}

impl HadithCollection {
    /// Define a collection that has a canonical citation numbering: QQL code,
    /// display name, and directory under `sources/hadith`.
    ///
    /// Requires a committed `canonical/{code}.json` map — build it with
    /// `scripts/build-canonical.py`.
    pub const fn new(code: &'static str, name: &'static str, dir: &'static str) -> Self {
        HadithCollection {
            code,
            name,
            dir,
            numbered: true,
        }
    }

    /// Define a collection addressable **only** as `CODE:chapter:number`.
    ///
    /// Some collections have no citation numbering QQL can source. The data
    /// carries a per-chapter position and nothing else, and the sequential
    /// position within the book is *not* the number these books are cited by
    /// — publishing it as one would answer real citations with the wrong
    /// hadith, which is the same trap `idInBook` sets for Bukhari.
    ///
    /// So `CODE::N` is refused outright, and `total` reports no book-wide
    /// axis, which turns an unscoped search into `QQL_UNSUPPORTED` rather
    /// than a silently narrowed one.
    pub const fn chaptered(code: &'static str, name: &'static str, dir: &'static str) -> Self {
        HadithCollection {
            code,
            name,
            dir,
            numbered: false,
        }
    }
}

/// File names and record metadata differ for the two separately filed sections.
#[derive(Clone, Copy, Deserialize)]
#[serde(try_from = "String")]
enum ChapterId {
    Number(u32),
    Introduction,
    Nasai35b,
}

impl TryFrom<String> for ChapterId {
    type Error = String;
    fn try_from(key: String) -> Result<Self, String> {
        match key.as_str() {
            "introduction" => Ok(Self::Introduction),
            "35b" => Ok(Self::Nasai35b),
            _ => key
                .parse::<u32>()
                .map(Self::Number)
                .map_err(|_| format!("invalid chapter {key}")),
        }
    }
}

impl std::fmt::Display for ChapterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Introduction => f.write_str("introduction"),
            Self::Nasai35b => f.write_str("35b"),
        }
    }
}

impl From<ChapterId> for serde_json::Value {
    fn from(id: ChapterId) -> Self {
        match id {
            ChapterId::Number(n) => n.into(),
            ChapterId::Introduction => 0.into(),
            ChapterId::Nasai35b => "35b".into(),
        }
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
        if !self.numbered {
            return Ok(None);
        }
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

        let chapter_id = ChapterId::Number(chapter_id);
        let chapter = self.chapter(repo, chapter_id)?;
        let total = u32::try_from(chapter.hadiths.len()).map_err(|_| Error::InvalidDataFile {
            path: format!("{DIR}/{}/{chapter_id}.json", self.dir),
            detail: "implausible hadith count".into(),
        })?;
        for number in reference.expand(total)? {
            out.push(self.record(&chapter, chapter_id, number)?);
        }
        Ok(())
    }

    fn resolve_numbered(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        numbering: crate::Numbering,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        if matches!(self.code, "B" | "M" | "AD" | "T" | "N" | "IM") && self.numbered {
            return self.resolve_edition(repo, reference, numbering, out);
        }
        if !reference.lettered_ranges.is_empty() {
            return Err(Error::ReferenceNotFound {
                detail: format!("{} has no lettered references", self.code),
            });
        }
        self.resolve(repo, reference, out)
    }
}

impl HadithCollection {
    fn chapter(
        &self,
        repo: &mut Repository,
        chapter_id: ChapterId,
    ) -> Result<Arc<ChapterFile>, Error> {
        let relative = format!("{DIR}/{}/{chapter_id}.json", self.dir);
        // A chapter past the end of the collection shows up as a missing file.
        // Translate it into a semantic error rather than leaking storage
        // layout to the caller.
        match repo.load(&relative) {
            Err(Error::DataFileNotFound { .. }) => Err(Error::ReferenceNotFound {
                detail: format!("{}:{chapter_id} (no such chapter)", self.code),
            }),
            result => result,
        }
    }

    fn record(
        &self,
        file: &ChapterFile,
        chapter_id: ChapterId,
        number: u32,
    ) -> Result<Record, Error> {
        let (chapter_ar, chapter_en) = match &file.chapter {
            Some(info) => (info.arabic.as_str(), info.english.as_str()),
            None => ("", ""),
        };

        let hadith = file
            .hadiths
            .iter()
            .find(|h| h.id == number)
            .ok_or_else(|| Error::ReferenceNotFound {
                detail: format!("{}:{chapter_id}:{number}", self.code),
            })?;

        Ok(Record {
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
            emlaei: String::new(),
            ar: hadith.arabic.clone(),
            en: hadith.english.text.clone(),
        })
    }

    /// The committed canonical-number map for this collection:
    /// canonical citation number → `(chapter, item)`.
    fn canonical(&self, repo: &mut Repository) -> Result<Arc<CanonicalMap>, Error> {
        if !self.numbered {
            return Err(Error::Unsupported {
                detail: format!(
                    "{0} has no canonical citation numbering; address it as {0}:chapter:number",
                    self.code
                ),
            });
        }
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

        let mut seen = HashSet::new();
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

            // Bukhari can print several numbers on one narration. Keep the
            // first requested label when a selection contains multiple aliases.
            if self.code == "B" && !seen.insert((chapter, item)) {
                continue;
            }
            if chapter == 0 {
                return Err(Error::ReferenceNotFound {
                    detail: format!("{}:0 (chapters start at 1)", self.code),
                });
            }
            let chapter = ChapterId::Number(chapter);
            let file = self.chapter(repo, chapter)?;
            let mut record = self.record(&file, chapter, item)?;
            record.extra.insert("number".into(), number.into());
            record.extra.insert("numbering".into(), "book".into());
            out.push(record);
        }

        Ok(())
    }
}
