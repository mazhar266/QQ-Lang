// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

use qql::{Context, Numbering};
use serde_json::{json, Value};
use std::process::Command;

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_qql"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap()
}

fn results(args: &[&str]) -> Vec<Value> {
    let output = cli(args);
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["results"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn muslim_162_returns_every_narration_in_edition_order() {
    let traditional = results(&["m::162", "--numbering=traditional"]);
    let labels: Vec<_> = traditional
        .iter()
        .map(|r| r["sunnah_number"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["162a", "162b", "162c", "162"]);
    let book = results(&["m::316-319", "--numbering=book"]);
    for (a, b) in traditional.iter().zip(&book) {
        assert_eq!((&a["ar"], &a["en"]), (&b["ar"], &b["en"]));
        assert_eq!(a["number"], 162);
        assert_eq!(a["numbering"], "traditional");
    }
    let sunnah = results(&["m::162a", "--numbering=sunnah"]);
    assert_eq!(sunnah[0]["ar"], book[0]["ar"]);
    assert_eq!(sunnah[0]["number"], "162a");
    assert_eq!(sunnah[0]["numbering"], "sunnah");
    assert!(
        String::from_utf8(cli(&["m::162a", "--numbering=sunnah", "--cite"]).stdout)
            .unwrap()
            .ends_with(" — Sahih Muslim 162a\n")
    );
}

#[test]
fn all_six_collections_agree_across_the_three_schemes() {
    let mut ctx = Context::new(concat!(env!("CARGO_MANIFEST_DIR"), "/sources"));
    for (code, label, base, book, chapter, item) in [
        ("B", "402b", 402, 398, 8, 54),
        ("M", "162a", 162, 316, 1, 316),
        ("AD", "907b", 907, 908, 2, 518),
        ("T", "815b", 815, 816, 9, 8),
        ("N", "125b", 125, 126, 1, 126),
        ("IM", "3930b", 3930, 3667, 36, 5),
    ] {
        let target = ctx
            .execute(&format!("{code}:{chapter}:{item}"))
            .unwrap()
            .remove(0);
        for (scheme, id) in [
            (Numbering::Book, book.to_string()),
            (Numbering::Traditional, base.to_string()),
            (Numbering::Sunnah, label.to_owned()),
        ] {
            ctx.set_numbering(Some(scheme));
            let hits = ctx.execute(&format!("{code}::{id}")).unwrap();
            assert!(
                hits.iter().any(|r| r.ar == target.ar && r.en == target.en),
                "{code} {id}"
            );
            if scheme != Numbering::Traditional {
                assert_eq!(hits.len(), 1);
            }
        }
    }
}

#[test]
fn special_chapters_and_compound_printed_references() {
    for (label, book, chapter) in [
        ("N::3857", "N::3866", json!("35b")),
        ("IM::1", "IM::4080", json!(0)),
    ] {
        let exact = results(&[label, "--numbering=sunnah"]);
        let position = results(&[book, "--numbering=book"]);
        assert_eq!(exact[0]["chapter"], chapter);
        assert_eq!(exact[0]["ar"], position[0]["ar"]);
    }
    assert_eq!(results(&["M::7459", "--numbering=book"])[0]["chapter"], 0);
    let shared = results(&["B::6173-6175", "--numbering=sunnah"]);
    assert_eq!(shared.len(), 1);
    assert_eq!(
        shared[0]["ar"],
        results(&["B::6175", "--numbering=sunnah"])[0]["ar"]
    );
}

#[test]
fn lettered_ranges_keep_order_and_numeric_sources_ignore_the_option() {
    let mixed = results(&["m::162c,162a-162b,162c;Q::8", "--numbering=sunnah"]);
    assert_eq!(mixed.len(), 4);
    assert_eq!(mixed[0]["number"], "162c");
    assert_eq!(mixed[1]["number"], "162a");
    assert_eq!(mixed[2]["number"], "162b");
    assert_eq!(mixed[3], results(&["Q::8"])[0]);
    for query in ["Q:26:193", "Q::8", "MA::1", "B:1:1"] {
        let normal = results(&[query]);
        for flag in [
            "--numbering=book",
            "--numbering=traditional",
            "--numbering=sunnah",
        ] {
            assert_eq!(results(&[query, flag]), normal);
        }
    }
    let wide = results(&["M::1211z-1211aa", "--numbering=sunnah"]);
    assert_eq!(wide.last().unwrap()["number"], "1211aa");
    for (query, flag) in [
        ("m::162a", "--numbering=book"),
        ("m::162a", "--numbering=traditional"),
        ("m::162zz", "--numbering=sunnah"),
        ("m::162b-162a", "--numbering=sunnah"),
        ("q::1a", "--numbering=sunnah"),
        ("m::1-4294967295", "--numbering=book"),
    ] {
        assert!(!cli(&[query, flag]).status.success(), "{query} {flag}");
    }
    assert!(!cli(&["m::162a"]).status.success());
    assert!(!cli(&["m::162", "--numbering=unknown"]).status.success());
}

#[test]
fn default_bukhari_citations_cover_shared_and_reordered_records() {
    let refs = results(&["B::3633,3634"]);
    assert!(refs[0]["en"].as_str().unwrap().contains("buckets of water"));
    assert!(refs[1]["en"].as_str().unwrap().contains("Gabriel came"));
    for label in ["6173", "6174", "6175"] {
        let row = results(&[&format!("B::{label}")]).remove(0);
        assert_eq!(row["ar"], results(&["B:78:199"])[0]["ar"]);
        assert_eq!(row["number"], label.parse::<u32>().unwrap());
    }
    let shared = results(&["B::6174,6173-6175;B::6175"]);
    assert_eq!(shared.len(), 2);
    assert_eq!(shared[0]["number"], 6174);
    assert_eq!(shared[1]["number"], 6175);
    let variants = results(&["B::402", "--numbering=traditional"]);
    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0]["sunnah_number"], "402");
    assert_eq!(variants[1]["sunnah_number"], "402b");
    assert_eq!(results(&["B::402"])[0]["ar"], variants[0]["ar"]);
}
