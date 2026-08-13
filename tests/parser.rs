// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Grammar tests. These must pass with no data directory present.

use qql::{parse, Error, Range};

fn refs(query: &str) -> Vec<(String, Option<u32>, Vec<(u32, u32)>)> {
    parse(query)
        .unwrap()
        .references
        .into_iter()
        .map(|r| {
            (
                r.source,
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
    assert_eq!(refs("Q:500:999"), [("Q".into(), Some(500), vec![(999, 999)])]);
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
        "", "Q", "q", "HM", ":", ";", ",", "-", "0", "1", "9", "4294967295",
        "99999999999999999999", " ", "\t", "*", "\u{0}", "٢", "🕋", "\u{200b}",
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
