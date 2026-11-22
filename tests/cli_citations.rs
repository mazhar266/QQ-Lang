// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

use std::process::{Command, Output};

fn qql(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qql"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap()
}

fn success(args: &[&str]) -> String {
    let output = qql(args);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn formats_the_resolved_verse_in_each_style() {
    let en = "The Trustworthy Spirit has brought it down";
    for (option, expected) in [
        ("--cite", format!("{en} — Quran 26:193")),
        ("--cite=apa", format!("“{en}” (Quran, n.d., 26:193).")),
        ("--cite=chicago", format!("“{en}” (Quran 26:193).")),
        (
            "--cite=arabic",
            "﴿نَزَلَ بِهِ ٱلرُّوحُ ٱلْأَمِينُ﴾ [الشعراء ٢٦:١٩٣]".into(),
        ),
        ("--cite=markdown", format!("> {en}\n\n— Quran 26:193")),
    ] {
        assert_eq!(success(&["26:193", option]), format!("{expected}\n"));
    }
}

#[test]
fn cites_each_match_in_order_and_uses_resolved_coordinates() {
    let text = success(&["--cite", "26:194,193-194;Q::8;B::6403"]);
    let refs: Vec<_> = text
        .lines()
        .filter_map(|s| s.rsplit_once(" — "))
        .map(|(_, r)| r)
        .collect();
    assert_eq!(&refs[..3], ["Quran 26:194", "Quran 26:193", "Quran 2:1"]);
    assert_eq!(refs.len(), 4);
    assert!(refs[3].ends_with(" 6403"));
    assert_eq!(success(&["--cite", "Q:26:\"no such phrase\""]), "");
    assert_eq!(
        success(&["--data", "tests/fixtures/custom", "X:1:2", "--cite"]),
        "Praise be to Allah — Poems 1:2\n"
    );
}

#[test]
fn citation_flags_preserve_query_errors_and_parse_mode() {
    let output = qql(&["26:999", "--cite"]);
    assert!(!output.status.success());
    assert_eq!(output.stdout, qql(&["26:999"]).stdout);
    assert_eq!(
        success(&["--parse", "26:193", "--cite"]),
        success(&["--parse", "26:193"])
    );
    for option in ["--cite=", "--cite=unknown"] {
        let output = qql(&["26:193", option]);
        assert!(!output.status.success());
        assert!(String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown citation style"));
    }
}
