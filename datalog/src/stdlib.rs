//! The `std` modules (`spec.md` §13): the builtins the language cannot spell.
//!
//! Some operations a rule cannot express are also unspellable as *functions* —
//! an `ident (` in expression position is ambiguous between an atom and a call,
//! which is what ruled out `float(A)` (§17). As a **body literal** there is no
//! ambiguity, so a builtin is a **relation**, and this module is the registry
//! of which relations exist and where they come from.
//!
//! **The gate is what pays for the names.** `year`, `month` and `day` are
//! exactly the names a program wants and exactly the names a data column has,
//! so they are in scope only where `import "std/time".` is written. A program
//! that does not import keeps its own `year` unchanged; one that does and also
//! defines `year` is an error naming both origins, never a silent preference
//! (§17, 2026-08-19; `notes/temporal-values.md`).
//!
//! The registry is one table because three consumers read it: lowering (which
//! relation is a builtin, and its collision check), §12 (which module a name a
//! program forgot to import comes from), and the resolver (which module names
//! exist at all).

use crate::ast::BuiltinOp;

/// One relation a `std` module provides.
pub struct StdRelation {
    pub name: &'static str,
    /// Total arity, output position included.
    pub arity: u32,
    pub op: BuiltinOp,
}

/// One `std` module: its path (after the `std/` prefix) and what it provides.
pub struct StdModule {
    pub name: &'static str,
    pub relations: &'static [StdRelation],
}

/// `std/time` — extraction and truncation over §4's temporal values.
///
/// Each relation reads a bound input and binds (or filters) one output, so
/// `day(D, N)` binds and `day(D, 15)` is a filter; that falls out of §10's
/// existing mode rules with no new statement. All are finite-domain maps, so
/// §10 exempts them from value-creating recursion for the same reason it
/// exempts a cast.
const TIME: &[StdRelation] = &[
    StdRelation {
        name: "year",
        arity: 2,
        op: BuiltinOp::Year,
    },
    StdRelation {
        name: "month",
        arity: 2,
        op: BuiltinOp::Month,
    },
    StdRelation {
        name: "day",
        arity: 2,
        op: BuiltinOp::Day,
    },
    StdRelation {
        name: "hour",
        arity: 2,
        op: BuiltinOp::Hour,
    },
    StdRelation {
        name: "minute",
        arity: 2,
        op: BuiltinOp::Minute,
    },
    StdRelation {
        name: "second",
        arity: 2,
        op: BuiltinOp::Second,
    },
    StdRelation {
        name: "truncate",
        arity: 3,
        op: BuiltinOp::Truncate,
    },
];

/// Every module `import "std/…".` accepts. `std/math` and `std/text` are
/// designed and deliberately not built (§8's *Not covered*): the shape is
/// settled, and they land when a consumer needs them.
pub const MODULES: &[StdModule] = &[StdModule {
    name: "time",
    relations: TIME,
}];

/// The units `truncate` accepts, largest first — the order is the message's.
pub const TRUNCATE_UNITS: &[&str] = &["year", "quarter", "month", "week", "day", "hour", "minute"];

/// The module named by a `std/…` import path, or `None` if there is no such
/// module.
pub fn module(name: &str) -> Option<&'static StdModule> {
    MODULES.iter().find(|module| module.name == name)
}

/// The relation `name/arity` provides, if `module` provides one.
pub fn relation(module: &StdModule, name: &str, arity: u32) -> Option<&'static StdRelation> {
    module
        .relations
        .iter()
        .find(|relation| relation.name == name && relation.arity == arity)
}

/// Which module provides `name/arity`, over **every** module — the lookup §12
/// needs to turn "referenced but never defined" into "you forgot the import".
pub fn provider(name: &str, arity: u32) -> Option<&'static str> {
    MODULES
        .iter()
        .find(|module| relation(module, name, arity).is_some())
        .map(|module| module.name)
}

/// Every module name, for the error that lists them.
pub fn module_names() -> Vec<&'static str> {
    MODULES.iter().map(|module| module.name).collect()
}
