// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Builds the tantivy indexes that `?"term"` searches.
//!
//! ```bash
//! cargo run --features fulltext --bin qql-index
//! cargo run --features fulltext --bin qql-index -- --source Q
//! ```

use std::process::ExitCode;

const USAGE: &str = "\
qql-index — build the full-text indexes for ?\"term\" search

USAGE:
    qql-index [--data <DIR>] [--source <CODE>]...

Writes <DIR>/fulltext/<CODE>/. Without --source, every source with data.
";

fn main() -> ExitCode {
    let mut data = String::from("./sources");
    let mut codes: Vec<String> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--data" => match args.next() {
                Some(dir) => data = dir,
                None => {
                    eprintln!("error: --data needs a directory");
                    return ExitCode::FAILURE;
                }
            },
            "--source" => match args.next() {
                Some(code) => codes.push(code),
                None => {
                    eprintln!("error: --source needs a code");
                    return ExitCode::FAILURE;
                }
            },
            other => {
                eprintln!("error: unexpected argument '{other}'\n\n{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }

    let mut ctx = qql::Context::new(&data);
    if let Err(e) = ctx.load_manifest() {
        eprintln!("warning: {e}");
    }

    let wanted: Vec<String> = if codes.is_empty() {
        ctx.sources().iter().map(|c| c.to_string()).collect()
    } else {
        codes.iter().map(|c| c.to_ascii_uppercase()).collect()
    };

    let mut failures = 0;
    for code in wanted {
        match qql::fulltext::build(&mut ctx, &code) {
            Ok(report) => println!(
                "{code:>3}: {:6} documents  {}",
                report.documents,
                report.path.display()
            ),
            Err(e) => {
                eprintln!("{code:>3}: {e}");
                failures += 1;
            }
        }
    }

    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
