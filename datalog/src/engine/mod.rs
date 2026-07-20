//! Evaluation engine: stratified, semi-naive bottom-up evaluation (`spec.md`
//! §15) with provenance recorded inside the fixpoint.
//!
//! The evaluator's input is [`crate::ir::Program`] — positional, resolved,
//! with `strata` fixing evaluation order. Delta bookkeeping, join order, and
//! storage are internal to this module, never part of the IR. The naive
//! reference evaluator ([`naive`], test-only, permanent) is the
//! differential-testing oracle: `naive(p) == seminaive(p)` (`testing.md` B1).
//!
//! Ratified contract (spec §17, 2026-07-19):
//! - [`eval`]`(&ir::Program) -> Result<Model, Error>`; [`Model`] holds all
//!   derived facts per predicate in set storage (input `facts` duplicates
//!   collapse on load), and queries are answered as projections over the
//!   `Model` via each query's `var_names` ([`Model::answer`]).
//! - The fixpoint records **all derivations per fact**, deduplicated by rule
//!   instance ([`crate::provenance::Derivation`]: `RuleId` + premise facts) —
//!   not just a first witness. Each fact is also stamped with the round it
//!   first appeared in ([`Model::first_round`]), which is what makes finite
//!   proof extraction possible ([`crate::provenance::ProofTree::explain`]).
//! - Structured not-yet-supported errors for: comparison literals (§8 open,
//!   incl. `=` semantics), negated atoms (already rejected in lowering), and
//!   programs with imports (until `crate::sources` lands).
//!
//! Still ahead here: stratified negation (roadmap step 3) makes the per-stratum
//! loop meaningful; builtins (§8) extend the join loop; magic sets are a future
//! optimization.

#[cfg(test)]
pub(crate) mod naive;

use std::collections::{BTreeSet, HashMap};

use crate::Result;
use crate::error::Error;
use crate::ir::{
    Atom, BodyLiteral, BodyLiteralKind, Fact, PredId, Program, Query, Rule, RuleId, Term, Tuple,
    Value,
};
use crate::provenance::Derivation;

/// The result of evaluation: every predicate's full extent, plus provenance.
///
/// Relations are genuine sets ([`BTreeSet`]), so iteration is deterministic
/// and already in the §14 canonical order ([`Value`]'s derived `Ord`).
#[derive(Debug, Clone)]
pub struct Model {
    /// Indexed by `PredId`: the predicate's full extent (base ∪ derived).
    relations: Vec<BTreeSet<Tuple>>,
    /// All derivations per derived fact, rule-instance-deduplicated.
    derivations: HashMap<Fact, BTreeSet<Derivation>>,
    /// Facts asserted by the program (set-collapsed). A fact can be both base
    /// and derived; base membership is what makes a proof-tree leaf.
    base: BTreeSet<Fact>,
    /// Fixpoint round each fact first appeared in (base facts: round 0).
    /// Monotone across strata; guarantees a well-founded derivation choice
    /// exists for every fact, so proof trees are finite.
    first_round: HashMap<Fact, u32>,
}

impl Model {
    fn new(num_predicates: usize) -> Model {
        Model {
            relations: vec![BTreeSet::new(); num_predicates],
            derivations: HashMap::new(),
            base: BTreeSet::new(),
            first_round: HashMap::new(),
        }
    }

    /// Does `fact` hold in the model?
    pub fn contains(&self, fact: &Fact) -> bool {
        self.relations[fact.pred.0 as usize].contains(&fact.tuple)
    }

    /// The full extent of `pred`, sorted in canonical order.
    pub fn relation(&self, pred: PredId) -> &BTreeSet<Tuple> {
        &self.relations[pred.0 as usize]
    }

    /// All facts in the model, sorted by (`PredId`, tuple).
    pub fn facts(&self) -> impl Iterator<Item = Fact> + '_ {
        self.relations.iter().enumerate().flat_map(|(i, relation)| {
            relation.iter().map(move |tuple| Fact {
                pred: PredId(i as u32),
                tuple: tuple.clone(),
            })
        })
    }

    /// All recorded derivations of `fact`, `Ord`-least first.
    pub fn derivations_of<'a>(&'a self, fact: &Fact) -> impl Iterator<Item = &'a Derivation> {
        self.derivations.get(fact).into_iter().flatten()
    }

    /// Was `fact` asserted by the program (as opposed to only derived)?
    pub fn is_base(&self, fact: &Fact) -> bool {
        self.base.contains(fact)
    }

    /// The fixpoint round `fact` first appeared in (0 for base facts), or
    /// `None` if the fact does not hold.
    pub fn first_round(&self, fact: &Fact) -> Option<u32> {
        self.first_round.get(fact).copied()
    }

    /// Answers a query as a projection over the model (spec §17): one row per
    /// distinct binding of the query's named variables (`var_names` entries
    /// that are `Some`, in slot order), sorted in canonical order.
    pub fn answer(&self, query: &Query) -> Result<Vec<Vec<Value>>> {
        validate_body(&query.body)?;
        let views = vec![AtomView::Full; query.body.len()];
        let cx = JoinCx {
            model: self,
            delta: &HashMap::new(),
            body: &query.body,
            views: &views,
        };
        let mut rows: BTreeSet<Vec<Value>> = BTreeSet::new();
        enumerate_matches(&cx, query.var_names.len(), &mut |bindings, _premises| {
            let row = query
                .var_names
                .iter()
                .enumerate()
                .filter(|(_, name)| name.is_some())
                .map(|(slot, _)| {
                    bindings[slot]
                        .clone()
                        .expect("query variables are bound by body atoms")
                })
                .collect();
            rows.insert(row);
        });
        Ok(rows.into_iter().collect())
    }

    /// Loads a program-asserted fact (round 0; duplicates collapse).
    fn insert_base(&mut self, fact: Fact) {
        self.relations[fact.pred.0 as usize].insert(fact.tuple.clone());
        self.first_round.entry(fact.clone()).or_insert(0);
        self.base.insert(fact);
    }

    /// Records one derivation, returning whether the fact itself is new.
    fn insert_derived(&mut self, fact: Fact, derivation: Derivation, round: u32) -> bool {
        self.derivations
            .entry(fact.clone())
            .or_default()
            .insert(derivation);
        let is_new = self.relations[fact.pred.0 as usize].insert(fact.tuple.clone());
        if is_new {
            self.first_round.insert(fact, round);
        }
        is_new
    }
}

/// Evaluates a lowered program to its least model.
pub fn eval(program: &Program) -> Result<Model> {
    validate(program)?;
    let mut model = Model::new(program.predicates.len());
    for fact in &program.facts {
        model.insert_base(fact.clone());
    }
    let mut round = 0;
    for stratum in &program.strata {
        round = eval_stratum(program, stratum, &mut model, round);
    }
    Ok(model)
}

/// Rejects program forms the step-2 evaluator does not support yet, and
/// enforces the IR contract that `strata` covers every rule exactly once.
fn validate(program: &Program) -> Result<()> {
    if let Some(import) = program.imports.first() {
        return Err(Error::Semantic(format!(
            "imports not yet supported at evaluation: `import \"{}\" as {}`",
            import.path,
            program.pred_info(import.pred).name,
        )));
    }
    for rule in &program.rules {
        validate_body(&rule.body)?;
    }
    for query in &program.queries {
        validate_body(&query.body)?;
    }
    let mut seen = vec![false; program.rules.len()];
    for &rule_id in program.strata.iter().flatten() {
        let covered = seen
            .get_mut(rule_id.0 as usize)
            .filter(|covered| !**covered)
            .map(|covered| *covered = true);
        if covered.is_none() {
            return Err(Error::Semantic(format!(
                "malformed IR: strata repeat rule {} or reference one out of range",
                rule_id.0
            )));
        }
    }
    if let Some(missing) = seen.iter().position(|covered| !covered) {
        return Err(Error::Semantic(format!(
            "malformed IR: strata do not cover rule {missing}"
        )));
    }
    Ok(())
}

/// Structured not-yet-supported errors for body forms beyond positive atoms
/// (spec §17, 2026-07-19 step-2 policy).
fn validate_body(body: &[BodyLiteral]) -> Result<()> {
    for literal in body {
        match &literal.kind {
            BodyLiteralKind::Atom(_) => {}
            BodyLiteralKind::NegAtom(_) => {
                return Err(Error::Semantic(
                    "negation not yet supported in evaluation".to_string(),
                ));
            }
            BodyLiteralKind::Compare { .. } => {
                return Err(Error::Semantic(
                    "comparison literals not yet supported in evaluation (§8)".to_string(),
                ));
            }
        }
    }
    Ok(())
}

/// Runs one stratum to fixpoint, semi-naively. Returns the updated round
/// counter (monotone across strata, for `first_round` stamping).
fn eval_stratum(program: &Program, stratum: &[RuleId], model: &mut Model, mut round: u32) -> u32 {
    // Seed pass: every rule against the full current relations. This finds
    // every instance derivable from base facts and earlier strata.
    round += 1;
    let no_delta: HashMap<PredId, BTreeSet<Tuple>> = HashMap::new();
    let mut pending: Vec<(Fact, Derivation)> = Vec::new();
    for &rule_id in stratum {
        let rule = &program.rules[rule_id.0 as usize];
        let views = vec![AtomView::Full; rule.body.len()];
        collect_rule_matches(model, &no_delta, rule, rule_id, &views, &mut pending);
    }

    loop {
        // Apply the round's matches. Every derivation is recorded (the
        // all-derivations contract); only genuinely new facts enter the delta.
        let mut delta: HashMap<PredId, BTreeSet<Tuple>> = HashMap::new();
        for (fact, derivation) in pending.drain(..) {
            let pred = fact.pred;
            let tuple = fact.tuple.clone();
            if model.insert_derived(fact, derivation, round) {
                delta.entry(pred).or_default().insert(tuple);
            }
        }
        if delta.is_empty() {
            return round;
        }

        // Delta round: for each rule and each body position i, join the delta
        // at i, the full relations before i, and the pre-delta relations after
        // i — the standard semi-naive rewrite. Every instance with at least
        // one delta premise is enumerated exactly once; instance-level
        // deduplication in the Model absorbs any overlap regardless.
        round += 1;
        for &rule_id in stratum {
            let rule = &program.rules[rule_id.0 as usize];
            for delta_pos in 0..rule.body.len() {
                let views: Vec<AtomView> = (0..rule.body.len())
                    .map(|i| match i.cmp(&delta_pos) {
                        std::cmp::Ordering::Less => AtomView::Full,
                        std::cmp::Ordering::Equal => AtomView::Delta,
                        std::cmp::Ordering::Greater => AtomView::Old,
                    })
                    .collect();
                collect_rule_matches(model, &delta, rule, rule_id, &views, &mut pending);
            }
        }
    }
}

/// Which slice of a predicate's extent a body position joins against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtomView {
    /// The full current relation.
    Full,
    /// Only the last round's newly derived tuples.
    Delta,
    /// The full relation minus the delta (its state before the last round).
    Old,
}

/// Everything the join loop reads; bundled so the recursion stays legible.
struct JoinCx<'a> {
    model: &'a Model,
    delta: &'a HashMap<PredId, BTreeSet<Tuple>>,
    body: &'a [BodyLiteral],
    views: &'a [AtomView],
}

/// Enumerates all matches of `rule`'s body under `views`, grounding the head
/// into pending (fact, derivation) pairs.
fn collect_rule_matches(
    model: &Model,
    delta: &HashMap<PredId, BTreeSet<Tuple>>,
    rule: &Rule,
    rule_id: RuleId,
    views: &[AtomView],
    pending: &mut Vec<(Fact, Derivation)>,
) {
    let cx = JoinCx {
        model,
        delta,
        body: &rule.body,
        views,
    };
    enumerate_matches(&cx, rule.var_names.len(), &mut |bindings, premises| {
        let tuple = Tuple(
            rule.head
                .args
                .iter()
                .map(|term| match term {
                    Term::Const(value) => value.clone(),
                    Term::Var(var) => bindings[var.0 as usize]
                        .clone()
                        .expect("range restriction: head variables are bound by the body"),
                })
                .collect(),
        );
        pending.push((
            Fact {
                pred: rule.head.pred,
                tuple,
            },
            Derivation {
                rule: rule_id,
                premises: premises.to_vec(),
            },
        ));
    });
}

/// A complete-match callback: receives the full bindings and the premise
/// facts (in body order — the `BodyIdx` alignment provenance relies on).
type OnMatch<'a> = dyn FnMut(&[Option<Value>], &[Fact]) + 'a;

/// Left-to-right nested-loop join over the body's positive atoms. Calls
/// `on_match` once per match of the whole body.
fn enumerate_matches(cx: &JoinCx<'_>, num_vars: usize, on_match: &mut OnMatch<'_>) {
    let mut bindings: Vec<Option<Value>> = vec![None; num_vars];
    let mut premises: Vec<Fact> = Vec::with_capacity(cx.body.len());
    enumerate_from(cx, 0, &mut bindings, &mut premises, on_match);
}

fn enumerate_from(
    cx: &JoinCx<'_>,
    idx: usize,
    bindings: &mut [Option<Value>],
    premises: &mut Vec<Fact>,
    on_match: &mut OnMatch<'_>,
) {
    if idx == cx.body.len() {
        on_match(bindings, premises);
        return;
    }
    let BodyLiteralKind::Atom(atom) = &cx.body[idx].kind else {
        unreachable!("validated: only positive atoms reach the join loop");
    };
    let full = cx.model.relation(atom.pred);
    let atom_delta = cx.delta.get(&atom.pred);
    let candidates: Box<dyn Iterator<Item = &Tuple>> = match cx.views[idx] {
        AtomView::Full => Box::new(full.iter()),
        AtomView::Delta => match atom_delta {
            Some(delta) => Box::new(delta.iter()),
            None => return,
        },
        AtomView::Old => Box::new(
            full.iter()
                .filter(move |tuple| atom_delta.is_none_or(|delta| !delta.contains(*tuple))),
        ),
    };
    for tuple in candidates {
        if let Some(bound) = try_match(atom, tuple, bindings) {
            premises.push(Fact {
                pred: atom.pred,
                tuple: tuple.clone(),
            });
            enumerate_from(cx, idx + 1, bindings, premises, on_match);
            premises.pop();
            for slot in bound {
                bindings[slot] = None;
            }
        }
    }
}

/// Unifies an atom against a ground tuple under the current bindings.
/// Returns the slots newly bound here (for backtracking), or `None` on
/// mismatch (with any partial bindings already undone).
fn try_match(atom: &Atom, tuple: &Tuple, bindings: &mut [Option<Value>]) -> Option<Vec<usize>> {
    let mut bound: Vec<usize> = Vec::new();
    for (term, value) in atom.args.iter().zip(&tuple.0) {
        let matches = match term {
            Term::Const(constant) => constant == value,
            Term::Var(var) => {
                let slot = var.0 as usize;
                match &bindings[slot] {
                    Some(existing) => existing == value,
                    None => {
                        bindings[slot] = Some(value.clone());
                        bound.push(slot);
                        true
                    }
                }
            }
        };
        if !matches {
            for slot in bound {
                bindings[slot] = None;
            }
            return None;
        }
    }
    Some(bound)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::ast::{CmpOp, Span};
    use crate::ir::fixtures::{example_16_1, fact2, string_value};
    use crate::ir::{Expr, ImportSpec, PredicateInfo, Var};

    /// Spec §16.1 end-to-end: full extents and the query's answers.
    #[test]
    fn example_16_1_evaluates() {
        let program = example_16_1();
        let parent = PredId(0);
        let ancestor = PredId(1);
        let model = eval(&program).unwrap();

        assert_eq!(model.relation(parent).len(), 3);
        let expected: BTreeSet<Tuple> = [
            ("alice", "bob"),
            ("alice", "carol"),
            ("alice", "dave"),
            ("bob", "carol"),
            ("bob", "dave"),
            ("carol", "dave"),
        ]
        .into_iter()
        .map(|(a, b)| Tuple(vec![string_value(a), string_value(b)]))
        .collect();
        assert_eq!(model.relation(ancestor), &expected);

        // ?- ancestor("alice", Who).  →  Who ∈ {"bob", "carol", "dave"}, sorted.
        let answers = model.answer(&program.queries[0]).unwrap();
        assert_eq!(
            answers,
            vec![
                vec![string_value("bob")],
                vec![string_value("carol")],
                vec![string_value("dave")],
            ]
        );
    }

    #[test]
    fn example_16_1_provenance_is_complete() {
        let program = example_16_1();
        let model = eval(&program).unwrap();
        for fact in model.facts() {
            if model.is_base(&fact) {
                assert_eq!(model.first_round(&fact), Some(0));
            } else {
                assert!(
                    model.derivations_of(&fact).next().is_some(),
                    "derived fact {fact:?} has no derivation"
                );
            }
        }
    }

    /// Two edge-paths a→b→d and a→c→d: path("a", "d") is one fact with two
    /// recorded derivations (all-derivations contract, spec §17).
    #[test]
    fn diamond_records_all_derivations() {
        let edge = PredId(0);
        let path = PredId(1);
        let program = Program {
            predicates: vec![
                PredicateInfo {
                    name: "edge".to_string(),
                    arity: 2,
                    fields: None,
                },
                PredicateInfo {
                    name: "path".to_string(),
                    arity: 2,
                    fields: None,
                },
            ],
            facts: vec![
                fact2(edge, "a", "b"),
                fact2(edge, "b", "d"),
                fact2(edge, "a", "c"),
                fact2(edge, "c", "d"),
            ],
            rules: vec![
                // path(X, Y) :- edge(X, Y).
                Rule {
                    head: Atom {
                        pred: path,
                        args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                    },
                    body: vec![BodyLiteral {
                        kind: BodyLiteralKind::Atom(Atom {
                            pred: edge,
                            args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                        }),
                        span: Span::DUMMY,
                    }],
                    var_names: vec![Some("X".to_string()), Some("Y".to_string())],
                    span: Span::DUMMY,
                },
                // path(X, Y) :- edge(X, Z), path(Z, Y).
                Rule {
                    head: Atom {
                        pred: path,
                        args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                    },
                    body: vec![
                        BodyLiteral {
                            kind: BodyLiteralKind::Atom(Atom {
                                pred: edge,
                                args: vec![Term::Var(Var(0)), Term::Var(Var(2))],
                            }),
                            span: Span::DUMMY,
                        },
                        BodyLiteral {
                            kind: BodyLiteralKind::Atom(Atom {
                                pred: path,
                                args: vec![Term::Var(Var(2)), Term::Var(Var(1))],
                            }),
                            span: Span::DUMMY,
                        },
                    ],
                    var_names: vec![
                        Some("X".to_string()),
                        Some("Y".to_string()),
                        Some("Z".to_string()),
                    ],
                    span: Span::DUMMY,
                },
            ],
            queries: Vec::new(),
            imports: Vec::new(),
            strata: vec![vec![RuleId(0), RuleId(1)]],
        };

        let model = eval(&program).unwrap();
        assert_eq!(model.relation(path).len(), 5); // 4 edges + path("a","d")

        let derivations: Vec<Derivation> = model
            .derivations_of(&fact2(path, "a", "d"))
            .cloned()
            .collect();
        assert_eq!(
            derivations,
            vec![
                Derivation {
                    rule: RuleId(1),
                    premises: vec![fact2(edge, "a", "b"), fact2(path, "b", "d")],
                },
                Derivation {
                    rule: RuleId(1),
                    premises: vec![fact2(edge, "a", "c"), fact2(path, "c", "d")],
                },
            ]
        );

        // A single-derivation fact for contrast.
        let single: Vec<Derivation> = model
            .derivations_of(&fact2(path, "a", "b"))
            .cloned()
            .collect();
        assert_eq!(
            single,
            vec![Derivation {
                rule: RuleId(0),
                premises: vec![fact2(edge, "a", "b")],
            }]
        );
    }

    #[test]
    fn imports_are_a_structured_error() {
        let mut program = Program::default();
        let pred = program.intern_pred("edge", 2);
        program.imports.push(ImportSpec {
            pred,
            path: "edges.csv".to_string(),
            span: Span::DUMMY,
        });
        let err = eval(&program).unwrap_err();
        assert!(err.to_string().contains("imports not yet supported"));
    }

    #[test]
    fn comparison_literals_are_a_structured_error() {
        // q(X) :- p(X), X < 2.
        let mut program = Program::default();
        let p = program.intern_pred("p", 1);
        let q = program.intern_pred("q", 1);
        program.rules.push(Rule {
            head: Atom {
                pred: q,
                args: vec![Term::Var(Var(0))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: p,
                        args: vec![Term::Var(Var(0))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Compare {
                        op: CmpOp::Lt,
                        lhs: Expr::Term(Term::Var(Var(0))),
                        rhs: Expr::Term(Term::Const(Value::Int(2))),
                    },
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![Some("X".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        let err = eval(&program).unwrap_err();
        assert!(
            err.to_string()
                .contains("comparison literals not yet supported")
        );
    }

    #[test]
    fn negated_atoms_are_a_structured_error() {
        // q(X) :- p(X), not p(X).  (lowering rejects this earlier; the engine
        // defends its own contract.)
        let mut program = Program::default();
        let p = program.intern_pred("p", 1);
        let q = program.intern_pred("q", 1);
        program.rules.push(Rule {
            head: Atom {
                pred: q,
                args: vec![Term::Var(Var(0))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: p,
                        args: vec![Term::Var(Var(0))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::NegAtom(Atom {
                        pred: p,
                        args: vec![Term::Var(Var(0))],
                    }),
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![Some("X".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        let err = eval(&program).unwrap_err();
        assert!(err.to_string().contains("negation not yet supported"));
    }

    #[test]
    fn malformed_strata_are_rejected() {
        let mut missing = example_16_1();
        missing.strata = vec![vec![RuleId(0)]];
        let err = eval(&missing).unwrap_err();
        assert!(err.to_string().contains("do not cover rule 1"));

        let mut repeated = example_16_1();
        repeated.strata = vec![vec![RuleId(0), RuleId(0), RuleId(1)]];
        let err = eval(&repeated).unwrap_err();
        assert!(err.to_string().contains("repeat rule 0"));
    }

    #[test]
    fn empty_program_evaluates_to_empty_model() {
        let model = eval(&Program::default()).unwrap();
        assert_eq!(model.facts().count(), 0);
    }

    #[test]
    fn facts_only_program_loads_base_facts() {
        let mut program = Program::default();
        let p = program.intern_pred("p", 2);
        program.facts.push(fact2(p, "a", "b"));
        program.facts.push(fact2(p, "a", "b")); // duplicate collapses on load
        let model = eval(&program).unwrap();
        assert_eq!(model.relation(p).len(), 1);
        assert!(model.is_base(&fact2(p, "a", "b")));
        assert_eq!(model.first_round(&fact2(p, "a", "b")), Some(0));
    }

    #[test]
    fn rule_over_empty_relation_derives_nothing() {
        // q(X) :- p(X).  with no p facts.
        let mut program = Program::default();
        let p = program.intern_pred("p", 1);
        let q = program.intern_pred("q", 1);
        program.rules.push(Rule {
            head: Atom {
                pred: q,
                args: vec![Term::Var(Var(0))],
            },
            body: vec![BodyLiteral {
                kind: BodyLiteralKind::Atom(Atom {
                    pred: p,
                    args: vec![Term::Var(Var(0))],
                }),
                span: Span::DUMMY,
            }],
            var_names: vec![Some("X".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        let model = eval(&program).unwrap();
        assert!(model.relation(p).is_empty());
        assert!(model.relation(q).is_empty());
    }

    // --- Phase B (B1–B7) and Phase E (E1–E4) properties (testing.md) ---

    mod properties {
        use std::collections::{BTreeSet, HashMap};

        use proptest::prelude::*;

        use super::super::naive::{ground, match_atom, naive_eval};
        use super::super::*;
        use crate::ir::fixtures::example_16_1;
        use crate::lower::lower;
        use crate::provenance::ProofTree;
        use crate::testgen::{
            arb_extension_pair, arb_parent_edges, arb_program_with_edb, arb_value,
            with_duplicated_facts, with_extra_fact, with_swapped_body, with_swapped_stratum_rules,
        };

        fn model_facts(model: &Model) -> BTreeSet<Fact> {
            model.facts().collect()
        }

        /// Facts keyed by predicate *name*, for comparing programs whose
        /// `PredId` interning order may differ (testing.md B4).
        fn named_facts(model: &Model, program: &Program) -> BTreeSet<(String, Tuple)> {
            model
                .facts()
                .map(|fact| (program.pred_info(fact.pred).name.clone(), fact.tuple))
                .collect()
        }

        /// Reapplies a derivation's rule instance to its premises (testing.md
        /// E3), using the naive oracle's matching — independent of the
        /// semi-naive join loop that recorded it.
        fn replay(program: &Program, derivation: &Derivation) -> Option<Fact> {
            let rule = program.rules.get(derivation.rule.0 as usize)?;
            if derivation.premises.len() != rule.body.len() {
                return None;
            }
            let mut env = HashMap::new();
            for (literal, premise) in rule.body.iter().zip(&derivation.premises) {
                let BodyLiteralKind::Atom(atom) = &literal.kind else {
                    return None;
                };
                if atom.pred != premise.pred {
                    return None;
                }
                env = match_atom(atom, &premise.tuple, &env)?;
            }
            Some(Fact {
                pred: rule.head.pred,
                tuple: Tuple(rule.head.args.iter().map(|t| ground(t, &env)).collect()),
            })
        }

        fn assert_leaves_are_base(
            tree: &ProofTree,
            model: &Model,
        ) -> std::result::Result<(), TestCaseError> {
            match tree {
                ProofTree::Leaf(fact) => {
                    prop_assert!(model.is_base(fact), "non-base leaf {fact:?}");
                }
                ProofTree::Derived { children, .. } => {
                    for child in children {
                        assert_leaves_are_base(child, model)?;
                    }
                }
            }
            Ok(())
        }

        proptest! {
            // Evaluator differentials are the expensive properties; keep the
            // case count modest (testing.md, Tooling).
            #![proptest_config(ProptestConfig::with_cases(64))]

            /// B1 — the anchor: naive and semi-naive agree as fact sets.
            #[test]
            fn b1_naive_matches_seminaive(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                prop_assert_eq!(model_facts(&model), naive_eval(&program));
            }

            /// B2 — re-running on the saturated fact set derives nothing new.
            #[test]
            fn b2_fixpoint_is_idempotent(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                let mut saturated = program.clone();
                saturated.facts = model.facts().collect();
                let again = eval(&saturated).unwrap();
                prop_assert_eq!(model_facts(&again), model_facts(&model));
            }

            /// B3 — set semantics: duplicated input facts change nothing.
            #[test]
            fn b3_duplicated_facts_change_nothing(
                program in arb_program_with_edb(),
                selectors in proptest::collection::vec(any::<u8>(), 0..4),
            ) {
                let expected = model_facts(&eval(&program).unwrap());
                let duplicated = with_duplicated_facts(program, &selectors);
                prop_assert_eq!(model_facts(&eval(&duplicated).unwrap()), expected);
            }

            /// B4 (fact half) — adding a fact only grows the output.
            #[test]
            fn b4_adding_a_fact_is_monotone(
                program in arb_program_with_edb(),
                pred_sel in any::<u8>(),
                values in proptest::collection::vec(arb_value(), 3),
            ) {
                let base = model_facts(&eval(&program).unwrap());
                let extended = with_extra_fact(program, pred_sel, values);
                let extended_facts = model_facts(&eval(&extended).unwrap());
                prop_assert!(base.is_subset(&extended_facts));
            }

            /// B4 (rule half) — adding statements only grows the output.
            /// Compared by predicate name: interning order may differ.
            #[test]
            fn b4_adding_statements_is_monotone(
                (base, extended) in arb_extension_pair(),
            ) {
                let base_ir = lower(&base).expect("safe by construction");
                let extended_ir = lower(&extended).expect("safe by construction");
                let base_facts = named_facts(&eval(&base_ir).unwrap(), &base_ir);
                let extended_facts =
                    named_facts(&eval(&extended_ir).unwrap(), &extended_ir);
                prop_assert!(base_facts.is_subset(&extended_facts));
            }

            /// B5 — body literal order does not affect the output.
            #[test]
            fn b5_body_order_is_irrelevant(
                program in arb_program_with_edb(),
                rule_sel in any::<u8>(),
                i in any::<u8>(),
                j in any::<u8>(),
            ) {
                let expected = model_facts(&eval(&program).unwrap());
                let swapped = with_swapped_body(program, rule_sel, i, j);
                prop_assert_eq!(model_facts(&eval(&swapped).unwrap()), expected);
            }

            /// B6 — rule application order within a stratum does not matter.
            #[test]
            fn b6_rule_order_is_irrelevant(
                program in arb_program_with_edb(),
                stratum_sel in any::<u8>(),
                i in any::<u8>(),
                j in any::<u8>(),
            ) {
                let expected = model_facts(&eval(&program).unwrap());
                let swapped = with_swapped_stratum_rules(program, stratum_sel, i, j);
                prop_assert_eq!(model_facts(&eval(&swapped).unwrap()), expected);
            }

            /// B7 — independent oracle: random edges through the §16.1
            /// ancestor program equal a hand-rolled DFS transitive closure.
            #[test]
            fn b7_ancestor_is_transitive_closure(edges in arb_parent_edges()) {
                let parent = PredId(0);
                let ancestor = PredId(1);
                let mut program = example_16_1();
                program.queries.clear();
                program.facts = edges
                    .iter()
                    .map(|(a, b)| Fact {
                        pred: parent,
                        tuple: Tuple(vec![
                            Value::String(a.clone()),
                            Value::String(b.clone()),
                        ]),
                    })
                    .collect();
                let model = eval(&program).unwrap();

                let mut expected: BTreeSet<Tuple> = BTreeSet::new();
                let nodes: BTreeSet<&str> =
                    edges.iter().map(|(a, _)| a.as_str()).collect();
                for &start in &nodes {
                    let mut reachable: BTreeSet<&str> = BTreeSet::new();
                    let mut stack = vec![start];
                    while let Some(node) = stack.pop() {
                        for (a, b) in &edges {
                            if a == node && reachable.insert(b) {
                                stack.push(b);
                            }
                        }
                    }
                    for target in reachable {
                        expected.insert(Tuple(vec![
                            Value::String(start.to_string()),
                            Value::String(target.to_string()),
                        ]));
                    }
                }
                prop_assert_eq!(model.relation(ancestor), &expected);
            }

            /// E1 — every derived fact has at least one derivation.
            #[test]
            fn e1_derived_facts_have_derivations(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                for fact in model.facts() {
                    if !model.is_base(&fact) {
                        prop_assert!(
                            model.derivations_of(&fact).next().is_some(),
                            "derived fact {:?} has no derivation", fact
                        );
                    }
                }
            }

            /// E2 — every fact has a proof, and every leaf is a base fact.
            #[test]
            fn e2_proof_leaves_are_base_facts(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                for fact in model.facts() {
                    let tree = ProofTree::explain(&model, &fact);
                    prop_assert!(tree.is_some(), "no proof for {:?}", fact);
                    assert_leaves_are_base(&tree.unwrap(), &model)?;
                }
            }

            /// E3 — replaying any recorded derivation rederives exactly the
            /// fact, and its premises all hold.
            #[test]
            fn e3_derivations_replay(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                for fact in model.facts() {
                    for derivation in model.derivations_of(&fact) {
                        for premise in &derivation.premises {
                            prop_assert!(model.contains(premise));
                        }
                        let replayed = replay(&program, derivation);
                        prop_assert_eq!(replayed.as_ref(), Some(&fact));
                    }
                }
            }

            /// E4 — base facts explain as leaves.
            #[test]
            fn e4_base_facts_explain_as_leaves(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                for fact in model.facts() {
                    if model.is_base(&fact) {
                        prop_assert_eq!(
                            ProofTree::explain(&model, &fact),
                            Some(ProofTree::Leaf(fact.clone()))
                        );
                    }
                }
            }
        }
    }
}
