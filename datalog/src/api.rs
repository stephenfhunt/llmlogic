//! Programmatic / agent API.
//!
//! Placeholder for the JSON-in/JSON-out interface (`spec.md` §14) that lets an
//! agent load facts, add rules, run queries, fetch provenance, and receive
//! structured errors — all without parsing human-facing text. This is a primary
//! interface (co-designed with the CLI), so it is a first-class module.
//!
//! Serialization crate (likely `serde`/`serde_json`) is deferred until the request
//! and response shapes are specified — see `spec.md` §17.
