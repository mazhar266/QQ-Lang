//! # QQL — Quran Query Language
//!
//! Parses compact textual references to Islamic texts and resolves them
//! against local JSON data.
//!
//! ```text
//! Q:2:1-5,255;Q:1;
//! ```
//!
//! ```no_run
//! let mut ctx = qql::Context::new("./sources");
//! for record in ctx.execute("Q:2:255")? {
//!     println!("{}", record.ar);
//! }
//! # Ok::<(), qql::Error>(())
//! ```
//!
//! Parsing alone touches no filesystem:
//!
//! ```
//! let query = qql::parse("Q:2:1-5,255")?;
//! assert_eq!(query.references.len(), 1);
//! # Ok::<(), qql::Error>(())
//! ```
//!
//! ## Layering
//!
//! ```text
//! QQL parser knows syntax.
//! Source handlers know Islamic-book structure.
//! Repository knows storage.
//! ```
//!
//! The parser only knows a reference is `IDENT : INT ( : selectors )`. It has
//! no table of Surah counts, so `Q:500:999` parses cleanly and is rejected by
//! the Quran resolver; `XYZ:1:2` parses cleanly and is rejected by the
//! registry.

#![deny(unsafe_code)]
#![deny(missing_docs)]

mod ast;
mod context;
mod error;
pub mod ffi;
mod lexer;
mod parser;
mod record;
mod registry;
mod repo;
mod sources;

pub use ast::{Query, Range, Reference};
pub use context::Context;
pub use error::Error;
pub use parser::parse;
pub use record::Record;
pub use registry::Registry;
pub use repo::Repository;
pub use sources::{HadithCollection, Quran, Source};

/// Crate version, e.g. `"0.1.0"`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
