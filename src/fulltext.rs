// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Ranked full-text search — the `?"term"` form, backed by tantivy.
//!
//! Behind the `fulltext` cargo feature, off by default. Tantivy is a large
//! dependency tree; the core crate keeps its two dependencies unless you ask
//! for this.
//!
//! # What it adds over `"term"`
//!
//! The built-in `"term"` search is a folded substring scan: exact, positional,
//! no index, always available. This is a real inverted index, so it brings
//! what a substring scan cannot:
//!
//! - **stemming** — `?"mercy"` finds *merciful*, which `"mercy"` does not;
//! - **ranking** by BM25, best first;
//! - **boolean and phrase syntax** inside the term: `?"prayer AND fasting"`,
//!   `?"\"the straight path\""`, `?"charity -wealth"`.
//!
//! It does not replace `"term"`, and the feature does not change what `"term"`
//! means. Two spellings, two behaviours, both explicit in the query.
//!
//! # Indexing
//!
//! One tantivy index per source, under `<data>/fulltext/<CODE>/`, built by the
//! `qql-index` binary. Documents carry `(primary, number)` — the same address
//! the vector index uses — so a hit resolves back through the ordinary
//! reference path and comes out shaped like every other record.
//!
//! Arabic is indexed **folded** (see [`crate::search::fold`]): the corpus is
//! fully diacritized, and an inverted index over raw diacritized tokens would
//! only ever match a query that reproduced every mark. English is indexed with
//! tantivy's `en_stem` tokenizer.

use crate::context::Context;
use crate::error::Error;
use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, FAST, INDEXED, STORED,
};
use tantivy::{doc, Index, IndexReader, TantivyDocument, Term};

/// Results returned when a full-text query gives no cap of its own.
pub const DEFAULT_LIMIT: u32 = 20;

/// How many consecutive missing primaries end a build.
///
/// Chapters are numbered from 1 with no gaps in any shipped source, but a
/// source is free to have one; stopping at the first miss would silently
/// truncate its index.
const GAP_TOLERANCE: u32 = 5;

/// What a build wrote.
#[derive(Debug)]
pub struct Report {
    /// Documents indexed.
    pub documents: usize,
    /// Where the index went.
    pub path: PathBuf,
}

/// The fields every QQL index carries.
struct Fields {
    primary: Field,
    number: Field,
    arabic: Field,
    english: Field,
}

fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();

    // Stored so a hit can be resolved; fast so a scope can filter cheaply.
    let primary = builder.add_u64_field("primary", INDEXED | STORED | FAST);
    let number = builder.add_u64_field("number", INDEXED | STORED | FAST);

    let arabic = builder.add_text_field(
        "ar",
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("default")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ),
    );
    let english = builder.add_text_field(
        "en",
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        ),
    );

    let schema = builder.build();
    (
        schema,
        Fields {
            primary,
            number,
            arabic,
            english,
        },
    )
}

fn index_path(data: &Path, code: &str) -> PathBuf {
    data.join("fulltext").join(code)
}

fn wrap(e: impl std::fmt::Display, path: &Path) -> Error {
    Error::InvalidDataFile {
        path: path.display().to_string(),
        detail: e.to_string(),
    }
}

/// Build the index for one source, replacing whatever was there.
///
/// Records are enumerated through the ordinary query path — `CODE:1`, `CODE:2`
/// and so on — so this needs no privileged access and cannot disagree with the
/// resolver about numbering. Within a primary, the *n*-th record is item *n*,
/// which is exactly what `CODE:primary:n` addresses.
pub fn build(ctx: &mut Context, code: &str) -> Result<Report, Error> {
    let path = index_path(ctx.data_dir(), code);
    let (schema, fields) = schema();

    if path.exists() {
        std::fs::remove_dir_all(&path).map_err(|e| wrap(e, &path))?;
    }
    std::fs::create_dir_all(&path).map_err(|e| wrap(e, &path))?;

    let index = Index::create_in_dir(&path, schema).map_err(|e| wrap(e, &path))?;
    let mut writer = index.writer(50_000_000).map_err(|e| wrap(e, &path))?;

    let mut documents = 0usize;
    let mut misses = 0u32;

    for primary in 1u32.. {
        let records = match ctx.execute(&format!("{code}:{primary}")) {
            Ok(records) => records,
            Err(e) if e.code() == "QQL_REFERENCE_NOT_FOUND" => {
                misses += 1;
                if misses > GAP_TOLERANCE {
                    break;
                }
                continue;
            }
            Err(e) => return Err(e),
        };
        misses = 0;

        for (offset, record) in records.iter().enumerate() {
            let number = offset as u64 + 1;
            writer
                .add_document(doc!(
                    fields.primary => u64::from(primary),
                    fields.number => number,
                    fields.arabic => crate::search::fold(&record.ar),
                    fields.english => record.en.clone(),
                ))
                .map_err(|e| wrap(e, &path))?;
            documents += 1;
        }
    }

    writer.commit().map_err(|e| wrap(e, &path))?;

    if documents == 0 {
        return Err(Error::Unsupported {
            detail: format!("{code} produced no documents to index"),
        });
    }

    Ok(Report { documents, path })
}

/// An opened index, cached for the life of the context.
pub struct Searcher {
    reader: IndexReader,
    parser: QueryParser,
    fields: Fields,
}

impl std::fmt::Debug for Searcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Searcher")
    }
}

impl Searcher {
    /// Open the index for `code` under `data`.
    pub fn open(data: &Path, code: &str) -> Result<Self, Error> {
        let path = index_path(data, code);
        if !path.join("meta.json").exists() {
            return Err(Error::DataFileNotFound {
                path: path.display().to_string(),
            });
        }

        let index = Index::open_in_dir(&path).map_err(|e| wrap(e, &path))?;
        let (_, fields) = schema();
        let reader = index.reader().map_err(|e| wrap(e, &path))?;
        let mut parser = QueryParser::for_index(&index, vec![fields.arabic, fields.english]);
        // A bare term should match either language, not require both.
        parser.set_conjunction_by_default();
        parser.set_field_boost(fields.english, 1.0);

        Ok(Searcher {
            reader,
            parser,
            fields,
        })
    }

    /// Rank documents for `term`, restricted to a scope.
    ///
    /// Returns `(primary, number, score)`, best first.
    pub fn search(
        &self,
        term: &str,
        primary: Option<u32>,
        ranges: &[crate::ast::Range],
        limit: usize,
    ) -> Result<Vec<(u32, u32, f32)>, Error> {
        // Fold for the same reason the index is folded: a diacritized corpus
        // will not match an undiacritized query otherwise.
        let folded = crate::search::fold(term);
        // The term carries its own boolean/phrase syntax, so a malformed one
        // is a bad query rather than an internal fault.
        let parsed = self
            .parser
            .parse_query(&folded)
            .map_err(|e| Error::Unsupported {
                detail: format!("could not parse the search term: {e}"),
            })?;

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Must, parsed)];

        if let Some(primary) = primary {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_u64(self.fields.primary, u64::from(primary)),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        if !ranges.is_empty() {
            let any: Vec<(Occur, Box<dyn Query>)> = ranges
                .iter()
                .map(|r| {
                    let q: Box<dyn Query> = Box::new(RangeQuery::new_u64(
                        "number".to_string(),
                        u64::from(r.from)..u64::from(r.to) + 1,
                    ));
                    (Occur::Should, q)
                })
                .collect();
            clauses.push((Occur::Must, Box::new(BooleanQuery::new(any))));
        }

        let query = BooleanQuery::new(clauses);
        let searcher = self.reader.searcher();
        let hits = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| Error::Internal {
                detail: e.to_string(),
            })?;

        let mut out = Vec::with_capacity(hits.len());
        for (score, address) in hits {
            let document: TantivyDocument = searcher.doc(address).map_err(|e| Error::Internal {
                detail: e.to_string(),
            })?;
            let field = |f: Field| -> Option<u32> {
                document
                    .get_first(f)
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok())
            };
            if let (Some(primary), Some(number)) =
                (field(self.fields.primary), field(self.fields.number))
            {
                out.push((primary, number, score));
            }
        }

        Ok(out)
    }
}
