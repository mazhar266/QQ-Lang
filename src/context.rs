// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Execution context: registry plus data cache.

use crate::ast::{MatchKind, Query, Range, Reference};
use crate::error::Error;
use crate::record::Record;
use crate::registry::Registry;
use crate::repo::Repository;
use crate::sources::{JsonSource, SourceSpec};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Holds the source registry and every data file loaded so far.
///
/// `execute` takes `&mut self`, so the compiler prevents concurrent use of one
/// context. `Context` is `Send`, so separate contexts on separate threads are
/// safe by construction.
pub struct Context {
    repo: Repository,
    registry: Registry,
    manifest_loaded: bool,
}

/// Manifest of user-defined sources, read from the data directory if present.
pub const MANIFEST: &str = "qql-sources.json";

impl Context {
    /// Create a context reading data from `data_dir`.
    ///
    /// Nothing is loaded here — files are read on first use.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Context {
            repo: Repository::new(data_dir),
            registry: Registry::with_defaults(),
            manifest_loaded: false,
        }
    }

    /// The registry, for inspecting or extending the known sources.
    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    /// Registered source codes.
    ///
    /// Call [`Context::load_manifest`] first if you want user-defined sources
    /// included — otherwise they appear only after the first query, since the
    /// manifest is read lazily.
    pub fn sources(&self) -> Vec<&str> {
        self.registry.codes()
    }

    /// Register a source described by a [`SourceSpec`].
    ///
    /// Sources are searched newest-first, so registering an existing code
    /// shadows it — which is how a custom source replaces a built-in one.
    ///
    /// Note that the data-directory manifest is read on the *first query*, so
    /// it lands after anything registered here. To override an entry the
    /// manifest defines, call [`Context::load_manifest`] first.
    pub fn register_spec(&mut self, spec: SourceSpec) {
        self.registry.register(Box::new(JsonSource::new(spec)));
    }

    /// Register every source in a manifest file: a JSON array of specs.
    ///
    /// `path` is relative to the data directory.
    pub fn add_sources_from(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        let specs: Arc<Vec<SourceSpec>> = self.repo.load(path)?;
        for spec in specs.iter() {
            self.register_spec(spec.clone());
        }
        Ok(())
    }

    /// Read `qql-sources.json` from the data directory, if it exists.
    ///
    /// Absent is not an error — most installations have no custom sources.
    /// A malformed manifest *is* an error, surfaced as `QQL_INVALID_DATA_FILE`
    /// rather than silently ignored.
    pub fn load_manifest(&mut self) -> Result<(), Error> {
        self.manifest_loaded = true;
        if !self.repo.root().join(MANIFEST).exists() {
            return Ok(());
        }
        self.add_sources_from(MANIFEST)
    }

    /// Parse, validate, and resolve.
    ///
    /// References resolve in the order written and are never reordered or
    /// deduplicated against each other — `Q:2:255;Q:2:255;` yields two records.
    pub fn execute(&mut self, query: &str) -> Result<Vec<Record>, Error> {
        let parsed: Query = crate::parser::parse(query)?;

        // Read the manifest on first use, matching how data files load. Doing
        // it here rather than in `new` keeps construction infallible and means
        // user-defined sources work through every surface — including the C
        // ABI, which has no way to pass them in.
        if !self.manifest_loaded {
            self.load_manifest()?;
        }

        let mut records = Vec::new();

        for reference in &parsed.references {
            // A query may omit the source — `1,2:255`. Substitute the
            // registry's default here so every resolver sees a concrete code
            // and the parser never has to know one.
            let code = match &reference.source {
                Some(code) => code.clone(),
                None => self.registry.default_code().to_string(),
            };

            let source = self
                .registry
                .get(&code)
                .ok_or_else(|| Error::UnknownSource { code: code.clone() })?;

            let concrete;
            let reference = if reference.source.is_some() {
                reference
            } else {
                concrete = Reference {
                    source: Some(code),
                    ..reference.clone()
                };
                &concrete
            };

            match &reference.search {
                Some(search) => match search.kind {
                    MatchKind::Exact => {
                        Self::exact(&mut self.repo, source, reference, &search.term, &mut records)?
                    }
                    MatchKind::Similar => {
                        Self::similar(&mut self.repo, source, reference, search, &mut records)?
                    }
                },
                None => source.resolve(&mut self.repo, reference, &mut records)?,
            }
        }

        Ok(records)
    }

    /// Resolve a similarity search — the `` `term` `` form.
    ///
    /// Without the `vector` feature this refuses the query outright. Falling
    /// back to substring matching would answer a different question than the
    /// one asked, which is worse than saying no.
    #[cfg(not(feature = "vector"))]
    fn similar(
        _repo: &mut Repository,
        source: &dyn crate::Source,
        _reference: &Reference,
        _search: &crate::ast::Search,
        _out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        Err(Error::Unsupported {
            detail: format!(
                "similarity search needs the `vector` feature; {}:\"...\" does substring matching instead",
                source.code()
            ),
        })
    }

    /// Resolve a similarity search against the source's vector index.
    ///
    /// The index is addressed by `(primary, number)`, so the scope filters
    /// during the scan and every hit is resolved back through the ordinary
    /// reference path — the ranked list decides *which* records, never what
    /// they look like.
    #[cfg(feature = "vector")]
    fn similar(
        repo: &mut Repository,
        source: &dyn crate::Source,
        reference: &Reference,
        search: &crate::ast::Search,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        use crate::vector::{Index, DEFAULT_LIMIT};

        let path = format!("vectors/{}.qv", source.code());
        let index: Arc<Index> = match repo.load_bytes_with(&path, |bytes| Index::parse(&path, bytes))
        {
            Ok(index) => index,
            Err(Error::DataFileNotFound { .. }) => {
                return Err(Error::Unsupported {
                    detail: format!(
                        "no vector index for {}; build one with scripts/build-vectors.py (expected {path})",
                        source.code()
                    ),
                })
            }
            Err(other) => return Err(other),
        };

        // Scope: `Q:1:` limits to one primary, `Q:1:3~5:` to a range in it.
        let primary = reference.primary;
        let ranges = reference.ranges.clone();
        let accept = move |key: &crate::vector::Key| {
            if let Some(primary) = primary {
                if key.primary != primary {
                    return false;
                }
            }
            ranges.is_empty()
                || ranges
                    .iter()
                    .any(|r| key.number >= r.from && key.number <= r.to)
        };

        let limit = search.limit.unwrap_or(DEFAULT_LIMIT);
        let query = index.embed(&search.term);
        let hits = index.nearest(&query, limit as usize, accept);

        for (key, score) in hits {
            let mut found = Vec::new();
            source.resolve(
                repo,
                &Reference {
                    source: reference.source.clone(),
                    primary: Some(key.primary),
                    ranges: vec![Range {
                        from: key.number,
                        to: key.number,
                    }],
                    search: None,
                },
                &mut found,
            )?;

            for mut record in found {
                record
                    .extra
                    .insert("score".to_string(), score_value(score));
                record
                    .extra
                    .insert("ranked".to_string(), serde_json::Value::Bool(true));
                out.push(record);
            }
        }

        Ok(())
    }

    /// Resolve a search's scope, then keep the records that match.
    ///
    /// Search is deliberately source-agnostic: the scope is an ordinary
    /// reference, so `Q:1:"x"`, `B:2:"x"` and a user-defined `X:1:"x"` all work
    /// through the same path, and a source only has to say how many items it
    /// holds for the unscoped form to reach them.
    ///
    /// The scan is linear over the resolved scope. At Quran and hadith sizes —
    /// 6236 ayat, 7277 hadiths — that is far cheaper than maintaining an
    /// index, and it can never fall out of step with the text.
    fn exact(
        repo: &mut Repository,
        source: &dyn crate::Source,
        reference: &Reference,
        needle: &str,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let mut scope = Reference {
            search: None,
            ..reference.clone()
        };

        // `Q:"text"` names no scope at all, so run the collection's whole
        // flat axis. A source without one says so rather than searching a
        // silently narrower slice.
        if scope.primary.is_none() && scope.ranges.is_empty() {
            let total = source.total(repo)?.ok_or_else(|| Error::Unsupported {
                detail: format!(
                    "{} cannot be searched whole; scope the search, as in {}:1:\"...\"",
                    source.code(),
                    source.code()
                ),
            })?;
            scope.ranges = vec![Range { from: 1, to: total }];
        }

        let mut candidates = Vec::new();
        source.resolve(repo, &scope, &mut candidates)?;

        let folded = crate::search::fold(needle);
        out.extend(candidates.into_iter().filter(|record| {
            crate::search::matches(&record.ar, &folded)
                || crate::search::matches(&record.en, &folded)
        }));

        Ok(())
    }

    /// Execute and build the response envelope. Never fails — errors are
    /// serialized into the value instead.
    pub fn execute_value(&mut self, query: &str) -> serde_json::Value {
        match self.execute(query) {
            Ok(results) => serde_json::json!({
                "ok": true,
                "query": query,
                "results": results,
            }),
            Err(e) => e.to_json(query),
        }
    }

    /// Execute and serialize. Never fails — errors are serialized instead.
    pub fn execute_json(&mut self, query: &str) -> String {
        let value = self.execute_value(query);

        // Serializing a value we just built cannot fail; if it somehow does,
        // still return valid JSON rather than panicking.
        serde_json::to_string(&value).unwrap_or_else(|_| {
            r#"{"ok":false,"error":{"code":"QQL_INTERNAL_ERROR","message":"serialization failed"}}"#
                .to_string()
        })
    }
}

/// Round a score to three decimals so responses are stable to compare.
#[cfg(feature = "vector")]
fn score_value(score: f32) -> serde_json::Value {
    let rounded = (f64::from(score) * 1000.0).round() / 1000.0;
    serde_json::Number::from_f64(rounded)
        .map(serde_json::Value::Number)
        .unwrap_or(serde_json::Value::Null)
}
