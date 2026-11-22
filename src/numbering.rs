// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

use serde::Serialize;
use std::{cmp::Ordering, fmt};

/// Numbering scheme for explicit book-wide hadith references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Numbering {
    /// A printed reference number, including all its narrations.
    Traditional,
    /// The original whole-book edition’s sequential `idInBook`.
    Book,
    /// Exact Sunnah.com label, including its letter suffix when present.
    Sunnah,
}

impl Numbering {
    /// The CLI spelling of the scheme.
    pub fn name(self) -> &'static str {
        match self {
            Self::Traditional => "traditional",
            Self::Book => "book",
            Self::Sunnah => "sunnah",
        }
    }
}

/// A numeric reference with an optional letter suffix, such as `162a`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NumberLabel {
    /// The numeric part.
    pub number: u32,
    /// Lowercase suffix. Empty for an unsuffixed label.
    pub suffix: String,
}

impl NumberLabel {
    pub(crate) fn plain(number: u32) -> Self {
        Self {
            number,
            suffix: String::new(),
        }
    }
}

impl Ord for NumberLabel {
    fn cmp(&self, other: &Self) -> Ordering {
        // Printed subdivisions run a..z, aa..az.
        (self.number, self.suffix.len(), &self.suffix).cmp(&(
            other.number,
            other.suffix.len(),
            &other.suffix,
        ))
    }
}

impl PartialOrd for NumberLabel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for NumberLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.number, self.suffix)
    }
}
