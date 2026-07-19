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
//! [`crate::ir::PredicateInfo::name`] and [`crate::ir::Rule::var_names`].

use crate::engine::Model;
use crate::ir::{Fact, RuleId};

/// One way a fact was derived: a ground rule instance.
///
/// `premises[i]` is the fact that matched body literal `i` of rule `rule`
/// ([`crate::ir::BodyIdx`] alignment), so the instance can be replayed against
/// the rule to revalidate the derivation (testing.md E3).
///
/// Equality/ordering is by rule + premises — the deduplication key for "all
/// derivations per fact" storage (spec §17): the same instance rediscovered in
/// a later round collapses to one record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Derivation {
    pub rule: RuleId,
    pub premises: Vec<Fact>,
}

/// A finite proof of one fact: recursive derivations bottoming out at base
/// (EDB/imported) facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofTree {
    /// A base fact — asserted in the program (or, later, imported). Leaves are
    /// always base facts (testing.md E2/E4).
    Leaf(Fact),
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
    /// premises all first appeared in a strictly earlier fixpoint round than
    /// the fact itself (spec §17: `first_round` stamping). At least one
    /// recorded derivation always qualifies — the one that first produced the
    /// fact — and the strictly-decreasing round bound makes the recursion (and
    /// so the proof) finite even when facts support each other cyclically.
    pub fn explain(model: &Model, fact: &Fact) -> Option<ProofTree> {
        if !model.contains(fact) {
            return None;
        }
        if model.is_base(fact) {
            return Some(ProofTree::Leaf(fact.clone()));
        }
        let round = model.first_round(fact)?;
        let derivation = model.derivations_of(fact).find(|d| {
            d.premises
                .iter()
                .all(|p| model.first_round(p).is_some_and(|r| r < round))
        })?;
        let children = derivation
            .premises
            .iter()
            .map(|premise| ProofTree::explain(model, premise))
            .collect::<Option<Vec<ProofTree>>>()?;
        Some(ProofTree::Derived {
            fact: fact.clone(),
            rule: derivation.rule,
            children,
        })
    }

    /// The fact this tree proves.
    pub fn fact(&self) -> &Fact {
        match self {
            ProofTree::Leaf(fact) => fact,
            ProofTree::Derived { fact, .. } => fact,
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
