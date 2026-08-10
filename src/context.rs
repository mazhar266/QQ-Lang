//! Execution context: registry plus data cache.

use crate::ast::Query;
use crate::error::Error;
use crate::record::Record;
use crate::registry::Registry;
use crate::repo::Repository;
use std::path::PathBuf;

/// Holds the source registry and every data file loaded so far.
///
/// `execute` takes `&mut self`, so the compiler prevents concurrent use of one
/// context. `Context` is `Send`, so separate contexts on separate threads are
/// safe by construction.
pub struct Context {
    repo: Repository,
    registry: Registry,
}

impl Context {
    /// Create a context reading data from `data_dir`.
    ///
    /// Nothing is loaded here — files are read on first use.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Context {
            repo: Repository::new(data_dir),
            registry: Registry::with_defaults(),
        }
    }

    /// The registry, for inspecting or extending the known sources.
    pub fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    /// Registered source codes.
    pub fn sources(&self) -> Vec<&str> {
        self.registry.codes()
    }

    /// Parse, validate, and resolve.
    ///
    /// References resolve in the order written and are never reordered or
    /// deduplicated against each other — `Q:2:255;Q:2:255;` yields two records.
    pub fn execute(&mut self, query: &str) -> Result<Vec<Record>, Error> {
        let parsed: Query = crate::parser::parse(query)?;
        let mut records = Vec::new();

        for reference in &parsed.references {
            let source =
                self.registry
                    .get(&reference.source)
                    .ok_or_else(|| Error::UnknownSource {
                        code: reference.source.clone(),
                    })?;
            source.resolve(&mut self.repo, reference, &mut records)?;
        }

        Ok(records)
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
