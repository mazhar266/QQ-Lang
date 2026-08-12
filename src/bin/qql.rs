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
    --sources      List registered source codes
    -h, --help     Show this help
    -V, --version  Show version

EXAMPLES:
    qql \"Q:2:255\"
    qql \"Q:2:1-5,255;Q:1;\"
    qql --data ./sources \"B:1:1-3\"
    qql --source my-sources.json \"X:1:1\"
";

fn main() -> ExitCode {
    let mut data = String::from("./sources");
    let mut parse_only = false;
    let mut compact = false;
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
                "references": parsed.references.iter().map(|r| serde_json::json!({
                    "source": r.source,
                    "primary": r.primary,
                    "all": r.selects_all(),
                    "ranges": r.ranges.iter()
                        .map(|range| [range.from, range.to])
                        .collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
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
        match failed {
            Some(error) => error,
            None => ctx.execute_value(&query),
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
