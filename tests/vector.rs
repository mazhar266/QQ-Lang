// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Vector similarity search, both with and without the `vector` feature.
//!
//! With the feature, these need `sources/vectors/Q.qv`:
//!
//! ```bash
//! python3 scripts/build-vectors.py --source Q
//! cargo test --features vector --test vector
//! ```

use qql::Context;

const DATA: &str = "sources";

fn context() -> Option<Context> {
    std::path::Path::new(DATA)
        .join("quran/chapters/1.json")
        .exists()
        .then(|| Context::new(DATA))
}

/// The `*"…"` form parses whether or not the feature is compiled in — the
/// grammar is the same either way, only the execution differs.
#[test]
fn the_similarity_form_always_parses() {
    let query = qql::parse(r#"q:1:*"mercy""#).unwrap();
    let search = query.references[0].search.as_ref().unwrap();
    assert_eq!(search.term, "mercy");
    assert_eq!(search.kind, qql::MatchKind::Similar);
    assert_eq!(search.limit, None);

    let query = qql::parse(r#"q:*"mercy"~5"#).unwrap();
    assert_eq!(query.references[0].search.as_ref().unwrap().limit, Some(5));

    // Either quote works after the marker, identically.
    assert_eq!(
        qql::parse(r#"q:1:*"mercy""#).unwrap(),
        qql::parse("q:1:*'mercy'").unwrap()
    );

    // `~N` ranks, so it is meaningless on an exact term and rejected there.
    assert_eq!(
        qql::parse(r#"q:"mercy"~5"#).unwrap_err().code(),
        "QQL_INVALID_CHARACTER"
    );
}

#[cfg(not(feature = "vector"))]
mod without_the_feature {
    use super::*;

    /// Refused, not quietly downgraded to substring matching — answering a
    /// different question than the one asked is worse than saying no.
    #[test]
    fn similarity_is_refused_with_a_message_that_says_why() {
        let Some(mut ctx) = context() else { return };

        let error = ctx.execute(r#"q:1:*"mercy""#).unwrap_err();
        assert_eq!(error.code(), "QQL_UNSUPPORTED");
        assert!(
            error.to_string().contains("vector"),
            "unhelpful message: {error}"
        );

        // The exact form is unaffected.
        assert!(ctx.execute(r#"q:1:"الحمد""#).is_ok());
    }
}

#[cfg(feature = "vector")]
mod with_the_feature {
    use super::*;

    fn ready() -> Option<Context> {
        let ctx = context()?;
        std::path::Path::new(DATA)
            .join("vectors/Q.qv")
            .exists()
            .then_some(ctx)
    }

    macro_rules! ctx {
        () => {
            match ready() {
                Some(ctx) => ctx,
                None => {
                    eprintln!("skipping: run scripts/build-vectors.py --source Q");
                    return;
                }
            }
        };
    }

    #[test]
    fn similarity_ranks_the_best_match_first() {
        let mut ctx = ctx!();

        // Al-Kafirun is where worship is discussed most densely.
        let hits = ctx.execute(r#"q:*"worship"~3"#).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].extra["surah"], 109);

        // Every hit is marked as ranked and carries its score.
        assert!(hits.iter().all(|r| r.extra["ranked"] == true));
        assert!(hits.iter().all(|r| r.extra["score"].is_number()));
    }

    /// The one place in QQL where output is ordered by relevance rather than
    /// position. Everything else preserves the order written.
    #[test]
    fn results_are_ordered_by_score_descending() {
        let mut ctx = ctx!();
        let scores: Vec<f64> = ctx
            .execute(r#"q:*"mercy"~10"#)
            .unwrap()
            .iter()
            .map(|r| r.extra["score"].as_f64().unwrap())
            .collect();

        assert!(scores.len() > 1);
        assert!(
            scores.windows(2).all(|w| w[0] >= w[1]),
            "not descending: {scores:?}"
        );
    }

    #[test]
    fn the_scope_narrows_the_search() {
        let mut ctx = ctx!();

        let surah = ctx.execute(r#"q:1:*"worship""#).unwrap();
        assert!(surah.iter().all(|r| r.extra["surah"] == 1));
        assert_eq!(surah[0].extra["ayah"], 5);

        // An ayah range inside that Surah.
        let ranged = ctx.execute(r#"q:1:3~5:*"help""#).unwrap();
        assert!(ranged
            .iter()
            .all(|r| r.extra["ayah"].as_u64().unwrap() >= 3
                && r.extra["ayah"].as_u64().unwrap() <= 5));
        assert_eq!(ranged[0].extra["ayah"], 5);
    }

    /// Trigrams give the hashed embedder tolerance for diacritics and for the
    /// prefixes Arabic attaches, which plain substring matching lacks.
    #[test]
    fn arabic_matches_without_diacritics_or_the_article() {
        let mut ctx = ctx!();

        let hits = ctx.execute(r#"q:1:*"حمد""#).unwrap();
        assert_eq!(hits[0].extra["ayah"], 2, "got {hits:?}");
    }

    #[test]
    fn the_limit_caps_the_result_count() {
        let mut ctx = ctx!();
        assert!(ctx.execute(r#"q:*"mercy"~3"#).unwrap().len() <= 3);
        assert!(ctx.execute(r#"q:*"mercy"~1"#).unwrap().len() <= 1);
    }

    #[test]
    fn hits_are_ordinary_records() {
        let mut ctx = ctx!();
        let hit = &ctx.execute(r#"q:1:*"worship""#).unwrap()[0];

        // Same shape and same text as addressing the ayah directly.
        let direct = &ctx.execute("q:1:5").unwrap()[0];
        assert_eq!(hit.ar, direct.ar);
        assert_eq!(hit.en, direct.en);
        assert_eq!(hit.extra["surah_name_en"], direct.extra["surah_name_en"]);
    }

    /// A source with no `.qv` file names the fix instead of failing obscurely.
    /// The custom-source fixtures have data but no index, which keeps this
    /// independent of which built-in indexes happen to be present.
    #[test]
    fn a_source_without_an_index_says_so() {
        let mut ctx = Context::new("tests/fixtures/custom");

        let error = ctx.execute(r#"x:1:*"opening""#).unwrap_err();
        assert_eq!(error.code(), "QQL_UNSUPPORTED");
        assert!(
            error.to_string().contains("build-vectors"),
            "the message should name the fix: {error}"
        );

        // The same source still answers an exact search.
        assert!(ctx.execute(r#"x:1:"Praise""#).is_ok());
    }

    #[test]
    fn similarity_mixes_with_ordinary_references() {
        let mut ctx = ctx!();
        let records = ctx.execute(r#"q:1:1;q:1:*"worship"~1"#).unwrap();

        assert_eq!(records.len(), 2);
        assert!(!records[0].extra.contains_key("ranked"));
        assert_eq!(records[1].extra["ranked"], true);
    }
}
