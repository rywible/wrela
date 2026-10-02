//! The compiler driver: the query layer and the pipeline built on it.
//!
//! The pipeline so far: [`SourceText`] (input) → [`Lex`] → [`Check`]. A [`Session`] owns the
//! query database and the [`SourceMap`] that diagnostics are rendered against.

pub mod query;

use std::sync::Arc;

use wrela_diag::{Diagnostic, FileId, SourceMap, SourceTooLarge};
use wrela_syntax::Lexed;

use crate::query::{Db, Derived, Input, Query, QueryError};

/// The text of a file.
#[derive(Debug)]
pub struct SourceText;

impl Query for SourceText {
    type Key = FileId;
    type Value = Arc<str>;
    const NAME: &'static str = "source_text";
}

impl Input for SourceText {}

/// A file's tokens and lexical diagnostics.
#[derive(Debug)]
pub struct Lex;

impl Query for Lex {
    type Key = FileId;
    type Value = Arc<Lexed>;
    const NAME: &'static str = "lex";
}

impl Derived for Lex {
    fn compute(db: &Db, file: &FileId) -> Result<Arc<Lexed>, QueryError> {
        let text = db.input::<SourceText>(file)?;
        Ok(Arc::new(wrela_syntax::lex(*file, &text)))
    }
}

/// Every diagnostic for a file, in report order (by position, then code).
#[derive(Debug)]
pub struct Check;

impl Query for Check {
    type Key = FileId;
    type Value = Arc<[Diagnostic]>;
    const NAME: &'static str = "check";
}

impl Derived for Check {
    fn compute(db: &Db, file: &FileId) -> Result<Arc<[Diagnostic]>, QueryError> {
        let lexed = db.get::<Lex>(file)?;
        let mut diagnostics = lexed.diagnostics.clone();
        diagnostics.sort_by_key(Diagnostic::sort_key);
        Ok(diagnostics.into())
    }
}

/// One compilation: its files and the query database over them.
#[derive(Debug, Default)]
pub struct Session {
    db: Db,
    sources: SourceMap,
}

impl Session {
    pub fn new() -> Self {
        Session::default()
    }

    /// Adds a file. `name` is what diagnostics show, usually the path as the user wrote it.
    pub fn add_file(
        &mut self,
        name: impl Into<String>,
        text: impl Into<Arc<str>>,
    ) -> Result<FileId, SourceTooLarge> {
        let text = text.into();
        let file = self.sources.add(name, Arc::clone(&text))?;
        self.db.set::<SourceText>(&file, text);
        Ok(file)
    }

    /// Replaces a file's text; queries that read it re-run on demand. Returns `Ok(false)` for an
    /// unknown file.
    pub fn set_text(
        &mut self,
        file: FileId,
        text: impl Into<Arc<str>>,
    ) -> Result<bool, SourceTooLarge> {
        let text = text.into();
        if !self.sources.replace(file, Arc::clone(&text))? {
            return Ok(false);
        }
        self.db.set::<SourceText>(&file, text);
        Ok(true)
    }

    /// Checks one file.
    pub fn check(&self, file: FileId) -> Result<Arc<[Diagnostic]>, QueryError> {
        self.db.get::<Check>(&file)
    }

    /// Checks every file, in the order they were added; diagnostics stay grouped by file.
    pub fn check_all(&self) -> Result<Vec<Diagnostic>, QueryError> {
        let mut all = Vec::new();
        for (file, _) in self.sources.files() {
            all.extend(self.check(file)?.iter().cloned());
        }
        Ok(all)
    }

    pub fn sources(&self) -> &SourceMap {
        &self.sources
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn db_mut(&mut self) -> &mut Db {
        &mut self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_reports_lexical_diagnostics_in_order() {
        let mut session = Session::new();
        let file = session.add_file("a.wrela", "/* x */ $ y\n").unwrap();
        let codes: Vec<&str> = session
            .check(file)
            .unwrap()
            .iter()
            .map(|d| d.code.id())
            .collect();
        assert_eq!(codes, ["E0002", "E0001"]);
    }

    #[test]
    fn editing_a_file_re_checks_only_that_file() {
        let mut session = Session::new();
        let a = session.add_file("a.wrela", "$").unwrap();
        let b = session.add_file("b.wrela", "x").unwrap();
        assert_eq!(session.check_all().unwrap().len(), 1);
        session.db_mut().enable_log();
        assert!(session.set_text(b, "y $").unwrap());
        assert_eq!(session.check(a).unwrap().len(), 1);
        assert_eq!(session.check(b).unwrap().len(), 1);
        let log = session.db_mut().take_log();
        assert_eq!(log, ["execute lex(FileId(1))", "execute check(FileId(1))"]);
        assert_eq!(session.sources().get(b).unwrap().text(), "y $");
    }

    #[test]
    fn an_edit_that_leaves_the_tokens_alone_stops_at_lex() {
        let mut session = Session::new();
        let a = session.add_file("a.wrela", "x").unwrap();
        assert!(session.check(a).unwrap().is_empty());
        session.db_mut().enable_log();
        // Trailing whitespace: `lex` re-runs, produces equal tokens, and `check` is reused.
        session.set_text(a, "x ").unwrap();
        assert!(session.check(a).unwrap().is_empty());
        assert_eq!(session.db_mut().take_log(), ["execute lex(FileId(0))"]);
    }

    #[test]
    fn unknown_files_are_rejected_without_panicking() {
        let mut other = Session::new();
        let foreign = other.add_file("x.wrela", "x").unwrap();
        let mut session = Session::new();
        assert!(!session.set_text(foreign, "y").unwrap());
        assert!(session.check(foreign).is_err());
    }
}
