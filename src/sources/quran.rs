// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Quran resolver.
//!
//! Reads `quran-json-arabic/dist/chapters/en/{surah}.json` — one file per
//! Surah, so `Q:2:255` loads roughly 50 KB rather than the whole mushaf.

use crate::ast::Reference;
use crate::error::Error;
use crate::record::Record;
use crate::repo::Repository;
use crate::sources::Source;
use serde::Deserialize;

const SURAH_COUNT: u32 = 114;
const DIR: &str = "quran-json-arabic/dist/chapters/en";

/// `Q` — the Quran.
#[derive(Debug, Default)]
pub struct Quran;

#[derive(Debug, Deserialize)]
struct Chapter {
    name: String,
    transliteration: String,
    verses: Vec<Verse>,
}

#[derive(Debug, Deserialize)]
struct Verse {
    id: u32,
    text: String,
    translation: String,
}

impl Source for Quran {
    fn code(&self) -> &str {
        "Q"
    }

    fn name(&self) -> &str {
        "Quran"
    }

    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        // The parser cheerfully accepts Q:0 and Q:115; rejecting them is this
        // module's job, not its.
        if reference.primary == 0 || reference.primary > SURAH_COUNT {
            return Err(Error::ReferenceNotFound {
                detail: format!("Surah {} (Quran has 1..=114)", reference.primary),
            });
        }

        let surah = reference.primary;
        let chapter: std::sync::Arc<Chapter> = repo.load(format!("{DIR}/{surah}.json"))?;

        let total = u32::try_from(chapter.verses.len()).map_err(|_| Error::InvalidDataFile {
            path: format!("{DIR}/{surah}.json"),
            detail: "implausible verse count".into(),
        })?;

        for ayah in reference.expand(total)? {
            // Files are ordered and complete, but index by `id` rather than
            // trusting position.
            let verse = chapter
                .verses
                .iter()
                .find(|v| v.id == ayah)
                .ok_or_else(|| Error::ReferenceNotFound {
                    detail: format!("Q:{surah}:{ayah}"),
                })?;

            out.push(Record {
                source: self.code().to_string(),
                collection: self.name().to_string(),
                extra: [
                    ("surah".to_string(), surah.into()),
                    ("surah_name_ar".to_string(), chapter.name.clone().into()),
                    (
                        "surah_name_en".to_string(),
                        chapter.transliteration.clone().into(),
                    ),
                    ("ayah".to_string(), ayah.into()),
                ]
                .into_iter()
                .collect(),
                ar: verse.text.clone(),
                en: verse.translation.clone(),
            });
        }

        Ok(())
    }
}
