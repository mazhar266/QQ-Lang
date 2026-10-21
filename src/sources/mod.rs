// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Source handlers: the only place that knows Islamic-book structure.

mod bible;
mod hadith;
mod hisnul;
mod json;
mod psalms;
mod quran;

pub use bible::Bible;
pub use hadith::HadithCollection;
pub use hisnul::HisnulMuslim;
pub use json::{JsonSource, SourceSpec};
pub use psalms::Psalms;
pub use quran::Quran;

use crate::ast::{Range, Reference};
use crate::error::Error;
use crate::record::Record;
use crate::repo::Repository;

/// How a source's records are keyed in its ranked-search indexes.
///
/// Every hit resolves back through the ordinary reference path, so the index
/// only has to agree with the resolver about numbering — this says which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKeys {
    /// `(primary, number)`, addressed as `CODE:primary:number`, in an index of
    /// the source's own. Every two-level source.
    Primary,
    /// The book-wide number. Each key is `(0, N)`, addressed as `index::N`.
    ///
    /// A multi-book scope — the Torah, a testament — cannot be one primary,
    /// but it is always one run of the book-wide number. A source may share
    /// another's index as a window onto it: the Old and New Testaments read
    /// the Bible's, `OT::N` being `BI::N` and `NT::N` being
    /// `BI::(N + 23145)` — see [`Source::flat_offset`] — so both rank exactly
    /// as the whole Bible does.
    Flat {
        /// Code of the source whose index this is.
        index: &'static str,
    },
}

/// A collection QQL can resolve references against.
///
/// Implementing this plus one registry entry is the whole cost of adding a
/// collection. If a change here forces an edit to the lexer or parser, the
/// design has been violated.
pub trait Source: Send + Sync {
    /// Canonical code, uppercase, e.g. `"Q"`.
    fn code(&self) -> &str;

    /// Display name, e.g. `"Quran"`.
    fn name(&self) -> &str;

    /// Alternate codes that also select this source.
    ///
    /// Returns owned references rather than a `&'static` slice so that
    /// sources configured at runtime — see [`JsonSource`] — can carry aliases
    /// too. Built-in sources return an empty `Vec`, which does not allocate.
    fn aliases(&self) -> Vec<&str> {
        Vec::new()
    }

    /// How many items the collection holds end to end, if it can say.
    ///
    /// This is the count the `SOURCE::N` axis runs over, and it is what lets
    /// `SOURCE:"text"` search everything without the caller naming a range.
    /// `None` means the source has no such axis, and a whole-collection search
    /// is refused rather than silently narrowed.
    fn total(&self, _repo: &mut Repository) -> Result<Option<u32>, Error> {
        Ok(None)
    }

    /// Whether names may follow this source's code — the book in `OT:GEN:1:1`.
    ///
    /// `false` for every source but the Bible's. [`Context`](crate::Context)
    /// rejects a path on such a source before anything resolves, with the
    /// very error the parser gave before paths existed, so a source that takes
    /// no names never sees one.
    fn has_books(&self) -> bool {
        false
    }

    /// How the ranked-search indexes address this source's records.
    fn index_keys(&self) -> IndexKeys {
        IndexKeys::Primary
    }

    /// For an [`IndexKeys::Flat`] source: where its `::1` sits in the index,
    /// less one. `CODE::N` is the index's `::(offset + N)`.
    fn flat_offset(&self, _repo: &mut Repository) -> Result<u32, Error> {
        Ok(0)
    }

    /// For an [`IndexKeys::Flat`] source: the scope of `reference` as ranges
    /// of this source's own book-wide numbers, `CODE::N`. Empty means the whole
    /// source.
    ///
    /// Only called for a search. `reference.search` is set; `path`, `primary`
    /// and `ranges` say where to look.
    fn flat_scope(
        &self,
        _repo: &mut Repository,
        _reference: &Reference,
    ) -> Result<Vec<Range>, Error> {
        Ok(Vec::new())
    }

    /// Validate, load, and append records — in the order the query asked for.
    ///
    /// Semantic validation lives here rather than in a separate `validate`
    /// method: every check a dry run would do is the first thing `resolve`
    /// does anyway, and nothing in the crate needs validation without
    /// resolution.
    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error>;
}

/// Sum a verse-count table. `const` so a source's book-wide total is derived
/// from its table and cannot drift from it.
const fn total(counts: &[u32]) -> u32 {
    let mut total = 0;
    let mut index = 0;
    while index < counts.len() {
        total += counts[index];
        index += 1;
    }
    total
}

/// Split a book-wide item number into `(primary, item)` against a table of
/// item counts per primary — `Q::100` into Surah 2, ayah 93.
fn locate(counts: &[u32], global: u32) -> Option<(u32, u32)> {
    if global == 0 {
        return None;
    }
    let mut remaining = global;
    for (index, &count) in counts.iter().enumerate() {
        if remaining <= count {
            let primary = u32::try_from(index).ok()?.checked_add(1)?;
            return Some((primary, remaining));
        }
        remaining -= count;
    }
    None
}
