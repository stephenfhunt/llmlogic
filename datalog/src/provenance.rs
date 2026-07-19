//! Provenance / explainability.
//!
//! Placeholder for derivation tracking (`spec.md` §11). The engine must be able to
//! answer *why* a fact holds, as a proof tree / derivation trace, and surface it
//! through the agent API. This is a headline pillar, so its data model is designed
//! alongside (not after) the evaluator.
//!
//! Anticipated types (TODO, designed with the evaluator against the IR's
//! stable coordinates — [`crate::ir::RuleId`], [`crate::ir::BodyIdx`], and
//! [`crate::ir::Fact`] identity; original names recover via
//! [`crate::ir::Rule::var_names`] and [`crate::ir::PredicateInfo::name`]):
//! - `Derivation` — the rule instance and premises that produced a fact
//! - `ProofTree`  — recursive derivations bottoming out at base (source) facts
