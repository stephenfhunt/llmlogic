//! Evaluation engine.
//!
//! Placeholder for the bottom-up evaluator (`spec.md` §15). The plan is stratified,
//! semi-naive evaluation supporting recursion, stratified negation, arithmetic and
//! comparison builtins, and aggregation, with magic-sets as a future optimization.
//!
//! Evaluation must cooperate with the provenance layer ([`crate::provenance`]) so
//! derivations can be explained — this constrains the fixpoint loop and is designed
//! in from the start rather than retrofitted.
//!
//! The evaluator's input is [`crate::ir::Program`] — positional, resolved, with
//! `strata` fixing evaluation order (`ir::fixtures::example_16_1` is the first
//! test input). Delta bookkeeping, join order, and indexes are internal to this
//! module, never part of the IR. A naive reference evaluator lives here too
//! (test-only, permanent) as the differential-testing oracle:
//! `naive(p) == seminaive(p)` (`testing.md`, property B1).
