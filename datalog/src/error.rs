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

/// A non-fatal diagnostic: the program is valid and still runs, but something
/// looks likely to be a mistake. Warnings print to **stderr** (never stdout,
/// which is reserved for the canonical fact stream) and do not change the exit
/// code. This is the first concrete piece of the §12 severity axis.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Warning {
    /// A predicate is referenced in a rule/query body but never defined (no
    /// fact, rule head, or import) — so it denotes the empty relation, which is
    /// valid Datalog but usually a typo. `suggestion` is the nearest defined
    /// predicate name, when one is close enough to be worth offering.
    UndefinedPredicate {
        name: String,
        arity: u32,
        suggestion: Option<String>,
    },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::UndefinedPredicate {
                name,
                arity,
                suggestion,
            } => {
                write!(
                    f,
                    "warning: predicate `{name}/{arity}` is referenced but never defined; \
                     it will always be empty"
                )?;
                if let Some(suggestion) = suggestion {
                    write!(f, " (did you mean `{suggestion}`?)")?;
                }
                Ok(())
            }
        }
    }
}
