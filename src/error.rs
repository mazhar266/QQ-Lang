// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mazhar Ahmed

//! Error model.
//!
//! Every variant maps to a stable `QQL_*` wire code via [`Error::code`]. The
//! mapping is an exhaustive `match` with no wildcard arm, so adding a variant
//! fails to compile until its wire code is chosen.

use std::fmt;

/// Anything that can go wrong parsing or resolving a query.
#[derive(Debug)]
pub enum Error {
    /// The query contained no references.
    EmptyQuery,
    /// A character that cannot appear in a query, or a token in a position
    /// where it is not allowed.
    InvalidCharacter {
        /// Byte offset into the query.
        position: usize,
    },
    /// A reference must start with a source identifier.
    ExpectedSource {
        /// Byte offset into the query.
        position: usize,
    },
    /// A `:` was required here.
    ExpectedColon {
        /// Byte offset into the query.
        position: usize,
    },
    /// An integer was required here, or the integer did not fit in a `u32`.
    ExpectedNumber {
        /// Byte offset into the query.
        position: usize,
    },
    /// A quoted search term was required here.
    ExpectedText {
        /// Byte offset into the query.
        position: usize,
    },
    /// A quoted search term was opened and never closed.
    UnterminatedText {
        /// Byte offset of the opening quote.
        position: usize,
    },
    /// The source cannot do what the query asked of it.
    Unsupported {
        /// What is not supported, and by which source.
        detail: String,
    },
    /// A range whose start is greater than its end.
    InvalidRange {
        /// Byte offset of the start of the range.
        position: usize,
    },
    /// No source is registered under this code.
    UnknownSource {
        /// The offending code, as normalized.
        code: String,
    },
    /// The reference is syntactically fine but names something that does not
    /// exist in the collection.
    ReferenceNotFound {
        /// Human-readable description of what was missing.
        detail: String,
    },
    /// A data file the resolver needed is not on disk.
    DataFileNotFound {
        /// Path that was tried.
        path: String,
    },
    /// A data file exists but could not be read or deserialized.
    InvalidDataFile {
        /// Path that failed.
        path: String,
        /// Underlying reason.
        detail: String,
    },
    /// A bug in QQL.
    Internal {
        /// What went wrong.
        detail: String,
    },
}

impl Error {
    /// The stable wire code, as serialized in error JSON.
    pub fn code(&self) -> &'static str {
        match self {
            Error::EmptyQuery => "QQL_EMPTY_QUERY",
            Error::InvalidCharacter { .. } => "QQL_INVALID_CHARACTER",
            Error::ExpectedSource { .. } => "QQL_EXPECTED_SOURCE",
            Error::ExpectedColon { .. } => "QQL_EXPECTED_COLON",
            Error::ExpectedNumber { .. } => "QQL_EXPECTED_NUMBER",
            Error::ExpectedText { .. } => "QQL_EXPECTED_TEXT",
            Error::UnterminatedText { .. } => "QQL_UNTERMINATED_TEXT",
            Error::Unsupported { .. } => "QQL_UNSUPPORTED",
            Error::InvalidRange { .. } => "QQL_INVALID_RANGE",
            Error::UnknownSource { .. } => "QQL_UNKNOWN_SOURCE",
            Error::ReferenceNotFound { .. } => "QQL_REFERENCE_NOT_FOUND",
            Error::DataFileNotFound { .. } => "QQL_DATA_FILE_NOT_FOUND",
            Error::InvalidDataFile { .. } => "QQL_INVALID_DATA_FILE",
            Error::Internal { .. } => "QQL_INTERNAL_ERROR",
        }
    }

    /// Byte offset into the query, for errors that have one.
    pub fn position(&self) -> Option<usize> {
        match self {
            Error::InvalidCharacter { position }
            | Error::ExpectedSource { position }
            | Error::ExpectedColon { position }
            | Error::ExpectedNumber { position }
            | Error::ExpectedText { position }
            | Error::UnterminatedText { position }
            | Error::InvalidRange { position } => Some(*position),
            Error::EmptyQuery
            | Error::Unsupported { .. }
            | Error::UnknownSource { .. }
            | Error::ReferenceNotFound { .. }
            | Error::DataFileNotFound { .. }
            | Error::InvalidDataFile { .. }
            | Error::Internal { .. } => None,
        }
    }

    /// Serialize as the canonical error envelope.
    ///
    /// `position` is omitted entirely for variants that do not carry one —
    /// never emitted as a placeholder `0`.
    pub fn to_json(&self, query: &str) -> serde_json::Value {
        let mut error = serde_json::Map::new();
        error.insert("code".into(), self.code().into());
        error.insert("message".into(), self.to_string().into());
        if let Some(position) = self.position() {
            error.insert("position".into(), position.into());
        }
        serde_json::json!({
            "ok": false,
            "query": query,
            "error": serde_json::Value::Object(error),
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::EmptyQuery => f.write_str("Query is empty"),
            Error::InvalidCharacter { .. } => f.write_str("Unexpected character"),
            Error::ExpectedSource { .. } => f.write_str("Expected a source identifier"),
            Error::ExpectedColon { .. } => f.write_str("Expected ':'"),
            Error::ExpectedNumber { .. } => f.write_str("Expected a number"),
            Error::ExpectedText { .. } => f.write_str("Expected a quoted search term"),
            Error::UnterminatedText { .. } => f.write_str("Unclosed quoted search term"),
            Error::Unsupported { detail } => write!(f, "Not supported: {detail}"),
            Error::InvalidRange { .. } => {
                f.write_str("Range start cannot be greater than range end")
            }
            Error::UnknownSource { code } => write!(f, "Unknown source '{code}'"),
            Error::ReferenceNotFound { detail } => write!(f, "Not found: {detail}"),
            Error::DataFileNotFound { path } => write!(f, "Data file not found: {path}"),
            Error::InvalidDataFile { path, detail } => {
                write!(f, "Invalid data file {path}: {detail}")
            }
            Error::Internal { detail } => write!(f, "Internal error: {detail}"),
        }
    }
}

impl std::error::Error for Error {}
