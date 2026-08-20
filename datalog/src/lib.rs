//! # datalog
//!
//! A Datalog engine targeted at LLM/agent use, with convenient import of fact
//! tables from external sources.
//!
//! The pipeline is `parse → resolve modules → load imports → lower → typecheck →
//! eval`: a hand-rolled lexer and recursive-descent parser, module and data
//! imports (§13), lowering to a positional core IR, static type inference, and a
//! stratified semi-naive fixpoint that records provenance. The language it
//! implements is specified in `spec.md`. The engine is exposed as a library so it
//! stays reusable and testable; the `datalog` binary is a thin CLI wrapper over it.
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
pub mod resolve;
pub mod schedule;
pub mod sources;
pub mod temporal;
pub mod typecheck;

#[cfg(test)]
pub(crate) mod testgen;

pub use api::{
    RunResult, program_with_queries, run, run_at, run_at_reporting, run_with_queries,
    run_with_queries_at, run_with_queries_at_reporting,
};
pub use error::{Error, Result, Warning};
pub use parser::parse;

// Re-exported for the import integration tests, which write fixtures (e.g.
// parquet via `COPY`) through DuckDB itself; not part of the API contract.
#[cfg(feature = "duckdb")]
#[doc(hidden)]
pub use duckdb;

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
