// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! QQL command-line tool.
//!
//! ```text
//! qql "Q:2:255"
//! qql --data ./sources "Q:1;Q:2:255"
//! qql --parse "Q:2:1-5,255;Q:1;"
//! ```

use std::process::ExitCode;

#[path = "qql/citation.rs"]
mod citation;
use citation::Style;

const USAGE: &str = "\
qql — Quran Query Language

USAGE:
    qql [OPTIONS] <QUERY>

OPTIONS:
    --data <DIR>   Data directory (default: ./sources)
    --source <F>   Register sources from a manifest, relative to --data.
                   Repeatable. <DIR>/qql-sources.json loads automatically.
    --parse        Print the parsed query instead of resolving it
    --compact      Emit compact JSON instead of pretty-printed
    --cite[=STYLE] Print each resolved passage as a quotation and reference.
                   Default: plain. Styles: apa, chicago, arabic, markdown.
    --sources      List registered source codes
    -h, --help     Show this help
    -V, --version  Show version

EXAMPLES:
    qql 26:193 --cite
    qql 26:193 --cite=arabic
    qql \"Q:2:255\"
    qql \"Q:2:1-5,255;Q:1;\"
    qql --data ./sources \"B:1:1-3\"
    qql --source my-sources.json \"X:1:1\"
";

fn main() -> ExitCode {
    let mut data = String::from("./sources");
    let mut parse_only = false;
    let mut compact = false;
    let mut citation = None;
    let mut manifests: Vec<String> = Vec::new();
    let mut query: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("qql {}", qql::VERSION);
                return ExitCode::SUCCESS;
            }
            "--sources" => {
                let mut ctx = qql::Context::new(&data);
                // Surface user-defined sources too, which otherwise appear
                // only once a query triggers the lazy load.
                if let Err(e) = ctx.load_manifest() {
                    eprintln!("warning: {e}");
                }
                for manifest in &manifests {
                    if let Err(e) = ctx.add_sources_from(manifest) {
                        eprintln!("warning: {e}");
                    }
                }
                println!("{}", ctx.sources().join(" "));
                return ExitCode::SUCCESS;
            }
            "--parse" => parse_only = true,
            "--compact" => compact = true,
            "--cite" => citation = Some(Style::Plain),
            other if other.starts_with("--cite=") => {
                let name = &other["--cite=".len()..];
                let Some(style) = Style::named(name) else {
                    eprintln!("error: unknown citation style '{name}' (use --cite, or --cite=apa|chicago|arabic|markdown)");
                    return ExitCode::FAILURE;
                };
                citation = Some(style);
            }
            "--source" => match args.next() {
                Some(path) => manifests.push(path),
                None => {
                    eprintln!("error: --source needs a manifest path\n\n{USAGE}");
                    return ExitCode::FAILURE;
                }
            },
            "--data" => match args.next() {
                Some(dir) => data = dir,
                None => {
                    eprintln!("error: --data needs a directory\n\n{USAGE}");
                    return ExitCode::FAILURE;
                }
            },
            other if other.starts_with('-') => {
                eprintln!("error: unknown option '{other}'\n\n{USAGE}");
                return ExitCode::FAILURE;
            }
            other => query = Some(other.to_string()),
        }
    }

    let Some(query) = query else {
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };

    let value = if parse_only {
        match qql::parse(&query) {
            Ok(parsed) => serde_json::json!({
                "ok": true,
                "query": query,
                "references": parsed.references.iter().map(|r| {
                    let mut reference = serde_json::json!({
                        "source": r.source,
                        "primary": r.primary,
                        "all": r.selects_all(),
                        "ranges": r.ranges.iter()
                            .map(|range| [range.from, range.to])
                            .collect::<Vec<_>>(),
                        "search": r.search,
                    });
                    // Only a query that names a book has a path, so every
                    // other query prints exactly what it always did.
                    if !r.path.is_empty() {
                        reference["path"] =
                            r.path.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().into();
                    }
                    reference
                }).collect::<Vec<_>>(),
            }),
            Err(e) => e.to_json(&query),
        }
    } else {
        let mut ctx = qql::Context::new(&data);
        let mut failed = None;
        for manifest in &manifests {
            if let Err(e) = ctx.add_sources_from(manifest) {
                failed = Some(e.to_json(&query));
                break;
            }
        }
        match (failed, citation) {
            (Some(error), _) => error,
            (None, Some(style)) => match ctx.execute(&query) {
                Ok(records) => {
                    for (i, record) in records.iter().enumerate() {
                        if i > 0 {
                            println!();
                        }
                        let source = ctx
                            .registry_mut()
                            .get(&record.source)
                            .expect("resolved record belongs to a registered source");
                        println!("{}", style.render(record, source));
                    }
                    return ExitCode::SUCCESS;
                }
                Err(e) => e.to_json(&query),
            },
            (None, None) => ctx.execute_value(&query),
        }
    };

    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let rendered = if compact {
        serde_json::to_string(&value)
    } else {
        serde_json::to_string_pretty(&value)
    };

    match rendered {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
