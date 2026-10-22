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

    /// The data directory this context reads from.
    pub fn data_dir(&self) -> &Path {
        self.repo.root()
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

    /// How the ranked-search indexes address `code`'s records, or `None` for a
    /// code no source answers to. `qql-index` uses it to skip the sources that
    /// search another's index, as `OT` and `NT` search the Bible's.
    pub fn index_keys(&self, code: &str) -> Option<crate::IndexKeys> {
        self.registry.get(code).map(|source| source.index_keys())
    }

    /// `code`'s book-wide item count — the `CODE::N` axis — if it has one.
    #[cfg(feature = "fulltext")]
    pub(crate) fn total(&mut self, code: &str) -> Result<Option<u32>, Error> {
        let source = self
            .registry
            .get(code)
            .ok_or_else(|| Error::UnknownSource {
                code: code.to_string(),
            })?;
        source.total(&mut self.repo)
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
        let parsed: Query = match crate::parser::parse_tracked(query) {
            Ok(parsed) => parsed,
            Err((error, names)) => return Err(self.first_syntax_error(&names, error)),
        };

        // Read the manifest on first use, matching how data files load. Doing
        // it here rather than in `new` keeps construction infallible and means
        // user-defined sources work through every surface — including the C
        // ABI, which has no way to pass them in.
        if !self.manifest_loaded {
            self.load_manifest()?;
        }

        self.check_paths(&parsed)?;

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
                    MatchKind::Exact => Self::exact(
                        &mut self.repo,
                        source,
                        reference,
                        &search.term,
                        &mut records,
                    )?,
                    MatchKind::Similar => {
                        Self::similar(&mut self.repo, source, reference, search, &mut records)?
                    }
                    MatchKind::FullText => {
                        Self::full_text(&mut self.repo, source, reference, search, &mut records)?
                    }
                },
                None => source.resolve(&mut self.repo, reference, &mut records)?,
            }
        }

        Ok(records)
    }

    /// Reject a path on a source that takes no names, with the error the
    /// parser gave before paths existed.
    ///
    /// Names are legal only right after a code, where an identifier used to
    /// be a syntax error, so this is what keeps every such query failing
    /// exactly as it did: `Q:A` is `QQL_EXPECTED_NUMBER` at the `A`, and
    /// `Q:1SA` is `QQL_INVALID_CHARACTER` at the `SA`, since the parser read
    /// the `1` as a Surah and stopped at the letters. An unknown code with a
    /// path fails the same way, ahead of `QQL_UNKNOWN_SOURCE`, as before.
    ///
    /// It runs over the whole query before anything resolves, because that is
    /// when the parser's errors came.
    fn check_paths(&self, query: &Query) -> Result<(), Error> {
        for reference in &query.references {
            let (Some(code), Some(first)) = (reference.source.as_deref(), reference.path.first())
            else {
                continue;
            };
            if let Some(error) = self.name_error(code, first) {
                return Err(error);
            }
        }
        Ok(())
    }

    /// The error a query that failed to parse has always reported.
    ///
    /// A name on a source that takes none was the old parser's first error,
    /// so when one sits ahead of where parsing failed now, that is the answer:
    /// `Q:A"x"` is still `QQL_EXPECTED_NUMBER` at the `A`, not an error at the
    /// quote. A name on a source that takes names is ordinary syntax, and the
    /// parse error stands.
    fn first_syntax_error(&self, names: &[(String, crate::ast::Segment)], error: Error) -> Error {
        names
            .iter()
            .find_map(|(code, first)| self.name_error(code, first))
            .filter(|name| match (name.position(), error.position()) {
                (Some(name), Some(parse)) => name < parse,
                _ => false,
            })
            .unwrap_or(error)
    }

    /// The old parser's error for a name after `code`, if `code` takes none.
    fn name_error(&self, code: &str, first: &crate::ast::Segment) -> Option<Error> {
        if self
            .registry
            .get(code)
            .is_some_and(|source| source.has_books())
        {
            return None;
        }
        let digits = first.name.bytes().take_while(u8::is_ascii_digit).count();
        let number_ok = digits > 0 && first.name[..digits].parse::<u32>().is_ok();
        Some(if number_ok {
            Error::InvalidCharacter {
                position: first.position + digits,
            }
        } else {
            Error::ExpectedNumber {
                position: first.position,
            }
        })
    }

    /// Resolve a similarity search — the `*"term"` form.
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

    /// Query text for the embedder: stopwords dropped, alternate spellings
    /// appended so their tokens are in the sum too.
    ///
    /// Only for plain terms — a phrase or boolean term is embedded as written.
    #[cfg(feature = "vector")]
    fn prepare(term: &str) -> String {
        let folded = crate::search::fold(term);
        if !crate::search::is_plain(&folded) {
            return folded;
        }

        let mut out = crate::search::without_stopwords(&folded);
        for word in out.clone().split_whitespace() {
            for spelling in crate::search::aliases(word) {
                if *spelling != word {
                    out.push(' ');
                    out.push_str(spelling);
                }
            }
        }
        out
    }

    /// Where a ranked search looks: which index, which keys in it, and how a
    /// hit comes back as a reference to the source that was asked.
    ///
    /// A two-level source reads its own index, keyed `(primary, number)`, and
    /// the scope filters on those directly. A book-wide-keyed source — the
    /// Bible, the Torah — is keyed `(0, N)`: the source turns its scope into
    /// runs of its own `CODE::N`, and the window shifts them onto the index,
    /// which for `OT` and `NT` is the Bible's.
    #[cfg(any(feature = "vector", feature = "fulltext"))]
    fn window(
        repo: &mut Repository,
        source: &dyn crate::Source,
        reference: &Reference,
    ) -> Result<Window, Error> {
        match source.index_keys() {
            crate::IndexKeys::Primary => Ok(Window {
                index: source.code().to_string(),
                primary: reference.primary,
                ranges: reference.ranges.clone(),
                offset: None,
            }),
            crate::IndexKeys::Flat { index } => {
                let offset = source.flat_offset(repo)?;
                let mut ranges = source.flat_scope(repo, reference)?;
                if ranges.is_empty() {
                    let total = source.total(repo)?.unwrap_or(0);
                    ranges.push(Range { from: 1, to: total });
                }
                for range in &mut ranges {
                    range.from += offset;
                    range.to += offset;
                }
                Ok(Window {
                    index: index.to_string(),
                    primary: Some(0),
                    ranges,
                    offset: Some(offset),
                })
            }
        }
    }

    /// Resolve one ranked hit through the ordinary reference path and tag it
    /// with its score.
    #[cfg(any(feature = "vector", feature = "fulltext"))]
    fn ranked_hit(
        repo: &mut Repository,
        source: &dyn crate::Source,
        reference: &Reference,
        window: &Window,
        (primary, number, score): (u32, u32, f32),
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        let (primary, number) = match window.offset {
            // `(0, N)` in the index is `CODE::(N - offset)` in the source.
            Some(offset) => (None, number.saturating_sub(offset)),
            None => (Some(primary), number),
        };
        let mut found = Vec::new();
        source.resolve(
            repo,
            &Reference {
                source: reference.source.clone(),
                path: Vec::new(),
                primary,
                ranges: vec![Range {
                    from: number,
                    to: number,
                }],
                search: None,
            },
            &mut found,
        )?;

        for mut record in found {
            record.extra.insert("score".to_string(), score_value(score));
            record
                .extra
                .insert("ranked".to_string(), serde_json::Value::Bool(true));
            out.push(record);
        }
        Ok(())
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

        let window = Self::window(repo, source, reference)?;
        let path = format!("vectors/{}.qv", window.index);
        let index: Arc<Index> = match repo.load_bytes_with(&path, |bytes| Index::parse(&path, bytes))
        {
            Ok(index) => index,
            Err(Error::DataFileNotFound { .. }) => {
                return Err(Error::Unsupported {
                    detail: format!(
                        "no vector index for {}; build one with scripts/build-vectors.py (expected {path})",
                        window.index
                    ),
                })
            }
            Err(other) => return Err(other),
        };

        // Scope: `Q:1:` limits to one primary, `Q:1:3~5:` to a range in it;
        // a book-wide window limits to its own run of the index.
        let primary = window.primary;
        let ranges = window.ranges.clone();
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
        // The embedder sums token vectors, so a stopword is pure noise and an
        // alternate spelling is a token the corpus may actually carry. Both
        // are cheap to fix before embedding; the index needs no rebuild for
        // either, which is the point of doing it here rather than at build
        // time.
        let query = index.embed(&Self::prepare(&search.term));
        let hits = index.nearest(&query, limit as usize, accept);

        for (key, score) in hits {
            let hit = (key.primary, key.number, score);
            Self::ranked_hit(repo, source, reference, &window, hit, out)?;
        }

        Ok(())
    }

    /// Resolve a full-text search — the `?"term"` form.
    #[cfg(not(feature = "fulltext"))]
    fn full_text(
        _repo: &mut Repository,
        source: &dyn crate::Source,
        _reference: &Reference,
        _search: &crate::ast::Search,
        _out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        Err(Error::Unsupported {
            detail: format!(
                "ranked full-text search needs the `fulltext` feature; {}:\"...\" does substring matching instead",
                source.code()
            ),
        })
    }

    /// Resolve a full-text search against the source's tantivy index.
    ///
    /// Like similarity, the engine decides *which* records and the ordinary
    /// reference path decides what they look like.
    #[cfg(feature = "fulltext")]
    fn full_text(
        repo: &mut Repository,
        source: &dyn crate::Source,
        reference: &Reference,
        search: &crate::ast::Search,
        out: &mut Vec<Record>,
    ) -> Result<(), Error> {
        use crate::fulltext::{Searcher, DEFAULT_LIMIT};

        let window = Self::window(repo, source, reference)?;
        let data = repo.root().to_path_buf();
        let code = window.index.clone();
        let searcher: Arc<Searcher> =
            match repo.cached(&format!("fulltext/{code}"), || Searcher::open(&data, &code)) {
                Ok(searcher) => searcher,
                Err(Error::DataFileNotFound { .. }) => {
                    return Err(Error::Unsupported {
                        detail: format!(
                        "no full-text index for {code}; build one with `qql-index --source {code}`"
                    ),
                    })
                }
                Err(other) => return Err(other),
            };

        let limit = search.limit.unwrap_or(DEFAULT_LIMIT);
        let hits = searcher.search(&search.term, window.primary, &window.ranges, limit as usize)?;

        for hit in hits {
            Self::ranked_hit(repo, source, reference, &window, hit, out)?;
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
        // silently narrower slice. A path is a scope already — `OT:GEN:"x"`
        // is all of Genesis, which the resolver reads without a range, and
        // the source's total would overrun the book.
        if scope.path.is_empty() && scope.primary.is_none() && scope.ranges.is_empty() {
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

        // A single word may be spelled several ways — `koran` for `Qur'an`,
        // `abraham` for `ibrahim`. Only single-word needles expand: a longer
        // needle is a literal substring, and substituting inside one would
        // produce a phrase nobody wrote.
        let folded = crate::search::fold(needle);
        let needles: Vec<String> = match crate::search::aliases(&folded) {
            [] => vec![folded],
            group => group.iter().map(|w| (*w).to_string()).collect(),
        };

        out.extend(candidates.into_iter().filter(|record| {
            needles.iter().any(|needle| {
                crate::search::matches(&record.ar, needle)
                    || crate::search::matches(&record.emlaei, needle)
                    || crate::search::matches(&record.en, needle)
            })
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

/// Where a ranked search looks; see [`Context::window`].
#[cfg(any(feature = "vector", feature = "fulltext"))]
struct Window {
    /// Code of the source whose index to read.
    index: String,
    /// Keys must have this primary; `Some(0)` for a book-wide index.
    primary: Option<u32>,
    /// Keys' numbers must fall in one of these; empty is no limit.
    ranges: Vec<Range>,
    /// For a book-wide index, where the asking source's `::1` sits, less one.
    offset: Option<u32>,
}

/// Round a score to three decimals so responses are stable to compare.
#[cfg(any(feature = "vector", feature = "fulltext"))]
fn score_value(score: f32) -> serde_json::Value {
    let rounded = (f64::from(score) * 1000.0).round() / 1000.0;
    serde_json::Number::from_f64(rounded)
        .map(serde_json::Value::Number)
        .unwrap_or(serde_json::Value::Null)
}
