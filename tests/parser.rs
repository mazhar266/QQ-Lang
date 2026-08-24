// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Grammar tests. These must pass with no data directory present.

use qql::{parse, Error, Range};

/// `(source, primary, ranges)`. An empty source string means the query left
/// the code out, which the registry fills in at resolve time.
type Ref = (String, Option<u32>, Vec<(u32, u32)>);

fn refs(query: &str) -> Vec<Ref> {
    parse(query)
        .unwrap()
        .references
        .into_iter()
        .map(|r| {
            (
                r.source.unwrap_or_default(),
                r.primary,
                r.ranges
                    .into_iter()
                    .map(|Range { from, to }| (from, to))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn whole_primary_has_no_ranges() {
    assert_eq!(refs("Q:1"), [("Q".into(), Some(1), vec![])]);
    assert!(parse("Q:1").unwrap().references[0].selects_all());
}

#[test]
fn single_item_is_a_degenerate_range() {
    assert_eq!(refs("Q:2:255"), [("Q".into(), Some(2), vec![(255, 255)])]);
    assert!(!parse("Q:2:255").unwrap().references[0].selects_all());
}

#[test]
fn ranges_and_lists() {
    assert_eq!(refs("Q:2:1-5"), [("Q".into(), Some(2), vec![(1, 5)])]);
    assert_eq!(
        refs("Q:2:1-5,255"),
        [("Q".into(), Some(2), vec![(1, 5), (255, 255)])]
    );
    assert_eq!(
        refs("Q:2:1,3,5"),
        [("Q".into(), Some(2), vec![(1, 1), (3, 3), (5, 5)])]
    );
    assert_eq!(
        refs("Q:2:1-5,10,20-25,255"),
        [(
            "Q".into(),
            Some(2),
            vec![(1, 5), (10, 10), (20, 25), (255, 255)]
        )]
    );
}

#[test]
fn multiple_references_keep_their_order() {
    assert_eq!(
        refs("Q:1;Q:2:255;Q:112;"),
        [
            ("Q".into(), Some(1), vec![]),
            ("Q".into(), Some(2), vec![(255, 255)]),
            ("Q".into(), Some(112), vec![]),
        ]
    );
}

#[test]
fn trailing_semicolon_is_optional() {
    assert_eq!(refs("Q:1;Q:2"), refs("Q:1;Q:2;"));
}

#[test]
fn other_sources_parse_without_the_parser_knowing_them() {
    assert_eq!(refs("B:1:1-10"), [("B".into(), Some(1), vec![(1, 10)])]);
    assert_eq!(refs("HM:27"), [("HM".into(), Some(27), vec![])]);
    // Unknown to the registry, but syntactically fine — §35.
    assert_eq!(refs("XYZ:1:2"), [("XYZ".into(), Some(1), vec![(2, 2)])]);
    // Out of range for the Quran, but the parser has no idea.
    assert_eq!(
        refs("Q:500:999"),
        [("Q".into(), Some(500), vec![(999, 999)])]
    );
}

/// `B::100` skips the chapter and numbers across the whole collection.
#[test]
fn the_primary_can_be_skipped() {
    assert_eq!(refs("B::100"), [("B".into(), None, vec![(100, 100)])]);
    assert_eq!(refs("Q::1-3"), [("Q".into(), None, vec![(1, 3)])]);
    assert_eq!(
        refs("B::10,20-25"),
        [("B".into(), None, vec![(10, 10), (20, 25)])]
    );

    let flat = &parse("B::100").unwrap().references[0];
    assert!(flat.is_flat());
    assert!(!parse("B:1:1").unwrap().references[0].is_flat());

    // Mixes with the ordinary form in one query, order preserved.
    assert_eq!(
        refs("B:1:1;B::100;Q:2:255"),
        [
            ("B".into(), Some(1), vec![(1, 1)]),
            ("B".into(), None, vec![(100, 100)]),
            ("Q".into(), Some(2), vec![(255, 255)]),
        ]
    );

    assert_eq!(refs("b :: 100"), refs("B::100"));
}

/// `q:1:2,3,2:3,4-6` — an integer followed by `:` starts a new group, so this
/// is Surah 1 ayat 2,3 followed by Surah 2 ayat 3 and 4–6.
#[test]
fn a_reference_can_carry_several_groups() {
    assert_eq!(
        refs("q:1:2,3,2:3,4-6"),
        [
            ("Q".into(), Some(1), vec![(2, 2), (3, 3)]),
            ("Q".into(), Some(2), vec![(3, 3), (4, 6)]),
        ]
    );

    // The source applies to every group after it.
    assert_eq!(
        refs("B:1:1,2:5"),
        [
            ("B".into(), Some(1), vec![(1, 1)]),
            ("B".into(), Some(2), vec![(5, 5)]),
        ]
    );

    // A group with no selector means the whole primary.
    assert_eq!(
        refs("Q:1,2:255"),
        [
            ("Q".into(), Some(1), vec![]),
            ("Q".into(), Some(2), vec![(255, 255)]),
        ]
    );
    assert_eq!(
        refs("Q:1,2,3"),
        [
            ("Q".into(), Some(1), vec![]),
            ("Q".into(), Some(2), vec![]),
            ("Q".into(), Some(3), vec![]),
        ]
    );

    // A range is never a primary, so the trailing `:` here is a syntax error
    // rather than a silently different reading. The two things it might have
    // meant are both writable, and both mean something different:
    assert_eq!(
        parse("Q:1:1-5:3").unwrap_err().code(),
        "QQL_INVALID_CHARACTER"
    );
    // `;3` — Surah 3, a new reference inheriting the stated `Q`.
    assert_eq!(
        refs("Q:1:1-5;3"),
        [
            ("Q".into(), Some(1), vec![(1, 5)]),
            ("Q".into(), Some(3), vec![]),
        ]
    );
    // `,3` — ayah 3 of the same Surah, which the dedup pass then folds away.
    assert_eq!(
        refs("Q:1:1-5,3"),
        [("Q".into(), Some(1), vec![(1, 5), (3, 3)])]
    );
}

/// Omitting the source means the Quran. The parser records the omission; the
/// registry decides what it means.
#[test]
fn the_source_may_be_omitted() {
    // Empty string in this helper means "no source was written".
    assert_eq!(refs("1"), [("".into(), Some(1), vec![])]);
    assert_eq!(refs("2:255"), [("".into(), Some(2), vec![(255, 255)])]);
    assert_eq!(
        refs("1,2:255"),
        [
            ("".into(), Some(1), vec![]),
            ("".into(), Some(2), vec![(255, 255)]),
        ]
    );
    assert_eq!(
        refs("1:2,3,2:3,4-6"),
        [
            ("".into(), Some(1), vec![(2, 2), (3, 3)]),
            ("".into(), Some(2), vec![(3, 3), (4, 6)]),
        ]
    );

    for reference in parse("1,2:255").unwrap().references {
        assert!(reference.source.is_none());
    }

    // Mixes with explicit sources across a `;`.
    assert_eq!(
        refs("1:1;b:1:1"),
        [
            ("".into(), Some(1), vec![(1, 1)]),
            ("B".into(), Some(1), vec![(1, 1)]),
        ]
    );

    // A stated source carries forward; only another code changes it.
    assert_eq!(
        refs("b:1:1;3"),
        [
            ("B".into(), Some(1), vec![(1, 1)]),
            ("B".into(), Some(3), vec![]),
        ]
    );
    assert_eq!(
        refs("b:1:1;q:3"),
        [
            ("B".into(), Some(1), vec![(1, 1)]),
            ("Q".into(), Some(3), vec![]),
        ]
    );
    assert_eq!(
        refs("b:1:1;3;q:1;2"),
        [
            ("B".into(), Some(1), vec![(1, 1)]),
            ("B".into(), Some(3), vec![]),
            ("Q".into(), Some(1), vec![]),
            ("Q".into(), Some(2), vec![]),
        ]
    );

    // Inheritance never turns a bare number into the flat `::` form.
    assert_eq!(
        refs("b::100;3"),
        [
            ("B".into(), None, vec![(100, 100)]),
            ("B".into(), Some(3), vec![]),
        ]
    );

    assert_eq!(refs(" 1 , 2 : 255 "), refs("1,2:255"));
}

/// `;` is only needed to change source; a trailing one is always optional.
#[test]
fn the_semicolon_separates_sources_only() {
    assert_eq!(refs("Q:1,2"), refs("Q:1;Q:2"));
    assert_eq!(refs("q:1;b:1"), refs("q:1;b:1;"));
    assert_eq!(refs("1;b:1"), refs("1;b:1;"));
}

#[test]
fn source_codes_normalize_to_uppercase() {
    assert_eq!(refs("q:2:255"), [("Q".into(), Some(2), vec![(255, 255)])]);
    assert_eq!(refs("hm:27"), [("HM".into(), Some(27), vec![])]);
}

#[test]
fn whitespace_around_tokens_is_legal() {
    assert_eq!(refs("Q : 2 : 255"), refs("Q:2:255"));
    assert_eq!(refs("Q:2:1-5, 255"), refs("Q:2:1-5,255"));
    assert_eq!(refs("  Q:1 ;  Q:2  "), refs("Q:1;Q:2"));
}

/// Search scopes: `Q:"t"`, `Q:1:"t"`, `Q:1:3~5:"t"`, and a bare `"t"`.
#[test]
fn search_terms_parse_as_scoped_references() {
    fn search(query: &str) -> (String, Option<u32>, Vec<(u32, u32)>, String) {
        let r = parse(query).unwrap().references.remove(0);
        (
            r.source.clone().unwrap_or_default(),
            r.primary,
            r.ranges.iter().map(|g| (g.from, g.to)).collect(),
            r.search.clone().unwrap().term,
        )
    }

    // Whole collection.
    assert_eq!(
        search(r#"q:"text""#),
        ("Q".into(), None, vec![], "text".into())
    );
    // Bare term — no code stated, so the registry's default applies.
    assert_eq!(
        search(r#""text""#),
        ("".into(), None, vec![], "text".into())
    );
    // Within a primary.
    assert_eq!(
        search(r#"q:1:"text""#),
        ("Q".into(), Some(1), vec![], "text".into())
    );
    // Within a range of one primary.
    assert_eq!(
        search(r#"q:1:3~5:"text""#),
        ("Q".into(), Some(1), vec![(3, 5)], "text".into())
    );

    let reference = &parse(r#"q:1:"text""#).unwrap().references[0];
    assert!(reference.is_search());
    // A search without a Surah scope is not the `Q::N` flat form.
    assert!(!parse(r#"q:"text""#).unwrap().references[0].is_flat());
    assert!(!parse("Q:1:1").unwrap().references[0].is_search());

    // Either quote delimits a term, identically.
    assert_eq!(search("q:1:'text'"), search(r#"q:1:"text""#));
    assert_eq!(search("'text'"), search(r#""text""#));
    assert_eq!(search("q:1:3~5:'text'"), search(r#"q:1:3~5:"text""#));

    // Each quote carries the other verbatim, which is how a term containing
    // one is written.
    assert_eq!(search(r#"q:"Allah's""#).3, "Allah's");
    assert_eq!(search(r#"q:'say "this"'"#).3, r#"say "this""#);

    // A bare term of any engine starts a reference — the source defaults just
    // as it does for a bare number.
    for query in [
        r#""mercy""#,
        r#"?"mercy""#,
        r#"*"mercy""#,
        "?'mercy'",
        "*'mercy'",
    ] {
        let parsed = parse(query).unwrap_or_else(|e| panic!("{query} should parse: {e}"));
        assert_eq!(parsed.references.len(), 1, "{query}");
        assert!(parsed.references[0].source.is_none(), "{query}");
        assert_eq!(parsed.references[0].search.as_ref().unwrap().term, "mercy");
    }

    // Terms are taken literally: spaces, colons and Arabic all survive.
    assert_eq!(search(r#"q:"a b:c-1""#).3, "a b:c-1");
    assert_eq!(search(r#"q:"الحمد""#).3, "الحمد");

    // Searches mix with ordinary references, and inherit the stated source.
    let mixed = parse(r#"b:1:1;"text""#).unwrap().references;
    assert_eq!(mixed.len(), 2);
    assert!(!mixed[0].is_search());
    assert_eq!(mixed[1].source.as_deref(), Some("B"));
    assert_eq!(mixed[1].search.as_ref().unwrap().term, "text");
}

/// Each invalid query pins both the variant and the reported offset.
#[test]
fn invalid_queries() {
    let cases: &[(&str, &str, Option<usize>)] = &[
        ("", "QQL_EMPTY_QUERY", None),
        ("   ", "QQL_EMPTY_QUERY", None),
        (";", "QQL_EXPECTED_SOURCE", Some(0)),
        (":2", "QQL_EXPECTED_SOURCE", Some(0)),
        ("Q", "QQL_EXPECTED_COLON", Some(1)),
        ("Q:", "QQL_EXPECTED_NUMBER", Some(2)),
        // `Q::` opens the flat form, so the missing number is after the
        // second colon, not at it.
        ("Q::", "QQL_EXPECTED_NUMBER", Some(3)),
        ("Q::,5", "QQL_EXPECTED_NUMBER", Some(3)),
        ("Q:::1", "QQL_EXPECTED_NUMBER", Some(3)),
        ("Q:A", "QQL_EXPECTED_NUMBER", Some(2)),
        ("Q:2:", "QQL_EXPECTED_NUMBER", Some(4)),
        ("Q:2:-5", "QQL_EXPECTED_NUMBER", Some(4)),
        ("Q:2:1-", "QQL_EXPECTED_NUMBER", Some(6)),
        ("Q:2:1,,5", "QQL_EXPECTED_NUMBER", Some(6)),
        ("Q:2:1-,5", "QQL_EXPECTED_NUMBER", Some(6)),
        ("Q:2:5-1", "QQL_INVALID_RANGE", Some(4)),
        ("Q:2*3", "QQL_INVALID_CHARACTER", Some(3)),
        ("Q:1;;Q:2", "QQL_EXPECTED_SOURCE", Some(4)),
        ("Q:99999999999999999999", "QQL_EXPECTED_NUMBER", Some(2)),
        // Search terms.
        (r#"q:1:"abc"#, "QQL_UNTERMINATED_TEXT", Some(4)),
        ("q:1:'abc", "QQL_UNTERMINATED_TEXT", Some(4)),
        // A term opened with one quote is not closed by the other.
        (r#"q:1:'abc""#, "QQL_UNTERMINATED_TEXT", Some(4)),
        ("q:1:''", "QQL_EXPECTED_TEXT", Some(4)),
        (r#"""#, "QQL_UNTERMINATED_TEXT", Some(0)),
        (r#"q:1:"""#, "QQL_EXPECTED_TEXT", Some(4)),
        (r#"q:1:"   ""#, "QQL_EXPECTED_TEXT", Some(4)),
        (r#"q:1:3~5"#, "QQL_EXPECTED_COLON", Some(7)),
        (r#"q:1:3~5:"#, "QQL_EXPECTED_TEXT", Some(8)),
        (r#"q:1:5~3:"x""#, "QQL_INVALID_RANGE", Some(4)),
        (r#"q:1:3~"#, "QQL_EXPECTED_NUMBER", Some(6)),
    ];

    for (query, code, position) in cases {
        let error = parse(query).expect_err(&format!("{query:?} should not parse"));
        assert_eq!(error.code(), *code, "wrong code for {query:?}");
        assert_eq!(error.position(), *position, "wrong position for {query:?}");
    }
}

/// The same invariant `cargo fuzz run parse` checks, in a form that runs on
/// stable and in CI: parsing never panics, and expansion never tries to
/// allocate its way out of a bad selector.
#[test]
fn parsing_never_panics_on_garbage() {
    let alphabet = [
        "",
        "Q",
        "q",
        "HM",
        ":",
        ";",
        ",",
        "-",
        "~",
        "\"",
        "\"a\"",
        "'",
        "'a'",
        "0",
        "1",
        "9",
        "4294967295",
        "99999999999999999999",
        " ",
        "\t",
        "*",
        "\u{0}",
        "٢",
        "🕋",
        "\u{200b}",
    ];

    // Every pair and triple over a nasty alphabet — ~9000 queries.
    for a in alphabet {
        for b in alphabet {
            let _ = parse(&format!("{a}{b}"));
            for c in alphabet {
                let query = format!("{a}{b}{c}");
                if let Ok(parsed) = parse(&query) {
                    for reference in &parsed.references {
                        let _ = reference.expand(1000);
                        let _ = reference.expand(0);
                    }
                }
            }
        }
    }
}

#[test]
fn errors_always_serialize_to_valid_json() {
    let error = parse("Q:2:5-1").unwrap_err();
    let json = error.to_json("Q:2:5-1");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "QQL_INVALID_RANGE");
    assert_eq!(json["error"]["position"], 4);

    // Variants with no position must omit the key, not emit a placeholder.
    let json = Error::EmptyQuery.to_json("");
    assert!(json["error"].get("position").is_none());
}
