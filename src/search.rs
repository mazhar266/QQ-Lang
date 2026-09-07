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
//! A record matches when the folded needle appears in any of its folded text
//! fields — Arabic, the simplified `emlaei` spelling where a source has one,
//! or English. Folding lowercases ASCII and, for Arabic, drops the marks that
//! a reader would not type:
//!
//! - harakat and sukun (`U+064B..U+0652`), the superscript alef (`U+0670`),
//!   and the Quranic annotation marks (`U+06D6..U+06ED`);
//! - tatweel (`U+0640`);
//! - the invisible joiners and bidi marks (`U+200C..U+200F`, `U+061C`), which
//!   nobody types and which five ayat of the Emlaei text carry mid-sentence;
//! - the apostrophes transliteration uses for hamza and ayn — `'`, `‘`, `’`,
//!   backtick, `ʼ`, `ʻ`, `ʾ`, `ʿ`. Every tokenizer here splits on
//!   non-alphanumerics, so leaving them in cut `Qur'an` into `qur` + `an` and
//!   a search for `quran` matched neither. The corpus is not even consistent
//!   with itself: it writes `qur'an` 199 times and `qur’an` 50, `rak'ahs`
//!   beside `rak’ahs`, and `` `Asr `` with a backtick. Dropping the class
//!   collapses all of them onto one token;
//! - the hamza and madda seats on alef, so `أ`, `إ`, `آ`, `ٱ` all fold to `ا`;
//! - `ى` to `ي` and `ة` to `ه`, which are written interchangeably.
//!
//! Without that, searching the Quran would be nearly useless: the text is
//! fully diacritized, so a typed `الحمد` shares no substring with the stored
//! `ٱلْحَمْدُ`.
//!
//! Folding goes only so far, which is why `Record::emlaei` exists. Uthmani and
//! modern spelling differ in the letters themselves — `ٱلصَّلَوٰةَ` keeps a waw
//! where `الصلاة` has an alef, and no amount of mark-dropping bridges that.
//! Matching the second spelling as well is what makes `"السماوات"` find the
//! 185 ayat that store it as `ٱلسَّمَٰوَٰتِ`.
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
            // Invisible formatting: joiners and bidi controls.
            '\u{200C}'..='\u{200F}' | '\u{061C}' => {}
            // Apostrophes: hamza and ayn in transliterated names.
            '\'' | '`' | '\u{2018}' | '\u{2019}' | '\u{02BB}' | '\u{02BC}' | '\u{02BE}'
            | '\u{02BF}' => {}
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

/// Other spellings of a folded word — transliteration variants, and the
/// English names the translations use for prophets.
///
/// Deliberately a short, curated list rather than a general scheme. The misses
/// worth fixing here are a closed set: this corpus spells one name several
/// ways, and a reader knows only one of them. Anything broader would start
/// guessing at meaning, which is what the ranked engines are for.
///
/// Every entry is written folded, so `qur'an` does not appear — it already
/// folds to `quran`. Words that risk colliding with ordinary English are left
/// out on purpose: `lut`/`lot` and `ayyub`/`job` would both match common
/// prose.
const ALIASES: &[&[&str]] = &[
    // Transliteration variants.
    &["quran", "koran", "qoran"],
    &["muhammad", "mohammed", "mohammad", "muhammed"],
    &["salah", "salat", "salaat", "salaah"],
    &["zakah", "zakat", "zakaat"],
    &["kabah", "kaaba", "kaba"],
    &["ramadan", "ramadhan", "ramzan"],
    &["hadith", "hadeeth"],
    &["sunnah", "sunna"],
    &["wudu", "wudhu", "ablution"],
    &["hajj", "haj", "pilgrimage"],
    &["masjid", "mosque"],
    &["jannah", "paradise"],
    &["jahannam", "hellfire"],
    &["shaytan", "satan", "shaitan"],
    &["jibril", "gabriel", "jibreel"],
    // Companions and wives, whose names the translations transliterate
    // inconsistently.
    &["aishah", "aisha", "ayesha"],
    &["umar", "omar"],
    &["uthman", "usman", "othman"],
    &["abdullah", "abdallah"],
    &["khadijah", "khadija"],
    // Prophets: the Arabic name a reader types against the English name the
    // translation prints.
    &["ibrahim", "abraham"],
    &["musa", "moses"],
    &["isa", "jesus"],
    &["maryam", "mary"],
    &["yusuf", "joseph"],
    &["dawud", "david"],
    &["sulaiman", "solomon", "suleiman"],
    &["nuh", "noah"],
    &["harun", "aaron"],
    &["yahya", "john"],
    &["ismail", "ishmael"],
    &["yaqub", "jacob"],
    &["ishaq", "isaac"],
    &["yunus", "jonah"],
    &["zakariya", "zechariah"],
];

/// English words that carry no signal in a query.
///
/// Read from `stopwords.txt` so the vector build script can use the same list
/// — it gives these zero weight when embedding, and a list that drifted
/// between the two sides would silently change every score.
///
/// Used only to drop terms from a *query* and to zero them when *embedding*,
/// never to strip the full-text index: phrases still need them in place, so
/// `?'"the straight path"'` keeps matching.
const STOPWORDS: &str = include_str!("stopwords.txt");

/// Other spellings of `word`, including `word` itself. Empty when there are
/// none, which is the common case.
///
/// A linear scan: the table is a few dozen entries and is checked once per
/// query word.
pub fn aliases(word: &str) -> &'static [&'static str] {
    ALIASES
        .iter()
        .find(|group| group.contains(&word))
        .copied()
        .unwrap_or(&[])
}

/// Whether a word adds nothing to a query.
pub fn is_stopword(word: &str) -> bool {
    STOPWORDS
        .lines()
        .any(|line| !line.starts_with('#') && line.trim() == word)
}

/// Whether a term is a plain bag of words, and so safe to rewrite.
///
/// The full-text term carries its own boolean and phrase syntax straight
/// through to tantivy. Dropping a stopword out of `"the straight path"` or
/// expanding a word inside `title:x` would change what the user asked for, so
/// anything carrying syntax is passed along untouched.
pub fn is_plain(term: &str) -> bool {
    !term.chars().any(|c| "\"()[]{}:+-^*?~\\/".contains(c))
        && !term
            .split_whitespace()
            .any(|w| matches!(w, "AND" | "OR" | "NOT" | "TO" | "IN"))
}

/// Drop stopwords from a plain term, keeping the original if nothing is left.
///
/// `"quran is easy"` becomes `"quran easy"`. A query that is *only* stopwords
/// is left alone — the user asked for something, and an empty query would
/// answer with everything.
pub fn without_stopwords(term: &str) -> String {
    let kept: Vec<&str> = term
        .split_whitespace()
        .filter(|w| !is_stopword(w))
        .collect();
    if kept.is_empty() {
        term.to_string()
    } else {
        kept.join(" ")
    }
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
    fn folding_collapses_every_apostrophe_the_corpus_uses() {
        // The corpus writes this name at least four ways.
        for spelling in ["Qur'an", "Qur’an", "Qur`an", "Qurʼan", "Quran"] {
            assert_eq!(fold(spelling), "quran", "{spelling}");
        }
        assert_eq!(fold("Allah's"), "allahs");
        assert_eq!(fold("`Asr"), "asr");
        assert_eq!(fold("rak’ahs"), fold("rak'ahs"));
    }

    #[test]
    fn aliases_are_symmetric_and_rare() {
        assert!(aliases("quran").contains(&"koran"));
        assert!(aliases("koran").contains(&"quran"));
        assert!(aliases("abraham").contains(&"ibrahim"));
        // Ordinary words have none, which is the common path.
        assert!(aliases("mercy").is_empty());
        assert!(aliases("lot").is_empty(), "too collision-prone to alias");
    }

    #[test]
    fn only_plain_terms_are_rewritten() {
        assert!(is_plain("quran is easy"));
        assert!(!is_plain("\"straight path\""));
        assert!(!is_plain("mercy OR forgiveness"));
        assert!(!is_plain("prayer -charity"));

        assert_eq!(without_stopwords("quran is easy"), "quran easy");
        // Nothing but stopwords: leave it be rather than match everything.
        assert_eq!(without_stopwords("the is of"), "the is of");
    }

    #[test]
    fn folding_leaves_ordinary_text_alone() {
        assert_eq!(fold("mercy"), "mercy");
        assert_eq!(fold("123"), "123");
    }
}
