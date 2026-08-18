// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! End-to-end tests against the real data in `sources/`.
//!
//! These need the git submodules checked out. If they are not, every test here
//! is skipped rather than failing — a missing submodule is not a code defect.

use qql::Context;
use serde_json::Value;

const DATA: &str = "sources";

fn context() -> Option<Context> {
    std::path::Path::new(DATA)
        .join("quran-json-arabic/dist/chapters/en/1.json")
        .exists()
        .then(|| Context::new(DATA))
}

macro_rules! ctx {
    () => {
        match context() {
            Some(ctx) => ctx,
            None => {
                eprintln!("skipping: sources/ submodules not checked out");
                return;
            }
        }
    };
}

#[test]
fn resolves_a_single_ayah() {
    let mut ctx = ctx!();
    let records = ctx.execute("Q:2:255").unwrap();
    assert_eq!(records.len(), 1);

    let record = &records[0];
    assert_eq!(record.source, "Q");
    assert_eq!(record.collection, "Quran");
    assert_eq!(record.extra["surah"], 2);
    assert_eq!(record.extra["ayah"], 255);
    assert_eq!(record.extra["surah_name_en"], "Al-Baqarah");
    assert_eq!(record.extra["surah_name_ar"], "البقرة");
    // Byte-exactness is pinned by `arabic_survives_the_round_trip_byte_for_byte`;
    // asserting a literal here would only pin this test to one tashkeel encoding.
    assert!(!record.ar.is_empty());
    assert!(record.en.contains("Allah"));
}

#[test]
fn whole_surah_selects_every_ayah() {
    let mut ctx = ctx!();
    assert_eq!(ctx.execute("Q:1").unwrap().len(), 7);
    assert_eq!(ctx.execute("Q:2").unwrap().len(), 286);
    assert_eq!(ctx.execute("Q:114").unwrap().len(), 6);
}

#[test]
fn query_order_is_preserved_not_sorted() {
    let mut ctx = ctx!();
    let ayat: Vec<_> = ctx
        .execute("Q:2:255,1-3")
        .unwrap()
        .iter()
        .map(|r| r.extra["ayah"].as_u64().unwrap())
        .collect();
    assert_eq!(ayat, [255, 1, 2, 3]);
}

#[test]
fn duplicates_go_within_a_reference_but_not_across() {
    let mut ctx = ctx!();

    let ayat: Vec<_> = ctx
        .execute("Q:2:1-5,3,4")
        .unwrap()
        .iter()
        .map(|r| r.extra["ayah"].as_u64().unwrap())
        .collect();
    assert_eq!(ayat, [1, 2, 3, 4, 5]);

    assert_eq!(ctx.execute("Q:2:255;Q:2:255;").unwrap().len(), 2);
}

#[test]
fn multiple_references_resolve_in_order() {
    let mut ctx = ctx!();
    let records = ctx.execute("Q:2:1-3,255;Q:1:1;").unwrap();
    let pairs: Vec<_> = records
        .iter()
        .map(|r| {
            (
                r.extra["surah"].as_u64().unwrap(),
                r.extra["ayah"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(pairs, [(2, 1), (2, 2), (2, 3), (2, 255), (1, 1)]);
}

/// `q:1:2,3,2:3,4-6` resolves as two groups: Surah 1 ayat 2,3 then Surah 2
/// ayat 3,4,5,6.
#[test]
fn grouped_references_resolve_in_written_order() {
    let mut ctx = ctx!();
    let pairs: Vec<_> = ctx
        .execute("q:1:2,3,2:3,4-6")
        .unwrap()
        .iter()
        .map(|r| {
            (
                r.extra["surah"].as_u64().unwrap(),
                r.extra["ayah"].as_u64().unwrap(),
            )
        })
        .collect();

    assert_eq!(pairs, [(1, 2), (1, 3), (2, 3), (2, 4), (2, 5), (2, 6)]);
}

/// A query with no source code is the Quran.
#[test]
fn an_omitted_source_means_the_quran() {
    let mut ctx = ctx!();

    // A bare number is a whole Surah.
    assert_eq!(ctx.execute("1").unwrap().len(), 7);
    assert_eq!(ctx.execute("1").unwrap()[0].source, "Q");

    // Identical to spelling the source out.
    assert_eq!(
        ctx.execute("2:255").unwrap()[0].ar,
        ctx.execute("Q:2:255").unwrap()[0].ar
    );

    let pairs: Vec<_> = ctx
        .execute("1,2:255")
        .unwrap()
        .iter()
        .map(|r| {
            (
                r.extra["surah"].as_u64().unwrap(),
                r.extra["ayah"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(pairs.len(), 8);
    assert_eq!(pairs[0], (1, 1));
    assert_eq!(pairs[7], (2, 255));

    // Semantic errors still come from the Quran resolver.
    assert_eq!(
        ctx.execute("115").unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
}

/// `;` is only needed to switch source, and the last one may be dropped.
#[test]
fn the_semicolon_is_only_needed_between_sources() {
    let mut ctx = ctx!();

    assert_eq!(
        ctx.execute("Q:1,2:255").unwrap().len(),
        ctx.execute("Q:1;Q:2:255").unwrap().len()
    );

    let mixed = ctx.execute("1:1;b:1:1").unwrap();
    assert_eq!(mixed.len(), 2);
    assert_eq!(mixed[0].source, "Q");
    assert_eq!(mixed[1].source, "B");

    assert_eq!(
        ctx.execute("q:1;b:1:1").unwrap().len(),
        ctx.execute("q:1;b:1:1;").unwrap().len()
    );
}

/// `Q:1:1-5:3` is rejected. Spelling out what it might have meant gives two
/// different, both-valid queries — this pins them apart.
#[test]
fn a_range_cannot_be_followed_by_a_group_colon() {
    let mut ctx = ctx!();

    assert_eq!(
        ctx.execute("q:1:1-5:3").unwrap_err().code(),
        "QQL_INVALID_CHARACTER"
    );

    // `;3` starts a new reference: Surah 3, whole.
    let semicolon = ctx.execute("q:1:1-5;3").unwrap();
    assert_eq!(semicolon.len(), 5 + 200);
    assert_eq!(semicolon[5].extra["surah"], 3);

    // `,3` is ayah 3 of the same Surah — already inside 1-5, so dedup drops it.
    let comma: Vec<_> = ctx
        .execute("q:1:1-5,3")
        .unwrap()
        .iter()
        .map(|r| r.extra["ayah"].as_u64().unwrap())
        .collect();
    assert_eq!(comma, [1, 2, 3, 4, 5]);

    // Outside the range it is kept, in written order.
    let comma: Vec<_> = ctx
        .execute("q:1:1-2,7")
        .unwrap()
        .iter()
        .map(|r| r.extra["ayah"].as_u64().unwrap())
        .collect();
    assert_eq!(comma, [1, 2, 7]);
}

/// A stated source carries forward: once a line says `B`, everything after it
/// is Bukhari until another code says otherwise.
#[test]
fn a_stated_source_carries_forward_across_semicolons() {
    let mut ctx = ctx!();

    let records = ctx.execute("b:1:1;3").unwrap();
    assert!(records.iter().all(|r| r.source == "B"));
    assert_eq!(records[0].extra["chapter"], 1);
    assert_eq!(records[1].extra["chapter"], 3);
    // Chapter 3 of Bukhari, not Surah 3 — 76 hadiths, not 200 ayat.
    assert_eq!(records.len(), 1 + 76);

    // An explicit code switches back.
    let switched = ctx.execute("b:1:1;q:3").unwrap();
    assert_eq!(switched[0].source, "B");
    assert_eq!(switched[1].source, "Q");
    assert_eq!(switched[1].extra["surah"], 3);
    assert_eq!(switched.len(), 1 + 200);

    // The switch is sticky too.
    let back = ctx.execute("b:1:1;q:1:1;2:255").unwrap();
    assert_eq!(back[0].source, "B");
    assert_eq!(back[1].source, "Q");
    assert_eq!(back[2].source, "Q");
    assert_eq!(back[2].extra["ayah"], 255);

    // Only a code stated *earlier* is inherited; with none, the default holds.
    let leading = ctx.execute("1:1;b:1:1").unwrap();
    assert_eq!(leading[0].source, "Q");
    assert_eq!(leading[1].source, "B");
}

#[test]
fn boundaries() {
    let mut ctx = ctx!();
    assert_eq!(ctx.execute("Q:1:1").unwrap().len(), 1);
    assert_eq!(ctx.execute("Q:1:1-7").unwrap().len(), 7);
    assert!(ctx.execute("Q:1:8").is_err());
}

#[test]
fn semantic_errors_come_from_the_resolver_and_registry() {
    let mut ctx = ctx!();

    for query in ["Q:0", "Q:115", "Q:2:999", "Q:1:8"] {
        let error = ctx.execute(query).unwrap_err();
        assert_eq!(error.code(), "QQL_REFERENCE_NOT_FOUND", "for {query}");
    }

    // §35: syntactically valid, rejected by the registry.
    assert_eq!(
        ctx.execute("XYZ:1:2").unwrap_err().code(),
        "QQL_UNKNOWN_SOURCE"
    );
}

#[test]
fn a_huge_range_is_rejected_rather_than_allocated() {
    let mut ctx = ctx!();
    let error = ctx.execute("Q:1:1-4294967295").unwrap_err();
    assert_eq!(error.code(), "QQL_REFERENCE_NOT_FOUND");
}

#[test]
fn resolves_hadith_through_the_same_parser() {
    let mut ctx = ctx!();
    let records = ctx.execute("B:1:1-3").unwrap();
    assert_eq!(records.len(), 3);

    let record = &records[0];
    assert_eq!(record.source, "B");
    assert_eq!(record.collection, "Sahih al-Bukhari");
    assert_eq!(record.extra["chapter"], 1);
    assert_eq!(record.extra["number"], 1);
    assert_eq!(record.extra["chapter_name_en"], "Revelation");
    assert!(record.en.contains("intentions"));
    assert!(!record.ar.is_empty());

    assert_eq!(ctx.execute("M:1:1").unwrap()[0].collection, "Sahih Muslim");
    assert_eq!(
        ctx.execute("T:1:1").unwrap()[0].collection,
        "Jami' at-Tirmidhi"
    );
}

#[test]
fn resolves_hisnul_muslim() {
    let mut ctx = ctx!();

    let records = ctx.execute("HM:1").unwrap();
    assert_eq!(records.len(), 4);

    let record = &records[0];
    assert_eq!(record.source, "HM");
    assert_eq!(record.collection, "Hisnul Muslim");
    assert_eq!(record.extra["chapter"], 1);
    assert_eq!(record.extra["number"], 1);
    assert_eq!(record.extra["repeat"], 1);
    assert!(!record.ar.is_empty());
    assert!(record.en.contains("All praise is for Allah"));
}

/// The upstream file stores chapters out of order — array position 0 holds
/// chapter 27. Indexing by position instead of by `ID` would silently return
/// the wrong supplication, which is why this test exists.
#[test]
fn hisnul_chapters_are_found_by_id_not_by_array_position() {
    let mut ctx = ctx!();

    let first = &ctx.execute("HM:1:1").unwrap()[0];
    assert_eq!(
        first.extra["chapter_title"],
        "supplications for when you wake up"
    );

    let twenty_seven = ctx.execute("HM:27").unwrap();
    assert_eq!(twenty_seven.len(), 24);
    assert_eq!(
        twenty_seven[0].extra["chapter_title"],
        "Words of remembrance for morning and evening"
    );
}

/// Three quirks in the upstream file that a strict schema would choke on:
/// duplicate keys, a misspelled `Text` key, and missing fields.
#[test]
fn hisnul_survives_the_upstream_data_quirks() {
    let mut ctx = ctx!();

    // The file carries a UTF-8 BOM and two objects with repeated keys. If
    // either were mishandled, no HM query would resolve at all.
    assert!(ctx.execute("HM:132").is_ok());

    // HM:132:1 spells its Arabic key `Text` rather than `ARABIC_TEXT`.
    let record = &ctx.execute("HM:132:1").unwrap()[0];
    assert!(!record.ar.is_empty(), "the `Text` key fallback should apply");
}

#[test]
fn source_aliases_resolve() {
    let mut ctx = ctx!();
    assert_eq!(
        ctx.execute("HISN:1:1").unwrap()[0].extra["chapter"],
        ctx.execute("HM:1:1").unwrap()[0].extra["chapter"]
    );
}

/// `B::100` skips the chapter and uses traditional book-wide numbering.
#[test]
fn flat_numbering_reads_across_the_whole_book() {
    let mut ctx = ctx!();

    let records = ctx.execute("B::100").unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source, "B");
    assert_eq!(records[0].extra["number"], 100);
    // Tagged so a response mixing both forms stays unambiguous.
    assert_eq!(records[0].extra["numbering"], "book");
    // Hadith 100 of Bukhari sits in chapter 3, Kitab al-'Ilm.
    assert_eq!(records[0].extra["chapter"], 3);
    assert_eq!(records[0].extra["chapter_name_en"], "Knowledge");
    assert!(!records[0].ar.is_empty());

    // Ranges, order, and dedup behave as everywhere else.
    let numbers: Vec<_> = ctx
        .execute("B::100,1-3,2")
        .unwrap()
        .iter()
        .map(|r| r.extra["number"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, [100, 1, 2, 3]);
}

/// The first hadith of chapter 1 *is* the first hadith of the book, for every
/// collection. This is what catches the `id` / `idInBook` confusion: the
/// `by_book` files number `id` across all nine books at once (Muslim starts at
/// 7278), so using it would silently return the wrong hadith for everything
/// except Bukhari.
#[test]
fn flat_and_chapter_numbering_agree_at_the_start_of_every_collection() {
    let mut ctx = ctx!();

    for code in ["B", "M", "AD", "T", "N", "IM"] {
        let flat = ctx.execute(&format!("{code}::1")).unwrap();
        let chaptered = ctx.execute(&format!("{code}:1:1")).unwrap();

        assert_eq!(flat.len(), 1, "{code}::1");
        assert_eq!(flat[0].ar, chaptered[0].ar, "{code}: Arabic differs");
        assert_eq!(flat[0].en, chaptered[0].en, "{code}: English differs");
        assert_eq!(flat[0].extra["chapter"], 1, "{code}: wrong chapter");
    }
}

/// Sunan an-Nasa'i has a chapter numbered `35.2`, so chapter identifiers are
/// not always integers. Forcing `u32` would fail the whole file.
#[test]
fn non_integer_chapter_ids_do_not_break_the_book_file() {
    let mut ctx = ctx!();
    assert!(ctx.execute("N::1-5").is_ok());
    assert_eq!(ctx.execute("N::5768").unwrap().len(), 1);
}

#[test]
fn flat_numbering_works_for_quran_and_hisnul_muslim() {
    let mut ctx = ctx!();

    // Global ayah 100 is Surah 2, ayah 93.
    let ayah = &ctx.execute("Q::100").unwrap()[0];
    assert_eq!(ayah.extra["surah"], 2);
    assert_eq!(ayah.extra["ayah"], 93);
    assert_eq!(ayah.extra["number"], 100);
    assert_eq!(ayah.extra["numbering"], "book");
    // Same text as addressing it by Surah and ayah.
    assert_eq!(ayah.ar, ctx.execute("Q:2:93").unwrap()[0].ar);

    // Supplication 75 opens chapter 27.
    let dua = &ctx.execute("HM::75").unwrap()[0];
    assert_eq!(dua.extra["chapter"], 27);
    assert_eq!(dua.extra["number"], 75);
    assert_eq!(dua.ar, ctx.execute("HM:27:1").unwrap()[0].ar);

    assert_eq!(ctx.execute("Q::1").unwrap()[0].ar, ctx.execute("Q:1:1").unwrap()[0].ar);
    assert_eq!(ctx.execute("Q::6236").unwrap()[0].extra["surah"], 114);
}

#[test]
fn flat_references_are_bounds_checked() {
    let mut ctx = ctx!();

    for query in ["B::99999", "Q::6237", "HM::268", "Q::1-4294967295"] {
        assert_eq!(
            ctx.execute(query).unwrap_err().code(),
            "QQL_REFERENCE_NOT_FOUND",
            "for {query}"
        );
    }
}

#[test]
fn missing_chapters_read_as_semantic_errors_not_storage_errors() {
    let mut ctx = ctx!();
    assert_eq!(
        ctx.execute("B:9999:1").unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
    assert_eq!(
        ctx.execute("B:0:1").unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
    for query in ["HM:0", "HM:133", "HM:1:99"] {
        assert_eq!(
            ctx.execute(query).unwrap_err().code(),
            "QQL_REFERENCE_NOT_FOUND",
            "for {query}"
        );
    }
}

#[test]
fn arabic_survives_the_round_trip_byte_for_byte() {
    let mut ctx = ctx!();

    let expected = {
        let raw =
            std::fs::read_to_string("sources/quran-json-arabic/dist/chapters/en/1.json").unwrap();
        let file: Value = serde_json::from_str(&raw).unwrap();
        file["verses"][0]["text"].as_str().unwrap().to_string()
    };

    let json = ctx.execute_json("Q:1:1");
    let value: Value = serde_json::from_str(&json).unwrap();
    let got = value["results"][0]["ar"].as_str().unwrap();

    assert_eq!(got, expected);
    assert_eq!(got.as_bytes(), expected.as_bytes());
    assert!(!got.contains('\u{fffd}'), "replacement character in output");
}

#[test]
fn execute_json_always_returns_valid_json() {
    let mut ctx = ctx!();

    for query in ["Q:2:255", "", "Q:2:5-1", "XYZ:1", "Q:115", "!!!"] {
        let json = ctx.execute_json(query);
        let value: Value = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("{query:?} produced invalid JSON: {e}"));
        assert!(value["ok"].is_boolean());
        assert_eq!(value["query"], query);
    }
}

#[test]
fn context_is_send_so_separate_threads_are_safe_by_construction() {
    fn assert_send<T: Send>() {}
    assert_send::<Context>();
}

#[test]
fn caching_serves_repeat_queries() {
    let mut ctx = ctx!();
    let first = ctx.execute("Q:2:255").unwrap();
    let second = ctx.execute("Q:2:255").unwrap();
    assert_eq!(first[0].ar, second[0].ar);
}
