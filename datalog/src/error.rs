//! Crate-wide error types.
//!
//! Errors are a design pillar: they must be *structured* and *actionable* so an
//! LLM/agent can react to them programmatically rather than scraping text. This is
//! a skeleton — the concrete taxonomy is specified in `spec.md` §12 (Error model)
//! and will expand as the lexer, parser, and engine land.

use std::fmt;

/// Convenient result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// A structured error produced by the engine.
///
/// TODO(spec §12): flesh out variants with source spans, machine-readable codes,
/// and suggested fixes suitable for agent consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A lexical error (bad token, unterminated literal, …).
    Lex(String),
    /// A syntax error (grammar violation).
    Parse(String),
    /// A semantic/safety error (e.g. unsafe rule, stratification violation).
    Semantic(String),
    /// An error while loading facts from an external source.
    Source(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Lex(msg) => write!(f, "lexical error: {msg}"),
            Error::Parse(msg) => write!(f, "syntax error: {msg}"),
            Error::Semantic(msg) => write!(f, "semantic error: {msg}"),
            Error::Source(msg) => write!(f, "source error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
