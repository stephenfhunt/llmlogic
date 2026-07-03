//! Evaluation engine.
//!
//! Placeholder for the bottom-up evaluator (`spec.md` §15). The plan is stratified,
//! semi-naive evaluation supporting recursion, stratified negation, arithmetic and
//! comparison builtins, and aggregation, with magic-sets as a future optimization.
//!
//! Evaluation must cooperate with the provenance layer ([`crate::provenance`]) so
//! derivations can be explained — this constrains the fixpoint loop and is designed
//! in from the start rather than retrofitted.
