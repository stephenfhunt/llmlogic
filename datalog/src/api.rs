//! Programmatic / agent API.
//!
//! Placeholder for the agent-facing surface (`spec.md` §14). The primary usage is
//! skill-based: an agent drives the CLI, with **Datalog as the interchange format
//! in both directions** — query results are emitted as ground facts (canonical
//! syntax, deterministically ordered), so output is valid input and runs compose
//! over pipes. This module covers the machine-readable edges of that surface:
//! canonical fact printing, and the JSON encodings of structured errors (§12) and
//! provenance trees (§11).
//!
//! Serialization crate (likely `serde`/`serde_json`) is deferred until the shapes
//! are specified — see `spec.md` §17.
