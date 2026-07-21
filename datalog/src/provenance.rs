//! Provenance / explainability.
//!
//! Derivation tracking (`spec.md` §11): the engine must be able to answer *why*
//! a fact holds. This module owns the two provenance data types and proof-tree
//! extraction; recording happens inside the evaluator's fixpoint loop
//! ([`crate::engine`]), which stores **all** derivations per fact, deduplicated
//! by rule instance (spec §17, 2026-07-19).
//!
//! Everything references the IR's stable coordinates: [`crate::ir::RuleId`]
//! (source-order rule index, never renumbered) and premise order aligned with
//! [`crate::ir::BodyIdx`] (body literal order, never reordered). Original
//! predicate and variable names for rendering recover via
//! [`crate::ir::PredicateInfo::name`] and [`crate::ir::Rule::var_names`];
//! [`crate::ir::PredicateInfo::fields`] additionally allows rendering a fact
//! over a wide relation in named form — `employee(name: "alice", title:
//! "manager")` rather than eight positional columns (§17, 2026-07-20).

use crate::ast::CmpOp;
use crate::engine::Model;
use crate::ir::{Fact, PredId, RuleId, Tuple, Value};

/// A ground-but-for-wildcards pattern whose *absence* satisfied a negated
/// body literal (§7): the negated atom under the rule's bindings, with `None`
/// for wildcard-fresh slots (existential under the negation).
///
/// `root("alice")` holds *because no `parent(_, "alice")` fact exists* — the
/// pattern is what makes that sentence renderable (the explainability
/// pillar). Deliberately a proof-tree-level why-not record, not a semiring
/// construction (§17, 2026-07-20).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AbsentPattern {
    pub pred: PredId,
    /// One entry per column: `Some` closes the slot to that value, `None`
    /// leaves it open (matches anything).
    pub args: Vec<Option<Value>>,
}

impl AbsentPattern {
    /// Does `tuple` fall under the pattern? A match *refutes* the absence:
    /// the engine's anti-join prunes on it, and replay (testing.md E3)
    /// asserts no model tuple satisfies it.
    pub fn matches(&self, tuple: &Tuple) -> bool {
        self.args.len() == tuple.0.len()
            && self
                .args
                .iter()
                .zip(&tuple.0)
                .all(|(pattern, value)| pattern.as_ref().is_none_or(|expected| expected == value))
    }
}

/// One premise of a derivation, aligned with its body literal
/// ([`crate::ir::BodyIdx`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Premise {
    /// The fact that matched a positive literal.
    Fact(Fact),
    /// The pattern no fact matched, satisfying a negated literal.
    Absent(AbsentPattern),
    /// A satisfied comparison/assignment builtin (§8), carrying the operator
    /// and the evaluated operand values (for an assignment `N = expr`, both
    /// values are the assigned value). Self-justifying — like [`Premise::Absent`]
    /// it carries no fixpoint round and recurses into nothing.
    Builtin { op: CmpOp, lhs: Value, rhs: Value },
}

/// One way a fact was derived: a ground rule instance.
///
/// `premises[i]` answers body literal `i` of rule `rule`
/// ([`crate::ir::BodyIdx`] alignment) — the matched fact for a positive
/// literal, the unmatched pattern for a negated one — so the instance can be
/// replayed against the rule to revalidate the derivation (testing.md E3).
///
/// Equality/ordering is by rule + premises — the deduplication key for "all
/// derivations per fact" storage (spec §17): the same instance rediscovered in
/// a later round collapses to one record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Derivation {
    pub rule: RuleId,
    pub premises: Vec<Premise>,
}

/// A finite proof of one fact: recursive derivations bottoming out at base
/// (EDB/imported) facts and absence patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofTree {
    /// A base fact — asserted in the program (or, later, imported).
    Leaf(Fact),
    /// A satisfied negation: no fact matches the pattern (§7). Terminates its
    /// branch — an absence needs no sub-proof.
    Absent(AbsentPattern),
    /// A satisfied comparison/assignment builtin (§8). Terminates its branch —
    /// a builtin holds on its evaluated operands and needs no sub-proof.
    Builtin { op: CmpOp, lhs: Value, rhs: Value },
    /// A derived fact with one supporting rule instance; `children[i]` proves
    /// the instance's `premises[i]`.
    Derived {
        fact: Fact,
        rule: RuleId,
        children: Vec<ProofTree>,
    },
}

impl ProofTree {
    /// Builds one finite proof of `fact` from an evaluated [`Model`], or
    /// `None` if the fact does not hold.
    ///
    /// A fact that is base *and* derivable explains as a [`ProofTree::Leaf`].
    /// For derived facts, the chosen derivation is the [`Ord`]-least one whose
    /// fact premises all first appeared in a strictly earlier fixpoint round
    /// than the fact itself (spec §17: `first_round` stamping); absence
    /// premises always qualify — they carry no round and recurse into
    /// nothing, so they cannot found a cycle. At least one recorded
    /// derivation always qualifies — the one that first produced the fact —
    /// and the strictly-decreasing round bound makes the recursion (and so
    /// the proof) finite even when facts support each other cyclically.
    pub fn explain(model: &Model, fact: &Fact) -> Option<ProofTree> {
        if !model.contains(fact) {
            return None;
        }
        if model.is_base(fact) {
            return Some(ProofTree::Leaf(fact.clone()));
        }
        let round = model.first_round(fact)?;
        let derivation = model.derivations_of(fact).find(|d| {
            d.premises.iter().all(|premise| match premise {
                Premise::Fact(f) => model.first_round(f).is_some_and(|r| r < round),
                Premise::Absent(_) | Premise::Builtin { .. } => true,
            })
        })?;
        let children = derivation
            .premises
            .iter()
            .map(|premise| match premise {
                Premise::Fact(f) => ProofTree::explain(model, f),
                Premise::Absent(pattern) => Some(ProofTree::Absent(pattern.clone())),
                Premise::Builtin { op, lhs, rhs } => Some(ProofTree::Builtin {
                    op: *op,
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                }),
            })
            .collect::<Option<Vec<ProofTree>>>()?;
        Some(ProofTree::Derived {
            fact: fact.clone(),
            rule: derivation.rule,
            children,
        })
    }

    /// The fact this tree proves — `None` for an [`ProofTree::Absent`] node,
    /// which proves the absence of anything matching its pattern rather than
    /// a fact. (The root of an [`ProofTree::explain`] result is never
    /// `Absent`.)
    pub fn fact(&self) -> Option<&Fact> {
        match self {
            ProofTree::Leaf(fact) => Some(fact),
            ProofTree::Derived { fact, .. } => Some(fact),
            ProofTree::Absent(_) | ProofTree::Builtin { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::eval;
    use crate::ir::PredId;
    use crate::ir::fixtures::{example_16_1, fact2};

    #[test]
    fn base_facts_explain_as_leaves() {
        let program = example_16_1();
        let parent = PredId(0);
        let model = eval(&program).unwrap();

        let fact = fact2(parent, "alice", "bob");
        assert_eq!(
            ProofTree::explain(&model, &fact),
            Some(ProofTree::Leaf(fact.clone()))
        );
    }

    #[test]
    fn absent_facts_have_no_proof() {
        let program = example_16_1();
        let ancestor = PredId(1);
        let model = eval(&program).unwrap();

        let fact = fact2(ancestor, "dave", "alice");
        assert_eq!(ProofTree::explain(&model, &fact), None);
    }

    #[test]
    fn recursive_fact_explains_down_to_parent_leaves() {
        // ancestor("alice", "dave") holds via the recursive rule (RuleId(1)):
        //   ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
        // unwinding through bob and carol to base parent facts.
        let program = example_16_1();
        let parent = PredId(0);
        let ancestor = PredId(1);
        let model = eval(&program).unwrap();

        let tree = ProofTree::explain(&model, &fact2(ancestor, "alice", "dave")).unwrap();
        let expected = ProofTree::Derived {
            fact: fact2(ancestor, "alice", "dave"),
            rule: crate::ir::RuleId(1),
            children: vec![
                ProofTree::Leaf(fact2(parent, "alice", "bob")),
                ProofTree::Derived {
                    fact: fact2(ancestor, "bob", "dave"),
                    rule: crate::ir::RuleId(1),
                    children: vec![
                        ProofTree::Leaf(fact2(parent, "bob", "carol")),
                        ProofTree::Derived {
                            fact: fact2(ancestor, "carol", "dave"),
                            rule: crate::ir::RuleId(0),
                            children: vec![ProofTree::Leaf(fact2(parent, "carol", "dave"))],
                        },
                    ],
                },
            ],
        };
        assert_eq!(tree, expected);
    }
}
