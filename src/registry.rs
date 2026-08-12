// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Source registry — code to handler, and nothing more.

use crate::sources::{HadithCollection, HisnulMuslim, Quran, Source};

/// Every source QQL knows about.
///
/// Adding a collection is one line here plus one `impl Source`. A linear scan
/// over a dozen entries is not worth a hash map.
pub struct Registry {
    sources: Vec<Box<dyn Source>>,
}

impl Registry {
    /// The built-in sources.
    pub fn with_defaults() -> Self {
        Registry {
            sources: vec![
                Box::new(Quran),
                Box::new(HadithCollection::new("B", "Sahih al-Bukhari", "bukhari")),
                Box::new(HadithCollection::new("M", "Sahih Muslim", "muslim")),
                Box::new(HadithCollection::new("AD", "Sunan Abi Dawud", "abudawud")),
                Box::new(HadithCollection::new("T", "Jami' at-Tirmidhi", "tirmidhi")),
                Box::new(HadithCollection::new("N", "Sunan an-Nasa'i", "nasai")),
                Box::new(HadithCollection::new("IM", "Sunan Ibn Majah", "ibnmajah")),
                Box::new(HisnulMuslim),
            ],
        }
    }

    /// Register an additional source.
    pub fn register(&mut self, source: Box<dyn Source>) {
        self.sources.push(source);
    }

    /// Look up by canonical code or alias. `code` is expected uppercase, as
    /// the parser normalizes it.
    pub fn get(&self, code: &str) -> Option<&dyn Source> {
        self.sources
            .iter()
            .find(|s| s.code() == code || s.aliases().contains(&code))
            .map(|s| s.as_ref())
    }

    /// Every registered code, in registration order.
    pub fn codes(&self) -> Vec<&str> {
        self.sources.iter().map(|s| s.code()).collect()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
