// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Full-text matching for `Q:"text"` and friends.
//!
//! Search is source-agnostic: [`Context`](crate::Context) resolves whatever
//! the scope names — a whole collection, one chapter, an ayah range — and this
//! module decides which of those records match. Nothing here knows about
//! Surahs or hadith, so every source gets search for free.
//!
//! # Matching
//!
//! A record matches when the folded needle appears in its folded Arabic or its
//! folded English. Folding lowercases ASCII and, for Arabic, drops the marks
//! that a reader would not type:
//!
//! - harakat and sukun (`U+064B..U+0652`), the superscript alef (`U+0670`),
//!   and the Quranic annotation marks (`U+06D6..U+06ED`);
//! - tatweel (`U+0640`);
//! - the hamza and madda seats on alef, so `أ`, `إ`, `آ`, `ٱ` all fold to `ا`;
//! - `ى` to `ي` and `ة` to `ه`, which are written interchangeably.
//!
//! Without that, searching the Quran would be nearly useless: the text is
//! fully diacritized, so a typed `الحمد` shares no substring with the stored
//! `ٱلْحَمْدُ`.
//!
//! Folding happens only for comparison. Records are returned with their text
//! exactly as stored — this module never rewrites scripture.

/// Fold a string for comparison. See the module docs for what is dropped.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for ch in text.chars() {
        match ch {
            // Marks a reader would not type.
            '\u{064B}'..='\u{0652}' | '\u{0670}' | '\u{06D6}'..='\u{06ED}' | '\u{0640}' => {}
            // Alef, however it is seated.
            '\u{0622}' | '\u{0623}' | '\u{0625}' | '\u{0671}' => out.push('\u{0627}'),
            // Alef maqsura is written for ya, ta marbuta for ha.
            '\u{0649}' => out.push('\u{064A}'),
            '\u{0629}' => out.push('\u{0647}'),
            _ => {
                for lower in ch.to_lowercase() {
                    out.push(lower);
                }
            }
        }
    }

    out
}

/// Whether `haystack` contains `needle`, both folded.
pub fn matches(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && fold(haystack).contains(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_drops_the_marks_a_reader_would_not_type() {
        assert_eq!(fold("ٱلْحَمْدُ"), fold("الحمد"));
        assert_eq!(fold("بِسْمِ"), fold("بسم"));
        // Alef seats collapse together.
        assert_eq!(fold("أحمد"), fold("احمد"));
        assert_eq!(fold("إن"), fold("ان"));
        assert_eq!(fold("آمن"), fold("امن"));
    }

    #[test]
    fn folding_lowercases_ascii() {
        assert_eq!(fold("Allah"), "allah");
        assert_eq!(fold("MERCIFUL"), "merciful");
    }

    #[test]
    fn matching_needs_a_pre_folded_needle() {
        assert!(matches("ٱلْحَمْدُ لِلَّهِ", &fold("الحمد")));
        assert!(matches("In the name of Allah", &fold("ALLAH")));
        assert!(!matches("In the name of Allah", &fold("Bukhari")));
        assert!(!matches("anything", &fold("")));
    }

    #[test]
    fn folding_leaves_ordinary_text_alone() {
        assert_eq!(fold("mercy"), "mercy");
        assert_eq!(fold("123"), "123");
    }
}
