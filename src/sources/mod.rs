//! Source handlers: the only place that knows Islamic-book structure.

mod hadith;
mod quran;

pub use hadith::HadithCollection;
pub use quran::Quran;

use crate::ast::Reference;
use crate::error::Error;
use crate::record::Record;
use crate::repo::Repository;

/// A collection QQL can resolve references against.
///
/// Implementing this plus one registry entry is the whole cost of adding a
/// collection. If a change here forces an edit to the lexer or parser, the
/// design has been violated.
pub trait Source: Send + Sync {
    /// Canonical code, uppercase, e.g. `"Q"`.
    fn code(&self) -> &str;

    /// Display name, e.g. `"Quran"`.
    fn name(&self) -> &str;

    /// Alternate codes that also select this source.
    fn aliases(&self) -> &[&str] {
        &[]
    }

    /// Validate, load, and append records — in the order the query asked for.
    ///
    /// Semantic validation lives here rather than in a separate `validate`
    /// method: every check a dry run would do is the first thing `resolve`
    /// does anyway, and nothing in the crate needs validation without
    /// resolution.
    fn resolve(
        &self,
        repo: &mut Repository,
        reference: &Reference,
        out: &mut Vec<Record>,
    ) -> Result<(), Error>;
}
