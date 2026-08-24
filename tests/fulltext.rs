// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Ranked full-text search, with and without the `fulltext` feature.
//!
//! With the feature, these need the indexes:
//!
//! ```bash
//! cargo run --features fulltext --bin qql-index
//! cargo test --features fulltext --test fulltext
//! ```

use qql::Context;

const DATA: &str = "sources";

fn context() -> Option<Context> {
    std::path::Path::new(DATA)
        .join("quran/chapters/1.json")
        .exists()
        .then(|| Context::new(DATA))
}

/// The grammar is the same whether or not the engine is compiled in.
#[test]
fn the_marked_form_always_parses() {
    let query = qql::parse(r#"q:1:?"mercy""#).unwrap();
    let search = query.references[0].search.as_ref().unwrap();
    assert_eq!(search.term, "mercy");
    assert_eq!(search.kind, qql::MatchKind::FullText);

    // The marker is part of the term, not a separate reference.
    assert_eq!(query.references.len(), 1);

    // `~N` caps it, as with similarity.
    let query = qql::parse(r#"q:?"mercy"~5"#).unwrap();
    assert_eq!(query.references[0].search.as_ref().unwrap().limit, Some(5));

    // Either quote, and the other survives inside — which is how a phrase
    // query is written.
    assert_eq!(
        qql::parse(r#"q:?'"straight path"'"#).unwrap().references[0]
            .search
            .as_ref()
            .unwrap()
            .term,
        r#""straight path""#
    );

    // Either quote works after the marker, identically.
    assert_eq!(
        qql::parse(r#"q:1:?"mercy""#).unwrap(),
        qql::parse("q:1:?'mercy'").unwrap()
    );

    // A bare `?` is not a term.
    assert!(qql::parse("q:1:?").is_err());
}

#[cfg(not(feature = "fulltext"))]
mod without_the_feature {
    use super::*;

    #[test]
    fn full_text_is_refused_rather_than_downgraded() {
        let Some(mut ctx) = context() else { return };

        let error = ctx.execute(r#"q:1:?"mercy""#).unwrap_err();
        assert_eq!(error.code(), "QQL_UNSUPPORTED");
        assert!(
            error.to_string().contains("fulltext"),
            "the message should name the feature: {error}"
        );

        // Substring search is untouched by the feature being off.
        assert!(ctx.execute(r#"q:1:"Allah""#).is_ok());
    }
}

#[cfg(feature = "fulltext")]
mod with_the_feature {
    use super::*;

    fn ready() -> Option<Context> {
        let ctx = context()?;
        std::path::Path::new(DATA)
            .join("fulltext/Q/meta.json")
            .exists()
            .then_some(ctx)
    }

    macro_rules! ctx {
        () => {
            match ready() {
                Some(ctx) => ctx,
                None => {
                    eprintln!("skipping: run `cargo run --features fulltext --bin qql-index`");
                    return;
                }
            }
        };
    }

    /// The reason this engine exists: an inverted index with a stemmer finds
    /// word forms a substring scan cannot.
    #[test]
    fn stemming_finds_what_substring_search_misses() {
        let mut ctx = ctx!();

        // "Merciful" does not contain "mercy".
        assert!(ctx.execute(r#"q:1:"mercy""#).unwrap().is_empty());

        let hits = ctx.execute(r#"q:1:?"mercy""#).unwrap();
        assert!(!hits.is_empty(), "stemming should reach 'Merciful'");
        assert!(hits.iter().all(|r| r.extra["surah"] == 1));
    }

    #[test]
    fn results_are_ranked_and_scored() {
        let mut ctx = ctx!();
        let hits = ctx.execute(r#"q:?"mercy"~5"#).unwrap();

        assert!(hits.len() <= 5);
        assert!(hits.iter().all(|r| r.extra["ranked"] == true));

        let scores: Vec<f64> = hits
            .iter()
            .map(|r| r.extra["score"].as_f64().unwrap())
            .collect();
        assert!(
            scores.windows(2).all(|w| w[0] >= w[1]),
            "not descending: {scores:?}"
        );
    }

    #[test]
    fn the_scope_narrows_the_search() {
        let mut ctx = ctx!();

        let surah = ctx.execute(r#"q:1:?"praise""#).unwrap();
        assert!(surah.iter().all(|r| r.extra["surah"] == 1));

        let ranged = ctx.execute(r#"q:1:3~5:?"help""#).unwrap();
        assert!(!ranged.is_empty());
        assert!(ranged.iter().all(|r| {
            let ayah = r.extra["ayah"].as_u64().unwrap();
            (3..=5).contains(&ayah)
        }));
    }

    /// Arabic is indexed folded, so an undiacritized query reaches the
    /// diacritized text.
    #[test]
    fn arabic_matches_without_diacritics() {
        let mut ctx = ctx!();
        let hits = ctx.execute(r#"q:1:?"الحمد""#).unwrap();
        assert_eq!(hits[0].extra["ayah"], 2, "got {hits:?}");
    }

    /// The term carries the engine's own query syntax.
    #[test]
    fn boolean_and_phrase_syntax_reach_the_engine() {
        let mut ctx = ctx!();

        assert!(!ctx.execute(r#"q:?"mercy OR forgiveness"~3"#).unwrap().is_empty());
        assert!(!ctx.execute(r#"q:?"prayer AND charity"~3"#).unwrap().is_empty());

        // A phrase needs the inner quotes, so the term takes the other quote.
        let phrase = ctx.execute(r#"q:?'"straight path"'~3"#).unwrap();
        assert!(!phrase.is_empty());
        assert!(phrase
            .iter()
            .any(|r| r.en.to_lowercase().contains("straight path")));
    }

    #[test]
    fn hits_are_ordinary_records() {
        let mut ctx = ctx!();
        let hit = &ctx.execute(r#"q:1:?"help""#).unwrap()[0];
        let direct = &ctx.execute("q:1:5").unwrap()[0];

        assert_eq!(hit.ar, direct.ar);
        assert_eq!(hit.en, direct.en);
        assert_eq!(hit.extra["surah_name_en"], direct.extra["surah_name_en"]);
    }

    #[test]
    fn every_source_can_be_searched() {
        let mut ctx = ctx!();

        for query in [
            r#"b:?"intention"~2"#,
            r#"m:?"ablution"~2"#,
            r#"hm:?"morning"~2"#,
        ] {
            match ctx.execute(query) {
                Ok(hits) => assert!(!hits.is_empty(), "{query} found nothing"),
                // A source whose index has not been built says so clearly.
                Err(e) => assert_eq!(e.code(), "QQL_UNSUPPORTED", "{query}: {e}"),
            }
        }
    }

    #[test]
    fn a_malformed_term_is_a_query_error_not_a_crash() {
        let mut ctx = ctx!();
        // An unbalanced phrase quote inside the term.
        let error = ctx.execute(r#"q:?'"unbalanced'"#).unwrap_err();
        assert_eq!(error.code(), "QQL_UNSUPPORTED");
    }

    #[test]
    fn full_text_mixes_with_other_forms() {
        let mut ctx = ctx!();
        let records = ctx.execute(r#"q:1:1;q:1:?"help"~1"#).unwrap();

        assert_eq!(records.len(), 2);
        assert!(records[0].extra.get("ranked").is_none());
        assert_eq!(records[1].extra["ranked"], true);
    }
}
