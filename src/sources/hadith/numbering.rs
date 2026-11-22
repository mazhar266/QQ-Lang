// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

use super::{ChapterId, HadithCollection};
use crate::{Error, NumberLabel, Numbering, Record, Reference, Repository};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

type Address = (ChapterId, u32);

#[derive(Deserialize)]
#[serde(try_from = "Vec<(ChapterId, Vec<String>)>")]
struct NumberMap {
    book: Vec<Address>,
    labels: BTreeMap<NumberLabel, Vec<usize>>,
}

impl TryFrom<Vec<(ChapterId, Vec<String>)>> for NumberMap {
    type Error = String;

    fn try_from(chapters: Vec<(ChapterId, Vec<String>)>) -> Result<Self, Self::Error> {
        let mut book = Vec::new();
        let mut labels = BTreeMap::<_, Vec<_>>::new();
        for (chapter, entries) in chapters {
            for (item, entry) in entries.into_iter().enumerate() {
                let item = u32::try_from(item + 1).map_err(|e| e.to_string())?;
                let index = book.len();
                book.push((chapter, item));
                for label in entry.split(',').filter(|s| !s.is_empty()) {
                    let digits = label.bytes().take_while(u8::is_ascii_digit).count();
                    let (base, suffix) = label.split_at(digits);
                    let number = base
                        .parse::<u32>()
                        .map_err(|_| format!("invalid label {label}"))?;
                    if number == 0 || !suffix.bytes().all(|b| b.is_ascii_lowercase()) {
                        return Err(format!("invalid label {label}"));
                    }
                    let key = NumberLabel {
                        number,
                        suffix: suffix.to_owned(),
                    };
                    labels.entry(key).or_default().push(index);
                }
            }
        }
        Ok(Self { book, labels })
    }
}

impl HadithCollection {
    pub(super) fn resolve_edition(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        numbering: Numbering,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let map: Arc<NumberMap> = repo.load(format!("numbering/{}.json", self.code))?;
        let mut append = |(chapter, item): Address, number, label: Option<&NumberLabel>| {
            let file = self.chapter(repo, chapter)?;
            let mut record = self.record(&file, chapter, item)?;
            record.extra.insert("number".into(), number);
            record
                .extra
                .insert("numbering".into(), numbering.name().into());
            if let Some(label) = label {
                record
                    .extra
                    .insert("sunnah_number".into(), label.to_string().into());
            }
            out.push(record);
            Ok(())
        };
        if numbering == Numbering::Book {
            let max = u32::try_from(map.book.len()).map_err(|e| Error::InvalidDataFile {
                path: format!("numbering/{}.json", self.code),
                detail: e.to_string(),
            })?;
            for number in reference.expand(max)? {
                append(map.book[(number - 1) as usize], number.into(), None)?;
            }
            return Ok(());
        }

        for (label, index) in map.select_labels(reference, numbering)? {
            let number = if numbering == Numbering::Sunnah && !label.suffix.is_empty() {
                label.to_string().into()
            } else {
                label.number.into()
            };
            append(map.book[index], number, Some(label))?;
        }
        Ok(())
    }
}

impl NumberMap {
    fn select_labels(
        &self,
        reference: &Reference,
        numbering: Numbering,
    ) -> Result<Vec<(&NumberLabel, usize)>, Error> {
        let ranges: Vec<_> = if reference.lettered_ranges.is_empty() {
            reference
                .ranges
                .iter()
                .map(|range| (NumberLabel::plain(range.from), NumberLabel::plain(range.to)))
                .collect()
        } else {
            reference.lettered_ranges.clone()
        };
        let max = self.labels.last_key_value().map_or(0, |(k, _)| k.number);
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for (from, to) in ranges {
            let missing = || Error::ReferenceNotFound {
                detail: format!(
                    "{}:{from}-{to} ({} numbering)",
                    reference.label(),
                    numbering.name()
                ),
            };
            if from.number == 0 || to.number > max || to < from {
                return Err(missing());
            }
            let start = result.len();
            let matches = self.labels.range(&from..).take_while(|(key, _)| {
                if numbering == Numbering::Traditional {
                    key.number <= to.number
                } else {
                    *key <= &to
                }
            });
            let mut matched = false;
            for (key, addresses) in matches {
                matched = true;
                for &address in addresses {
                    if seen.insert(address) {
                        result.push((key, address));
                    }
                }
            }
            if !matched && from == to {
                return Err(missing());
            }
            if numbering == Numbering::Traditional {
                // Some groups end with an unsuffixed narration (Muslim 162).
                result[start..].sort_by_key(|(label, address)| (label.number, *address));
            }
        }
        Ok(result)
    }
}
