// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! End-to-end tests against the real data in `sources/`.
//!
//! These need `sources/` populated — the git submodules checked out. If they are
//! not, every test here is skipped rather than failing.

use qql::Context;
use serde_json::Value;

const DATA: &str = "sources";

fn context() -> Option<Context> {
    std::path::Path::new(DATA)
        .join("quran/chapters/1.json")
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

/// Full-text search, scoped four ways.
#[test]
fn search_finds_text_within_its_scope() {
    let mut ctx = ctx!();

    // Within one Surah. Al-Fatihah 1:2 is the only "الحمد" there.
    let hits = ctx.execute(r#"q:1:"الحمد""#).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].extra["surah"], 1);
    assert_eq!(hits[0].extra["ayah"], 2);

    // Within an ayah range of one Surah.
    let scoped = ctx.execute(r#"q:1:3~5:"You""#).unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].extra["ayah"], 5);
    // The same term outside the scope is not reported.
    assert!(ctx.execute(r#"q:1:3~4:"You""#).unwrap().is_empty());

    // The whole collection, with and without the code written out.
    let whole = ctx.execute(r#"q:"الحمد""#).unwrap();
    assert!(whole.len() > 20, "expected many hits, got {}", whole.len());
    assert_eq!(whole.len(), ctx.execute(r#""الحمد""#).unwrap().len());
    assert!(whole.iter().all(|r| r.source == "Q"));
    // Results stay in mushaf order.
    assert_eq!(whole[0].extra["surah"], 1);

    // English matches too, case-insensitively.
    assert_eq!(
        ctx.execute(r#"q:112:"Allah""#).unwrap().len(),
        ctx.execute(r#"q:112:"ALLAH""#).unwrap().len()
    );
}

/// Arabic and English are searched together — a term is tried against both
/// fields of every record in scope.
#[test]
fn search_covers_arabic_and_english_alike() {
    let mut ctx = ctx!();

    // English only appears in `en`.
    let english = ctx.execute(r#"q:1:"Allah""#).unwrap();
    assert_eq!(english.len(), 2);
    assert!(english.iter().all(|r| r.en.to_lowercase().contains("allah")));

    // Arabic only appears in `ar`.
    let arabic = ctx.execute(r#"q:1:"الحمد""#).unwrap();
    assert_eq!(arabic.len(), 1);

    // Both reach the same ayah when each field mentions it.
    assert!(!ctx.execute(r#"q:"Pharaoh""#).unwrap().is_empty());
    assert!(!ctx.execute(r#"q:2:"prayer""#).unwrap().is_empty());
    assert!(!ctx.execute(r#"b:1:"revelation""#).unwrap().is_empty());
}

/// `'` and `"` delimit a term identically; each carries the other verbatim.
#[test]
fn either_quote_delimits_a_search_term() {
    let mut ctx = ctx!();

    for (single, double) in [
        ("q:1:'الحمد'", r#"q:1:"الحمد""#),
        ("q:1:'Allah'", r#"q:1:"Allah""#),
        ("q:1:3~5:'You'", r#"q:1:3~5:"You""#),
        ("'mercy'", r#""mercy""#),
    ] {
        assert_eq!(
            ctx.execute(single).unwrap().len(),
            ctx.execute(double).unwrap().len(),
            "{single} and {double} should agree"
        );
    }

    // An apostrophe inside a term needs the other quote around it.
    assert!(!ctx.execute(r#"b:1:"Allah's""#).unwrap().is_empty());
    assert_eq!(
        ctx.execute("b:1:'Allah's'").unwrap_err().code(),
        "QQL_UNTERMINATED_TEXT"
    );
}

/// The Quran text is fully diacritized, so a typed needle shares no substring
/// with it unless the marks are folded away for comparison.
#[test]
fn search_folds_arabic_diacritics() {
    let mut ctx = ctx!();

    // Undiacritized needle against diacritized scripture.
    assert_eq!(ctx.execute(r#"q:1:"الحمد""#).unwrap().len(), 1);
    // The stored spelling still matches, and both find the same ayah.
    assert_eq!(ctx.execute(r#"q:1:"ٱلْحَمْدُ""#).unwrap().len(), 1);

    // Folding is for comparison only — the record keeps its marks.
    let hit = &ctx.execute(r#"q:1:"الحمد""#).unwrap()[0];
    assert_eq!(hit.ar, ctx.execute("q:1:2").unwrap()[0].ar);
    assert!(hit.ar.contains('\u{064E}'), "diacritics were stripped from output");
}

/// Search is source-agnostic: it filters whatever the scope resolves to.
#[test]
fn search_works_for_every_source() {
    let mut ctx = ctx!();

    let bukhari = ctx.execute(r#"b:1:"intentions""#).unwrap();
    assert_eq!(bukhari.len(), 1);
    assert_eq!(bukhari[0].source, "B");
    assert_eq!(bukhari[0].extra["chapter"], 1);

    assert!(!ctx.execute(r#"hm:1:"Allah""#).unwrap().is_empty());

    // A stated source carries into a bare search term, as anywhere else — but
    // only the source. The term itself is unscoped, so it searches all of
    // Bukhari rather than the chapter named just before it.
    let inherited = ctx.execute(r#"b:1:1;"intentions""#).unwrap();
    assert!(inherited.iter().all(|r| r.source == "B"));
    assert!(
        inherited.len() > ctx.execute(r#"b:1:"intentions""#).unwrap().len(),
        "an unscoped term should reach past chapter 1"
    );
}

#[test]
fn a_search_that_matches_nothing_is_empty_not_an_error() {
    let mut ctx = ctx!();
    let value = ctx.execute_value(r#"q:1:"zzzznotpresent""#);
    assert_eq!(value["ok"], true);
    assert_eq!(value["results"].as_array().unwrap().len(), 0);
}

#[test]
fn search_scopes_are_validated_like_any_other_reference() {
    let mut ctx = ctx!();

    // Surah 115 does not exist, search or not.
    assert_eq!(
        ctx.execute(r#"q:115:"x""#).unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
    // Neither does ayah 99 of Al-Fatihah.
    assert_eq!(
        ctx.execute(r#"q:1:3~99:"x""#).unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
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

/// `Q::N` is served by a 114-entry verse-count table plus the ordinary chapter
/// files. If the table and the data ever disagree, `Q::N` silently returns the
/// wrong ayah — so check every Surah boundary against the real files.
#[test]
fn the_verse_count_table_matches_the_data() {
    let mut ctx = ctx!();

    let mut global = 0;
    for surah in 1..=114u64 {
        let ayat = ctx.execute(&format!("Q:{surah}")).unwrap();
        global += ayat.len() as u64;

        // The last ayah of this Surah, addressed both ways.
        let flat = &ctx.execute(&format!("Q::{global}")).unwrap()[0];
        assert_eq!(flat.extra["surah"], surah, "at global ayah {global}");
        assert_eq!(
            flat.extra["ayah"],
            ayat.len() as u64,
            "at global ayah {global}"
        );
        assert_eq!(flat.ar, ayat[ayat.len() - 1].ar, "at global ayah {global}");
    }

    assert_eq!(global, 6236);
    assert!(ctx.execute("Q::6237").is_err());
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
        let raw = std::fs::read_to_string("sources/quran/chapters/1.json").unwrap();
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

/// The submodule spells three marks with codepoints that mean something else
/// in Unicode — U+0657 INVERTED DAMMA for an open fathatan, U+065E for a
/// dammatan, U+0656 for a kasratan — so a font that follows Unicode draws a
/// damma where a fathatan belongs and 2:286 reads *isru* rather than *isran*.
/// `scripts/build-quran.py` takes the Arabic from Tanzil instead; this makes
/// sure a regenerated dataset never reintroduces them.
#[test]
fn quran_text_uses_standard_marks_only() {
    let mut ctx = ctx!();

    let json = ctx.execute_json("Q:1;Q:2;Q:112;Q:2:286");
    let value: Value = serde_json::from_str(&json).unwrap();
    let results = value["results"].as_array().unwrap();
    assert!(!results.is_empty());

    for record in results {
        let ar = record["ar"].as_str().unwrap();
        for bad in ['\u{0656}', '\u{0657}', '\u{065E}'] {
            assert!(
                !ar.contains(bad),
                "U+{:04X} in {}: {ar}",
                bad as u32,
                record["surah"]
            );
        }
    }

    // The reported case, spelled out: reh + fathatan, not reh + inverted damma.
    let json = ctx.execute_json("Q:2:286");
    let value: Value = serde_json::from_str(&json).unwrap();
    let ar = value["results"][0]["ar"].as_str().unwrap();
    assert!(ar.contains('\u{064B}'), "2:286 should carry a fathatan");
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
