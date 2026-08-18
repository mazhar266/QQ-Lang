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

/// Source assumed when a query omits the code — `1,2:255` means `Q:1,2:255`.
///
/// This lives here rather than in the parser: which code is the default is a
/// fact about the registered sources, not about the grammar.
pub const DEFAULT_CODE: &str = "Q";

impl Registry {
    /// The code used when a reference names no source.
    pub fn default_code(&self) -> &str {
        DEFAULT_CODE
    }

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
    ///
    /// Reusing an existing code is allowed and shadows the earlier entry, so a
    /// user manifest can deliberately replace a built-in source.
    pub fn register(&mut self, source: Box<dyn Source>) {
        self.sources.push(source);
    }

    /// Look up by canonical code or alias. `code` is expected uppercase, as
    /// the parser normalizes it.
    ///
    /// Searched newest-first so later registrations win.
    pub fn get(&self, code: &str) -> Option<&dyn Source> {
        self.sources
            .iter()
            .rev()
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
