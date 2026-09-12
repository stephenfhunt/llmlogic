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

use crate::ast::{AggOp, CmpOp, TypeName};
use crate::engine::{Model, Provenance};
use crate::ir::{Fact, PredId, RuleId, Tuple, Value};

/// A ground-but-for-wildcards pattern that **no fact matched**, which is what
/// satisfied a negated body literal (§7): the negated atom under the rule's
/// bindings, with `None` for wildcard-fresh slots (existential under the
/// negation).
///
/// `root("alice")` holds *because no `parent(_, "alice")` fact exists* — the
/// pattern is what makes that sentence renderable (the explainability
/// pillar). Deliberately a proof-tree-level why-not record, not a semiring
/// construction (§17, 2026-07-20).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoMatchPattern {
    pub pred: PredId,
    /// One entry per column: `Some` closes the slot to that value, `None`
    /// leaves it open (matches anything).
    pub args: Vec<Option<Value>>,
}

impl NoMatchPattern {
    /// Does `tuple` fall under the pattern? A match *refutes* the no-match:
    /// the engine's anti-join prunes on it, and replay (testing.md E3)
    /// asserts no model tuple satisfies it.
    ///
    /// A closed slot compares **structurally** — the one place in the engine
    /// that does not use [`Value::unifies_with`] (§4/§7). A negated atom binds
    /// nothing, so this is a membership test rather than a join: it asks
    /// whether the tuple is *in* the relation, and `absent` is a perfectly
    /// identifiable member. The `absent`-matches-nothing rule exists to stop a
    /// missing foreign key joining another into a cartesian blowup, and a
    /// membership test never brings in a binding to blow up.
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
    NoMatch(NoMatchPattern),
    /// A satisfied comparison/assignment builtin (§8), carrying the operator
    /// and the evaluated operand values (for an assignment `N = expr`, both
    /// values are the assigned value). Self-justifying — like [`Premise::NoMatch`]
    /// it carries no fixpoint round and recurses into nothing.
    Builtin {
        op: CmpOp,
        lhs: Value,
        rhs: Value,
        /// Conversions (§8's `as`) that **failed on data** while evaluating this
        /// literal: a value existed, could not be represented, and became
        /// `absent` (§17, 2026-08-16). `None` is the ordinary case.
        ///
        /// It rides here rather than in a counter beside the loop because a rule
        /// instance may be rediscovered in a later round, and derivations
        /// deduplicate by rule + premises — so the count is right by
        /// construction, exactly as [`Premise::Aggregate`]'s `skipped` is.
        lost: Option<LostConversion>,
    },
    /// A satisfied presence test `expr is [not] absent` (§4/§8), carrying the
    /// evaluated operand and the operator's `negated` flag. Self-justifying like
    /// [`Premise::Builtin`]: it holds on its evaluated operand and recurses into
    /// nothing.
    Presence { value: Value, negated: bool },
    /// A satisfied aggregate (§9): the operator, the produced value, and the
    /// counts that keep the absent-skip non-silent — `present` values folded and
    /// `skipped` absent inputs (the §9 skip-count report surface). Self-justifying
    /// like [`Premise::Builtin`]: it summarises the fold over the goal's witnesses
    /// and recurses into nothing (the aggregated relation is lower-stratum and
    /// complete when it runs).
    Aggregate {
        op: AggOp,
        value: Value,
        present: usize,
        skipped: usize,
    },
}

/// A conversion that failed on data, summarised for one premise: the target
/// type it failed into, and how many values failed it there.
///
/// The distinction it preserves is the one the value model cannot: an `absent`
/// that arrived as data is *missing*, and an `absent` a failed conversion
/// manufactured is *malformed* (§12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LostConversion {
    pub to: TypeName,
    pub count: u32,
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

impl Derivation {
    /// Does this instance carry something a §9 or §12 warning reads?
    ///
    /// The two reporting surfaces — an aggregate that skipped `absent` inputs,
    /// and a conversion that lost values on data — are counted by walking the
    /// recorded premises and deduplicating by rule instance
    /// (`crate::api::absent_skip_warnings`). This names the derivations that
    /// walk looks at, so [`crate::engine::Provenance::Reports`] can keep those
    /// and drop the rest without changing a single count.
    pub fn reports(&self) -> bool {
        premises_report(&self.premises)
    }
}

/// [`Derivation::reports`] over premises not yet collected into a derivation —
/// which is what lets the fixpoint decide whether to build one at all.
pub(crate) fn premises_report<'a>(premises: impl IntoIterator<Item = &'a Premise>) -> bool {
    premises.into_iter().any(|premise| match premise {
        Premise::Aggregate { skipped, .. } => *skipped > 0,
        Premise::Builtin { lost, .. } => lost.is_some(),
        Premise::Fact(_) | Premise::NoMatch(_) | Premise::Presence { .. } => false,
    })
}

/// The answer to a goal that does **not** hold (§11): why not, per rule.
///
/// The explanation of a missing answer is a *failure trace* — the third of the
/// three names, and a different thing from both the no-match pattern (a
/// satisfied negation's premise) and an absent value (§17, 2026-08-16).
#[derive(Debug, Clone, PartialEq)]
pub struct FailureTrace {
    pub goal: Fact,
    /// One entry per rule whose head unifies with the goal, in rule order.
    /// Empty when no rule could produce the goal at all — which is itself the
    /// answer, and the commonest one for a mistyped relation.
    pub near_misses: Vec<NearMiss>,
}

/// How far one rule got, and what stopped it.
///
/// **A near-miss is a rule, not a binding, and that is the whole bound** (§17,
/// 2026-08-16): one entry per rule, carrying the longest prefix *any* binding
/// satisfied and the first binding that reached it. So a body over
/// `parent("alice", Y)` reports one near-miss and not one per child, nothing
/// truncates, and the size of the answer is the program's own rule count.
#[derive(Debug, Clone, PartialEq)]
pub struct NearMiss {
    pub rule: RuleId,
    /// Premises for the literals the body satisfied, at their true
    /// [`crate::ir::BodyIdx`]; `None` from the block onwards.
    pub satisfied: Vec<Option<Premise>>,
    /// The body index of the first literal the body could not satisfy.
    pub blocked: usize,
    /// For a blocked **comparison**, the operand values it was evaluated on, so
    /// the trace can say *15 >= 18* and not only *A >= 18* — the same
    /// literal-beside-its-values pairing a proof uses for a satisfied one (§11).
    /// They are the first binding that reached the block, which is the binding
    /// the near-miss is about.
    pub blocked_values: Option<(CmpOp, Value, Value)>,
    pub repair: Repair,
}

/// What would advance a blocked rule **one step**.
///
/// A repair is a step, not a promise: the literals past the block were never
/// evaluated, so supplying what it names advances this rule's prefix and need
/// not derive the goal (§17, 2026-08-16). Three of the five name no fact.
#[derive(Debug, Clone, PartialEq)]
pub enum Repair {
    /// The blocked literal reads a relation no rule defines: assert this fact.
    Add(Fact),
    /// The blocked predicate is **derived**, so the repair is not a fact but
    /// the next question to ask.
    Ask(Fact),
    /// The blocked pattern has a slot nothing bound, so it names no one fact.
    Unbound(NoMatchPattern),
    /// A slot of the blocked pattern is bound to `absent`, and `absent` unifies
    /// with nothing (§4) — so **no fact would advance this rule**, not even the
    /// one the pattern spells. Split out from [`Repair::Add`] because naming
    /// that fact would be a repair that does not repair: the row can be
    /// asserted and the join still cannot use it (found by `testing.md` E10).
    AbsentKey(NoMatchPattern),
    /// A negation refuted by a row that exists. The language has no retraction,
    /// so this names the row rather than proposing a deletion.
    Refuted(Fact),
    /// A comparison, presence test or aggregate that did not hold on its
    /// operands: nothing to add or remove.
    Builtin,
}

/// A finite proof of one fact: recursive derivations bottoming out at base
/// (EDB/imported) facts and no-match patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofTree {
    /// A base fact — asserted in the program (or, later, imported).
    Leaf(Fact),
    /// A satisfied negation: no fact matches the pattern (§7). Terminates its
    /// branch — a no-match needs no sub-proof.
    NoMatch(NoMatchPattern),
    /// A satisfied comparison/assignment builtin (§8). Terminates its branch —
    /// a builtin holds on its evaluated operands and needs no sub-proof.
    ///
    /// `lost` carries [`Premise::Builtin`]'s conversion losses through to the
    /// rendering. Without it the §12 *missing* / *malformed* distinction could
    /// not reach a proof at all: both flow through the value model as `absent`
    /// (§4), so the record here and the diagnostic are the only two places it
    /// survives, and a proof that dropped it would be silent about data loss it
    /// holds the evidence for.
    Builtin {
        op: CmpOp,
        lhs: Value,
        rhs: Value,
        lost: Option<LostConversion>,
    },
    /// A satisfied presence test `expr is [not] absent` (§4/§8). Terminates its
    /// branch — it holds on its evaluated operand and needs no sub-proof.
    Presence { value: Value, negated: bool },
    /// A satisfied aggregate (§9): the operator, the folded value, and the
    /// present/skipped counts (the skip is never silent — §9). Terminates its
    /// branch: the aggregated relation is lower-stratum and complete, so the fold
    /// needs no sub-proof in v1.
    Aggregate {
        op: AggOp,
        value: Value,
        present: usize,
        skipped: usize,
    },
    /// A derived fact with one supporting rule instance; `children[i]` proves
    /// the instance's `premises[i]`.
    Derived {
        fact: Fact,
        rule: RuleId,
        children: Vec<ProofTree>,
    },
}

/// What an evaluated model can say about one fact — §11's union, minus the
/// near-miss half that `?whynot` adds.
///
/// The third arm is the one that has to exist: under
/// [`Provenance::Unrecorded`] there is no store to extract from, and reporting
/// that as "no proof" would be indistinguishable from the fact not holding
/// (§17, 2026-08-21).
#[derive(Debug, Clone, PartialEq)]
pub enum Explained {
    /// One finite proof.
    Proof(ProofTree),
    /// The fact does not hold in this model. What *would* have derived it is a
    /// failure trace, not a proof.
    DoesNotHold,
    /// The run did not record provenance, so no proof exists to extract. Not a
    /// statement about the fact.
    Unrecorded,
}

impl Explained {
    /// The proof, if there is one — for callers that have already established
    /// the model is [`Provenance::Recorded`].
    pub fn proof(self) -> Option<ProofTree> {
        match self {
            Explained::Proof(tree) => Some(tree),
            Explained::DoesNotHold | Explained::Unrecorded => None,
        }
    }
}

impl ProofTree {
    /// Builds one finite proof of `fact` from an evaluated [`Model`].
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
    ///
    /// The three arms of [`Explained`] are kept apart here rather than at the
    /// call site because collapsing *unrecorded* into *does not hold* is a
    /// wrong answer, not a missing one.
    pub fn explain(model: &Model, fact: &Fact) -> Explained {
        // Anything but `Recorded` is a partial store or none at all: under
        // `Reports` the derivations that skipped nothing were never kept, so a
        // proof built from what is there would be a proof of the wrong thing.
        if model.provenance() != Provenance::Recorded {
            return Explained::Unrecorded;
        }
        if !model.contains(fact) {
            return Explained::DoesNotHold;
        }
        match ProofTree::extract(model, fact) {
            Some(tree) => Explained::Proof(tree),
            // Unreachable for a held fact: every derived fact has a
            // well-founded derivation (E1), and every fact has a proof (E2).
            None => Explained::DoesNotHold,
        }
    }

    /// The recursive half of [`explain`](Self::explain), on a model already
    /// known to be [`Provenance::Recorded`]. `None` only where a fact does not
    /// hold.
    fn extract(model: &Model, fact: &Fact) -> Option<ProofTree> {
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
                Premise::NoMatch(_)
                | Premise::Builtin { .. }
                | Premise::Presence { .. }
                | Premise::Aggregate { .. } => true,
            })
        })?;
        let children = derivation
            .premises
            .iter()
            .map(|premise| match premise {
                Premise::Fact(f) => ProofTree::extract(model, f),
                Premise::NoMatch(pattern) => Some(ProofTree::NoMatch(pattern.clone())),
                Premise::Builtin { op, lhs, rhs, lost } => Some(ProofTree::Builtin {
                    op: *op,
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    lost: *lost,
                }),
                Premise::Presence { value, negated } => Some(ProofTree::Presence {
                    value: value.clone(),
                    negated: *negated,
                }),
                Premise::Aggregate {
                    op,
                    value,
                    present,
                    skipped,
                } => Some(ProofTree::Aggregate {
                    op: *op,
                    value: value.clone(),
                    present: *present,
                    skipped: *skipped,
                }),
            })
            .collect::<Option<Vec<ProofTree>>>()?;
        Some(ProofTree::Derived {
            fact: fact.clone(),
            rule: derivation.rule,
            children,
        })
    }

    /// The fact this tree proves — `None` for a [`ProofTree::NoMatch`] node,
    /// which proves that nothing matches its pattern rather than proving a
    /// fact. (The root of an [`ProofTree::explain`] result is never
    /// `NoMatch`.)
    pub fn fact(&self) -> Option<&Fact> {
        match self {
            ProofTree::Leaf(fact) => Some(fact),
            ProofTree::Derived { fact, .. } => Some(fact),
            ProofTree::NoMatch(_)
            | ProofTree::Builtin { .. }
            | ProofTree::Presence { .. }
            | ProofTree::Aggregate { .. } => None,
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
            ProofTree::explain(&model, &fact).proof(),
            Some(ProofTree::Leaf(fact.clone()))
        );
    }

    #[test]
    fn absent_facts_have_no_proof() {
        let program = example_16_1();
        let ancestor = PredId(1);
        let model = eval(&program).unwrap();

        let fact = fact2(ancestor, "dave", "alice");
        assert_eq!(ProofTree::explain(&model, &fact), Explained::DoesNotHold);
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

        let tree = ProofTree::explain(&model, &fact2(ancestor, "alice", "dave"))
            .proof()
            .unwrap();
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
