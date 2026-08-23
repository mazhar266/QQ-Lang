// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! The parsed query.
//!
//! These are plain structs. The AST is never built from JSON — `Serialize` is
//! derived only so `qql --parse` can print it.

use crate::error::Error;
use serde::Serialize;
use std::collections::HashSet;

/// An inclusive range of item numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Range {
    /// First item, inclusive.
    pub from: u32,
    /// Last item, inclusive.
    pub to: u32,
}

/// One `SOURCE:primary[:selector]` reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reference {
    /// Source code, normalized to uppercase.
    ///
    /// `None` when the query omitted it — `1,2:255` rather than `Q:1,2:255`.
    /// Which source that means is the registry's decision, not the parser's;
    /// [`Context`](crate::Context) substitutes the default before resolving,
    /// so source handlers always see a concrete code.
    pub source: Option<String>,
    /// First-level index — Surah for Quran, chapter for Hadith.
    ///
    /// `None` is the flat form `B::100`, where the primary is skipped and the
    /// selector counts across the whole collection instead. The parser only
    /// records that it was omitted; what that means is the resolver's business.
    pub primary: Option<u32>,
    /// Selected items. Empty means "everything in `primary`".
    ///
    /// For a search this is the `3~5` scope rather than a selection.
    pub ranges: Vec<Range>,
    /// Search term, for `Q:1:"text"`. `None` is an ordinary reference.
    ///
    /// When set, `primary` and `ranges` narrow *where* to search rather than
    /// what to return: `None`/empty mean the whole collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// A whole query: one or more references, in the order written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Query {
    /// References, never reordered.
    pub references: Vec<Reference>,
}

impl Reference {
    /// Whether this reference selects everything under `primary`.
    pub fn selects_all(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Whether the primary was skipped — the `B::100` form.
    ///
    /// A search with no Surah scope also leaves `primary` empty, but it is not
    /// book-wide numbering, so it does not count.
    pub fn is_flat(&self) -> bool {
        self.primary.is_none() && self.text.is_none()
    }

    /// Whether this is a search rather than a reference.
    pub fn is_search(&self) -> bool {
        self.text.is_some()
    }

    /// `"B:1"`, or `"B:"` when the primary is skipped. For error messages.
    pub fn label(&self) -> String {
        let source = self.source.as_deref().unwrap_or_default();
        match self.primary {
            Some(primary) => format!("{source}:{primary}"),
            None => format!("{source}:"),
        }
    }

    /// Expand the selector into concrete item numbers.
    ///
    /// Bounds-checks against `max` (the number of items that actually exist),
    /// which is what keeps `Q:1:1-4294967295` from trying to allocate four
    /// billion entries. Order is the order written; duplicates are dropped
    /// within this reference only.
    ///
    /// An empty selector yields `1..=max`.
    pub fn expand(&self, max: u32) -> Result<Vec<u32>, Error> {
        if self.selects_all() {
            return Ok((1..=max).collect());
        }

        let mut items = Vec::new();
        let mut seen = HashSet::new();
        for range in &self.ranges {
            if range.from == 0 || range.to > max {
                return Err(Error::ReferenceNotFound {
                    detail: format!(
                        "{}:{}-{} is outside 1..={max}",
                        self.label(),
                        range.from,
                        range.to
                    ),
                });
            }
            for item in range.from..=range.to {
                if seen.insert(item) {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(ranges: &[(u32, u32)]) -> Reference {
        Reference {
            source: Some("Q".into()),
            primary: Some(2),
            text: None,
            ranges: ranges
                .iter()
                .map(|&(from, to)| Range { from, to })
                .collect(),
        }
    }

    #[test]
    fn expansion_preserves_written_order() {
        let items = reference(&[(255, 255), (1, 3)]).expand(286).unwrap();
        assert_eq!(items, [255, 1, 2, 3]);
    }

    #[test]
    fn duplicates_are_dropped_within_one_reference() {
        let items = reference(&[(1, 5), (3, 3), (4, 4)]).expand(286).unwrap();
        assert_eq!(items, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn empty_selector_expands_to_everything() {
        assert_eq!(reference(&[]).expand(7).unwrap(), [1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn out_of_bounds_is_rejected_before_allocating() {
        assert!(reference(&[(1, u32::MAX)]).expand(286).is_err());
        assert!(reference(&[(0, 1)]).expand(286).is_err());
    }
}
