//! Provenance / explainability.
//!
//! Placeholder for derivation tracking (`spec.md` §11). The engine must be able to
//! answer *why* a fact holds, as a proof tree / derivation trace, and surface it
//! through the agent API. This is a headline pillar, so its data model is designed
//! alongside (not after) the evaluator.
//!
//! Anticipated types (TODO):
//! - `Derivation` — the rule instance and premises that produced a fact
//! - `ProofTree`  — recursive derivations bottoming out at base (source) facts
