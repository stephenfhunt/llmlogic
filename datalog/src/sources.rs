//! External fact sources.
//!
//! Placeholder for importing fact tables from outside the program (`spec.md` §13):
//! delimited/JSON files, and databases such as SQLite — with schema declarations
//! that map external columns onto predicate arguments. Imported facts are base
//! facts and act as the leaves of provenance proof trees.
//!
//! Anticipated shape (TODO):
//! - a `FactSource` trait yielding tuples for a declared predicate/schema
//! - concrete backends behind cargo features (the source list is an open decision;
//!   see `spec.md` §17)
