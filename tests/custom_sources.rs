// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! User-defined sources, driven by a JSON spec rather than Rust code.
//!
//! Fixtures live in `tests/fixtures/custom`, which doubles as the data
//! directory: `qql-sources.json` there is picked up automatically.

use qql::{Context, SourceSpec};
use serde_json::Value;
use std::collections::BTreeMap;

const DATA: &str = "tests/fixtures/custom";

fn context() -> Context {
    Context::new(DATA)
}

#[test]
fn a_manifest_in_the_data_directory_registers_automatically() {
    let mut ctx = context();

    // Not present until something triggers the lazy load.
    assert!(!ctx.sources().contains(&"X"));

    let records = ctx.execute("X:1:2").unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].source, "X");
    assert_eq!(records[0].collection, "Poems");
    assert_eq!(records[0].en, "Praise be to Allah");

    assert!(ctx.sources().contains(&"X"));
}

#[test]
fn one_file_per_primary_with_metadata_mapping() {
    let mut ctx = context();
    let records = ctx.execute("X:1").unwrap();

    assert_eq!(records.len(), 3);
    // `primary_key` renames the primary in the output.
    assert_eq!(records[0].extra["chapter"], 1);
    assert_eq!(records[0].extra["number"], 1);
    assert_eq!(records[0].extra["chapter_title"], "First Chapter");
    assert_eq!(records[0].extra["note"], "opening");
    // Item metadata is omitted where the field is absent, not emitted empty.
    assert!(!records[1].extra.contains_key("note"));
}

#[test]
fn one_file_with_chapters_selected_by_id() {
    let mut ctx = context();
    let records = ctx.execute("S:3").unwrap();

    assert_eq!(records.len(), 2);
    // Chapter 3 is stored *after* chapter 7 in the file — matched by id, not
    // by position, exactly like the Hisnul Muslim data.
    assert_eq!(records[0].extra["heading"], "Third");
    assert_eq!(records[0].extra["primary"], 3);
    assert_eq!(records[0].ar, "ثَلَاثَة");
    // A dotted path reaches a nested field.
    assert_eq!(records[0].en, "three");

    assert_eq!(ctx.execute("S:7").unwrap()[0].extra["heading"], "Seventh");
}

#[test]
fn aliases_from_the_spec_resolve() {
    let mut ctx = context();
    assert_eq!(
        ctx.execute("POEM:1:1").unwrap()[0].en,
        ctx.execute("X:1:1").unwrap()[0].en
    );
}

#[test]
fn ordering_and_dedup_apply_to_custom_sources_too() {
    let mut ctx = context();
    let numbers: Vec<_> = ctx
        .execute("X:1:3,1-2,2")
        .unwrap()
        .iter()
        .map(|r| r.extra["number"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, [3, 1, 2]);
}

#[test]
fn missing_chapters_and_items_are_semantic_errors() {
    let mut ctx = context();

    // Templated path: a missing file means a missing chapter.
    assert_eq!(
        ctx.execute("X:5:1").unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
    // Fixed path: a chapter id that is not in the file.
    assert_eq!(
        ctx.execute("S:99").unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
    // Out of range within a chapter.
    assert_eq!(
        ctx.execute("X:1:9").unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
}

#[test]
fn specs_can_be_registered_from_rust_without_a_manifest() {
    let mut ctx = context();
    ctx.register_spec(SourceSpec {
        code: "zz".into(),
        name: "Inline".into(),
        aliases: Vec::new(),
        path: "poems/{primary}.json".into(),
        chapters: None,
        chapter_id: None,
        items: "lines".into(),
        ar: "arabic".into(),
        en: "translation".into(),
        // Select by the item's own `n` field rather than by position.
        item_id: Some("n".into()),
        primary_key: None,
        metadata: BTreeMap::new(),
        container_metadata: BTreeMap::new(),
        flat: None,
    });

    let records = ctx.execute("ZZ:1:3").unwrap();
    assert_eq!(records[0].en, "Lord of the worlds");
    assert_eq!(records[0].collection, "Inline");
}

/// Sources are searched newest-first. Because the manifest loads on the first
/// query, overriding something it defines means loading it explicitly first —
/// otherwise the manifest lands last and wins.
#[test]
fn a_later_registration_shadows_an_earlier_code() {
    let mut ctx = context();
    ctx.load_manifest().unwrap();
    ctx.register_spec(SourceSpec {
        code: "X".into(),
        name: "Replacement".into(),
        aliases: Vec::new(),
        path: "poems/{primary}.json".into(),
        chapters: None,
        chapter_id: None,
        items: "lines".into(),
        ar: "arabic".into(),
        en: "translation".into(),
        item_id: None,
        primary_key: None,
        metadata: BTreeMap::new(),
        container_metadata: BTreeMap::new(),
        flat: None,
    });

    assert_eq!(ctx.execute("X:1:1").unwrap()[0].collection, "Replacement");
}

/// `X::4` — the collection numbered straight through, which a spec opts into
/// with a `flat` block.
#[test]
fn custom_sources_can_declare_book_wide_numbering() {
    let mut ctx = context();

    let records = ctx.execute("X::4").unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].en, "seven");
    assert_eq!(records[0].extra["number"], 4);
    assert_eq!(records[0].extra["numbering"], "book");

    // Order and dedup apply here too.
    let numbers: Vec<_> = ctx
        .execute("X::4,1-2,1")
        .unwrap()
        .iter()
        .map(|r| r.extra["number"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, [4, 1, 2]);

    assert_eq!(
        ctx.execute("X::99").unwrap_err().code(),
        "QQL_REFERENCE_NOT_FOUND"
    );
}

/// Without a `flat` block the error says what to do about it, rather than
/// falling back to something that might be the wrong text.
#[test]
fn a_source_without_flat_numbering_says_so() {
    let mut ctx = context();
    let error = ctx.execute("S::1").unwrap_err();

    assert_eq!(error.code(), "QQL_REFERENCE_NOT_FOUND");
    assert!(
        error.to_string().contains("no book-wide numbering"),
        "unhelpful message: {error}"
    );
}

#[test]
fn a_missing_manifest_is_not_an_error() {
    // The real data directory has no qql-sources.json.
    let mut ctx = Context::new("sources");
    assert!(ctx.load_manifest().is_ok());
}

#[test]
fn a_malformed_manifest_reports_rather_than_being_ignored() {
    let mut ctx = context();
    let error = ctx.add_sources_from("poems/1.json").unwrap_err();
    // A chapter file is not an array of specs.
    assert_eq!(error.code(), "QQL_INVALID_DATA_FILE");
}

#[test]
fn custom_sources_serialize_like_built_in_ones() {
    let mut ctx = context();
    let value: Value = serde_json::from_str(&ctx.execute_json("X:1:1")).unwrap();

    assert_eq!(value["ok"], true);
    assert_eq!(value["results"][0]["source"], "X");
    assert_eq!(value["results"][0]["collection"], "Poems");
    assert!(value["results"][0]["ar"].is_string());
    assert!(value["results"][0]["en"].is_string());
}
