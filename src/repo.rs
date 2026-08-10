//! Lazy-loading, caching JSON repository.
//!
//! Knows storage and nothing else: it has no idea what a Surah or a hadith is.
//! Source handlers hand it a relative path and the type they expect back.

use crate::error::Error;
use serde::de::DeserializeOwned;
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Owns the data directory and everything loaded from it.
///
/// Files are read on first use and kept until the [`Context`](crate::Context)
/// that owns this repository is dropped. There is no eviction policy — a full
/// Quran plus every hadith chapter touched is a few megabytes, and dropping
/// the context frees all of it.
#[derive(Debug)]
pub struct Repository {
    root: PathBuf,
    // The cache is heterogeneous: Quran chapters and hadith chapters
    // deserialize to different types. Keying on path and downcasting keeps
    // this module free of any source-specific schema.
    cache: HashMap<PathBuf, Arc<dyn Any + Send + Sync>>,
}

impl Repository {
    /// Create a repository rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Repository {
            root: root.into(),
            cache: HashMap::new(),
        }
    }

    /// The data directory this repository reads from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Load and deserialize `relative`, or return the cached value.
    ///
    /// Reading through [`std::fs::read_to_string`] rejects invalid UTF-8 up
    /// front rather than lossily replacing it — replacement characters in
    /// scripture are not an acceptable failure mode.
    pub fn load<T>(&mut self, relative: impl AsRef<Path>) -> Result<Arc<T>, Error>
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        let path = self.root.join(relative);

        if let Some(cached) = self.cache.get(&path) {
            return Arc::clone(cached)
                .downcast::<T>()
                .map_err(|_| Error::Internal {
                    detail: format!("cached {} under a different type", path.display()),
                });
        }

        let text = std::fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::DataFileNotFound {
                path: path.display().to_string(),
            },
            _ => Error::InvalidDataFile {
                path: path.display().to_string(),
                detail: e.to_string(),
            },
        })?;

        let value: Arc<T> =
            Arc::new(
                serde_json::from_str(&text).map_err(|e| Error::InvalidDataFile {
                    path: path.display().to_string(),
                    detail: e.to_string(),
                })?,
            );

        self.cache.insert(path, Arc::clone(&value) as Arc<dyn Any + Send + Sync>);
        Ok(value)
    }
}
