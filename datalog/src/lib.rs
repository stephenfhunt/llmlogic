//! # datalog
//!
//! A Datalog engine targeted at LLM/agent use, with convenient import of fact
//! tables from external sources.
//!
//! This crate is an early scaffold: the module layout below mirrors the intended
//! architecture, but most modules are stubs pending the language specification in
//! `spec.md`. The engine is exposed as a library so it stays reusable and
//! testable; the `datalog` binary is a thin CLI/REPL wrapper over it.
//!
//! ## Design pillars
//! 1. Provenance / explainability — explain *why* a fact was derived.
//! 2. LLM-friendly syntax + structured, actionable errors.
//! 3. An agent-native CLI: Datalog in, Datalog out (results are facts, so output
//!    composes as input), with JSON at the machine-readable edges.

pub mod api;
pub mod ast;
pub mod engine;
pub mod error;
pub mod ir;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod print;
pub mod provenance;
pub mod sources;
pub mod typecheck;

#[cfg(test)]
pub(crate) mod testgen;

pub use api::{RunResult, run};
pub use error::{Error, Result};
pub use parser::parse;

/// Returns the crate version string (from `CARGO_PKG_VERSION`).
///
/// Placeholder public entry point so the scaffold has something meaningful to
/// exercise from tests and the binary until the engine API lands.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        assert!(!version().is_empty());
    }
}
