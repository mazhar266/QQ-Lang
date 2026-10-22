// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! The Bible (`BI`, `OT`, `NT`) and the Jewish Torah (`TO`), against the real
//! data in `sources/bible/` and `sources/torah/`.

use qql::{Context, Record};
use serde_json::json;

const DATA: &str = "sources";

fn context() -> Option<Context> {
    let data = std::path::Path::new(DATA);
    (data.join("bible/books.json").exists() && data.join("torah/books.json").exists())
        .then(|| Context::new(DATA))
}

macro_rules! ctx {
    () => {
        match context() {
            Some(ctx) => ctx,
            None => {
                eprintln!("skipping: sources/bible or sources/torah missing");
                return;
            }
        }
    };
}

fn one(ctx: &mut Context, query: &str) -> Record {
    let records = ctx
        .execute(query)
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    assert_eq!(records.len(), 1, "{query}");
    records.into_iter().next().unwrap()
}

/// `(book, chapter, verse)` of a record.
fn place(record: &Record) -> (String, u64, u64) {
    (
        record.extra["book"].as_str().unwrap().to_string(),
        record.extra["chapter"].as_u64().unwrap(),
        record.extra["verse"].as_u64().unwrap(),
    )
}

fn at(book: &str, chapter: u64, verse: u64) -> (String, u64, u64) {
    (book.to_string(), chapter, verse)
}

fn text<'a>(record: &'a Record, key: &str) -> &'a str {
    record.extra[key].as_str().unwrap()
}

#[test]
fn resolves_a_verse_by_book_chapter_and_verse() {
    let mut ctx = ctx!();

    let verse = one(&mut ctx, "NT:JHN:3:16");
    assert_eq!(verse.source, "NT");
    assert_eq!(verse.collection, "New Testament");
    assert_eq!(place(&verse), at("JHN", 3, 16));
    assert_eq!(verse.extra["testament"], "NT");
    assert_eq!(verse.extra["book_name_en"], "John");
    assert!(!text(&verse, "book_name_ar").is_empty());
    // Van Dyck's Arabic, the KJV as `en`, the WEB alongside.
    assert!(!verse.ar.is_empty());
    assert!(
        verse.en.contains("For God so loved the world"),
        "{}",
        verse.en
    );
    assert!(text(&verse, "en_web").contains("For God so loved the world"));
    assert!(verse.emlaei.is_empty());
}

/// Every way of naming a verse reaches the same text.
#[test]
fn every_form_of_address_reaches_the_same_verse() {
    let mut ctx = ctx!();

    let direct = one(&mut ctx, "NT:JHN:3:16");
    for query in [
        "BI:NT:JHN:3:16",
        "BI:JHN:3:16",
        "nt:john:3:16",
        "NT:JOH:3:16",
    ] {
        let record = one(&mut ctx, query);
        assert_eq!(place(&record), place(&direct), "{query}");
        assert_eq!(record.ar, direct.ar, "{query}");
        assert_eq!(record.en, direct.en, "{query}");
    }
    // Only `source` tells them apart.
    assert_eq!(one(&mut ctx, "BI:NT:JHN:3:16").source, "BI");

    // Book names that start with digits.
    assert_eq!(place(&one(&mut ctx, "NT:1JN:4:8")), at("1JN", 4, 8));
    assert_eq!(place(&one(&mut ctx, "OT:1SAMUEL:17:45")), at("1SA", 17, 45));
}

#[test]
fn books_chapters_and_selectors_work_inside_a_book() {
    let mut ctx = ctx!();

    assert_eq!(ctx.execute("OT:GEN:1:1-5").unwrap().len(), 5);
    assert_eq!(ctx.execute("OT:GEN:1").unwrap().len(), 31);
    assert_eq!(ctx.execute("OT:GEN").unwrap().len(), 1533);

    // Groups share the book; the book carries over `;` with the source.
    let records = ctx.execute("OT:GEN:1:1,2:3;3:1").unwrap();
    let places: Vec<_> = records.iter().map(place).collect();
    assert_eq!(places, [at("GEN", 1, 1), at("GEN", 2, 3), at("GEN", 3, 1)]);

    // Order is preserved and never sorted, as everywhere else.
    let records = ctx.execute("NT:JHN:3:16,1-2").unwrap();
    let verses: Vec<_> = records.iter().map(|r| place(r).2).collect();
    assert_eq!(verses, [16, 1, 2]);
}

/// `::N` counts through whatever precedes it: a book, a testament, the Bible.
#[test]
fn book_wide_numbering_counts_through_its_span() {
    let mut ctx = ctx!();

    let verse = one(&mut ctx, "OT:GEN::32");
    assert_eq!(place(&verse), at("GEN", 2, 1));
    assert_eq!(verse.extra["number"], 32);
    assert_eq!(verse.extra["numbering"], "book");

    assert_eq!(place(&one(&mut ctx, "OT::1")), at("GEN", 1, 1));
    assert_eq!(place(&one(&mut ctx, "OT::23145")), at("MAL", 4, 6));
    assert_eq!(place(&one(&mut ctx, "NT::1")), at("MAT", 1, 1));
    assert_eq!(place(&one(&mut ctx, "NT::7959")), at("REV", 22, 21));
    assert_eq!(place(&one(&mut ctx, "BI::23146")), at("MAT", 1, 1));
    assert_eq!(place(&one(&mut ctx, "BI::31104")), at("REV", 22, 21));
    assert_eq!(place(&one(&mut ctx, "BI:NT::1")), at("MAT", 1, 1));
}

#[test]
fn bad_references_are_refused_with_a_reason() {
    let mut ctx = ctx!();

    for (query, reason) in [
        ("BI:OT:JHN:1:1", "not in the Old Testament"),
        ("OT:JHN:1:1", "not in the Old Testament"),
        ("NT:GEN:1:1", "not in the New Testament"),
        ("BI:XX:GEN:1:1", "not a testament"),
        ("OT:XYZ:1:1", "no such book"),
        ("OT:GEN:51", "chapters 1..=50"),
        ("OT:GEN:1:32", "1..=31"),
        ("OT:5:1", "needs a book"),
        ("BI:OT:1:1", "needs a book"),
        ("OT:OT:GEN:1:1", "too many names"),
        ("OT::23146", "1..=23145"),
        ("NT::7960", "1..=7959"),
        ("OT:GEN:1:1-4294967295", "1..=31"),
    ] {
        let error = ctx.execute(query).expect_err(query);
        assert_eq!(error.code(), "QQL_REFERENCE_NOT_FOUND", "{query}");
        assert!(error.to_string().contains(reason), "{query}: {error}");
    }
}

/// `books.json` drives book-wide numbering. Walk every chapter of the Bible
/// against the files, so the catalog and the text cannot disagree.
#[test]
fn the_catalog_matches_every_chapter() {
    let mut ctx = ctx!();

    let books = qql_books();
    assert_eq!(books.len(), 66);
    assert_eq!((books[0].as_str(), books[65].as_str()), ("GEN", "REV"));

    let mut global = 0u64;
    let mut chapters = 0;
    for book in &books {
        for chapter in 1u64.. {
            let Ok(verses) = ctx.execute(&format!("BI:{book}:{chapter}")) else {
                break;
            };
            chapters += 1;
            global += verses.len() as u64;
            let last = one(&mut ctx, &format!("BI::{global}"));
            assert_eq!(
                place(&last),
                at(book, chapter, verses.len() as u64),
                "at BI::{global}"
            );
        }
    }
    assert_eq!(chapters, 1189);
    assert_eq!(global, 31104);
}

/// The 66 codes, in order, read from the records themselves: each book's
/// first verse, by book-wide number.
fn qql_books() -> Vec<String> {
    let mut ctx = Context::new(DATA);
    let mut books = Vec::new();
    let mut global = 1u64;
    while let Ok(records) = ctx.execute(&format!("BI::{global}")) {
        let book = place(&records[0]).0;
        let count = ctx.execute(&format!("BI:{book}")).unwrap().len() as u64;
        books.push(book);
        global += count;
    }
    books
}

/// Where the three texts differ, each verse is carried with what each has.
#[test]
fn verses_only_one_text_numbers_are_carried_with_the_others_empty() {
    let mut ctx = ctx!();

    for query in ["NT:3JN:1:15", "NT:1TI:6:22"] {
        let verse = one(&mut ctx, query);
        assert!(!verse.ar.is_empty(), "{query}");
        assert_eq!(verse.en, "", "{query}");
        assert_eq!(text(&verse, "en_web"), "", "{query}");
    }

    // The WEB omits a few verses the KJV keeps…
    let verse = one(&mut ctx, "NT:LUK:17:36");
    assert!(!verse.en.is_empty());
    assert_eq!(text(&verse, "en_web"), "");

    // …and places Romans' doxology after 14:23; it is carried at 16:25.
    let verse = one(&mut ctx, "NT:ROM:16:25");
    assert!(text(&verse, "en_web").starts_with("Now to him who is able"));
    assert_eq!(ctx.execute("NT:ROM:14").unwrap().len(), 23);
}

/// The Bible's Psalms are numbered the English way; `PS` is the Hebrew edition,
/// and its `bible_ref` is a query for the same passage here.
#[test]
fn the_bibles_psalms_are_the_english_edition() {
    let mut ctx = ctx!();

    assert_eq!(ctx.execute("OT:PSA:3").unwrap().len(), 8);
    assert_eq!(ctx.execute("PS:3").unwrap().len(), 9);

    let english = one(&mut ctx, "OT:PSA:3:1");
    assert!(english.en.starts_with("A Psalm of David, when he fled"));
    assert!(english.en.contains("LORD, how are they increased"));

    for query in ["PS:3:1", "PS:3:2"] {
        let hebrew = one(&mut ctx, query);
        assert_eq!(hebrew.extra["bible_ref"], "OT:PSA:3:1", "{query}");
    }
    let joined = one(&mut ctx, "PS:13:6");
    assert_eq!(joined.extra["bible_ref"], "OT:PSA:13:5-6");
    assert_eq!(ctx.execute("OT:PSA:13:5-6").unwrap().len(), 2);
}

#[test]
fn exact_search_reaches_books_and_testaments() {
    let mut ctx = ctx!();

    let hits = ctx.execute(r#"NT:JHN:3:"born again""#).unwrap();
    let verses: Vec<_> = hits.iter().map(|r| place(r).2).collect();
    assert_eq!(verses, [3, 7]);

    // Unscoped, through the testament's book-wide numbering.
    let hits = ctx.execute(r#"NT:"Melchisedec""#).unwrap();
    assert!(hits.len() > 5);
    assert!(hits.iter().all(|r| r.extra["testament"] == "NT"));

    // A book with no chapter is the whole book, and finds what its chapters
    // find between them — in either edition, and through `BI` with the
    // testament named on the way.
    for (book, through) in [
        ("OT:GEN", "OT:GEN"),
        ("TO:GEN", "TO:GEN"),
        ("OT:GEN", "BI:OT:GEN"),
    ] {
        let by_chapter: Vec<_> = (1..=50)
            .flat_map(|c| ctx.execute(&format!(r#"{book}:{c}:"Pharaoh""#)).unwrap())
            .map(|r| place(&r))
            .collect();
        let whole = ctx.execute(&format!(r#"{through}:"Pharaoh""#)).unwrap();
        assert!(!by_chapter.is_empty(), "{through}");
        assert_eq!(
            whole.iter().map(place).collect::<Vec<_>>(),
            by_chapter,
            "{through}"
        );
    }

    // A testament through `BI` is the testament's own code.
    let through_bible: Vec<_> = ctx
        .execute(r#"BI:NT:"Melchisedec""#)
        .unwrap()
        .iter()
        .map(place)
        .collect();
    assert_eq!(through_bible, hits.iter().map(place).collect::<Vec<_>>());

    // Arabic without its marks, as anyone would type it.
    let hits = ctx.execute(r#"NT:JHN:3:"احب الله العالم""#).unwrap();
    assert_eq!(hits.iter().map(|r| place(r).2).collect::<Vec<_>>(), [16]);
}

/// The Jewish Torah numbers verses the Hebrew way; where that differs, each
/// record names the English verse.
#[test]
fn the_torah_follows_the_hebrew_numbering() {
    let mut ctx = ctx!();

    assert_eq!(ctx.execute("TO:GEN:32").unwrap().len(), 33);
    assert_eq!(ctx.execute("OT:GEN:32").unwrap().len(), 32);

    let verse = one(&mut ctx, "TO:GEN:32:1");
    assert_eq!(verse.source, "TO");
    assert_eq!(verse.collection, "Torah (Tawrat)");
    assert_eq!(verse.extra["bible_ref"], "OT:GEN:31:55");
    assert!(verse.en.starts_with("And early in the morning Laban"));
    assert!(text(&verse, "he")
        .chars()
        .any(|c| ('\u{05D0}'..='\u{05EA}').contains(&c)));

    // The same verse in the Bible: same Arabic, same KJV.
    let bible = one(&mut ctx, "OT:GEN:31:55");
    assert_eq!(verse.ar, bible.ar);
    assert_eq!(text(&verse, "en_kjv"), bible.en);

    assert_eq!(
        one(&mut ctx, "TO:EXO:7:26").extra["bible_ref"],
        "OT:EXO:8:1"
    );
    assert_eq!(
        one(&mut ctx, "TO:EXO:21:37").extra["bible_ref"],
        "OT:EXO:22:1"
    );
}

/// English Numbers 26:1 is two Hebrew verses; every translation is divided at
/// the same words.
#[test]
fn one_english_verse_is_two_hebrew_ones() {
    let mut ctx = ctx!();

    let first = one(&mut ctx, "TO:NUM:25:19");
    assert_eq!(first.en, "And it came to pass after the plague,");
    assert_eq!(
        text(&first, "en_kjv"),
        "And it came to pass after the plague,"
    );
    assert_eq!(text(&first, "en_web"), "After the plague,");
    let second = one(&mut ctx, "TO:NUM:26:1");
    assert!(second.en.starts_with("that the LORD spoke unto Moses"));
    assert!(text(&second, "en_web").starts_with("Yahweh spoke to Moses"));
    for verse in [&first, &second] {
        assert_eq!(verse.extra["bible_ref"], "OT:NUM:26:1");
    }
}

/// JPS numbers the Ten Commandments by the lower accents; the Hebrew text here
/// keeps each commandment a verse of its own, and so does `TO`.
#[test]
fn the_ten_commandments_keep_a_verse_each() {
    let mut ctx = ctx!();

    assert_eq!(ctx.execute("TO:EXO:20").unwrap().len(), 26);
    let records = ctx.execute("TO:EXO:20:13-16").unwrap();
    let texts: Vec<&str> = records.iter().map(|r| r.en.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Thou shalt not murder.",
            "Thou shalt not commit adultery.",
            "Thou shalt not steal.",
            "Thou shalt not bear false witness against thy neighbour.",
        ]
    );
}

#[test]
fn the_torah_takes_hebrew_book_names_and_nothing_outside_it() {
    let mut ctx = ctx!();

    let named = one(&mut ctx, "TO:BERESHIT:1:1");
    assert_eq!(place(&named), at("GEN", 1, 1));
    assert_eq!(named.extra["book_name_he"], "Bereshit");
    assert_eq!(place(&one(&mut ctx, "TO::5853")), at("DEU", 34, 12));

    for query in ["TO:JHN:1:1", "TO:PSA:23:1", "TO:OT:GEN:1:1", "TO::5854"] {
        let error = ctx.execute(query).expect_err(query);
        assert_eq!(error.code(), "QQL_REFERENCE_NOT_FOUND", "{query}");
    }
}

#[test]
fn the_torah_catalog_matches_every_chapter() {
    let mut ctx = ctx!();

    let mut global = 0u64;
    for book in ["GEN", "EXO", "LEV", "NUM", "DEU"] {
        for chapter in 1u64.. {
            let Ok(verses) = ctx.execute(&format!("TO:{book}:{chapter}")) else {
                break;
            };
            global += verses.len() as u64;
            let last = one(&mut ctx, &format!("TO::{global}"));
            assert_eq!(place(&last), at(book, chapter, verses.len() as u64));
        }
    }
    assert_eq!(global, 5853);
}

#[test]
fn the_torah_searches_jps_and_the_arabic() {
    let mut ctx = ctx!();

    // JPS reads "as long as men come to Shiloh" where the KJV reads "until
    // Shiloh come" — the Jewish translation is the one searched.
    let hits = ctx.execute(r#"TO:GEN:49:"men come to Shiloh""#).unwrap();
    assert_eq!(
        hits.iter().map(place).collect::<Vec<_>>(),
        [at("GEN", 49, 10)]
    );
    assert!(ctx
        .execute(r#"OT:GEN:49:"men come to Shiloh""#)
        .unwrap()
        .is_empty());
}

#[test]
fn records_serialize_with_their_fields() {
    let mut ctx = ctx!();
    let value = ctx.execute_value("TO:GEN:32:1");
    let result = &value["results"][0];
    assert_eq!(result["source"], "TO");
    assert_eq!(result["book"], "GEN");
    assert_eq!(result["bible_ref"], json!("OT:GEN:31:55"));
    for key in [
        "he",
        "ar",
        "en",
        "en_kjv",
        "en_web",
        "book_name_he",
        "book_name_ar",
    ] {
        assert!(result[key].is_string(), "{key}");
    }
}

/// Each Bible verse carries its original language: Hebrew (with its Aramaic
/// passages) for the Old Testament, Greek for the New.
#[test]
fn every_verse_carries_its_original_language() {
    let mut ctx = ctx!();

    // The Hebrew is fully pointed; compare its letters only.
    let letters = |text: &str| -> String {
        text.chars()
            .filter(|c| ('\u{05D0}'..='\u{05EA}').contains(c) || *c == ' ')
            .collect()
    };

    let genesis = one(&mut ctx, "OT:GEN:1:1");
    assert!(letters(text(&genesis, "he")).starts_with("בראשית ברא אלהים"));
    assert!(!genesis.extra.contains_key("grc"));

    let john = one(&mut ctx, "NT:JHN:1:1");
    assert!(text(&john, "grc").starts_with("Ἐν ἀρχῇ ἦν ὁ λόγος"));
    assert!(!john.extra.contains_key("he"));

    // Daniel 3 is in Aramaic: "Nebuchadnezzar the king".
    let daniel = one(&mut ctx, "BI:OT:DAN:3:1");
    assert!(letters(text(&daniel, "he")).starts_with("נבוכדנצר מלכא"));

    // The Textus Receptus has the verses the KJV has.
    assert!(!text(&one(&mut ctx, "NT:ACT:8:37"), "grc").is_empty());
}

/// The Hebrew is numbered differently, so it is placed on the English verses
/// through the same map that numbers the Jewish Torah and Psalms.
#[test]
fn the_hebrew_sits_on_the_english_verses() {
    let mut ctx = ctx!();

    // English Genesis 31:55 is Hebrew 32:1, which is TO:GEN:32:1.
    let english = one(&mut ctx, "OT:GEN:31:55");
    let torah = one(&mut ctx, "TO:GEN:32:1");
    assert_eq!(text(&english, "he"), text(&torah, "he"));

    // English Psalm 3:1 is Hebrew 3:1 (the title) and 3:2.
    let english = one(&mut ctx, "OT:PSA:3:1");
    let hebrew: Vec<String> = ctx
        .execute("PS:3:1-2")
        .unwrap()
        .iter()
        .map(|r| text(r, "he").to_string())
        .collect();
    assert_eq!(text(&english, "he"), hebrew.join(" "));

    // English Numbers 26:1 is Hebrew 25:19 and 26:1.
    let english = one(&mut ctx, "OT:NUM:26:1");
    let hebrew: Vec<String> = ctx
        .execute("TO:NUM:25:19;TO:NUM:26:1")
        .unwrap()
        .iter()
        .map(|r| text(r, "he").to_string())
        .collect();
    assert_eq!(text(&english, "he"), hebrew.join(" "));

    // One Hebrew verse over two English ones stands on the first. Hebrew
    // Malachi 3:19-24 is English 4:1-6, a chapter of its own.
    assert_eq!(text(&one(&mut ctx, "OT:PSA:13:6"), "he"), "");
    assert_eq!(
        text(&one(&mut ctx, "OT:PSA:13:5"), "he"),
        text(&one(&mut ctx, "PS:13:6"), "he")
    );
    assert!(!text(&one(&mut ctx, "OT:MAL:4:6"), "he").is_empty());

    // Nehemiah 7:68 is not in the Masoretic text; 7:73 is Hebrew 7:72.
    assert_eq!(text(&one(&mut ctx, "OT:NEH:7:68"), "he"), "");
    assert!(!text(&one(&mut ctx, "OT:NEH:7:73"), "he").is_empty());
}
