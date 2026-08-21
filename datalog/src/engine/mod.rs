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
//!   instance ([`crate::provenance::Derivation`]: `RuleId` + premises) —
//!   not just a first witness. Each fact is also stamped with the round it
//!   first appeared in ([`Model::first_round`]), which is what makes finite
//!   proof extraction possible ([`crate::provenance::ProofTree::explain`]).
//! - Comparison and arithmetic builtins (§8) are evaluated as body literals:
//!   ordered/equality comparisons act as anti-join *filters*, and `=` binds an
//!   otherwise-unbound variable to the evaluated other side (assignment),
//!   else compares (spec §17, 2026-07-21). Arithmetic is strict — `int op int`
//!   and `float op float` only; mixed/non-numeric operands, integer overflow,
//!   division by zero, and NaN are structured runtime errors.
//! - Structured not-yet-supported errors for programs with imports (until
//!   `crate::sources` lands).
//!
//! Stratified negation (§7, §17 2026-07-20): a negated atom is an anti-join
//! *filter* — it binds nothing, always reads the full relation (its predicate's
//! strata are strictly lower, so the relation is complete and frozen; validated
//! here as an IR contract), and is scheduled after the body's positive atoms
//! (evaluator-internal ordering; premises land at their true `BodyIdx`). The
//! satisfied negation is recorded as a [`crate::provenance::Premise::NoMatch`]
//! pattern.
//!
//! Still ahead here: magic sets are a future optimization.

#[cfg(test)]
pub(crate) mod naive;
mod seek;

use std::collections::{BTreeSet, HashMap};

use crate::Result;
use crate::ast::{AggOp, ArithOp, BuiltinOp, CmpOp, TypeName};
use crate::error::Error;
use crate::ir::{
    Atom, BodyLiteral, BodyLiteralKind, Expr, F64, Fact, PredId, Program, Query, Rule, RuleId,
    Term, Tuple, Value, f64_as_exact_i64, i64_as_exact_f64,
};
use crate::lexer::{CellClass, classify_cell, classify_symbol};
use crate::provenance::{Derivation, LostConversion, NoMatchPattern, Premise};
use crate::temporal;

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
    ///
    /// A round is coarser than a per-fact sequence number — every fact applied
    /// in one batch shares it — and that coarseness is **harmless here** only
    /// because application is batched (see `eval_stratum`'s apply loop): no
    /// derivation can have a premise from its own round *unless* it is a
    /// redundant rediscovery, which the first-producing derivation never is.
    /// Facts derived in the same round are therefore never ordered against each
    /// other, and never need to be.
    pub fn first_round(&self, fact: &Fact) -> Option<u32> {
        self.first_round.get(fact).copied()
    }

    /// Answers a query as a projection over the model (spec §17): one row per
    /// distinct binding of the query's [answer variables]
    /// ([`Query::projection`]), in slot order, sorted in canonical order.
    ///
    /// The projection is lowering's, not "every named slot": an aggregate's
    /// goal-local variables are named but bound only inside the sub-join (§9).
    /// A slot the match left unbound is malformed IR and surfaces as a
    /// structured error rather than a panic.
    pub fn answer(&self, query: &Query) -> Result<Vec<Vec<Value>>> {
        self.answer_reporting(query, &mut |_| {})
    }

    /// [`answer`](Self::answer), reporting each matched row's premises to
    /// `report` as they are produced.
    ///
    /// A query records no derivations — it is a projection over a finished
    /// model, not a rule that adds to it — but the premises are built either
    /// way, and §9's skip count rides on
    /// [`Premise::Aggregate`](crate::provenance::Premise::Aggregate). Handing
    /// them out here is what lets an aggregate written in a *query* report its
    /// skipped `absent` inputs, which was the one shape §9's warning could not
    /// see (§17, 2026-08-18).
    pub fn answer_reporting(
        &self,
        query: &Query,
        report: &mut dyn FnMut(&[Option<Premise>]),
    ) -> Result<Vec<Vec<Value>>> {
        validate_body(&query.body, &query.var_names)?;
        let views = vec![AtomView::Full; query.body.len()];
        let cx = JoinCx {
            model: self,
            delta: &HashMap::new(),
            body: &query.body,
            views: &views,
        };
        let mut rows: BTreeSet<Vec<Value>> = BTreeSet::new();
        let mut unbound: Option<u32> = None;
        enumerate_matches(&cx, query.var_names.len(), &mut |bindings, premises| {
            report(premises);
            let mut row = Vec::with_capacity(query.projection.len());
            for &slot in &query.projection {
                match bindings.get(slot as usize).and_then(|v| v.clone()) {
                    Some(value) => row.push(value),
                    None => {
                        unbound.get_or_insert(slot);
                        return;
                    }
                }
            }
            rows.insert(row);
        })?;
        if let Some(slot) = unbound {
            let name = query
                .var_names
                .get(slot as usize)
                .and_then(|name| name.as_deref())
                .unwrap_or("_");
            return Err(Error::semantic(format!(
                "malformed IR: query answer variable `{name}` is not bound by the query body"
            )));
        }
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
///
/// Runs until the fixpoint, with no round cap, fact cap or wall-clock budget —
/// **by decision** (§17, 2026-08-16), not by omission. What stops the loop is
/// §10's finiteness argument for programs the termination lint certifies; for
/// the ones it warns about, the interrupt is the operator's.
pub fn eval(program: &Program) -> Result<Model> {
    eval_capped(program, u32::MAX).map_err(|error| match error {
        Capped::Failed(error) => error,
        Capped::Diverged => unreachable!("u32::MAX rounds is not a cap anyone reaches"),
    })
}

/// How [`eval_capped`] can fail: an ordinary evaluation error, or the round cap.
#[derive(Debug)]
pub(crate) enum Capped {
    Failed(Error),
    /// The cap was reached with the fixpoint still growing.
    Diverged,
}

impl From<Error> for Capped {
    fn from(error: Error) -> Self {
        Capped::Failed(error)
    }
}

/// [`eval`] with a ceiling on fixpoint rounds, for **tests only** — nothing
/// shipped passes anything but `u32::MAX`.
///
/// C10 needs it: the property is that a certified program *terminates*, and a
/// wrong certification would otherwise hang the suite instead of failing it. A
/// deterministic round cap fails it, where a wall-clock timeout would also flake
/// under load. Its existence is not a budget and must not be read as one — see
/// [`eval`].
pub(crate) fn eval_capped(
    program: &Program,
    max_rounds: u32,
) -> std::result::Result<Model, Capped> {
    validate(program).map_err(Capped::Failed)?;
    let mut model = Model::new(program.predicates.len());
    for fact in &program.facts {
        model.insert_base(fact.clone());
    }
    let mut round = 0;
    for stratum in &program.strata {
        round = eval_stratum(program, stratum, &mut model, round, max_rounds)?;
    }
    Ok(model)
}

/// Rejects program forms the step-2 evaluator does not support yet, and
/// enforces the IR contract that `strata` covers every rule exactly once.
fn validate(program: &Program) -> Result<()> {
    // Imported facts are ordinary base facts by the time the engine runs
    // (§13): the source layer materialized them into `program.facts` before
    // lowering, and `ImportSpec` survives only as provenance/definedness
    // metadata. Nothing import-specific to reject here.
    for rule in &program.rules {
        validate_body(&rule.body, &rule.var_names)?;
    }
    for query in &program.queries {
        validate_body(&query.body, &query.var_names)?;
    }
    let mut seen = vec![false; program.rules.len()];
    for &rule_id in program.strata.iter().flatten() {
        let covered = seen
            .get_mut(rule_id.0 as usize)
            .filter(|covered| !**covered)
            .map(|covered| *covered = true);
        if covered.is_none() {
            return Err(Error::semantic(format!(
                "malformed IR: strata repeat rule {} or reference one out of range",
                rule_id.0
            )));
        }
    }
    if let Some(missing) = seen.iter().position(|covered| !covered) {
        return Err(Error::semantic(format!(
            "malformed IR: strata do not cover rule {missing}"
        )));
    }

    // The negation contract (§7): every rule defining a negated predicate
    // sits in a strictly lower stratum, so the negated relation is complete
    // and frozen when read. Well-lowered IR satisfies this by construction;
    // hand-built IR that violates it would be silently mis-evaluated.
    // (Queries are exempt — they run over the finished model.)
    let mut defining_stratum: Vec<Option<usize>> = vec![None; program.predicates.len()];
    for (level, stratum) in program.strata.iter().enumerate() {
        for &rule_id in stratum {
            let head = program.rules[rule_id.0 as usize].head.pred;
            let entry = &mut defining_stratum[head.0 as usize];
            *entry = Some(entry.map_or(level, |existing| existing.max(level)));
        }
    }
    for (level, stratum) in program.strata.iter().enumerate() {
        for &rule_id in stratum {
            check_stratum_contract(
                &program.rules[rule_id.0 as usize].body,
                level,
                &defining_stratum,
                program,
                rule_id,
                false,
            )?;
        }
    }
    Ok(())
}

/// The strict-lower-stratum contract (§7/§9): a negated atom's predicate, and
/// *every* predicate an aggregate reads (`under_aggregate`), must be defined in a
/// strictly lower stratum than the rule using it, so the relation is complete and
/// frozen when read. Well-lowered IR satisfies this by construction; this guards
/// hand-built IR. Recurses into aggregate goals.
fn check_stratum_contract(
    body: &[BodyLiteral],
    level: usize,
    defining_stratum: &[Option<usize>],
    program: &Program,
    rule_id: RuleId,
    under_aggregate: bool,
) -> Result<()> {
    let too_high = |pred: PredId| defining_stratum[pred.0 as usize].is_some_and(|def| def >= level);
    for literal in body {
        match &literal.kind {
            BodyLiteralKind::Atom(atom) if under_aggregate && too_high(atom.pred) => {
                return Err(Error::semantic(format!(
                    "malformed IR: rule {} aggregates over `{}`, which is not defined in a \
                     strictly lower stratum",
                    rule_id.0,
                    program.pred_info(atom.pred).name
                )));
            }
            BodyLiteralKind::NegAtom(atom) if too_high(atom.pred) => {
                return Err(Error::semantic(format!(
                    "malformed IR: rule {} negates `{}`, which is not defined in a \
                     strictly lower stratum",
                    rule_id.0,
                    program.pred_info(atom.pred).name
                )));
            }
            BodyLiteralKind::Aggregate { goal, .. } => {
                check_stratum_contract(goal, level, defining_stratum, program, rule_id, true)?;
            }
            BodyLiteralKind::Atom(_)
            | BodyLiteralKind::NegAtom(_)
            | BodyLiteralKind::Compare { .. }
            | BodyLiteralKind::Presence { .. } => {}
        }
    }
    Ok(())
}

/// Enforces the negation IR contract: a negated atom's *named* variables must be
/// bound by the same body — a positive atom, an `=`-assignment, or an aggregate
/// result (§7/§10, 2026-07-25). Well-lowered IR satisfies this via §10 safety;
/// hand-built IR that violates it would otherwise silently evaluate the named
/// variable as a wildcard. Comparison operand safety (§8) is enforced in
/// lowering; a malformed comparison fed as hand-built IR surfaces as a
/// structured error when its operand is evaluated.
///
/// The binding set comes from [`crate::schedule`], the same function lowering
/// checks against, so the two cannot drift — this used to be an independent
/// "occurs in a positive atom" walk, which meant the rule was stated twice and
/// had to be relaxed twice.
fn validate_body(body: &[BodyLiteral], var_names: &[Option<String>]) -> Result<()> {
    validate_body_seeded(body, var_names, &std::collections::HashSet::new())
}

/// [`validate_body`] with a `seed` of variables treated as positively bound by an
/// enclosing body — the group keys visible inside an aggregate's goal (§9).
fn validate_body_seeded(
    body: &[BodyLiteral],
    var_names: &[Option<String>],
    seed: &std::collections::HashSet<u32>,
) -> Result<()> {
    // Unschedulable IR is reported by `literal_order` when the body runs; here
    // an unschedulable body simply binds nothing this check can rely on, so fall
    // back to the positive atoms rather than reporting the same fault twice.
    let seed_vars: std::collections::HashSet<crate::ir::Var> =
        seed.iter().map(|slot| crate::ir::Var(*slot)).collect();
    let bound: std::collections::HashSet<u32> =
        match crate::schedule::schedule_body_with(body, &seed_vars) {
            Ok((_, bound)) => bound.iter().map(|var| var.0).collect(),
            Err(_) => body
                .iter()
                .filter_map(|literal| match &literal.kind {
                    BodyLiteralKind::Atom(atom) => Some(atom),
                    _ => None,
                })
                .flat_map(|atom| atom.args.iter())
                .filter_map(|arg| match arg {
                    Term::Var(var) => Some(var.0),
                    Term::Const(_) => None,
                })
                .chain(seed.iter().copied())
                .collect(),
        };

    for literal in body {
        match &literal.kind {
            BodyLiteralKind::Atom(_)
            | BodyLiteralKind::Compare { .. }
            | BodyLiteralKind::Presence { .. } => {}
            BodyLiteralKind::NegAtom(atom) => {
                for arg in &atom.args {
                    if let Term::Var(var) = arg
                        && let Some(Some(name)) = var_names.get(var.0 as usize)
                        && !bound.contains(&var.0)
                    {
                        return Err(Error::semantic(format!(
                            "malformed IR: named variable `{name}` in negated atom is never \
                             bound by the body"
                        )));
                    }
                }
            }
            BodyLiteralKind::Aggregate { goal, .. } => {
                // The goal sees the group keys: this body's positive variables
                // plus any inherited from an outer body.
                let mut inner = seed.clone();
                for literal in body {
                    if let BodyLiteralKind::Atom(atom) = &literal.kind {
                        for arg in &atom.args {
                            if let Term::Var(var) = arg {
                                inner.insert(var.0);
                            }
                        }
                    }
                }
                validate_body_seeded(goal, var_names, &inner)?;
            }
        }
    }
    Ok(())
}

/// Runs one stratum to fixpoint, semi-naively. Returns the updated round
/// counter (monotone across strata, for `first_round` stamping).
fn eval_stratum(
    program: &Program,
    stratum: &[RuleId],
    model: &mut Model,
    mut round: u32,
    max_rounds: u32,
) -> std::result::Result<u32, Capped> {
    // Seed pass: every rule against the full current relations. This finds
    // every instance derivable from base facts and earlier strata.
    round += 1;
    let no_delta: HashMap<PredId, BTreeSet<Tuple>> = HashMap::new();
    let mut pending: Vec<(Fact, Derivation)> = Vec::new();
    for &rule_id in stratum {
        let rule = &program.rules[rule_id.0 as usize];
        let views = vec![AtomView::Full; rule.body.len()];
        collect_rule_matches(model, &no_delta, rule, rule_id, &views, &mut pending)?;
    }

    loop {
        // Apply the round's matches. Every derivation is recorded (the
        // all-derivations contract); only genuinely new facts enter the delta.
        //
        // **Application is batched, and proof finiteness rests on it.** Nothing
        // inserted here is visible to the collection that produced `pending` —
        // that collection already ran, against the model as of the previous
        // round. So the derivation that *first* produces a fact always has
        // premises stamped strictly earlier, which is what makes
        // `ProofTree::explain`'s round bound (§17, 2026-07-19) able to find a
        // proof for every fact rather than merely a well-founded one.
        // Interleaving collection with insertion here would break that silently:
        // the test that fails is **E1** (`e1_derived_facts_have_derivations`),
        // and behind it E2.
        let mut delta: HashMap<PredId, BTreeSet<Tuple>> = HashMap::new();
        for (fact, derivation) in pending.drain(..) {
            let pred = fact.pred;
            let tuple = fact.tuple.clone();
            if model.insert_derived(fact, derivation, round) {
                delta.entry(pred).or_default().insert(tuple);
            }
        }
        if delta.is_empty() {
            return Ok(round);
        }
        // The fixpoint is still growing. Test-only: `eval` passes `u32::MAX`.
        if round >= max_rounds {
            return Err(Capped::Diverged);
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
                // Only positive positions take a delta view: a negated
                // predicate's relation is a frozen lower stratum and gains no
                // tuples mid-stratum, so there is no delta to join on.
                if !matches!(rule.body[delta_pos].kind, BodyLiteralKind::Atom(_)) {
                    continue;
                }
                let views: Vec<AtomView> = (0..rule.body.len())
                    .map(|i| match i.cmp(&delta_pos) {
                        std::cmp::Ordering::Less => AtomView::Full,
                        std::cmp::Ordering::Equal => AtomView::Delta,
                        std::cmp::Ordering::Greater => AtomView::Old,
                    })
                    .collect();
                collect_rule_matches(model, &delta, rule, rule_id, &views, &mut pending)?;
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
) -> Result<()> {
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
                premises: premises
                    .iter()
                    .map(|premise| {
                        premise
                            .clone()
                            .expect("complete match: every body literal contributed a premise")
                    })
                    .collect(),
            },
        ));
    })
}

/// A complete-match callback: receives the full bindings and one premise per
/// body literal, at its true index (the `BodyIdx` alignment provenance relies
/// on). Every entry is `Some` at match time; the `Option` is backtracking
/// state.
type OnMatch<'a> = dyn FnMut(&[Option<Value>], &[Option<Premise>]) + 'a;

/// Nested-loop join over the body: positive atoms first in source order, then
/// negated atoms as anti-join filters (§17 2026-07-20 — §10 safety guarantees
/// every named variable under negation is bound once the positives have
/// matched, and body-before-binder orderings would otherwise misread a named
/// variable as a wildcard). Evaluation order is evaluator-internal; premises
/// are recorded at their true body index, so `BodyIdx` alignment is
/// untouched. Calls `on_match` once per match of the whole body.
fn enumerate_matches(cx: &JoinCx<'_>, num_vars: usize, on_match: &mut OnMatch<'_>) -> Result<()> {
    let order = literal_order(cx.body)?;
    let mut bindings: Vec<Option<Value>> = vec![None; num_vars];
    let mut premises: Vec<Option<Premise>> = vec![None; cx.body.len()];
    enumerate_from(cx, &order, 0, &mut bindings, &mut premises, on_match)
}

/// The body's evaluation order, from [`crate::schedule`]: positive atoms first
/// (they bind variables), then negated atoms as anti-join filters, then the
/// builtins in **dependency** order.
///
/// Lowering has already rejected any body that cannot be scheduled, so a failure
/// here is malformed hand-built IR and surfaces as a structured error. The
/// scheduler is a pure function of the body, so lowering and both evaluators
/// derive the same order from the same input — shared the way `fold_aggregate`
/// is. Used for the top-level join and for each aggregate's sub-join.
fn literal_order(body: &[BodyLiteral]) -> Result<Vec<usize>> {
    crate::schedule::schedule_body(body).map_err(|failure| {
        Error::semantic(format!(
            "malformed IR: body literal {} can never run — variable slot {} is {}",
            failure.literal,
            failure.variable.0,
            match failure.cause {
                crate::schedule::ScheduleFailure::Unbound => "never bound",
                crate::schedule::ScheduleFailure::Cycle => "part of a circular dependency",
            }
        ))
    })
}

fn enumerate_from(
    cx: &JoinCx<'_>,
    order: &[usize],
    depth: usize,
    bindings: &mut [Option<Value>],
    premises: &mut [Option<Premise>],
    on_match: &mut OnMatch<'_>,
) -> Result<()> {
    if depth == order.len() {
        on_match(bindings, premises);
        return Ok(());
    }
    let idx = order[depth];
    match &cx.body[idx].kind {
        BodyLiteralKind::Atom(atom) => {
            let full = cx.model.relation(atom.pred);
            let atom_delta = cx.delta.get(&atom.pred);
            // The atom's constants and already-bound variables are a prefix of
            // leading columns, and a relation is ordered by column — so the
            // tuples that can match are one contiguous range, not the whole
            // relation ([`seek`], §17 2026-08-21). `try_match` still filters:
            // the range is exact on the prefix and a superset past it.
            let Some(prefix) = seek::bound_prefix(atom, bindings) else {
                return Ok(());
            };
            let candidates: Box<dyn Iterator<Item = &Tuple>> =
                match cx.views[idx] {
                    AtomView::Full => Box::new(seek::tuples_with_prefix(full, &prefix)),
                    AtomView::Delta => match atom_delta {
                        Some(delta) => Box::new(seek::tuples_with_prefix(delta, &prefix)),
                        None => return Ok(()),
                    },
                    AtomView::Old => Box::new(seek::tuples_with_prefix(full, &prefix).filter(
                        move |tuple| atom_delta.is_none_or(|delta| !delta.contains(*tuple)),
                    )),
                };
            for tuple in candidates {
                if let Some(bound) = try_match(atom, tuple, bindings) {
                    premises[idx] = Some(Premise::Fact(Fact {
                        pred: atom.pred,
                        tuple: tuple.clone(),
                    }));
                    let result = enumerate_from(cx, order, depth + 1, bindings, premises, on_match);
                    premises[idx] = None;
                    for slot in bound {
                        bindings[slot] = None;
                    }
                    result?;
                }
            }
        }
        BodyLiteralKind::NegAtom(atom) => {
            // Instantiate the no-match pattern: constants and bound variables
            // close a slot; an unbound slot is wildcard-fresh (validated)
            // and stays open — existential under the negation (§7).
            let pattern = NoMatchPattern {
                pred: atom.pred,
                args: atom
                    .args
                    .iter()
                    .map(|term| match term {
                        Term::Const(value) => Some(value.clone()),
                        Term::Var(var) => bindings[var.0 as usize].clone(),
                    })
                    .collect(),
            };
            // Always the full relation, never a delta view: the negated
            // predicate's strata are strictly lower (validated), so its
            // relation is complete and frozen here. A match refutes the
            // negation; no match records the no-match pattern and moves on. Negated
            // atoms bind nothing.
            if cx
                .model
                .relation(atom.pred)
                .iter()
                .any(|tuple| pattern.matches(tuple))
            {
                return Ok(());
            }
            premises[idx] = Some(Premise::NoMatch(pattern));
            let result = enumerate_from(cx, order, depth + 1, bindings, premises, on_match);
            premises[idx] = None;
            result?;
        }
        BodyLiteralKind::Compare { op, lhs, rhs } => {
            // A comparison is a filter; `=` with one bare unbound-variable side
            // is an assignment that binds it (§8). Runtime errors (overflow,
            // division by zero, NaN, type mismatch) short-circuit the whole
            // evaluation.
            match eval_compare(*op, lhs, rhs, bindings)? {
                CompareOutcome::Fail => {}
                CompareOutcome::Pass { lhs, rhs } => {
                    // A comparison filters; a conversion that failed inside one
                    // makes its operand absent, which makes the comparison false
                    // (§8), so the row never reaches here. That site is named in
                    // §12's *Not covered* rather than reported from a premise
                    // that does not exist.
                    premises[idx] = Some(Premise::Builtin {
                        op: *op,
                        lhs,
                        rhs,
                        lost: None,
                    });
                    let result = enumerate_from(cx, order, depth + 1, bindings, premises, on_match);
                    premises[idx] = None;
                    result?;
                }
                CompareOutcome::Bind { slot, value } => {
                    // An `=`-assignment is where every hoisted cast lands
                    // (`lower::ArgMode::Hoist` puts each compound argument here),
                    // so this is the one site that sees a conversion lose a value.
                    let lost = if value.is_absent() {
                        lost_conversions(lhs, rhs, bindings)
                    } else {
                        None
                    };
                    premises[idx] = Some(Premise::Builtin {
                        op: *op,
                        lhs: value.clone(),
                        rhs: value.clone(),
                        lost,
                    });
                    bindings[slot] = Some(value);
                    let result = enumerate_from(cx, order, depth + 1, bindings, premises, on_match);
                    premises[idx] = None;
                    bindings[slot] = None;
                    result?;
                }
            }
        }
        BodyLiteralKind::Presence { expr, negated } => {
            // A presence filter (§8): holds iff the operand is absent, flipped by
            // `negated`. Binds nothing. Range restriction guarantees the operand
            // is bound, so a runtime error here is only a malformed-IR unbound
            // operand, which short-circuits like any comparison error.
            let value = eval_expr(expr, bindings)?;
            let is_absent = value.is_absent();
            if is_absent != *negated {
                premises[idx] = Some(Premise::Presence {
                    value,
                    negated: *negated,
                });
                let result = enumerate_from(cx, order, depth + 1, bindings, premises, on_match);
                premises[idx] = None;
                result?;
            }
        }
        BodyLiteralKind::Aggregate {
            result,
            op,
            params: _,
            expr,
            goal,
        } => {
            // Evaluate the aggregate once per binding of the group keys — the
            // outer variables in `goal`, already bound by the positive atoms
            // scheduled before this builtin phase (§9). The goal's predicates are
            // strictly lower stratum (validated), so they are complete and frozen
            // in `model`: every goal atom reads the Full relation and the delta is
            // irrelevant. The sub-join shares `bindings`, so the group keys stay
            // fixed while the goal-local variables are enumerated and backtracked.
            let empty_delta: HashMap<PredId, BTreeSet<Tuple>> = HashMap::new();
            let goal_views = vec![AtomView::Full; goal.len()];
            let sub_cx = JoinCx {
                model: cx.model,
                delta: &empty_delta,
                body: goal,
                views: &goal_views,
            };
            let sub_order = literal_order(goal)?;
            let mut sub_premises: Vec<Option<Premise>> = vec![None; goal.len()];
            // The multiset of the collected expression across the goal's witness
            // tuples (§9: multiplicity follows distinct witnesses, not distinct
            // projected values).
            let mut values: Vec<Value> = Vec::new();
            let mut sub_error: Option<Error> = None;
            enumerate_from(
                &sub_cx,
                &sub_order,
                0,
                bindings,
                &mut sub_premises,
                &mut |witness, _| {
                    if sub_error.is_some() {
                        return;
                    }
                    match eval_expr(expr, witness) {
                        Ok(value) => values.push(value),
                        Err(error) => sub_error = Some(error),
                    }
                },
            )?;
            if let Some(error) = sub_error {
                return Err(error);
            }
            let outcome = fold_aggregate(*op, &values)?;
            premises[idx] = Some(Premise::Aggregate {
                op: *op,
                value: outcome.value.clone(),
                present: outcome.present,
                skipped: outcome.skipped,
            });
            bindings[result.0 as usize] = Some(outcome.value);
            let cont = enumerate_from(cx, order, depth + 1, bindings, premises, on_match);
            premises[idx] = None;
            bindings[result.0 as usize] = None;
            cont?;
        }
    }
    Ok(())
}

/// The result of folding an aggregate over its collected multiset (§9): the
/// value plus the counts that keep the absent-skip non-silent (recorded in
/// provenance).
#[derive(Debug)]
pub(crate) struct AggregateOutcome {
    pub(crate) value: Value,
    /// Present (non-absent) values folded.
    pub(crate) present: usize,
    /// Absent inputs skipped — always `0` for `count`, which counts them.
    pub(crate) skipped: usize,
}

/// Folds `values` (the collected expression over each witness tuple) per `op`,
/// honouring the §9 absent rules. Shared by the semi-naive engine and the naive
/// oracle so the two agree (testing.md B1).
///
/// - `count` counts every binding, absent ones included (skip `0`).
/// - `sum`/`avg`/`min`/`max` skip absents; an empty present-set yields `absent`.
/// - `sum` folds the multiset in §14 order — compensated over floats, wide over
///   ints and durations (§9) — and keeps §8's type rules; `avg` is the float
///   mean of the present values; `min`/`max` take the natural-order extreme of a
///   single ordered type.
///
/// The present values are folded in **§14 order**, not the order they were
/// collected in (`bugs/007`). §9 defines the aggregate over a *multiset*, so the
/// caller's enumeration order must not reach the answer — and it did: witnesses
/// arrive in the order the goal's literals happen to be scheduled, which is a
/// spelling §17 (2026-07-25) promises is irrelevant. Sorting is what makes that
/// promise true wherever the fold is not associative over the value type.
pub(crate) fn fold_aggregate(op: AggOp, values: &[Value]) -> Result<AggregateOutcome> {
    let total = values.len();
    let mut present: Vec<&Value> = values.iter().filter(|v| !v.is_absent()).collect();
    // One multiset, one answer. `Value`'s derived `Ord` is the total cross-type
    // order §14 already publishes, so this introduces no new notion of order —
    // and `count` reads `total`, so it is untouched by the sort.
    present.sort();
    let skipped = total - present.len();
    let value = match op {
        // A binding is a binding (§9): absent-valued ones are counted, not skipped.
        AggOp::Count => Value::Int(total as i64),
        AggOp::Sum => sum_values(&present)?,
        AggOp::Avg => avg_values(&present)?,
        AggOp::Min => extreme_value(&present, false)?,
        AggOp::Max => extreme_value(&present, true)?,
    };
    Ok(AggregateOutcome {
        value,
        present: present.len(),
        skipped: if op == AggOp::Count { 0 } else { skipped },
    })
}

/// Sums floats with **Neumaier compensation**, carrying the low-order bits the
/// naive fold drops. Shared by `sum_values` and `avg_values` so the two cannot
/// drift. B11's `avg_is_sum_over_count` cannot police that sharing — it states
/// the law over `int`, where both paths are exact and neither can be wrong — so
/// the float half of `a_float_sum_keeps_the_bits_a_naive_fold_drops` does:
/// measured, giving `avg_values` its own naive accumulator reddens it.
///
/// `fold_aggregate` sorts before folding, which fixes *which* association is
/// used (`bugs/007`); §14's order is by value, not by magnitude, so it is not
/// the numerically good one. Compensation is what makes the one canonical
/// answer also the accurate one: over `1e16, -1e16, 0.1` the sorted naive fold
/// answers `0.0` and this answers `0.1`.
fn compensated_sum(xs: impl Iterator<Item = f64>) -> f64 {
    let mut sum = 0.0f64;
    let mut compensation = 0.0f64;
    for x in xs {
        let t = sum + x;
        // Neumaier's branch, which unlike Kahan's is also right when the
        // incoming value is the larger of the two.
        compensation += if sum.abs() >= x.abs() {
            (sum - t) + x
        } else {
            (x - t) + sum
        };
        sum = t;
    }
    // `F64::new` rejects NaN but *permits* infinities, so a float sum that
    // overflows is already an answer today, not an error. An infinite
    // accumulator has an infinite correction term, and `inf + -inf` is NaN —
    // which would turn that answer into an error the naive fold never produced.
    // Compensating a non-finite total buys nothing anyway.
    if sum.is_finite() {
        sum + compensation
    } else {
        sum
    }
}

/// Sums present values through the §8 arithmetic (`int+int`/`float+float`,
/// checked overflow, type errors on a mix or a non-numeric). Empty → `absent`.
///
/// Each single-typed numeric run takes its own path — compensated for floats,
/// **wide** for ints and durations, where overflow is a property of the total
/// and not of an association nobody wrote (§9, §17 2026-08-20). **Every other
/// shape keeps the `apply_arith` fold**, which is what preserves the type errors
/// exactly: a mixed `int`/`float` list and a list of strings still fail where
/// and how they did. The single-element case falls through it untouched, so
/// `sum` over one `date` still returns that date rather than erroring (§9
/// rejects it in the type checker, ahead of here).
fn sum_values(present: &[&Value]) -> Result<Value> {
    let Some((first, rest)) = present.split_first() else {
        return Ok(Value::Absent);
    };
    if present.iter().all(|v| matches!(v, Value::Float(_))) {
        let total = compensated_sum(present.iter().map(|v| match v {
            Value::Float(f) => f.get(),
            // Unreachable: the `all` above is the guard.
            _ => unreachable!("every value in this arm is a float"),
        }));
        return F64::new(total).map(Value::Float);
    }
    // Ints and durations accumulate **wide** (§17, 2026-08-20): the partial sums
    // of a multiset fold are not observable, so only the total has to fit. An
    // `i128` cannot itself overflow here — it would take 2⁶⁴ terms.
    if present.iter().all(|v| matches!(v, Value::Int(_))) {
        let total: i128 = present
            .iter()
            .map(|v| match v {
                Value::Int(i) => i128::from(*i),
                _ => unreachable!("every value in this arm is an int"),
            })
            .sum();
        return i64::try_from(total).map(Value::Int).map_err(|_| {
            Error::semantic(format!(
                "arithmetic error: integer overflow — the sum of {} values is {total}, \
                 outside the int range",
                present.len(),
            ))
        });
    }
    if present.iter().all(|v| matches!(v, Value::Duration(_))) {
        let total: i128 = present
            .iter()
            .map(|v| match v {
                Value::Duration(d) => i128::from(d.micros()),
                _ => unreachable!("every value in this arm is a duration"),
            })
            .sum();
        return i64::try_from(total)
            .map(|micros| Value::Duration(temporal::Duration::from_micros(micros)))
            .map_err(|_| {
                Error::semantic(format!(
                    "arithmetic error: the sum of {} durations is outside the \
                     representable range",
                    present.len(),
                ))
            });
    }
    let mut acc = (*first).clone();
    for value in rest {
        acc = apply_arith(ArithOp::Add, acc, (*value).clone())?;
    }
    Ok(acc)
}

/// The mean of the present values: `float` over numbers, `duration` over
/// durations (§9). Empty → `absent`.
///
/// The two cases are one rule, not an exception: `avg` divides the fold by the
/// count, and §8 says dividing a duration by a number is scaling — so a mean of
/// durations is a duration, and only a mean of *numbers* has to leave its
/// operand type behind.
fn avg_values(present: &[&Value]) -> Result<Value> {
    if present.is_empty() {
        return Ok(Value::Absent);
    }
    if present.iter().all(|v| matches!(v, Value::Duration(_))) {
        let total = sum_values(present)?;
        return apply_arith(ArithOp::Div, total, Value::Int(present.len() as i64));
    }
    // Typed first, then folded, so the type error stays exactly where it was
    // while the fold itself is the same compensated one `sum_values` uses.
    let mut numbers: Vec<f64> = Vec::with_capacity(present.len());
    for value in present {
        numbers.push(match value {
            Value::Int(i) => *i as f64,
            Value::Float(f) => f.get(),
            other => {
                return Err(Error::semantic(format!(
                    "type error: avg requires values it can fold and divide — int, \
                     float, or duration — got {}",
                    value_type_name(other)
                )));
            }
        });
    }
    let sum = compensated_sum(numbers.into_iter());
    F64::new(sum / present.len() as f64).map(Value::Float)
}

/// The natural-order extreme (min or `max`) of the present values, which must
/// share one ordered type (a cross-type mix is a §8 comparison error). Empty →
/// `absent`.
fn extreme_value(present: &[&Value], want_max: bool) -> Result<Value> {
    let Some((first, rest)) = present.split_first() else {
        return Ok(Value::Absent);
    };
    let mut acc: &Value = first;
    for value in rest {
        let value: &Value = value;
        if std::mem::discriminant(acc) != std::mem::discriminant(value) {
            return Err(Error::semantic(format!(
                "type error: min/max requires values of the same type, got {} and {}",
                value_type_name(acc),
                value_type_name(value),
            )));
        }
        // Same-type non-absent values order by `Value`'s within-type `Ord` — the
        // natural order §8 comparisons use (floats via `F64`'s total order).
        if (want_max && value > acc) || (!want_max && value < acc) {
            acc = value;
        }
    }
    Ok(acc.clone())
}

/// Unifies an atom against a ground tuple under the current bindings.
/// Returns the slots newly bound here (for backtracking), or `None` on
/// mismatch (with any partial bindings already undone).
fn try_match(atom: &Atom, tuple: &Tuple, bindings: &mut [Option<Value>]) -> Option<Vec<usize>> {
    let mut bound: Vec<usize> = Vec::new();
    for (term, value) in atom.args.iter().zip(&tuple.0) {
        let matches = match term {
            Term::Const(constant) => constant.unifies_with(value),
            Term::Var(var) => {
                let slot = var.0 as usize;
                match &bindings[slot] {
                    // An already-bound value must semantically unify with the
                    // cell — `absent` unifies with nothing, so a slot bound to
                    // `absent` (or a cell that is `absent`) never re-matches.
                    Some(existing) => existing.unifies_with(value),
                    // A fresh slot binds to *whatever* is here, `absent`
                    // included — this is how a missing cell flows to the head
                    // (`recorded(F, N, A) :- measurement(…, amount: A)`). The
                    // binding is a value, so any *later* use of it unifies under
                    // the `absent`-matches-nothing rule above.
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

/// The effect of evaluating a comparison/assignment literal (§8) under the
/// current bindings.
enum CompareOutcome {
    /// A filter that held; carries the evaluated operands for the premise.
    Pass { lhs: Value, rhs: Value },
    /// A filter that did not hold — prune this branch.
    Fail,
    /// An `=`-assignment binding `slot` to `value`.
    Bind { slot: usize, value: Value },
}

/// Evaluates a comparison literal (§8): either an assignment (`=` with one bare
/// unbound-variable side) or a filter. Range restriction (§10) guarantees every
/// non-assignment operand variable is bound by the time this is scheduled.
fn eval_compare(
    op: CmpOp,
    lhs: &Expr,
    rhs: &Expr,
    bindings: &[Option<Value>],
) -> Result<CompareOutcome> {
    // Assignment: `=` where exactly one side is a bare, currently-unbound
    // variable and the other side fully evaluates (spec §17, 2026-07-21).
    if op == CmpOp::Eq {
        match (unbound_slot(lhs, bindings), unbound_slot(rhs, bindings)) {
            (Some(slot), None) => {
                let value = eval_expr(rhs, bindings)?;
                return Ok(CompareOutcome::Bind { slot, value });
            }
            (None, Some(slot)) => {
                let value = eval_expr(lhs, bindings)?;
                return Ok(CompareOutcome::Bind { slot, value });
            }
            // Both sides bare unbound variables cannot be assigned (neither
            // evaluates); range restriction rejects this, so it is malformed IR.
            // Fall through to the filter path, which surfaces the unbound operand.
            _ => {}
        }
    }
    let l = eval_expr(lhs, bindings)?;
    let r = eval_expr(rhs, bindings)?;
    if apply_compare(op, &l, &r)? {
        Ok(CompareOutcome::Pass { lhs: l, rhs: r })
    } else {
        Ok(CompareOutcome::Fail)
    }
}

/// The slot of a bare, currently-unbound variable expression, if that is what
/// `expr` is.
fn unbound_slot(expr: &Expr, bindings: &[Option<Value>]) -> Option<usize> {
    match expr {
        Expr::Term(Term::Var(var)) if bindings[var.0 as usize].is_none() => Some(var.0 as usize),
        _ => None,
    }
}

/// Applies a comparison to two evaluated operands. Strict: operands must share
/// a type (symbols never equal strings, ints never equal floats — §4); a
/// cross-type comparison is a structured type error, not a silent `false`. The
/// ordering used is the operand type's natural order (`Value`'s within-type
/// `Ord`).
fn apply_compare(op: CmpOp, lhs: &Value, rhs: &Value) -> Result<bool> {
    // Absent is two-valued (§8): a comparison with an absent operand is *false*
    // for every operator — not a cross-type error — so `X = 5` and `X != 5` are
    // both false when `X` is absent (which is why presence has its own operator,
    // `is [not] absent`). This precedes the same-type check below: `absent`
    // never reaches it.
    if lhs.is_absent() || rhs.is_absent() {
        return Ok(false);
    }
    if std::mem::discriminant(lhs) != std::mem::discriminant(rhs) {
        return Err(Error::semantic(format!(
            "type error: comparison `{}` requires operands of the same type, got {} and {}",
            cmp_symbol(op),
            value_type_name(lhs),
            value_type_name(rhs),
        )));
    }
    Ok(match op {
        CmpOp::Eq => lhs == rhs,
        CmpOp::Ne => lhs != rhs,
        CmpOp::Lt => lhs < rhs,
        CmpOp::Le => lhs <= rhs,
        CmpOp::Gt => lhs > rhs,
        CmpOp::Ge => lhs >= rhs,
    })
}

/// Evaluates an arithmetic expression to a value under the current bindings.
///
/// Exposed `pub(crate)` so lowering can constant-fold a ground fact argument
/// (`p(1+1).`) through the *same* §8 arithmetic — single source of truth for
/// strictness, overflow, and division-by-zero. Callers folding a constant pass
/// empty `bindings`; the expression must be variable-free (lowering checks this
/// first, since a `Var` here would index out of bounds).
pub(crate) fn eval_expr(expr: &Expr, bindings: &[Option<Value>]) -> Result<Value> {
    match expr {
        Expr::Term(Term::Const(value)) => Ok(value.clone()),
        Expr::Term(Term::Var(var)) => bindings[var.0 as usize].clone().ok_or_else(|| {
            Error::semantic(
                "malformed IR: arithmetic operand variable is not bound by the body".to_string(),
            )
        }),
        Expr::Binary { op, lhs, rhs } => {
            let l = eval_expr(lhs, bindings)?;
            let r = eval_expr(rhs, bindings)?;
            apply_arith(*op, l, r)
        }
        Expr::Cast { expr, ty } => apply_cast(eval_expr(expr, bindings)?, *ty),
        Expr::Builtin { op, args } => {
            let values = args
                .iter()
                .map(|arg| eval_expr(arg, bindings))
                .collect::<Result<Vec<_>>>()?;
            apply_builtin(*op, &values)
        }
    }
}

/// Counts the conversions (§8's `as`) that **failed on data** in an assignment
/// whose result came out `absent` — a value existed, could not be represented in
/// the target type, and became `absent` (§17, 2026-08-16).
///
/// Called only when the assignment produced `absent`, which is rare, so the
/// re-evaluation it does costs nothing on the ordinary path. The discriminator
/// is the one the value model does not keep: a cast whose *operand* is absent
/// annihilates (data that was missing stays missing), while a cast whose operand
/// is a real value and whose result is `absent` **lost** it.
///
/// Only the first losing target type is reported. Two different casts failing in
/// one assignment is a shape no program has needed to distinguish, and a warning
/// that lists types is harder to act on than one that names the count.
fn lost_conversions(lhs: &Expr, rhs: &Expr, bindings: &[Option<Value>]) -> Option<LostConversion> {
    let mut found: Option<LostConversion> = None;
    for expr in [lhs, rhs] {
        walk_lost(expr, bindings, &mut found);
    }
    found
}

/// Recurses through an expression, tallying casts that lost a value into `found`.
fn walk_lost(expr: &Expr, bindings: &[Option<Value>], found: &mut Option<LostConversion>) {
    match expr {
        Expr::Cast { expr: inner, ty } => {
            walk_lost(inner, bindings, found);
            let Ok(operand) = eval_expr(inner, bindings) else {
                return;
            };
            if operand.is_absent() {
                return; // annihilation: missing in, missing out
            }
            if let Ok(result) = apply_cast(operand, *ty)
                && result.is_absent()
            {
                match found {
                    Some(lost) if lost.to == *ty => lost.count += 1,
                    Some(_) => {}
                    None => *found = Some(LostConversion { to: *ty, count: 1 }),
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_lost(lhs, bindings, found);
            walk_lost(rhs, bindings, found);
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                walk_lost(arg, bindings, found);
            }
        }
        Expr::Term(_) => {}
    }
}

/// Applies a `std` module relation to its evaluated inputs (§13).
///
/// `absent` annihilates first, at the same choke point arithmetic and the cast
/// use: a missing input yields a missing output rather than an error, so a
/// sparse date column stays queryable.
fn apply_builtin(op: BuiltinOp, args: &[Value]) -> Result<Value> {
    if args.iter().any(Value::is_absent) {
        return Ok(Value::Absent);
    }
    match op {
        BuiltinOp::Truncate => truncate_value(args),
        _ => extract_component(op, args),
    }
}

/// `year`/`month`/`day`/`hour`/`minute`/`second` — one civil component of a
/// point in time, as an `int`.
fn extract_component(op: BuiltinOp, args: &[Value]) -> Result<Value> {
    let [value] = args else {
        return Err(Error::semantic(
            "malformed IR: an extraction takes exactly one input".to_string(),
        ));
    };
    let (year, month, day, hour, minute, second, _) = match value {
        Value::Date(date) => {
            let (y, m, d) = date.ymd();
            (y, m, d, 0, 0, 0, 0)
        }
        Value::Timestamp(timestamp) => timestamp.parts(),
        other => {
            return Err(Error::semantic(format!(
                "type error: `{}` reads a date or a timestamp, got {}",
                builtin_name(op),
                value_type_name(other),
            )));
        }
    };
    // A date has no time of day, and §4 gives it none — so asking for one is a
    // type error rather than a silent zero, which would read as midnight.
    if matches!(value, Value::Date(_))
        && matches!(op, BuiltinOp::Hour | BuiltinOp::Minute | BuiltinOp::Second)
    {
        return Err(Error::semantic(format!(
            "type error: `{}` reads a timestamp; a date has no time of day",
            builtin_name(op),
        ))
        .suggest("widen with `as timestamp` if midnight is the reading you want"));
    }
    Ok(Value::Int(match op {
        BuiltinOp::Year => year,
        BuiltinOp::Month => month,
        BuiltinOp::Day => day,
        BuiltinOp::Hour => hour,
        BuiltinOp::Minute => minute,
        BuiltinOp::Second => second,
        BuiltinOp::Truncate => unreachable!("handled by `truncate_value`"),
    }))
}

/// `truncate(V, unit, V')` — the start of the period containing `V`, at the
/// same point type. This is what `timestamp as date` refuses to be (§8), and
/// the only spelling that reaches a week or a quarter.
fn truncate_value(args: &[Value]) -> Result<Value> {
    let [value, unit] = args else {
        return Err(Error::semantic(
            "malformed IR: `truncate` takes a value and a unit".to_string(),
        ));
    };
    let Value::Symbol(unit) = unit else {
        return Err(Error::semantic(format!(
            "type error: `truncate`'s unit is a symbol, got {}",
            value_type_name(unit),
        )));
    };
    match value {
        Value::Date(date) => truncate_date(*date, unit).map(Value::Date),
        Value::Timestamp(timestamp) => {
            let (_, _, _, hour, minute, _, _) = timestamp.parts();
            match unit.as_str() {
                // Below a day the period lives inside the timestamp itself.
                "hour" | "minute" => {
                    let date = truncate_date(timestamp.date(), "day")?;
                    let kept = match unit.as_str() {
                        "hour" => hour * 3_600_000_000,
                        _ => hour * 3_600_000_000 + minute * 60_000_000,
                    };
                    date.at_midnight()
                        .shifted(kept)
                        .map(Value::Timestamp)
                        .ok_or_else(|| {
                            Error::semantic(
                                "arithmetic error: truncation left the representable range"
                                    .to_string(),
                            )
                        })
                }
                _ => truncate_date(timestamp.date(), unit).map(|date| {
                    // A truncated timestamp stays a timestamp: the result type
                    // follows the input's, so a group key never changes type
                    // with the unit.
                    Value::Timestamp(date.at_midnight())
                }),
            }
        }
        other => Err(Error::semantic(format!(
            "type error: `truncate` reads a date or a timestamp, got {}",
            value_type_name(other),
        ))),
    }
}

/// The civil day a date-level period starts on.
fn truncate_date(date: temporal::Date, unit: &str) -> Result<temporal::Date> {
    let (year, month, day) = date.ymd();
    let start = match unit {
        "year" => temporal::Date::from_ymd(year, 1, 1),
        "quarter" => temporal::Date::from_ymd(year, month - (month - 1) % 3, 1),
        "month" => temporal::Date::from_ymd(year, month, 1),
        // ISO-8601's week: Monday-based. 1970-01-01 was a Thursday — index 3
        // when Monday is 0 — so the day count is shifted by 3 before the
        // modulus.
        "week" => {
            let weekday = (date.days() as i64 + 3).rem_euclid(7);
            return temporal::Date::from_days(date.days() as i64 - weekday).map_err(|error| {
                Error::semantic(format!("arithmetic error: {}", error.message()))
            });
        }
        "day" => temporal::Date::from_ymd(year, month, day),
        other => {
            return Err(
                Error::semantic(format!("type error: `{other}` is not a truncation unit")).suggest(
                    format!("one of: {}", crate::stdlib::TRUNCATE_UNITS.join(", ")),
                ),
            );
        }
    };
    start.map_err(|error| Error::semantic(format!("arithmetic error: {}", error.message())))
}

/// The source spelling of a `std` relation, for messages.
fn builtin_name(op: BuiltinOp) -> &'static str {
    match op {
        BuiltinOp::Year => "year",
        BuiltinOp::Month => "month",
        BuiltinOp::Day => "day",
        BuiltinOp::Hour => "hour",
        BuiltinOp::Minute => "minute",
        BuiltinOp::Second => "second",
        BuiltinOp::Truncate => "truncate",
    }
}

/// Applies an explicit conversion `value as ty` (§8, ratified 2026-07-25;
/// failure semantics decided 2026-08-16).
///
/// Three outcomes, and which one applies is the whole design:
///
/// - **`absent` in, `absent` out** — annihilation, checked first, at the same
///   choke point as arithmetic's. A cast never manufactures a value for missing
///   data.
/// - **The conversion is undefined for this pair** (`true as int`) — a
///   structured error, exactly like [`apply_arith`]'s mixed-operand arm. This is
///   a program mistake, not a data failure.
/// - **The conversion is defined but this value fails it** — and here the two
///   failures part:
///   - *lossy*: a value exists and representing it would corrupt it
///     (`9007199254740993 as float`, `2.5 as int`) — a structured **error**,
///     the rule §13 already applies at the import boundary.
///   - *unrepresentable*: there is no value to represent (`"abc" as int`) —
///     **`absent`**. Erroring here would make a dirty column unqueryable, since
///     the language has no convertibility predicate to filter on (string
///     operations were *rejected*, §17 2026-07-27). Yielding absent puts the
///     guard back in the user's hands: `V = X as int, V is not absent`.
pub(super) fn apply_cast(value: Value, ty: TypeName) -> Result<Value> {
    // Absent annihilates (§8), *ahead* of every conversion check — including the
    // undefined-pair one, since `absent` inhabits any column and so belongs to
    // no source type.
    if value.is_absent() {
        return Ok(Value::Absent);
    }
    let undefined = || {
        // A duration against a number is the one undefined pair worth its own
        // message: it is the conversion §8 excludes *by construction* (a bare
        // number of what?), and the program wanted the divisor form instead.
        let unit_hazard = matches!(value, Value::Duration(_)) && is_numeric_type(ty)
            || matches!(value, Value::Int(_) | Value::Float(_)) && ty == TypeName::Duration;
        if unit_hazard {
            return Err(Error::semantic(format!(
                "type error: there is no conversion between duration and {}; divide by \
                 a duration to name the unit (`(C - O) / @1d` is a number of days)",
                ty.keyword(),
            )));
        }
        Err(Error::semantic(format!(
            "type error: there is no conversion from {} to {}; `as` converts \
             between numbers, between text and any other type, and from a date to \
             a timestamp",
            value_type_name(&value),
            ty.keyword(),
        )))
    };
    match ty {
        TypeName::Int => match &value {
            Value::Int(_) => Ok(value),
            Value::Float(f) => f64_as_exact_i64(f.get())
                .map(Value::Int)
                .ok_or_else(|| lossy(&value, ty)),
            Value::String(s) => Ok(match classify_cell(s) {
                CellClass::Int(n) => Value::Int(n),
                _ => Value::Absent,
            }),
            _ => undefined(),
        },
        TypeName::Float => match &value {
            Value::Float(_) => Ok(value),
            Value::Int(n) => widen(*n, &value),
            Value::String(s) => match classify_cell(s) {
                CellClass::Float(f) => F64::new(f).map(Value::Float),
                // An integral string widens under the same exactness rule a
                // literal int does — §13's import coercion reads a float column
                // the same way.
                CellClass::Int(n) => widen(n, &value),
                _ => Ok(Value::Absent),
            },
            _ => undefined(),
        },
        TypeName::Bool => match &value {
            Value::Bool(_) => Ok(value),
            Value::String(s) => Ok(match classify_cell(s) {
                CellClass::Bool(b) => Value::Bool(b),
                _ => Value::Absent,
            }),
            _ => undefined(),
        },
        // Every typed value has a canonical spelling, so rendering is total:
        // this is the one column of the table with no failure mode. It is also
        // the §14 spelling, so `V as string` and printing `V` agree.
        TypeName::String => Ok(Value::String(match &value {
            Value::String(_) => return Ok(value),
            // A temporal value renders **without** its `@` (§8): the sigil is a
            // delimiter the printer adds, exactly as a string's quotes are, and
            // dropping it is what makes `"2026-08-19" as date` — the shape a CSV
            // cell has — the inverse of this direction.
            Value::Date(d) => d.to_string(),
            Value::Timestamp(t) => t.to_string(),
            Value::Duration(d) => d.to_string(),
            other => crate::print::print_value(other),
        })),
        TypeName::Date => match &value {
            Value::Date(_) => Ok(value),
            Value::String(s) => Ok(read_temporal(temporal::parse_date(s), Value::Date)),
            // Truncation is lossy, and `as` never truncates (§8) — so this is
            // the one refusal that names another construct as its fix, because
            // the thing the program wants does exist.
            Value::Timestamp(_) => Err(Error::semantic(format!(
                "conversion error: `{}` has a time of day, and `as` does not truncate; \
                 use `truncate(T, day, D)` from `import \"std/time\".`",
                crate::print::print_value(&value),
            ))),
            _ => undefined(),
        },
        TypeName::Timestamp => match &value {
            Value::Timestamp(_) => Ok(value),
            // Exact, and exact only because a timestamp is civil: no zone can
            // move midnight (§4).
            Value::Date(d) => Ok(Value::Timestamp(d.at_midnight())),
            Value::String(s) => Ok(read_temporal(
                temporal::parse_timestamp(s),
                Value::Timestamp,
            )),
            _ => undefined(),
        },
        TypeName::Duration => match &value {
            Value::Duration(_) => Ok(value),
            Value::String(s) => Ok(read_temporal(temporal::parse_duration(s), Value::Duration)),
            _ => undefined(),
        },
        TypeName::Symbol => match &value {
            Value::Symbol(_) => Ok(value),
            // Not every string is a legal symbol — `"two words"` and `"Cap"`
            // are not identifiers — so this one can be unrepresentable.
            Value::String(s) => Ok(match classify_symbol(s) {
                Some(name) => Value::Symbol(name),
                None => Value::Absent,
            }),
            _ => undefined(),
        },
    }
}

/// Is this one of the two numeric types?
fn is_numeric_type(ty: TypeName) -> bool {
    matches!(ty, TypeName::Int | TypeName::Float)
}

/// A temporal read from text: the value, or `absent` when the text is not that
/// literal.
///
/// **Unrepresentable, not lossy** (§8): a cell that does not spell a date is the
/// data being dirty rather than the program being wrong, so it takes the same
/// branch `"abc" as int` does and stays guardable with `is not absent`.
fn read_temporal<T>(
    parsed: std::result::Result<T, crate::temporal::TemporalError>,
    wrap: fn(T) -> Value,
) -> Value {
    match parsed {
        Ok(value) => wrap(value),
        Err(_) => Value::Absent,
    }
}

/// `n` widened to a float, or the lossy-conversion error. `original` is the
/// value as written, so a string operand is reported as the string it was.
fn widen(n: i64, original: &Value) -> Result<Value> {
    match i64_as_exact_f64(n) {
        Some(f) => F64::new(f).map(Value::Float),
        None => Err(lossy(original, TypeName::Float)),
    }
}

/// The error for a conversion that exists but would not be exact (§8). Distinct
/// from `absent` on purpose: a value *is* here, and the engine refuses to
/// corrupt it rather than quietly reporting it missing.
fn lossy(value: &Value, ty: TypeName) -> Error {
    Error::semantic(format!(
        "conversion error: `{}` has no exact {} representation, and `as` does not \
         round (a rounded value silently breaks every join on it)",
        crate::print::print_value(value),
        ty.keyword(),
    ))
}

/// Applies an arithmetic operator. Strict (§4, spec §17 2026-07-21): `int op
/// int -> int` and `float op float -> float`; any mixed or non-numeric operand
/// is a structured type error.
fn apply_arith(op: ArithOp, lhs: Value, rhs: Value) -> Result<Value> {
    // Absent annihilates (§8), *ahead* of the type/div-by-zero/overflow checks:
    // `absent / 0` and `5 / absent` are both `absent`, never an error. This is
    // value-propagation, not a third truth value.
    if lhs.is_absent() || rhs.is_absent() {
        return Ok(Value::Absent);
    }
    match (&lhs, &rhs) {
        (Value::Int(a), Value::Int(b)) => arith_int(op, *a, *b),
        (Value::Float(a), Value::Float(b)) => arith_float(op, *a, *b),
        // §8's temporal algebra, the one heterogeneous case. Checked first as
        // a pair, so a `None` here means "not a temporal operation" and falls
        // through to the same type error every other mismatch gets.
        _ => match arith_temporal(op, &lhs, &rhs) {
            Some(result) => result,
            None => Err(Error::semantic(format!(
                "type error: arithmetic `{}` requires two ints, two floats, or one of \
                 §8's temporal operations, got {} and {}",
                arith_symbol(op),
                value_type_name(&lhs),
                value_type_name(&rhs),
            ))),
        },
    }
}

/// §8's temporal arithmetic: points and vectors.
///
/// `None` is "no rule for this pair", which the caller reports as the ordinary
/// type error. Every arm is checked — an overflow is a structured error, never
/// a wrap, exactly as integer arithmetic already is.
fn arith_temporal(op: ArithOp, lhs: &Value, rhs: &Value) -> Option<Result<Value>> {
    use ArithOp::{Add, Div, Mul, Sub};
    use Value::{Date, Duration, Float, Int, Timestamp};
    let overflow = || {
        Err(Error::semantic(format!(
            "arithmetic error: `{} {} {}` is outside the representable range",
            crate::print::print_value(lhs),
            arith_symbol(op),
            crate::print::print_value(rhs),
        )))
    };
    Some(match (op, lhs, rhs) {
        // Point − point is the displacement between them.
        (Sub, Date(a), Date(b)) => Ok(Duration(temporal::Duration::from_micros(
            a.micros_since(*b),
        ))),
        (Sub, Timestamp(a), Timestamp(b)) => match a.micros().checked_sub(b.micros()) {
            Some(micros) => Ok(Duration(temporal::Duration::from_micros(micros))),
            None => overflow(),
        },
        // Point ± displacement, in either order for `+`. A date shifts only by
        // a whole number of days: it has day precision, and rounding would be
        // the truncation `as` refuses (§8), so the error names the widening.
        (Add | Sub, Date(a), Duration(d)) | (Add, Duration(d), Date(a)) => {
            let delta = if op == Sub { -d.micros() } else { d.micros() };
            match a.shifted(delta) {
                Some(date) => Ok(Date(date)),
                None if delta % temporal::Date::DAY != 0 => Err(Error::semantic(format!(
                    "arithmetic error: `{}` is not a whole number of days, and a date has \
                     no time of day to carry the remainder",
                    crate::print::print_value(rhs),
                ))
                .suggest("widen the date first: `(D as timestamp) + @36h`")),
                None => overflow(),
            }
        }
        (Add | Sub, Timestamp(a), Duration(d)) | (Add, Duration(d), Timestamp(a)) => {
            let delta = if op == Sub { -d.micros() } else { d.micros() };
            match a.shifted(delta) {
                Some(timestamp) => Ok(Timestamp(timestamp)),
                None => overflow(),
            }
        }
        // Displacements add to each other.
        (Add | Sub, Duration(a), Duration(b)) => {
            let combined = if op == Sub {
                a.micros().checked_sub(b.micros())
            } else {
                a.micros().checked_add(b.micros())
            };
            match combined {
                Some(micros) => Ok(Duration(temporal::Duration::from_micros(micros))),
                None => overflow(),
            }
        }
        // The units cancel — the only route from a duration to a number, and
        // the reason there is no `duration as float` (§8).
        (Div, Duration(a), Duration(b)) => {
            if b.micros() == 0 {
                return Some(Err(Error::semantic(
                    "arithmetic error: division by a zero duration".to_string(),
                )));
            }
            Some(F64::new(a.micros() as f64 / b.micros() as f64).map(Value::Float))?
        }
        // Scaling preserves the unit. Truncating toward zero at the
        // microsecond, which is §8's integer division applied to the count.
        (Mul, Duration(a), Int(n)) | (Mul, Int(n), Duration(a)) => {
            match a.micros().checked_mul(*n) {
                Some(micros) => Ok(Duration(temporal::Duration::from_micros(micros))),
                None => overflow(),
            }
        }
        (Div, Duration(a), Int(n)) => {
            if *n == 0 {
                return Some(Err(Error::semantic(
                    "arithmetic error: division by zero".to_string(),
                )));
            }
            Ok(Duration(temporal::Duration::from_micros(a.micros() / n)))
        }
        (Mul, Duration(a), Float(f)) | (Mul, Float(f), Duration(a)) => scale(*a, f.get(), overflow),
        (Div, Duration(a), Float(f)) => {
            if f.get() == 0.0 {
                return Some(Err(Error::semantic(
                    "arithmetic error: division by zero".to_string(),
                )));
            }
            scale(*a, 1.0 / f.get(), overflow)
        }
        _ => return None,
    })
}

/// A duration scaled by a float, truncating toward zero and refusing a result
/// no `i64` of microseconds holds.
fn scale(
    duration: temporal::Duration,
    factor: f64,
    overflow: impl Fn() -> Result<Value>,
) -> Result<Value> {
    let scaled = duration.micros() as f64 * factor;
    if !scaled.is_finite() || scaled.abs() >= i64::MAX as f64 {
        return overflow();
    }
    Ok(Value::Duration(temporal::Duration::from_micros(
        scaled.trunc() as i64,
    )))
}

/// Integer arithmetic: truncating division, checked overflow and division by
/// zero (spec §17, 2026-07-21) — each a structured error rather than a wrap or
/// panic.
fn arith_int(op: ArithOp, a: i64, b: i64) -> Result<Value> {
    let checked = match op {
        ArithOp::Add => a.checked_add(b),
        ArithOp::Sub => a.checked_sub(b),
        ArithOp::Mul => a.checked_mul(b),
        ArithOp::Div => {
            if b == 0 {
                return Err(Error::semantic(format!(
                    "arithmetic error: division by zero in `{a} / {b}`"
                )));
            }
            a.checked_div(b)
        }
    };
    checked.map(Value::Int).ok_or_else(|| {
        Error::semantic(format!(
            "arithmetic error: integer overflow in `{a} {} {b}`",
            arith_symbol(op)
        ))
    })
}

/// Float arithmetic: ordinary IEEE operations, with a NaN result (e.g.
/// `0.0 / 0.0`) rejected as a structured error via [`F64::new`].
fn arith_float(op: ArithOp, a: F64, b: F64) -> Result<Value> {
    let (x, y) = (a.get(), b.get());
    let result = match op {
        ArithOp::Add => x + y,
        ArithOp::Sub => x - y,
        ArithOp::Mul => x * y,
        ArithOp::Div => x / y,
    };
    F64::new(result).map(Value::Float)
}

/// The name of a value's type, for error messages.
fn value_type_name(value: &Value) -> &'static str {
    match value {
        // Absent short-circuits ahead of every arithmetic/comparison type
        // error, so this arm is only reachable defensively.
        Value::Absent => "absent",
        Value::Symbol(_) => "symbol",
        Value::String(_) => "string",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Bool(_) => "bool",
        Value::Date(_) => "date",
        Value::Timestamp(_) => "timestamp",
        Value::Duration(_) => "duration",
    }
}

/// The source symbol of a comparison operator, for error messages.
fn cmp_symbol(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Eq => "=",
        CmpOp::Ne => "!=",
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
    }
}

/// The source symbol of an arithmetic operator, for error messages.
fn arith_symbol(op: ArithOp) -> &'static str {
    match op {
        ArithOp::Add => "+",
        ArithOp::Sub => "-",
        ArithOp::Mul => "*",
        ArithOp::Div => "/",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::ast::{CmpOp, Span};
    use crate::ir::fixtures::{example_16_1, fact2, string_value};
    use crate::ir::{Expr, ImportSpec, PredicateInfo, Var};

    // --- Absent value semantics (§4/§8) ---

    #[test]
    fn absent_annihilates_in_arithmetic_ahead_of_every_check() {
        use crate::ir::F64;
        let a = Value::Absent;
        // Annihilation over each operator, on either side.
        for op in [ArithOp::Add, ArithOp::Sub, ArithOp::Mul, ArithOp::Div] {
            assert_eq!(apply_arith(op, a.clone(), Value::Int(3)).unwrap(), a);
            assert_eq!(apply_arith(op, Value::Int(3), a.clone()).unwrap(), a);
        }
        // Precedence over the edge cases: no division-by-zero, no overflow, no
        // NaN, no cross-type error — all yield absent.
        assert_eq!(
            apply_arith(ArithOp::Div, Value::Int(5), a.clone()).unwrap(),
            a
        );
        assert_eq!(
            apply_arith(ArithOp::Div, a.clone(), Value::Int(0)).unwrap(),
            a
        );
        assert_eq!(apply_arith(ArithOp::Add, a.clone(), a.clone()).unwrap(), a);
        assert_eq!(
            apply_arith(ArithOp::Mul, a.clone(), Value::String("x".into())).unwrap(),
            a
        );
        assert_eq!(
            apply_arith(
                ArithOp::Add,
                a.clone(),
                Value::Float(F64::new(1.0).unwrap())
            )
            .unwrap(),
            a
        );
    }

    #[test]
    fn every_comparison_with_absent_is_false_never_an_error() {
        let a = Value::Absent;
        for op in [
            CmpOp::Eq,
            CmpOp::Ne,
            CmpOp::Lt,
            CmpOp::Le,
            CmpOp::Gt,
            CmpOp::Ge,
        ] {
            // Against a value, either side — including `!=`, which is *not* the
            // negation of `=` here: both are false.
            assert!(!apply_compare(op, &a, &Value::Int(5)).unwrap());
            assert!(!apply_compare(op, &Value::Int(5), &a).unwrap());
            // Against a cross-type operand: still false, not a type error.
            assert!(!apply_compare(op, &a, &Value::String("s".into())).unwrap());
            // Against another absent: still false (semantic absent ≠ absent).
            assert!(!apply_compare(op, &a, &a).unwrap());
        }
    }

    #[test]
    fn values_unify_is_structural_equality_minus_absent() {
        // Equal values unify; unequal do not.
        assert!(Value::Int(1).unifies_with(&Value::Int(1)));
        assert!(!Value::Int(1).unifies_with(&Value::Int(2)));
        // Absent unifies with nothing — not a value, not another absent — even
        // though it is *structurally* equal to itself (set dedup relies on that).
        assert!(!Value::Absent.unifies_with(&Value::Int(1)));
        assert!(!Value::Int(1).unifies_with(&Value::Absent));
        assert!(!Value::Absent.unifies_with(&Value::Absent));
        assert_eq!(Value::Absent, Value::Absent); // structural: still equal
    }

    #[test]
    fn try_match_binds_a_var_to_a_stored_absent_but_never_rematches_it() {
        // A fresh slot binds to a stored absent cell (missing value flows on).
        let pred = PredId(0);
        let atom = Atom {
            pred,
            args: vec![Term::Var(Var(0))],
        };
        let tuple = Tuple(vec![Value::Absent]);
        let mut bindings = vec![None];
        let bound = try_match(&atom, &tuple, &mut bindings).expect("binds");
        assert_eq!(bindings[0], Some(Value::Absent));
        assert_eq!(bound, vec![0]);

        // But `p(X, X)` on `(absent, absent)` does not match: the second X,
        // already bound to absent, unifies with nothing.
        let atom_xx = Atom {
            pred,
            args: vec![Term::Var(Var(0)), Term::Var(Var(0))],
        };
        let tuple_xx = Tuple(vec![Value::Absent, Value::Absent]);
        let mut bindings = vec![None];
        assert!(try_match(&atom_xx, &tuple_xx, &mut bindings).is_none());
        assert_eq!(bindings[0], None, "partial binding is undone on mismatch");
    }

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
                    field_types: None,
                },
                PredicateInfo {
                    name: "path".to_string(),
                    arity: 2,
                    fields: None,
                    field_types: None,
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
                    premises: vec![
                        Premise::Fact(fact2(edge, "a", "b")),
                        Premise::Fact(fact2(path, "b", "d")),
                    ],
                },
                Derivation {
                    rule: RuleId(1),
                    premises: vec![
                        Premise::Fact(fact2(edge, "a", "c")),
                        Premise::Fact(fact2(path, "c", "d")),
                    ],
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
                premises: vec![Premise::Fact(fact2(edge, "a", "b"))],
            }]
        );
    }

    /// An `ImportSpec` is inert metadata at eval time: the imported rows are
    /// already base facts, so a program carrying one evaluates like any other
    /// (§13). This replaces the pre-§13 "imports are rejected" pin.
    #[test]
    fn an_import_spec_with_its_facts_evaluates() {
        let mut program = Program::default();
        let edge = program.intern_pred("edge", 2);
        program.imports.push(ImportSpec {
            pred: edge,
            path: "edges.csv".to_string(),
            span: Span::DUMMY,
        });
        program.facts.push(fact2(edge, "a", "b"));
        let model = eval(&program).expect("a program with an import and its facts evaluates");
        assert_eq!(model.relation(edge).len(), 1);
    }

    #[test]
    fn comparison_literal_filters_matches() {
        // q(X) :- p(X), X < 2.  with p(0..=3)  =>  q(0), q(1).
        let mut program = Program::default();
        let p = program.intern_pred("p", 1);
        let q = program.intern_pred("q", 1);
        for n in 0..=3 {
            program.facts.push(Fact {
                pred: p,
                tuple: Tuple(vec![Value::Int(n)]),
            });
        }
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
        let model = eval(&program).unwrap();
        let expected: BTreeSet<Tuple> = [0, 1].map(|n| Tuple(vec![Value::Int(n)])).into();
        assert_eq!(model.relation(q), &expected);
    }

    /// Spec §16.3 end-to-end over hand-built IR: a comparison filter
    /// (`adult`), a join with a filter (`older`), and an `=`-assignment
    /// (`next_year`). Exercises all three §8 body shapes at once.
    #[test]
    fn example_16_3_comparisons_and_assignment_evaluate() {
        let mut program = Program::default();
        let age = program.intern_pred("age", 2);
        let adult = program.intern_pred("adult", 1);
        let older = program.intern_pred("older", 2);
        let next_year = program.intern_pred("next_year", 2);
        for (name, years) in [("alice", 30), ("bob", 15), ("carol", 42)] {
            program.facts.push(Fact {
                pred: age,
                tuple: Tuple(vec![string_value(name), Value::Int(years)]),
            });
        }
        // adult(X) :- age(X, A), A >= 18.
        program.rules.push(Rule {
            head: Atom {
                pred: adult,
                args: vec![Term::Var(Var(0))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: age,
                        args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Compare {
                        op: CmpOp::Ge,
                        lhs: Expr::Term(Term::Var(Var(1))),
                        rhs: Expr::Term(Term::Const(Value::Int(18))),
                    },
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![Some("X".to_string()), Some("A".to_string())],
            span: Span::DUMMY,
        });
        // older(X, Y) :- age(X, A), age(Y, B), A > B.
        program.rules.push(Rule {
            head: Atom {
                pred: older,
                args: vec![Term::Var(Var(0)), Term::Var(Var(2))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: age,
                        args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: age,
                        args: vec![Term::Var(Var(2)), Term::Var(Var(3))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Compare {
                        op: CmpOp::Gt,
                        lhs: Expr::Term(Term::Var(Var(1))),
                        rhs: Expr::Term(Term::Var(Var(3))),
                    },
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![
                Some("X".to_string()),
                Some("A".to_string()),
                Some("Y".to_string()),
                Some("B".to_string()),
            ],
            span: Span::DUMMY,
        });
        // next_year(X, N) :- age(X, A), N = A + 1.
        program.rules.push(Rule {
            head: Atom {
                pred: next_year,
                args: vec![Term::Var(Var(0)), Term::Var(Var(2))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: age,
                        args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Compare {
                        op: CmpOp::Eq,
                        lhs: Expr::Term(Term::Var(Var(2))),
                        rhs: Expr::Binary {
                            op: ArithOp::Add,
                            lhs: Box::new(Expr::Term(Term::Var(Var(1)))),
                            rhs: Box::new(Expr::Term(Term::Const(Value::Int(1)))),
                        },
                    },
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![
                Some("X".to_string()),
                Some("A".to_string()),
                Some("N".to_string()),
            ],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0), RuleId(1), RuleId(2)]];

        let model = eval(&program).unwrap();

        let expected_adult: BTreeSet<Tuple> = ["alice", "carol"]
            .map(|n| Tuple(vec![string_value(n)]))
            .into();
        assert_eq!(model.relation(adult), &expected_adult);

        let expected_older: BTreeSet<Tuple> =
            [("alice", "bob"), ("carol", "alice"), ("carol", "bob")]
                .map(|(x, y)| Tuple(vec![string_value(x), string_value(y)]))
                .into();
        assert_eq!(model.relation(older), &expected_older);

        let expected_next: BTreeSet<Tuple> = [("alice", 31), ("bob", 16), ("carol", 43)]
            .map(|(n, y)| Tuple(vec![string_value(n), Value::Int(y)]))
            .into();
        assert_eq!(model.relation(next_year), &expected_next);
    }

    /// Evaluates `t(M) :- seed(0), M = lhs <op> rhs.` and returns the error the
    /// single forced arithmetic evaluation raises. Shared by the arithmetic
    /// error-path tests below.
    fn arithmetic_error(lhs: Value, op: ArithOp, rhs: Value) -> Error {
        let mut program = Program::default();
        let seed = program.intern_pred("seed", 1);
        let t = program.intern_pred("t", 1);
        program.facts.push(Fact {
            pred: seed,
            tuple: Tuple(vec![Value::Int(0)]),
        });
        program.rules.push(Rule {
            head: Atom {
                pred: t,
                args: vec![Term::Var(Var(1))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: seed,
                        args: vec![Term::Var(Var(0))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Compare {
                        op: CmpOp::Eq,
                        lhs: Expr::Term(Term::Var(Var(1))),
                        rhs: Expr::Binary {
                            op,
                            lhs: Box::new(Expr::Term(Term::Const(lhs))),
                            rhs: Box::new(Expr::Term(Term::Const(rhs))),
                        },
                    },
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![Some("V".to_string()), Some("M".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        eval(&program).unwrap_err()
    }

    /// What §8 says a conversion yields, restated here so this test can
    /// disagree with the engine.
    #[derive(Debug, PartialEq)]
    enum Conversion {
        /// The named value.
        Is(Value),
        /// `absent` — the conversion is defined but there is no value to
        /// represent (`"abc" as int`).
        Missing,
        /// A value exists and representing it would corrupt it (`2.5 as int`).
        Lossy,
        /// No conversion exists between these two types (`true as int`).
        Undefined,
    }

    /// §8's conversion table, transcribed from the spec rather than derived from
    /// the code — the *independent* oracle `testing.md` asks for, since a
    /// differential against another evaluator would agree with a wrong table
    /// forever. Every cell of the 5×5 grid is reached, plus `absent` and the
    /// edge values each column can fail on.
    ///
    /// **Mutation-verified**: dropping the `f.fract() != 0.0` guard in
    /// `ir::f64_as_exact_i64` turns the `2.5 as int` row from `Lossy` into
    /// `Is(Int(2))` and reddens this test; so does swapping the `absent` arm of
    /// `apply_cast` below the undefined-pair check.
    #[test]
    fn the_conversion_table_matches_section_8() {
        use Conversion::*;
        use TypeName::*;

        fn float(f: f64) -> Value {
            Value::Float(F64::new(f).unwrap())
        }
        fn string(s: &str) -> Value {
            Value::String(s.to_string())
        }
        fn symbol(s: &str) -> Value {
            Value::Symbol(s.to_string())
        }

        // 2^53 + 1: the smallest positive integer with no exact `f64`.
        let inexact: i64 = 9_007_199_254_740_993;

        // §4's own worked examples, so the expected text is the spec's
        // spelling rather than this module's rendering of it.
        let date = temporal::Date::from_ymd(2026, 8, 19).expect("a real day");
        let noon =
            temporal::Timestamp::from_parts(2026, 8, 19, 12, 30, 0, 0).expect("parts are in range");
        let day_and_half = temporal::Duration::from_micros(129_600_000_000);

        // (value, [int, float, string, symbol, bool, date, timestamp, duration])
        //
        // The temporal columns are transcribed from §8's second table
        // (spec.md, "Temporal conversions"): a temporal renders to `string` and
        // reads back from one, `date as timestamp` widens exactly,
        // `timestamp as date` is a *lossy* structured error, and **every other
        // pair is a `—`** — in particular `duration` against `int` or `float`
        // in both directions, which §8 excludes deliberately because a duration
        // rendered as a bare number is a number of nothing.
        let table: Vec<(Value, [Conversion; 8])> = vec![
            // `absent` annihilates into every column (§4/§8) — it inhabits any
            // type, so it is never an undefined pair.
            (
                Value::Absent,
                [
                    Missing, Missing, Missing, Missing, Missing, Missing, Missing, Missing,
                ],
            ),
            (
                Value::Int(30),
                [
                    Is(Value::Int(30)),
                    Is(float(30.0)),
                    Is(string("30")),
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                ],
            ),
            (
                Value::Int(inexact),
                [
                    Is(Value::Int(inexact)),
                    // Above 2⁵³ the widening is not exact, and §8 refuses rather
                    // than rounds — §13's import rule, in-language.
                    Lossy,
                    Is(string("9007199254740993")),
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                ],
            ),
            (
                float(2.0),
                [
                    Is(Value::Int(2)),
                    Is(float(2.0)),
                    Is(string("2.0")),
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                ],
            ),
            (
                float(2.5),
                [
                    // Narrowing obeys the same rule as widening: exact or error.
                    // `as` does not truncate and does not round.
                    Lossy,
                    Is(float(2.5)),
                    Is(string("2.5")),
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                ],
            ),
            // Text converts by the language's own literal grammar: `"30" as int`
            // is `30` exactly when writing `30` would be that literal. What that
            // grammar does not read is `absent`, never an error.
            (
                string("30"),
                [
                    Is(Value::Int(30)),
                    Is(float(30.0)),
                    Is(string("30")),
                    Missing,
                    Missing,
                    Missing,
                    Missing,
                    Missing,
                ],
            ),
            (
                string("2.5"),
                [
                    Missing,
                    Is(float(2.5)),
                    Is(string("2.5")),
                    Missing,
                    Missing,
                    Missing,
                    Missing,
                    Missing,
                ],
            ),
            (
                string("true"),
                [
                    Missing,
                    Missing,
                    Is(string("true")),
                    // `true` lexes as the reserved literal, not an identifier.
                    Missing,
                    Is(Value::Bool(true)),
                    Missing,
                    Missing,
                    Missing,
                ],
            ),
            (
                string("abc"),
                [
                    Missing,
                    Missing,
                    Is(string("abc")),
                    Is(symbol("abc")),
                    Missing,
                    Missing,
                    Missing,
                    Missing,
                ],
            ),
            (
                // Not an identifier, so not a symbol either.
                string("two words"),
                [
                    Missing,
                    Missing,
                    Is(string("two words")),
                    Missing,
                    Missing,
                    Missing,
                    Missing,
                    Missing,
                ],
            ),
            (
                symbol("red"),
                [
                    Undefined,
                    Undefined,
                    Is(string("red")),
                    Is(symbol("red")),
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                ],
            ),
            (
                Value::Bool(true),
                [
                    Undefined,
                    Undefined,
                    Is(string("true")),
                    Undefined,
                    Is(Value::Bool(true)),
                    Undefined,
                    Undefined,
                    Undefined,
                ],
            ),
            // --- §8's temporal rows (2026-08-20) ---
            //
            // The `string` cell is the **unsigilled** canonical spelling: the
            // `@` is a delimiter the printer supplies, as a string's quotes
            // are, and dropping it is what makes render and read inverse
            // against the text a CSV cell actually holds (§8, §13).
            (
                Value::Date(date),
                [
                    Undefined,
                    Undefined,
                    Is(string("2026-08-19")),
                    Undefined,
                    Undefined,
                    Is(Value::Date(date)),
                    // The one widening §8 allows: midnight of that day.
                    Is(Value::Timestamp(date.at_midnight())),
                    Undefined,
                ],
            ),
            (
                Value::Timestamp(noon),
                [
                    Undefined,
                    Undefined,
                    Is(string("2026-08-19T12:30:00")),
                    Undefined,
                    Undefined,
                    // The one conversion §8 rejects for being *lossy* rather
                    // than meaningless — `truncate(T, day, D)` is the construct
                    // that does it, and the error says so.
                    Lossy,
                    Is(Value::Timestamp(noon)),
                    Undefined,
                ],
            ),
            (
                Value::Duration(day_and_half),
                [
                    // §8's deliberate hazard exclusion: no conversion between a
                    // duration and a number **in either direction**. This row
                    // and the `string` rows below are the whole of it.
                    Undefined,
                    Undefined,
                    Is(string("1d12h")),
                    Undefined,
                    Undefined,
                    Undefined,
                    Undefined,
                    Is(Value::Duration(day_and_half)),
                ],
            ),
            // --- the reading direction, per §3's grammar minus the `@` ---
            (
                string("2026-08-19"),
                [
                    Missing,
                    Missing,
                    Is(string("2026-08-19")),
                    Missing,
                    Missing,
                    Is(Value::Date(date)),
                    // §3's timestamp grammar wants a time component; a bare
                    // date is not one, so this reads nothing rather than
                    // widening. `as date as timestamp` is the spelling that
                    // widens.
                    Missing,
                    Missing,
                ],
            ),
            (
                string("2026-08-19T12:30:00"),
                [
                    Missing,
                    Missing,
                    Is(string("2026-08-19T12:30:00")),
                    Missing,
                    Missing,
                    Missing,
                    Is(Value::Timestamp(noon)),
                    Missing,
                ],
            ),
            (
                string("1d12h"),
                [
                    Missing,
                    Missing,
                    Is(string("1d12h")),
                    Missing,
                    Missing,
                    Missing,
                    Missing,
                    Is(Value::Duration(day_and_half)),
                ],
            ),
        ];

        for (value, expected) in table {
            for (ty, want) in [Int, Float, String, Symbol, Bool, Date, Timestamp, Duration]
                .into_iter()
                .zip(expected)
            {
                let got = match apply_cast(value.clone(), ty) {
                    Ok(Value::Absent) => Missing,
                    Ok(v) => Is(v),
                    Err(e) if e.to_string().contains("conversion error") => Lossy,
                    Err(e) if e.to_string().contains("type error") => Undefined,
                    Err(e) => panic!("unclassified error for {value:?} as {ty:?}: {e}"),
                };
                assert_eq!(
                    got,
                    want,
                    "`{} as {}` disagrees with spec.md §8",
                    crate::print::print_value(&value),
                    ty.keyword()
                );
            }
        }
    }

    #[test]
    fn division_by_zero_is_a_structured_error() {
        let err = arithmetic_error(Value::Int(1), ArithOp::Div, Value::Int(0));
        assert!(
            err.to_string().contains("division by zero"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn integer_overflow_is_a_structured_error() {
        let err = arithmetic_error(Value::Int(i64::MAX), ArithOp::Add, Value::Int(1));
        assert!(
            err.to_string().contains("integer overflow"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn nan_producing_arithmetic_is_a_structured_error() {
        let zero = F64::new(0.0).unwrap();
        let err = arithmetic_error(Value::Float(zero), ArithOp::Div, Value::Float(zero));
        assert!(err.to_string().contains("NaN"), "unexpected error: {err}");
    }

    #[test]
    fn mixed_numeric_types_are_a_structured_error() {
        let err = arithmetic_error(
            Value::Int(1),
            ArithOp::Add,
            Value::Float(F64::new(2.0).unwrap()),
        );
        assert!(
            err.to_string().contains("type error"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn cross_type_comparison_is_a_structured_error() {
        // t(X) :- seed(X), X > "a".  seed(0):int, "a":string  =>  type error.
        let mut program = Program::default();
        let seed = program.intern_pred("seed", 1);
        let t = program.intern_pred("t", 1);
        program.facts.push(Fact {
            pred: seed,
            tuple: Tuple(vec![Value::Int(0)]),
        });
        program.rules.push(Rule {
            head: Atom {
                pred: t,
                args: vec![Term::Var(Var(0))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: seed,
                        args: vec![Term::Var(Var(0))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Compare {
                        op: CmpOp::Gt,
                        lhs: Expr::Term(Term::Var(Var(0))),
                        rhs: Expr::Term(Term::Const(Value::String("a".to_string()))),
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
            err.to_string().contains("type error"),
            "unexpected error: {err}"
        );
    }

    /// Spec §16.2 end-to-end: the model, the exact recorded derivation with
    /// its no-match pattern, and the proof tree terminating at a `NoMatch`
    /// leaf. "alice" is the only person no `parent(_, x)` fact points at.
    #[test]
    fn example_16_2_evaluates_with_no_match_provenance() {
        let program = crate::ir::fixtures::example_16_2();
        let person = PredId(0);
        let parent = PredId(1);
        let root = PredId(2);
        let model = eval(&program).unwrap();

        let alice = Fact {
            pred: root,
            tuple: Tuple(vec![string_value("alice")]),
        };
        let expected: BTreeSet<Tuple> = [Tuple(vec![string_value("alice")])].into();
        assert_eq!(model.relation(root), &expected);

        // The derivation records the matched person fact and the pattern
        // whose absence satisfied `not parent(_, X)`: wildcard slot open,
        // `X` closed to "alice".
        let absent = NoMatchPattern {
            pred: parent,
            args: vec![None, Some(string_value("alice"))],
        };
        let derivations: Vec<Derivation> = model.derivations_of(&alice).cloned().collect();
        assert_eq!(
            derivations,
            vec![Derivation {
                rule: RuleId(0),
                premises: vec![
                    Premise::Fact(Fact {
                        pred: person,
                        tuple: Tuple(vec![string_value("alice")]),
                    }),
                    Premise::NoMatch(absent.clone()),
                ],
            }]
        );

        use crate::provenance::ProofTree;
        assert_eq!(
            ProofTree::explain(&model, &alice),
            Some(ProofTree::Derived {
                fact: alice,
                rule: RuleId(0),
                children: vec![
                    ProofTree::Leaf(Fact {
                        pred: person,
                        tuple: Tuple(vec![string_value("alice")]),
                    }),
                    ProofTree::NoMatch(absent),
                ],
            })
        );

        // Hand-run differential until the generator emits negation (C3):
        // the per-stratum naive oracle agrees.
        assert_eq!(
            naive::naive_eval(&program).unwrap(),
            model.facts().collect::<BTreeSet<Fact>>()
        );
    }

    /// Negation over a *recursive* IDB predicate: `path` closes in stratum 0,
    /// then `no_path` reads its frozen extent in stratum 1. Also exercises
    /// negation-before-binder scheduling: the negated literal sits first in
    /// the body, so left-to-right evaluation would misread it.
    #[test]
    fn negation_over_recursive_idb_uses_the_frozen_extent() {
        // node("a"). node("b"). node("c"). edge("a", "b"). edge("b", "c").
        // path(X, Y) :- edge(X, Y).
        // path(X, Y) :- edge(X, Z), path(Z, Y).
        // no_path(X, Y) :- not path(X, Y), node(X), node(Y).
        let mut program = Program::default();
        let node = program.intern_pred("node", 1);
        let edge = program.intern_pred("edge", 2);
        let path = program.intern_pred("path", 2);
        let no_path = program.intern_pred("no_path", 2);
        for name in ["a", "b", "c"] {
            program.facts.push(Fact {
                pred: node,
                tuple: Tuple(vec![string_value(name)]),
            });
        }
        program.facts.push(fact2(edge, "a", "b"));
        program.facts.push(fact2(edge, "b", "c"));
        program.rules.push(Rule {
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
        });
        program.rules.push(Rule {
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
        });
        program.rules.push(Rule {
            head: Atom {
                pred: no_path,
                args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::NegAtom(Atom {
                        pred: path,
                        args: vec![Term::Var(Var(0)), Term::Var(Var(1))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: node,
                        args: vec![Term::Var(Var(0))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: node,
                        args: vec![Term::Var(Var(1))],
                    }),
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![Some("X".to_string()), Some("Y".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0), RuleId(1)], vec![RuleId(2)]];

        let model = eval(&program).unwrap();
        // path = {(a,b), (b,c), (a,c)}; no_path = 9 pairs minus those 3.
        assert_eq!(model.relation(path).len(), 3);
        let expected: BTreeSet<Tuple> = [
            ("a", "a"),
            ("b", "a"),
            ("b", "b"),
            ("c", "a"),
            ("c", "b"),
            ("c", "c"),
        ]
        .into_iter()
        .map(|(x, y)| Tuple(vec![string_value(x), string_value(y)]))
        .collect();
        assert_eq!(model.relation(no_path), &expected);

        // Hand-run differential until the generator emits negation (C3).
        assert_eq!(
            naive::naive_eval(&program).unwrap(),
            model.facts().collect::<BTreeSet<Fact>>()
        );
    }

    /// A query with a negated literal answers over the finished model:
    /// `?- person(X), not parent(_, X).` on §16.2 — the first direct test of
    /// query negation (the rule-side path is the 16.2 end-to-end test).
    #[test]
    fn a_negated_query_answers_over_the_finished_model() {
        let mut program = crate::ir::fixtures::example_16_2();
        let person = PredId(0);
        let parent = PredId(1);
        program.queries.push(Query {
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: person,
                        args: vec![Term::Var(Var(0))],
                    }),
                    span: Span::DUMMY,
                },
                BodyLiteral {
                    kind: BodyLiteralKind::NegAtom(Atom {
                        pred: parent,
                        args: vec![Term::Var(Var(1)), Term::Var(Var(0))],
                    }),
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![Some("X".to_string()), None],
            projection: vec![0],
            span: Span::DUMMY,
        });
        let model = eval(&program).unwrap();
        assert_eq!(
            model.answer(&program.queries[0]).unwrap(),
            vec![vec![string_value("alice")]]
        );
    }

    /// Negation over an empty relation holds trivially: with no `parent`
    /// facts at all, every person is a root.
    #[test]
    fn negation_over_an_empty_relation_holds_trivially() {
        let mut program = crate::ir::fixtures::example_16_2();
        let parent = PredId(1);
        let root = PredId(2);
        program.facts.retain(|fact| fact.pred != parent);
        let model = eval(&program).unwrap();
        let expected: BTreeSet<Tuple> = ["alice", "bob", "carol"]
            .map(|name| Tuple(vec![string_value(name)]))
            .into();
        assert_eq!(model.relation(root), &expected);
        assert_eq!(
            naive::naive_eval(&program).unwrap(),
            model.facts().collect::<BTreeSet<Fact>>()
        );
    }

    /// Double negation across three strata: `b = d ∖ a`, `c = d ∖ b`, so `c`
    /// restores `a` within the domain `d` — and the strata chain
    /// a-rule < b-rule < c-rule is the first three-stratum evaluation test.
    #[test]
    fn double_negation_restores_within_the_domain() {
        // seed("x"). d("x"). d("y"). d("z").
        // a(X) :- seed(X).
        // b(X) :- d(X), not a(X).
        // c(X) :- d(X), not b(X).
        let mut program = Program::default();
        let seed = program.intern_pred("seed", 1);
        let d = program.intern_pred("d", 1);
        let a = program.intern_pred("a", 1);
        let b = program.intern_pred("b", 1);
        let c = program.intern_pred("c", 1);
        program.facts.push(Fact {
            pred: seed,
            tuple: Tuple(vec![string_value("x")]),
        });
        for name in ["x", "y", "z"] {
            program.facts.push(Fact {
                pred: d,
                tuple: Tuple(vec![string_value(name)]),
            });
        }
        let unary_rule = |head: PredId, body: Vec<BodyLiteral>| Rule {
            head: Atom {
                pred: head,
                args: vec![Term::Var(Var(0))],
            },
            body,
            var_names: vec![Some("X".to_string())],
            span: Span::DUMMY,
        };
        let positive = |pred: PredId| BodyLiteral {
            kind: BodyLiteralKind::Atom(Atom {
                pred,
                args: vec![Term::Var(Var(0))],
            }),
            span: Span::DUMMY,
        };
        let negative = |pred: PredId| BodyLiteral {
            kind: BodyLiteralKind::NegAtom(Atom {
                pred,
                args: vec![Term::Var(Var(0))],
            }),
            span: Span::DUMMY,
        };
        program.rules.push(unary_rule(a, vec![positive(seed)]));
        program
            .rules
            .push(unary_rule(b, vec![positive(d), negative(a)]));
        program
            .rules
            .push(unary_rule(c, vec![positive(d), negative(b)]));
        program.strata = vec![vec![RuleId(0)], vec![RuleId(1)], vec![RuleId(2)]];

        let model = eval(&program).unwrap();
        let expected_b: BTreeSet<Tuple> = ["y", "z"].map(|n| Tuple(vec![string_value(n)])).into();
        assert_eq!(model.relation(b), &expected_b);
        assert_eq!(model.relation(c), model.relation(a));
        assert_eq!(
            naive::naive_eval(&program).unwrap(),
            model.facts().collect::<BTreeSet<Fact>>()
        );
    }

    /// The engine defends the negation contract on hand-built IR: a *named*
    /// variable in a negated atom that nothing in the body binds is malformed
    /// (well-lowered IR is protected by §10 safety). An *assignment*-bound one
    /// is legal since 2026-07-25 — the scheduler defers the anti-join until it
    /// is ground.
    #[test]
    fn unbound_named_var_in_negated_atom_is_malformed_ir() {
        // q(X) :- p(X), not r(Y).   (Y named, bound by nothing at all)
        let mut program = Program::default();
        let p = program.intern_pred("p", 1);
        let q = program.intern_pred("q", 1);
        let r = program.intern_pred("r", 1);
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
                        pred: r,
                        args: vec![Term::Var(Var(1))],
                    }),
                    span: Span::DUMMY,
                },
            ],
            var_names: vec![Some("X".to_string()), Some("Y".to_string())],
            span: Span::DUMMY,
        });
        program.strata = vec![vec![RuleId(0)]];
        let err = eval(&program).unwrap_err();
        assert!(
            err.to_string()
                .contains("malformed IR: named variable `Y` in negated atom"),
            "unexpected error: {err}"
        );
    }

    /// The other half of the contract: the negated predicate's defining rules
    /// must sit in a strictly lower stratum, or the "complete and frozen"
    /// invariant the anti-join relies on does not hold.
    #[test]
    fn negation_within_its_own_stratum_is_malformed_ir() {
        // p(X) :- q(X).    r(X) :- q(X), not p(X).   — both in one stratum.
        let mut program = Program::default();
        let q = program.intern_pred("q", 1);
        let p = program.intern_pred("p", 1);
        let r = program.intern_pred("r", 1);
        program.rules.push(Rule {
            head: Atom {
                pred: p,
                args: vec![Term::Var(Var(0))],
            },
            body: vec![BodyLiteral {
                kind: BodyLiteralKind::Atom(Atom {
                    pred: q,
                    args: vec![Term::Var(Var(0))],
                }),
                span: Span::DUMMY,
            }],
            var_names: vec![Some("X".to_string())],
            span: Span::DUMMY,
        });
        program.rules.push(Rule {
            head: Atom {
                pred: r,
                args: vec![Term::Var(Var(0))],
            },
            body: vec![
                BodyLiteral {
                    kind: BodyLiteralKind::Atom(Atom {
                        pred: q,
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
        program.strata = vec![vec![RuleId(0), RuleId(1)]];
        let err = eval(&program).unwrap_err();
        assert!(
            err.to_string()
                .contains("negates `p`, which is not defined in a strictly lower stratum"),
            "unexpected error: {err}"
        );
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

    // --- Aggregation folds (§9) ---

    fn int(n: i64) -> Value {
        Value::Int(n)
    }

    fn float(x: f64) -> Value {
        Value::Float(crate::ir::F64::new(x).unwrap())
    }

    #[test]
    fn count_counts_every_binding_including_absent() {
        // count { X | Goal } counts bindings, absent ones included (§9).
        let values = vec![int(5), Value::Absent, int(3), Value::Absent];
        let outcome = fold_aggregate(AggOp::Count, &values).unwrap();
        assert_eq!(outcome.value, int(4));
        // count never "skips" — the report is 0.
        assert_eq!(outcome.skipped, 0);
    }

    #[test]
    fn sum_skips_absent_and_reports_the_skip_count() {
        // sum skips absents and folds the present values; the skip is reported.
        let values = vec![int(5), Value::Absent, int(3)];
        let outcome = fold_aggregate(AggOp::Sum, &values).unwrap();
        assert_eq!(outcome.value, int(8));
        assert_eq!(outcome.present, 2);
        assert_eq!(outcome.skipped, 1);
    }

    #[test]
    fn avg_is_the_float_mean_of_present_values() {
        // avg is always float, over the present values only (never divided by
        // the missing, §9).
        let values = vec![int(5), Value::Absent, int(3)];
        let outcome = fold_aggregate(AggOp::Avg, &values).unwrap();
        assert_eq!(outcome.value, float(4.0));
        assert_eq!(outcome.skipped, 1);
    }

    #[test]
    fn min_and_max_skip_absent() {
        let values = vec![int(5), Value::Absent, int(3), int(9)];
        assert_eq!(fold_aggregate(AggOp::Min, &values).unwrap().value, int(3));
        assert_eq!(fold_aggregate(AggOp::Max, &values).unwrap().value, int(9));
    }

    #[test]
    fn min_and_max_work_over_non_numeric_ordered_values() {
        // min/max extend to any single ordered type (§9): strings lexicographic.
        let names = vec![
            string_value("bob"),
            string_value("alice"),
            string_value("zoe"),
        ];
        assert_eq!(
            fold_aggregate(AggOp::Min, &names).unwrap().value,
            string_value("alice")
        );
        assert_eq!(
            fold_aggregate(AggOp::Max, &names).unwrap().value,
            string_value("zoe")
        );
    }

    #[test]
    fn an_empty_present_set_yields_absent_for_all_but_count() {
        // An empty group, or one whose values are all absent: count = 0, the
        // others = absent (§9 — no value to report).
        for values in [vec![], vec![Value::Absent, Value::Absent]] {
            assert_eq!(
                fold_aggregate(AggOp::Count, &values).unwrap().value,
                int(values.len() as i64)
            );
            for op in [AggOp::Sum, AggOp::Avg, AggOp::Min, AggOp::Max] {
                assert_eq!(
                    fold_aggregate(op, &values).unwrap().value,
                    Value::Absent,
                    "{op:?} over an empty present-set must be absent"
                );
            }
        }
    }

    #[test]
    fn sum_counts_equal_values_from_distinct_witnesses() {
        // The "duplicates and aggregates" rule (§9): the multiset carries one
        // element per witness tuple, so two equal salaries both count.
        let salaries = vec![int(100), int(100), int(50)];
        assert_eq!(
            fold_aggregate(AggOp::Sum, &salaries).unwrap().value,
            int(250)
        );
        assert_eq!(
            fold_aggregate(AggOp::Count, &salaries).unwrap().value,
            int(3)
        );
    }

    #[test]
    fn sum_of_non_numeric_is_a_structured_error() {
        let values = vec![string_value("a"), string_value("b")];
        let err = fold_aggregate(AggOp::Sum, &values).unwrap_err();
        assert!(
            err.to_string().contains("type error"),
            "unexpected error: {err}"
        );
    }

    /// The float sum carries its low-order bits (`compensated_sum`).
    ///
    /// `bugs/007`'s own repro, at the fold: sorting alone answers `0.0` here,
    /// because §14's order puts `-1e16` first and `0.1` is lost against it.
    /// That is a *canonical* answer, and this is the accurate one. Stated as an
    /// exact equality because the compensated result is exactly `0.1`, not
    /// nearly it.
    ///
    /// *Mutation*: dropping the correction term reddens this and nothing else;
    /// so does giving `avg_values` back its own naive accumulator, which is
    /// what makes the `avg` half worth asserting separately.
    #[test]
    fn a_float_sum_keeps_the_bits_a_naive_fold_drops() {
        let values = vec![float(1e16), float(-1e16), float(0.1)];
        assert_eq!(
            fold_aggregate(AggOp::Sum, &values).unwrap().value,
            float(0.1)
        );
        // And `avg` folds through the same helper, so it agrees.
        assert_eq!(
            fold_aggregate(AggOp::Avg, &values).unwrap().value,
            float(0.1 / 3.0)
        );
    }

    /// **An int sum overflows only when its own total does** (§9, §17
    /// 2026-08-20), not when some association of it does.
    ///
    /// `bugs/007` reached this by reordering a conjunction — one spelling
    /// answered and the other exited 2. Sorting made that deterministic;
    /// accumulating wide is what makes it *right*, because the partial sums of a
    /// multiset fold are not something the program asked for. The first case
    /// below has a representable total and errored under every fixed
    /// association, sorted included.
    #[test]
    fn an_int_sum_overflows_only_when_its_total_does() {
        let big = i64::MAX;
        let values = vec![int(-big), int(-big), int(big), int(big)];
        assert_eq!(fold_aggregate(AggOp::Sum, &values).unwrap().value, int(0));

        // The total itself does not fit, so this is still an error.
        let err = fold_aggregate(AggOp::Sum, &[int(big), int(big)]).unwrap_err();
        assert!(
            err.to_string().contains("integer overflow"),
            "unexpected error: {err}"
        );
    }

    /// The same rule for durations, which are `i64` microseconds underneath
    /// (§4) and so have the same edge.
    #[test]
    fn a_duration_sum_overflows_only_when_its_total_does() {
        let big = temporal::Duration::from_micros(i64::MAX);
        let small = temporal::Duration::from_micros(-i64::MAX);
        let values = vec![
            Value::Duration(big),
            Value::Duration(small),
            Value::Duration(big),
            Value::Duration(small),
        ];
        assert_eq!(
            fold_aggregate(AggOp::Sum, &values).unwrap().value,
            Value::Duration(temporal::Duration::from_micros(0))
        );

        let err =
            fold_aggregate(AggOp::Sum, &[Value::Duration(big), Value::Duration(big)]).unwrap_err();
        assert!(
            err.to_string().contains("representable range"),
            "unexpected error: {err}"
        );
    }

    /// A float sum that overflows stays `inf` — it does **not** become an error.
    ///
    /// This pins `compensated_sum`'s finiteness guard, which is the whole reason
    /// the guard exists: `F64::new` rejects NaN but permits infinities, so the
    /// naive fold answers `inf` here. An infinite accumulator has an infinite
    /// correction term, and adding them is NaN — so an unguarded compensation
    /// would turn a (strange, but documented) answer into a new error.
    ///
    /// *Mutation*: compensating unconditionally reddens this and nothing else.
    #[test]
    fn a_float_sum_that_overflows_is_infinite_not_an_error() {
        let values = vec![float(f64::MAX), float(f64::MAX)];
        let Value::Float(total) = fold_aggregate(AggOp::Sum, &values).unwrap().value else {
            panic!("summing floats yields a float");
        };
        assert!(
            total.get().is_infinite(),
            "expected inf, got {}",
            total.get()
        );
    }

    #[test]
    fn min_over_mixed_types_is_a_structured_error() {
        // A cross-type comparison is a §8 error, never a silent result.
        let values = vec![int(1), string_value("a")];
        let err = fold_aggregate(AggOp::Min, &values).unwrap_err();
        assert!(
            err.to_string().contains("type error"),
            "unexpected error: {err}"
        );
    }

    /// A negation over a *computed* argument records the argument's **value**
    /// in its no-match pattern, not an open slot (§7/§11, `bugs/001`).
    ///
    /// This is the provenance half of the fix and the half that is easiest to
    /// get silently wrong: `NoMatchPattern` uses `None` for a slot left open
    /// (wildcard, existential) and `Some` for one the bindings closed. The bug
    /// was exactly a slot that *should* have been closed being recorded — and
    /// evaluated — as open, so "why does `r(1)` hold?" must answer "because no
    /// `q(2)` exists", never the far stronger "because no `q` fact exists".
    #[test]
    fn a_computed_negated_argument_is_recorded_as_a_closed_no_match() {
        let src = "p(1).\np(2).\np(3).\nq(3).\nr(X) :- p(X), not q(X + 1).\n";
        let ast = crate::parser::parse(src).expect("parses");
        let program = crate::lower::lower(&ast).expect("lowers");
        let model = eval(&program).expect("evaluates");
        let q = program
            .rules
            .iter()
            .find_map(|rule| {
                rule.body.iter().find_map(|literal| match &literal.kind {
                    BodyLiteralKind::NegAtom(atom) => Some(atom.pred),
                    _ => None,
                })
            })
            .expect("the rule negates q");

        // r(1) holds because no q(2) exists; r(3) because no q(4) exists.
        let mut absences: Vec<(i64, Vec<Option<Value>>)> = model
            .facts()
            .filter(|fact| program.pred_info(fact.pred).name == "r")
            .flat_map(|fact| {
                let Value::Int(x) = fact.tuple.0[0] else {
                    panic!("r's argument is an int")
                };
                model
                    .derivations_of(&fact)
                    .flat_map(|d| d.premises.iter())
                    .filter_map(move |premise| match premise {
                        Premise::NoMatch(pattern) if pattern.pred == q => {
                            Some((x, pattern.args.clone()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        absences.sort();
        assert_eq!(
            absences,
            vec![
                (1, vec![Some(Value::Int(2))]),
                (3, vec![Some(Value::Int(4))]),
            ],
            "a computed negated argument must close its slot, not leave it open"
        );
    }

    // --- Phase B (B1–B7) and Phase E (E1–E4) properties (testing.md) ---

    mod properties {
        use std::collections::{BTreeSet, HashMap};

        use proptest::prelude::*;

        use super::super::naive::{ground, match_atom, naive_eval};
        use super::super::*;
        use crate::ast::TypeName;
        use crate::error::Warning;
        use crate::ir::fixtures::example_16_1;
        use crate::lower::check_program;
        use crate::lower::lower;
        use crate::provenance::ProofTree;
        use crate::testgen::{
            ArithShape, arb_comparison_program, arb_extension_pair, arb_neg_shift_spellings,
            arb_parent_edges, arb_program_with_edb, arb_recursive_arithmetic_program, arb_value,
            arb_well_typed_program, with_duplicated_facts, with_extra_fact, with_swapped_body,
            with_swapped_stratum_rules,
        };
        use crate::typecheck::typecheck;

        /// The primitive type of a ground value (for C4). Generated programs
        /// (`arb_value`) carry no `absent`, so the type-neutral value never
        /// reaches this column-type check.
        fn value_type(value: &Value) -> TypeName {
            match value {
                Value::Absent => unreachable!("arb_value generates no absent"),
                Value::Symbol(_) => TypeName::Symbol,
                Value::String(_) => TypeName::String,
                Value::Int(_) => TypeName::Int,
                Value::Float(_) => TypeName::Float,
                Value::Bool(_) => TypeName::Bool,
                Value::Date(_) => TypeName::Date,
                Value::Timestamp(_) => TypeName::Timestamp,
                Value::Duration(_) => TypeName::Duration,
            }
        }

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
        ///
        /// Two passes, mirroring evaluation order: fact premises build the
        /// environment first, then each `Absent` premise is re-derived from
        /// its literal under that environment and must equal the recorded
        /// pattern (the negated literal's own kind must match too). The
        /// caller separately checks `Absent` patterns as non-matches against
        /// the model.
        fn replay(program: &Program, derivation: &Derivation) -> Option<Fact> {
            let rule = program.rules.get(derivation.rule.0 as usize)?;
            if derivation.premises.len() != rule.body.len() {
                return None;
            }
            let mut env = HashMap::new();
            for (literal, premise) in rule.body.iter().zip(&derivation.premises) {
                let (BodyLiteralKind::Atom(atom), Premise::Fact(fact)) = (&literal.kind, premise)
                else {
                    continue;
                };
                if atom.pred != fact.pred {
                    return None;
                }
                env = match_atom(atom, &fact.tuple, &env)?;
            }
            for (literal, premise) in rule.body.iter().zip(&derivation.premises) {
                match (&literal.kind, premise) {
                    (BodyLiteralKind::Atom(_), Premise::Fact(_)) => {}
                    (BodyLiteralKind::NegAtom(atom), Premise::NoMatch(pattern)) => {
                        if atom.pred != pattern.pred {
                            return None;
                        }
                        let expected: Vec<Option<Value>> = atom
                            .args
                            .iter()
                            .map(|term| match term {
                                Term::Const(value) => Some(value.clone()),
                                Term::Var(var) => env.get(var).cloned(),
                            })
                            .collect();
                        if expected != pattern.args {
                            return None;
                        }
                    }
                    _ => return None, // premise kind does not match its literal
                }
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
                ProofTree::NoMatch(pattern) => {
                    // An no-match leaf must actually be absent: no tuple of
                    // the predicate falls under the pattern.
                    prop_assert!(
                        !model
                            .relation(pattern.pred)
                            .iter()
                            .any(|tuple| pattern.matches(tuple)),
                        "no-match leaf {pattern:?} is refuted by the model"
                    );
                }
                ProofTree::Builtin { .. }
                | ProofTree::Presence { .. }
                | ProofTree::Aggregate { .. } => {
                    // A satisfied comparison/assignment/presence/aggregate leaf
                    // carries its own justification (the evaluated operands or the
                    // fold) — nothing to check against the model.
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
            /// The absent value laws (§4/§8), over the typed value pool.

            /// Arithmetic annihilation: absent on either side of any operator
            /// yields absent, no matter the other operand.
            #[test]
            fn absent_annihilates_over_any_operand(v in arb_value()) {
                for op in [ArithOp::Add, ArithOp::Sub, ArithOp::Mul, ArithOp::Div] {
                    prop_assert_eq!(
                        apply_arith(op, Value::Absent, v.clone()).unwrap(),
                        Value::Absent
                    );
                    prop_assert_eq!(
                        apply_arith(op, v.clone(), Value::Absent).unwrap(),
                        Value::Absent
                    );
                }
            }

            /// Every comparison with an absent operand is false — never an error,
            /// whatever the other operand's type.
            #[test]
            fn absent_comparison_is_always_false(v in arb_value()) {
                for op in [CmpOp::Eq, CmpOp::Ne, CmpOp::Lt, CmpOp::Le, CmpOp::Gt, CmpOp::Ge] {
                    prop_assert!(!apply_compare(op, &Value::Absent, &v).unwrap());
                    prop_assert!(!apply_compare(op, &v, &Value::Absent).unwrap());
                }
            }

            /// Semantic unify agrees with structural equality on typed values,
            /// but absent unifies with nothing.
            #[test]
            fn values_unify_matches_eq_off_absent(a in arb_value(), b in arb_value()) {
                prop_assert_eq!(a.unifies_with(&b), a == b);
                prop_assert!(!a.unifies_with(&Value::Absent));
                prop_assert!(!Value::Absent.unifies_with(&a));
            }

            /// `absent` sorts before every typed value under the canonical `Ord`.
            #[test]
            fn absent_sorts_first(v in arb_value()) {
                prop_assert!(Value::Absent < v);
            }
        }

        // --- The two logical laws `absent` is measured against (§4/§7) ---
        //
        // `try_match` binds a *fresh* slot to a stored `absent` — that is how a
        // missing cell flows to a head — while every *subsequent* use of that
        // slot goes through `unifies_with`, which absent always fails. The two
        // tests below pin what that asymmetry does and does not cost: the
        // anti-join is exempted from it (structural membership, §7), so
        // non-contradiction holds; joining is not, so idempotence does not.

        /// Answers of `?- <goal>.` against `facts`, as printed values.
        fn answers_of(facts: &str, rule: &str, query_pred: &str) -> Vec<Vec<Value>> {
            let src = format!("{facts}{rule}?- {query_pred}(X).\n");
            let ast = crate::parser::parse(&src).expect("parses");
            let program = lower(&ast).expect("lowers");
            let model = eval(&program).expect("evaluates");
            model.answer(&program.queries[0]).expect("answers")
        }

        /// **Non-contradiction**: no body can be satisfied by both `p(X)` and
        /// `not p(X)`, for any `X`.
        ///
        /// `q(X) :- p(X), not p(X).` used to derive `q(absent)` when `p(absent)`
        /// was stored — P ∧ ¬P, in an engine whose advertised uses include
        /// consistency checking. The anti-join's structural membership test
        /// (§7) is what refutes the no-match: `p(absent)` *is* in the relation.
        #[test]
        fn a_fact_never_satisfies_its_own_negation() {
            let answers = answers_of("p(1).\np(absent).\n", "q(X) :- p(X), not p(X).\n", "q");
            assert!(
                answers.is_empty(),
                "P and not-P were both satisfied: {answers:?}"
            );
        }

        /// **Idempotence of conjunction does not hold over `absent`, by design**
        /// (§4/§7, 2026-07-29): `p(X), p(X)` selects strictly less than `p(X)`,
        /// dropping the absent row, because the second occurrence of `X` — by
        /// then bound to `absent` — unifies with nothing.
        ///
        /// This is not the negation defect's twin, and the two were separated
        /// deliberately. It is the direct consequence of `absent ≠ absent` in a
        /// *join*, which is the foreign-key-blowup protection the value model
        /// exists to give (§4), and SQL drops the `NULL` from a self-join for
        /// exactly the same reason. Restoring the law means giving that up.
        /// Pinned rather than deleted so a later session finds the answer here
        /// instead of re-filing the defect.
        #[test]
        fn repeating_a_body_literal_drops_absent_rows() {
            let facts = "p(1).\np(absent).\n";
            let one = answers_of(facts, "one(X) :- p(X).\n", "one");
            let two = answers_of(facts, "two(X) :- p(X), p(X).\n", "two");
            assert_eq!(one, vec![vec![Value::Absent], vec![Value::Int(1)]]);
            assert_eq!(two, vec![vec![Value::Int(1)]]);
        }

        /// Which parities of negation each predicate can reach `q` by, recomputed
        /// from the lowered rules — C11's dependency walk, shared by the property
        /// and its non-vacuity guard so the two cannot drift apart. `[even, odd]`
        /// per predicate; `q` reaches itself by the empty (even) path.
        ///
        /// Deliberately independent of `crate::schedule` and of `program.strata`:
        /// C1 recomputes its graph the same way, and for the same reason — a check
        /// that asks the code under test for the answer agrees with it forever.
        fn negation_parities(program: &Program, q: PredId) -> Vec<[bool; 2]> {
            let n = program.predicates.len();
            let mut parities: Vec<[bool; 2]> = vec![[false; 2]; n];
            parities[q.0 as usize][0] = true;
            let mut changed = true;
            while changed {
                changed = false;
                for rule in &program.rules {
                    for literal in &rule.body {
                        let (body_pred, flip) = match &literal.kind {
                            BodyLiteralKind::Atom(atom) => (atom.pred, 0),
                            BodyLiteralKind::NegAtom(atom) => (atom.pred, 1),
                            _ => continue,
                        };
                        for parity in 0..2 {
                            if parities[body_pred.0 as usize][parity] {
                                let target = (parity + flip) % 2;
                                let slot = &mut parities[rule.head.pred.0 as usize][target];
                                if !*slot {
                                    *slot = true;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            parities
        }

        /// The predicates a fact added to `q` may be added to: EDB-only, so the
        /// addition is a pure input change rather than a change to a derivation.
        fn edb_only_preds(program: &Program) -> Vec<PredId> {
            let derived: BTreeSet<PredId> = program.rules.iter().map(|r| r.head.pred).collect();
            (0..program.predicates.len() as u32)
                .map(PredId)
                .filter(|p| !derived.contains(p))
                .collect()
        }

        /// **C11's non-vacuity guard.** The parity property is satisfied by a
        /// program where nothing depends on `q` oddly, and by one where every odd
        /// dependent is empty — `∅ ⊆ ∅` proves nothing. This asserts the
        /// discriminating case is reached across the generator: an odd-parity
        /// dependent whose relation is **non-empty before the addition**, so the
        /// subset claim has rows to constrain.
        ///
        /// Checked against C11's *sentence*, per `testing.md` rule 2 — C11 claims
        /// odd dependents shrink, so what the guard must see is an odd dependent
        /// with something to lose. A per-case assertion would be wrong here (one
        /// small program need not contain the shape), which is why this is a
        /// deterministic sampler in the `generator_emits_absent_in_facts_only`
        /// style rather than a `proptest!` arm.
        #[test]
        fn c11_generator_reaches_a_non_empty_odd_dependent() {
            use proptest::strategy::{Strategy, ValueTree};
            use proptest::test_runner::TestRunner;

            let mut runner = TestRunner::deterministic();
            let strategy = arb_program_with_edb();
            let mut reached = 0;
            for _ in 0..400 {
                let program = strategy
                    .new_tree(&mut runner)
                    .expect("strategy produces a value")
                    .current();
                let model = eval(&program).unwrap();
                for q in edb_only_preds(&program) {
                    let parities = negation_parities(&program, q);
                    for pred in (0..program.predicates.len() as u32).map(PredId) {
                        let [even, odd] = parities[pred.0 as usize];
                        if odd && !even && !model.relation(pred).is_empty() {
                            reached += 1;
                        }
                    }
                }
            }
            assert!(
                reached > 0,
                "no generated program had a non-empty odd-parity dependent — C11 \
             would be asserting `∅ ⊆ ∅` and the antitone half would be blind"
            );
        }

        proptest! {
            // Evaluator differentials are the expensive properties; keep the
            // case count modest (testing.md, Tooling).
            #![proptest_config(ProptestConfig::with_cases(64))]

            /// B1 — the anchor: naive and semi-naive agree as fact sets.
            #[test]
            fn b1_naive_matches_seminaive(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                prop_assert_eq!(model_facts(&model), naive_eval(&program).unwrap());
            }

            /// The evaluation-property generator produces well-typed programs
            /// (the migration to the reachable state space, 2026-07-21): every
            /// program the type checker accepts, so the whole B/E suite runs
            /// over programs `eval` could actually see in production.
            #[test]
            fn evaluation_generator_is_well_typed(program in arb_program_with_edb()) {
                prop_assert!(
                    typecheck(&program).is_ok(),
                    "evaluation generator produced an ill-typed program: {:?}",
                    typecheck(&program).err()
                );
            }

            /// A negation over a computed argument means the same thing in all
            /// three spellings — inline `not n(_, V + c)`, the assignment
            /// hoisted before it, and hoisted after it (`bugs/001`, §7/§10
            /// 2026-07-25). This is the property the fix exists for: a
            /// conjunction may not depend on where in it a literal is written.
            #[test]
            fn negation_over_a_computed_argument_is_spelling_independent(
                spellings in arb_neg_shift_spellings()
            ) {
                let [inline, binder_first, binder_last] = spellings;
                let inline = model_facts(&eval(&inline).unwrap());
                prop_assert_eq!(
                    &inline,
                    &model_facts(&eval(&binder_first).unwrap()),
                    "inline and binder-first disagree"
                );
                prop_assert_eq!(
                    &inline,
                    &model_facts(&eval(&binder_last).unwrap()),
                    "inline and binder-last disagree"
                );
            }

            /// C10 — **the termination guarantee itself**: a program the §10
            /// lint does not warn about reaches its fixpoint, on every input.
            ///
            /// The generated `step` graph is freely cyclic, which is the whole
            /// point: a certified program has to terminate on the graphs that
            /// make an uncertified one run forever. The oracle is a round cap,
            /// so a wrong certification **fails** here rather than hanging the
            /// suite, and it is deterministic where a wall-clock timeout would
            /// flake. The naive differential rides along, so certification
            /// cannot buy termination by computing the wrong model.
            ///
            /// *Mutation verified* (testing.md rule 3): dropping transitive
            /// taint from `schedule::computed_vars` — so that
            /// `K = M + 1, N = K` reads as a bare-variable assignment —
            /// certifies `ArithShape::Hoisted`, and this property fails on the
            /// cap. Dropping cast propagation does the same through
            /// `ArithShape::Casted`.
            #[test]
            fn c10_a_certified_program_reaches_its_fixpoint(
                generated in arb_recursive_arithmetic_program()
            ) {
                let (src, program, _) = generated;
                let warned = check_program(&program).iter().any(|warning| {
                    matches!(warning, Warning::ValueCreatingRecursion { .. })
                });
                prop_assume!(!warned);
                // Well above what any generated program needs: 4 nodes, 6 edges.
                let model = match eval_capped(&program, 200) {
                    Ok(model) => model,
                    Err(Capped::Failed(_)) => return Ok(()),
                    Err(Capped::Diverged) => {
                        prop_assert!(
                            false,
                            "certified as terminating, then did not terminate:\n{}",
                            src
                        );
                        unreachable!("prop_assert!(false) returns")
                    }
                };
                prop_assert_eq!(model_facts(&model), naive_eval(&program).unwrap());
            }

            /// C10's non-vacuity guard (testing.md rule 2). Without it the
            /// property could hold over nothing but arithmetic-free programs,
            /// which have never been in doubt: what has to be certified *and*
            /// evaluated is a program with arithmetic **and** a positive cycle.
            ///
            /// **The `StdBuiltin` shape (2026-08-20) is what makes this guard
            /// more than bookkeeping.** §10 exempts a `std` relation from
            /// value-creating recursion as a finite-domain map — a
            /// termination-soundness claim whose failure mode is a program that
            /// never finishes — and no shape exercised it. *Mutation*: making
            /// `schedule::has_arithmetic` return `true` for `Expr::Builtin`
            /// reddens **this and only this**, with "shape StdBuiltin changed
            /// sides". C10 itself stays green, because a warned program is
            /// skipped by its `prop_assume!(!warned)` — which is precisely why
            /// the exemption needs a guard watching the *classification* and not
            /// only the fixpoint.
            #[test]
            fn c10_generator_certifies_programs_that_do_arithmetic_in_a_cycle(
                generated in arb_recursive_arithmetic_program()
            ) {
                let (_, program, shape) = generated;
                let warned = check_program(&program).iter().any(|warning| {
                    matches!(warning, Warning::ValueCreatingRecursion { .. })
                });
                let certified_with_arithmetic = matches!(
                    shape,
                    ArithShape::Outside
                        | ArithShape::CastOnly
                        | ArithShape::Ground
                        | ArithShape::Aggregated
                        | ArithShape::FilterOnly
                        // A `std/time` builtin in a positive cycle (2026-08-20).
                        // §10 exempts it as a finite-domain map, so it must land
                        // on the certified side — and C10 above then has to show
                        // it actually reaches a fixpoint, which is the whole
                        // point of exercising an exemption whose failure mode is
                        // a program that never finishes.
                        | ArithShape::StdBuiltin
                );
                prop_assert_eq!(
                    !warned,
                    certified_with_arithmetic,
                    "shape {:?} changed sides",
                    shape
                );
            }

            /// B1 extended over §8 comparison/arithmetic programs: naive and
            /// semi-naive agree, including on the error path (a `/ 0` in a
            /// generated rule makes both reject).
            #[test]
            fn b1_comparison_programs_agree(program in arb_comparison_program()) {
                match (eval(&program), naive_eval(&program)) {
                    (Ok(model), Ok(facts)) => prop_assert_eq!(model_facts(&model), facts),
                    (Err(_), Err(_)) => {}
                    (engine, naive) => prop_assert!(
                        false,
                        "engine/naive disagree: engine ok={}, naive ok={}",
                        engine.is_ok(),
                        naive.is_ok()
                    ),
                }
            }

            /// The comparison generator produces well-typed programs, like
            /// `evaluation_generator_is_well_typed` above — and this is the
            /// property that carries `bugs/006`, not B1 beside it. `eval` is
            /// deliberately type-agnostic (§17, 2026-07-21), so B1 would stay
            /// green over string comparisons however the type checker treats
            /// them; only a property that *calls* `typecheck` fails when the
            /// numeric constraint on `<` comes back.
            #[test]
            fn comparison_generator_is_well_typed(program in arb_comparison_program()) {
                prop_assert!(
                    typecheck(&program).is_ok(),
                    "comparison generator produced an ill-typed program: {:?}",
                    typecheck(&program).err()
                );
            }

            /// C5 — typed-generator completeness: every well-typed-by-
            /// construction program is accepted by the type checker (no false
            /// rejections).
            #[test]
            fn c5_well_typed_programs_are_accepted(program in arb_well_typed_program()) {
                prop_assert!(
                    typecheck(&program).is_ok(),
                    "well-typed program rejected: {:?}",
                    typecheck(&program).err()
                );
            }

            /// A self-contained type conflict is always rejected: appending a
            /// fresh predicate with two facts of different column types must
            /// make the type checker fail (the C-series analogue of A11's
            /// lowering-defect property).
            #[test]
            fn injected_type_conflict_is_rejected(program in arb_well_typed_program()) {
                let mut bad = program.clone();
                let conflict = bad.intern_pred("conflict", 1);
                bad.facts.push(Fact {
                    pred: conflict,
                    tuple: Tuple(vec![Value::String("x".to_string())]),
                });
                bad.facts.push(Fact {
                    pred: conflict,
                    tuple: Tuple(vec![Value::Int(0)]),
                });
                prop_assert!(
                    typecheck(&bad).is_err(),
                    "conflicting `conflict(\"x\").`/`conflict(0).` facts accepted"
                );
            }

            /// C6 — declare-signature verification (accept half): asserting the
            /// *inferred* types as a `declare` signature never changes
            /// acceptance. A correct signature buys documentation, not new
            /// obligations (§4).
            #[test]
            fn c6_correct_declared_signature_is_accepted(program in arb_well_typed_program()) {
                let env = typecheck(&program).expect("well-typed program");
                let mut annotated = program.clone();
                for p in 0..annotated.predicates.len() {
                    let arity = annotated.predicates[p].arity as usize;
                    let pred = crate::ir::PredId(p as u32);
                    let types: Vec<Option<TypeName>> =
                        (0..arity).map(|c| env.column_type(pred, c)).collect();
                    let names = (0..arity).map(|c| format!("f{c}")).collect();
                    annotated.predicates[p].fields = Some(names);
                    annotated.predicates[p].field_types = Some(types);
                }
                prop_assert!(
                    typecheck(&annotated).is_ok(),
                    "correct declared signature rejected: {:?}",
                    typecheck(&annotated).err()
                );
            }

            /// C6 — declare-signature verification (reject half): a declared
            /// column type that contradicts inference is rejected, naming the
            /// column. Self-contained (like `injected_type_conflict_is_rejected`)
            /// so there is always a typed column to contradict.
            #[test]
            fn c6_wrong_declared_type_is_rejected(program in arb_well_typed_program()) {
                let mut bad = program.clone();
                let probe = bad.intern_pred("c6probe", 1);
                bad.facts.push(Fact {
                    pred: probe,
                    tuple: Tuple(vec![Value::Int(0)]),
                });
                // Declare the (inferred int) column as string — a contradiction.
                bad.predicates[probe.0 as usize].fields = Some(vec!["v".to_string()]);
                bad.predicates[probe.0 as usize].field_types = Some(vec![Some(TypeName::String)]);
                let result = typecheck(&bad);
                prop_assert!(
                    result.is_err(),
                    "declaring int column `c6probe.v` as string was accepted"
                );
                let err = format!("{:?}", result.err());
                prop_assert!(
                    err.contains("c6probe.v"),
                    "type error did not name the column: {err}"
                );
            }

            /// C4 — inference soundness: a type-checked program evaluates
            /// without a type-based runtime error, and every fact's values
            /// match the inferred column types.
            #[test]
            fn c4_inference_is_sound(program in arb_well_typed_program()) {
                let env = typecheck(&program).expect("well-typed program");
                // The typed generator emits no division, so evaluation cannot
                // raise even a non-type arithmetic error.
                let model = eval(&program).expect("type-checked program evaluates");
                for fact in model.facts() {
                    for (col, value) in fact.tuple.0.iter().enumerate() {
                        if let Some(ty) = env.column_type(fact.pred, col) {
                            prop_assert_eq!(
                                value_type(value),
                                ty,
                                "fact {:?} column {} has type {:?}, inferred {:?}",
                                fact, col, value_type(value), ty
                            );
                        }
                    }
                }
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

            /// B5 over §8 comparison/arithmetic programs, which — unlike
            /// `arb_program_with_edb` (every column a symbol) — can carry a
            /// *computed* negated argument (`CompRule::NegShift`). That is the
            /// shape whose whole point is that body order stops mattering
            /// (`bugs/001`), so it wants the order-permutation property and not
            /// only the fixed three spellings of C7.
            ///
            /// Compared only when both orders evaluate: swapping literals
            /// changes which of several *ready* literals runs first, and a
            /// filter that prunes a row ahead of a `/ 0` is the one way order is
            /// legitimately observable (see the `crate::schedule` module docs).
            #[test]
            fn b5_comparison_body_order_is_irrelevant(
                program in arb_comparison_program(),
                rule_sel in any::<u8>(),
                i in any::<u8>(),
                j in any::<u8>(),
            ) {
                let before = eval(&program);
                let swapped = with_swapped_body(program, rule_sel, i, j);
                if let (Ok(before), Ok(after)) = (before, eval(&swapped)) {
                    prop_assert_eq!(model_facts(&after), model_facts(&before));
                }
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

            /// **C13 — statement order carries no meaning**, in three senses at
            /// once: permuting a program's statements changes neither the
            /// accept/reject verdict, nor the inferred column types, nor the
            /// model.
            ///
            /// B6 swaps two rules *within a stratum*. This reorders facts,
            /// `declare`s, rules and queries against each other, which varies
            /// three things B6 holds fixed — the order predicates are
            /// **interned** (so `PredId`s differ and every comparison here goes
            /// by name), the numbers **Ullman relaxation** assigns, and the
            /// order **type inference** unifies columns.
            ///
            /// The type half is the one with nothing else looking at it.
            /// **Inference confluence has no other property**: C4 checks
            /// soundness and C5 completeness, both at one fixed statement order,
            /// so a unifier that seeded a column from whichever constraint it
            /// met first would satisfy both and fail here.
            ///
            /// Verified free of ordering constraints before being written: a
            /// program with its query first and its facts last runs identically,
            /// and §13 puts no position requirement on an `import` either — so
            /// an unconstrained permutation is the right generator and not an
            /// over-reach.
            ///
            /// **Mutation-verified, and the pair of results is the record worth
            /// keeping.** Making `set_type` take the *last* constraint rather
            /// than unifying reddens C13 while **C4, C5 and C6 all stay green** —
            /// the measurement behind the confluence claim above. (It also
            /// reddens three conflict-detection tests, since last-wins removes
            /// the error too, so the aim is broad; the C4/C5/C6-versus-C13
            /// contrast is the part that discriminates.)
            ///
            /// The second is an **equivalent mutant, deliberately**: gathering
            /// fact constraints in reverse order reddens *nothing at all*. That
            /// is not a gap — it is what confluence means, and it is the
            /// positive evidence that C13 asserts something true rather than
            /// something merely unbroken. A property whose claim is "order does
            /// not matter" should be immune to a pure reordering of the code
            /// that implements it.
            #[test]
            fn c13_statement_order_changes_nothing(
                (base, permuted) in crate::testgen::arb_statement_permutation()
            ) {
                let (a, b) = (lower(&base), lower(&permuted));
                let (a, b) = match (a, b) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(_), Err(_)) => return Ok(()),
                    (a, b) => {
                        prop_assert!(
                            false,
                            "reordering changed whether the program lowers: {} vs {}",
                            a.is_ok(),
                            b.is_ok()
                        );
                        unreachable!("prop_assert!(false) returns")
                    }
                };

                // The verdict, and the inferred types behind it.
                match (crate::typecheck::typecheck(&a), crate::typecheck::typecheck(&b)) {
                    (Ok(ta), Ok(tb)) => {
                        // By predicate *name*: interning order differs, which is
                        // half of what this property is about.
                        for pred in (0..a.predicates.len() as u32).map(PredId) {
                            let info = a.pred_info(pred);
                            let other = (0..b.predicates.len() as u32)
                                .map(PredId)
                                .find(|p| b.pred_info(*p).name == info.name);
                            let Some(other) = other else {
                                prop_assert!(false, "predicate {} vanished", info.name);
                                unreachable!("prop_assert!(false) returns")
                            };
                            for col in 0..info.arity as usize {
                                prop_assert_eq!(
                                    ta.column_type(pred, col),
                                    tb.column_type(other, col),
                                    "column {}.{} inferred differently under a reordering",
                                    info.name,
                                    col
                                );
                            }
                        }
                    }
                    (Err(_), Err(_)) => return Ok(()),
                    (ta, tb) => {
                        prop_assert!(
                            false,
                            "reordering changed the type verdict: {} vs {}",
                            ta.is_ok(),
                            tb.is_ok()
                        );
                        unreachable!("prop_assert!(false) returns")
                    }
                }

                prop_assert_eq!(
                    named_facts(&eval(&a).unwrap(), &a),
                    named_facts(&eval(&b).unwrap(), &b),
                    "reordering changed the model"
                );
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

            /// B8 — query/rule equivalence: a query answers exactly what a
            /// rule with the query's body and a head projecting its named
            /// variables derives. The synthesized rule runs in a fresh final
            /// stratum, which is sound because everything a query reads —
            /// negated or not — is defined in earlier strata (§7: queries run
            /// over the finished model). Under the Phase-C generator this
            /// covers negated query bodies.
            #[test]
            fn b8_queries_equal_synthesized_rules(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                for query in &program.queries {
                    let named_slots: Vec<u32> = query
                        .var_names
                        .iter()
                        .enumerate()
                        .filter(|(_, name)| name.is_some())
                        .map(|(slot, _)| slot as u32)
                        .collect();
                    // The generator skips queries binding no named variable.
                    prop_assert!(!named_slots.is_empty());

                    let mut variant = program.clone();
                    variant.queries.clear();
                    let q_ans = variant.intern_pred("q_ans", named_slots.len() as u32);
                    let rule_id = RuleId(variant.rules.len() as u32);
                    variant.rules.push(Rule {
                        head: Atom {
                            pred: q_ans,
                            args: named_slots
                                .iter()
                                .map(|&slot| Term::Var(crate::ir::Var(slot)))
                                .collect(),
                        },
                        body: query.body.clone(),
                        var_names: query.var_names.clone(),
                        span: query.span,
                    });
                    variant.strata.push(vec![rule_id]);

                    let variant_model = eval(&variant).unwrap();
                    let answers: BTreeSet<Vec<Value>> =
                        model.answer(query).unwrap().into_iter().collect();
                    let derived: BTreeSet<Vec<Value>> = variant_model
                        .relation(q_ans)
                        .iter()
                        .map(|tuple| tuple.0.clone())
                        .collect();
                    prop_assert_eq!(answers, derived);
                }
            }

            /// C2 — independent perfect-model oracle on the §16.2 shape:
            /// random person/parent EDBs through `root(X) :- person(X),
            /// not parent(_, X).` equal a hand-rolled set difference that
            /// touches neither evaluator nor the strata machinery.
            #[test]
            fn c2_roots_are_a_set_difference(
                persons in proptest::collection::btree_set(0u8..6, 0..=6),
                edges in arb_parent_edges(),
            ) {
                let mut program = crate::ir::fixtures::example_16_2();
                let person = PredId(0);
                let parent = PredId(1);
                let root = PredId(2);
                program.facts = persons
                    .iter()
                    .map(|p| Fact {
                        pred: person,
                        tuple: Tuple(vec![Value::String(format!("n{p}"))]),
                    })
                    .chain(edges.iter().map(|(a, b)| Fact {
                        pred: parent,
                        tuple: Tuple(vec![
                            Value::String(a.clone()),
                            Value::String(b.clone()),
                        ]),
                    }))
                    .collect();
                let model = eval(&program).unwrap();

                let expected: BTreeSet<Tuple> = persons
                    .iter()
                    .map(|p| format!("n{p}"))
                    .filter(|p| edges.iter().all(|(_, child)| child != p))
                    .map(|p| Tuple(vec![Value::String(p)]))
                    .collect();
                prop_assert_eq!(model.relation(root), &expected);
            }

            /// **C11 — negation is antitone, by parity.** Adding a fact to an
            /// EDB relation grows every predicate that depends on it through an
            /// **even** number of negations and shrinks every predicate that
            /// depends on it through an **odd** number.
            ///
            /// This is the half of the fact-addition space B4 excludes rather
            /// than covers. `with_extra_fact` (`testgen.rs:1563`) adds only to
            /// `negation_independent_preds`, because over a negated predicate
            /// the monotone claim is false — but excluding it left the *true*
            /// law for that half unstated, so a stratified evaluator whose
            /// anti-join read the wrong relation had nothing looking at it from
            /// either side. B4 is this property's even case restricted to
            /// parity-0-only predicates; C11 states both and derives which
            /// applies from the dependency graph.
            ///
            /// **Why parity rather than "negation shrinks".** The simple form is
            /// wrong two strata up: if `h` negates `q` then `h` shrinks, but
            /// `s :- not h.` *grows* again. And it has to be stated over
            /// generated programs rather than the §16.2 shape, because on that
            /// shape **C2 already pins `root` exactly** for every person/parent
            /// EDB — a monotonicity claim there would restate a theorem an
            /// oracle already proves. The dependency walk below is recomputed
            /// from the lowered rules, in the C1 style, and never asks
            /// `crate::schedule` or the strata for an opinion.
            ///
            /// Predicates reachable by paths of *both* parities carry no claim:
            /// the union of a growing and a shrinking relation moves either way.
            ///
            /// **Mutation, and what aiming it revealed.** Dropping the
            /// strictness of a negated dependency in Ullman relaxation
            /// (`lower.rs`, `stratum[body] + u32::from(dep.strict())` →
            /// `stratum[body]`) reddens C11 **while C2 stays green** — C2's
            /// program is `ir::fixtures::example_16_2`, whose strata are
            /// hand-built, so the exact oracle covering this shape cannot see a
            /// stratification bug at all. That contrast is C11's justification:
            /// its contribution is not a stronger claim than C2's but a wider
            /// one, over *generated* multi-stratum programs.
            ///
            /// Worth recording that two earlier aims failed. Mutating the
            /// anti-join itself — reading `cx.delta` instead of the frozen
            /// relation — reddens fourteen tests including B1, because a change
            /// at one call site is *one-sided* and B1 catches one-sided changes
            /// by construction. A mutation that would isolate C11 has to be made
            /// **consistently in both evaluators**, which is the same reason C9
            /// is asserted against the model rather than across them.
            #[test]
            fn c11_adding_a_fact_moves_dependents_by_negation_parity(
                program in arb_program_with_edb(),
                pred_sel in any::<u8>(),
                values in proptest::collection::vec(arb_value(), 3),
            ) {
                let candidates = edb_only_preds(&program);
                if candidates.is_empty() {
                    return Ok(());
                }
                let q = candidates[pred_sel as usize % candidates.len()];
                let parities = negation_parities(&program, q);
                let n = program.predicates.len();

                let arity = program.pred_info(q).arity as usize;
                let mut extended = program.clone();
                extended.facts.push(Fact {
                    pred: q,
                    tuple: Tuple(values.into_iter().take(arity).collect()),
                });

                let before = eval(&program).unwrap();
                let after = eval(&extended).unwrap();

                for pred in (0..n as u32).map(PredId) {
                    let [even, odd] = parities[pred.0 as usize];
                    let old = before.relation(pred);
                    let new = after.relation(pred);
                    match (even, odd) {
                        (true, false) => prop_assert!(
                            old.is_subset(new),
                            "{} depends on {} evenly but shrank",
                            program.pred_info(pred).name,
                            program.pred_info(q).name
                        ),
                        (false, true) => prop_assert!(
                            new.is_subset(old),
                            "{} depends on {} oddly but grew",
                            program.pred_info(pred).name,
                            program.pred_info(q).name
                        ),
                        // Unreachable from `q`, or reachable both ways: no claim.
                        _ => {}
                    }
                }
            }

            /// **C14 — the *explanations* are order-invariant too**, not just
            /// the model. Permuting a stratum's rules, or a rule's body,
            /// leaves the set of recorded derivations unchanged.
            ///
            /// B5 and B6 compare `model_facts` and nothing else. So the model
            /// was order-invariant by property and the derivations were
            /// order-invariant only by hope — and provenance is pillar 1, where
            /// an answer to "why?" that depends on how the body was typed out is
            /// the failure the pillar cannot afford. E1–E4 all check derivations
            /// against a *single* evaluation and never across two.
            ///
            /// The comparison is exact rather than a multiset, because
            /// `premises[i]` is aligned with body literal `i`
            /// ([`crate::ir::BodyIdx`]): swapping body positions `i` and `j`
            /// must swap exactly those two premises, so the property un-swaps
            /// them and asserts equality. Keying by rule + premise multiset
            /// would have been the weaker claim, and would not have noticed a
            /// premise landing at the wrong `BodyIdx` — which is precisely what
            /// E3's replay depends on and what
            /// `a_computed_negated_argument_is_recorded_as_a_closed_no_match`
            /// pins for one shape.
            ///
            /// **Mutation-verified, and it is the cleanest kill in the suite.**
            /// Recording premises in *schedule* order rather than at their
            /// `BodyIdx` — evaluation untouched, only the recording changed —
            /// reddens **C14 and nothing else**. B5 and B6 stay green because
            /// they compare models; the model is genuinely unaffected.
            ///
            /// **E3 stays green too, and that is worth recording.** Replay zips
            /// body literals against premises and `continue`s on a mismatched
            /// pair rather than failing, so a permutation that moves a positive
            /// premise opposite a negated literal slips straight through the
            /// strongest provenance property in the suite. E1/E2/E4 are equally
            /// blind, checking derivations against a single evaluation. Nothing
            /// before this asserted that an explanation is a function of the
            /// program's *meaning* rather than of how its body was typed out.
            #[test]
            fn c14_reordering_does_not_change_the_derivations(
                program in arb_program_with_edb(),
                rule_sel in any::<u8>(),
                stratum_sel in any::<u8>(),
                i in any::<u8>(),
                j in any::<u8>(),
                swap_body in any::<bool>(),
            ) {
                let base = eval(&program).unwrap();

                if swap_body {
                    if program.rules.is_empty() {
                        return Ok(());
                    }
                    let rule_idx = rule_sel as usize % program.rules.len();
                    let len = program.rules[rule_idx].body.len();
                    if len < 2 {
                        return Ok(());
                    }
                    let (a, b) = (i as usize % len, j as usize % len);
                    let swapped = crate::testgen::with_swapped_body(
                        program.clone(), rule_sel, i, j,
                    );
                    let after = eval(&swapped).unwrap();

                    for fact in base.facts() {
                        let want: BTreeSet<Derivation> =
                            base.derivations_of(&fact).cloned().collect();
                        // Un-swap: premises follow their body literal, so the
                        // two moved positions come back before comparing.
                        let got: BTreeSet<Derivation> = after
                            .derivations_of(&fact)
                            .map(|d| {
                                let mut d = d.clone();
                                if d.rule.0 as usize == rule_idx && a != b {
                                    d.premises.swap(a, b);
                                }
                                d
                            })
                            .collect();
                        prop_assert_eq!(
                            got, want,
                            "swapping body literals {} and {} of rule {} changed the \
                             derivations of {:?}",
                            a, b, rule_idx, &fact
                        );
                    }
                } else {
                    let swapped = crate::testgen::with_swapped_stratum_rules(
                        program.clone(), stratum_sel, i, j,
                    );
                    let after = eval(&swapped).unwrap();
                    // Rule ids are unchanged by a stratum reordering, so this
                    // half needs no remapping at all.
                    for fact in base.facts() {
                        let want: BTreeSet<Derivation> =
                            base.derivations_of(&fact).cloned().collect();
                        let got: BTreeSet<Derivation> =
                            after.derivations_of(&fact).cloned().collect();
                        prop_assert_eq!(
                            got, want,
                            "reordering rules within a stratum changed the \
                             derivations of {:?}",
                            &fact
                        );
                    }
                }
            }

            /// E1 — every derived fact has at least one derivation, and at
            /// least one of them is *well-founded*: every fact premise first
            /// appeared in a strictly earlier round than the fact itself
            /// (no-match premises exempt — they carry no round). This is the
            /// invariant `ProofTree::explain` selects by, stated directly
            /// rather than transitively via E2, and it pins first-round
            /// stamping's monotonicity across strata.
            #[test]
            fn e1_derived_facts_have_derivations(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                for fact in model.facts() {
                    if model.is_base(&fact) {
                        continue;
                    }
                    prop_assert!(
                        model.derivations_of(&fact).next().is_some(),
                        "derived fact {:?} has no derivation", fact
                    );
                    let round = model.first_round(&fact).expect("held facts are stamped");
                    prop_assert!(
                        model.derivations_of(&fact).any(|derivation| {
                            derivation.premises.iter().all(|premise| match premise {
                                Premise::Fact(f) => model
                                    .first_round(f)
                                    .is_some_and(|r| r < round),
                                Premise::NoMatch(_)
                                | Premise::Builtin { .. }
                                | Premise::Presence { .. }
                                | Premise::Aggregate { .. } => true,
                            })
                        }),
                        "no well-founded derivation for {:?}", fact
                    );
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
            /// fact; fact premises all hold and no-match premises are genuine
            /// non-matches against the model (checked with this test's own
            /// scan, independent of the engine's matcher).
            #[test]
            fn e3_derivations_replay(program in arb_program_with_edb()) {
                let model = eval(&program).unwrap();
                for fact in model.facts() {
                    for derivation in model.derivations_of(&fact) {
                        for premise in &derivation.premises {
                            match premise {
                                Premise::Fact(f) => prop_assert!(model.contains(f)),
                                Premise::NoMatch(pattern) => {
                                    let refuted = model
                                        .relation(pattern.pred)
                                        .iter()
                                        .any(|tuple| pattern.matches(tuple));
                                    prop_assert!(
                                        !refuted,
                                        "no-match premise {pattern:?} is refuted"
                                    );
                                }
                                Premise::Builtin { .. }
                                | Premise::Presence { .. }
                                | Premise::Aggregate { .. } => {
                                    // Self-justifying, and unreached: this
                                    // generator emits no builtins (`monotype`
                                    // makes every column a symbol, so
                                    // arithmetic cannot appear). `replay` below
                                    // cannot handle them either — it returns
                                    // `None` on a builtin premise — so E3 does
                                    // not currently cover §8 at all. Widening it
                                    // is testing.md E6.
                                }
                            }
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

        // --- The absent value (§4/§8) ---

        /// Builds an absent-rich program from generated `p`/`q` edges over the
        /// pool `{1, 2, absent}`, exercising every place §4's "absent unifies
        /// with nothing" rule can bite:
        ///
        /// - `chain` joins two relations on a middle column that may be absent;
        /// - `self_join` repeats a variable *within* one atom;
        /// - `unmatched` anti-joins on a key that may be absent;
        /// - `contra` asserts a literal and its negation in one body — the
        ///   non-contradiction shape (C9), which must derive nothing;
        /// - `flows` just carries a column through, which must keep absent;
        /// - `above` filters on a comparison whose operand may be absent (false,
        ///   never a type error);
        /// - `bumped` does arithmetic on one (annihilation);
        /// - `present`/`missing` are the two halves of the presence filter, which
        ///   the oracle used to ignore outright.
        ///
        /// A shape-targeted generator in the `aggregate_ir` style: the general
        /// program generator does emit absent facts, but the odds that two of
        /// them meet in a joined position of the same small program are slim, so
        /// the discriminating cases need to be built deliberately.
        fn absent_ir(p_rows: &[(u8, u8)], q_rows: &[(u8, u8)]) -> Program {
            // 2 is the absent slot in the pool; 0 and 1 are ordinary ints.
            let cell = |n: u8| match n % 3 {
                0 => "0".to_string(),
                1 => "1".to_string(),
                _ => "absent".to_string(),
            };
            let mut src = String::new();
            for (a, b) in p_rows {
                src.push_str(&format!("p({}, {}).\n", cell(*a), cell(*b)));
            }
            for (a, b) in q_rows {
                src.push_str(&format!("q({}, {}).\n", cell(*a), cell(*b)));
            }
            src.push_str(
                "chain(X, Y) :- p(X, Z), q(Z, Y).\n\
                 self_join(X) :- p(X, X).\n\
                 unmatched(X) :- p(X, _), not q(X, _).\n\
                 contra(X) :- p(X, _), not p(X, _).\n\
                 flows(X, Y) :- p(X, Y).\n\
                 above(X) :- p(X, _), X > 0.\n\
                 bumped(X, N) :- p(X, Y), N = Y + 1.\n\
                 present(X) :- p(X, Y), Y is not absent.\n\
                 missing(X) :- p(X, Y), Y is absent.\n",
            );
            let ast = crate::parser::parse(&src).expect("generated source parses");
            lower(&ast).expect("generated absent program lowers")
        }

        proptest! {
            // An evaluator differential; keep the case count modest.
            #![proptest_config(ProptestConfig::with_cases(96))]

            /// B1 for the absent value: naive and semi-naive agree over programs
            /// that actually *join* on absent.
            ///
            /// This is the property that was missing. Before 2026-07-25 the
            /// oracle had no absent semantics at all (plain `==`, no
            /// annihilation), so it silently disagreed with the engine on every
            /// absent value — and no generator emitted one, which is the only
            /// reason B1 stayed green. Reverting either half (the oracle's
            /// `same_value`, or `absent_ir`'s pool) must make this fail.
            #[test]
            fn b1_absent_programs_agree(
                p_rows in prop::collection::vec((0u8..3, 0u8..3), 0..8),
                q_rows in prop::collection::vec((0u8..3, 0u8..3), 0..8),
            ) {
                let program = absent_ir(&p_rows, &q_rows);
                let model = eval(&program).unwrap();
                prop_assert_eq!(model_facts(&model), naive_eval(&program).unwrap());
            }

            /// **C9 — non-contradiction**: a body asserting both `p(X, _)` and
            /// `not p(X, _)` derives nothing, for every row set including ones
            /// where `X` binds to `absent`.
            ///
            /// **B1 cannot carry this**, and that is the point of stating it
            /// separately: both evaluators implement the same semantics, so a
            /// differential agrees with itself while the law is broken — the
            /// same blindness `bugs/006` found in a differential over a type
            /// error. The law has to be asserted directly against the model.
            /// Reverting `NoMatchPattern::matches` to `unifies_with` must make
            /// this fail.
            #[test]
            fn c9_a_body_and_its_negation_derive_nothing(
                p_rows in prop::collection::vec((0u8..3, 0u8..3), 0..8),
                q_rows in prop::collection::vec((0u8..3, 0u8..3), 0..8),
            ) {
                let program = absent_ir(&p_rows, &q_rows);
                let contra = program
                    .predicates
                    .iter()
                    .position(|p| p.name == "contra")
                    .expect("the generated program defines `contra`");
                let model = eval(&program).unwrap();
                prop_assert!(
                    model.relation(PredId(contra as u32)).is_empty(),
                    "P and not-P were both satisfied: {:?}",
                    model.relation(PredId(contra as u32))
                );
            }
        }

        /// Non-vacuity for C9 (the generator-coverage convention, testing.md):
        /// the shape only discriminates when `p`'s key column actually holds
        /// `absent`, and a green C9 over row sets that never produce one would
        /// prove nothing. Pins that `absent_ir`'s pool reaches the key column,
        /// and that the contradiction rule is reachable at all — `unmatched`
        /// derives from the same positive prefix.
        #[test]
        fn absent_ir_binds_an_absent_key_under_negation() {
            let program = absent_ir(&[(2, 0)], &[]);
            let pred = |name: &str| {
                PredId(
                    program
                        .predicates
                        .iter()
                        .position(|p| p.name == name)
                        .expect("predicate exists") as u32,
                )
            };
            let model = eval(&program).expect("evaluates");
            // `p(absent, 0)` binds X := absent, so the negated literal is an
            // no-match pattern closed to `absent` — the discriminating case.
            assert_eq!(
                model.relation(pred("unmatched")),
                &BTreeSet::from([Tuple(vec![Value::Absent])]),
                "the positive prefix must bind an absent key"
            );
            assert!(model.relation(pred("contra")).is_empty());
        }

        // --- Aggregation (§9) ---

        /// Builds an aggregate program from generated edges: `op`-aggregate the
        /// second column of `edge`, grouped by the first. Parsed and lowered
        /// through the real front end (so the hoist/stratification path is
        /// exercised too), then handed to both evaluators.
        fn aggregate_ir(op: &str, edges: &[(u8, u8)]) -> Program {
            let mut src = String::new();
            for (a, b) in edges {
                src.push_str(&format!("edge({a}, {b}).\n"));
            }
            src.push_str(&format!(
                "result(K, N) :- edge(K, _), N = {op} {{ V | edge(K, V) }}.\n"
            ));
            let ast = crate::parser::parse(&src).expect("generated source parses");
            lower(&ast).expect("generated aggregate program lowers")
        }

        /// The pool the aggregate order-invariance arms draw their **values**
        /// from (`bugs/007`, 2026-08-20): a huge magnitude beside small ones, so
        /// that `(a + b) + c` and `a + (c + b)` differ. Every aggregate
        /// generator used to be int-pooled in a small range, where `sum` is
        /// associative and cannot overflow — which is why the arms below stated
        /// order-invariance for a month while unable to reach the case that
        /// breaks it.
        ///
        /// Kept as **source text** rather than `f64`s formatted at use: Rust
        /// prints `1e16` as `10000000000000000`, which the lexer reads back as
        /// an `int` (`lexer.rs`), silently restoring the int-pooled generator
        /// this exists to replace.
        const ILL_CONDITIONED_FLOATS: [&str; 6] = ["1e16", "-1e16", "0.1", "1e-16", "2.5", "-3.5"];

        /// The pool member `index` selects, wrapping — so a generator can draw a
        /// plain `u8` and stay a value the shrinker understands.
        fn float_literal(index: u8) -> &'static str {
            ILL_CONDITIONED_FLOATS[index as usize % ILL_CONDITIONED_FLOATS.len()]
        }

        /// The §16.4 shape with the rule's three body literals in a chosen
        /// order — two positive atoms supplying the group key and the aggregate.
        /// Returns `None` if that order does not lower (see
        /// `b5_aggregate_body_order_does_not_change_the_model`).
        ///
        /// The aggregated column is float-pooled (`float_literal`); the group
        /// key stays an int.
        fn aggregate_ir_permuted(op: &str, edges: &[(u8, u8)], perm: usize) -> Option<Program> {
            let mut src = String::new();
            for (a, b) in edges {
                src.push_str(&format!("edge({a}, {}).\nnode({a}).\n", float_literal(*b)));
            }
            let literals = [
                "edge(K, _)".to_string(),
                "node(K)".to_string(),
                format!("N = {op} {{ V | edge(K, V) }}"),
            ];
            // The `perm`-th of the 3! orderings, by Lehmer code.
            let mut pool: Vec<&String> = literals.iter().collect();
            let mut ordered: Vec<&String> = Vec::with_capacity(3);
            let mut rest = perm;
            for radix in (1..=3).rev() {
                ordered.push(pool.remove(rest % radix));
                rest /= radix;
            }
            let body: Vec<&str> = ordered.iter().map(|s| s.as_str()).collect();
            src.push_str(&format!("result(K, N) :- {}.\n", body.join(", ")));
            let ast = crate::parser::parse(&src).expect("generated source parses");
            lower(&ast).ok()
        }

        proptest! {
            /// B5 for aggregation: **every ordering lowers, and all agree**.
            ///
            /// A body is a conjunction, so where its literals are written must
            /// not decide what it means. Since builtins are dependency-scheduled
            /// (`crate::schedule`, 2026-07-25) this holds unconditionally for the
            /// shapes here, rather than only for the permutations that happened
            /// to be written in dependency order. Two earlier states this pins
            /// against: before scheduling, moving a group key's binder past the
            /// aggregate was rejected; before *that*, it silently turned the key
            /// existential and aggregated over everything.
            ///
            /// Float-pooled since 2026-08-20 (`bugs/007`), which widens what the
            /// permutations carry — typing, grouping and printing now see floats
            /// — but **measured, this arm cannot see witness order at all**, and
            /// deleting `fold_aggregate`'s sort leaves it green. Its goal is the
            /// single atom `edge(K, V)`, so for a fixed group key the witnesses
            /// arrive in the relation's own `BTreeSet` order, which *is* `V`
            /// ascending, which is what sorting produces: an equivalent mutant,
            /// not a hole. The arm that carries the order claim is
            /// `b1_aggregate_goal_shapes_agree`, whose goal has two atoms and so
            /// has two enumeration orders to disagree about.
            #[test]
            fn b5_aggregate_body_order_does_not_change_the_model(
                edges in prop::collection::vec((0u8..4, 0u8..6), 0..10),
                op in prop::sample::select(vec!["count", "sum", "min", "max", "avg"]),
            ) {
                let baseline = aggregate_ir_permuted(op, &edges, 0)
                    .map(|p| model_facts(&eval(&p).unwrap()));
                prop_assert!(baseline.is_some(), "the source order must lower");
                for perm in 1..6 {
                    let program = aggregate_ir_permuted(op, &edges, perm);
                    prop_assert!(program.is_some(), "perm {} failed to lower", perm);
                    let facts = model_facts(&eval(&program.unwrap()).unwrap());
                    prop_assert_eq!(&facts, baseline.as_ref().unwrap(), "perm {}", perm);
                }
            }

            /// The same invariance where it used to fail hardest: the group key
            /// is bound by an **`=`-assignment**, and every position of that
            /// assignment relative to the aggregate must give one answer.
            #[test]
            fn b5_a_computed_group_key_is_order_independent(
                edges in prop::collection::vec((0u8..5, 0u8..6), 0..10),
                op in prop::sample::select(vec!["count", "sum", "min", "max", "avg"]),
            ) {
                let mut facts = String::new();
                for (a, b) in &edges {
                    facts.push_str(&format!("edge({a}, {b}).\nnode({a}).\n"));
                }
                // `Y = X + 1` binds the group key; try it before and after the
                // aggregate that reads it.
                let build = |body: &str| {
                    let src = format!("{facts}result(X, N) :- {body}.\n");
                    let ast = crate::parser::parse(&src).expect("parses");
                    lower(&ast).expect("both orderings schedule")
                };
                let before = build(&format!(
                    "node(X), Y = X + 1, N = {op} {{ V | edge(Y, V) }}"
                ));
                let after = build(&format!(
                    "node(X), N = {op} {{ V | edge(Y, V) }}, Y = X + 1"
                ));
                prop_assert_eq!(
                    model_facts(&eval(&before).unwrap()),
                    model_facts(&eval(&after).unwrap())
                );
            }
        }

        /// The §9 grouping shape the original `aggregate_ir` could not express:
        /// group keys come from a **different relation** than the goal, so a key
        /// with no matching rows is a genuine **empty group**, and the values may
        /// be `absent`.
        ///
        /// `nodes` are the group keys; `edges` are `(key, Some(value) | None)`
        /// where `None` prints as `absent`.
        fn grouped_ir(op: AggOp, nodes: &[u8], edges: &[(u8, Option<i8>)]) -> Program {
            let mut src = String::new();
            for k in nodes {
                src.push_str(&format!("node({k}).\n"));
            }
            for (k, v) in edges {
                match v {
                    Some(v) => src.push_str(&format!("edge({k}, {v}).\n")),
                    None => src.push_str(&format!("edge({k}, absent).\n")),
                }
            }
            src.push_str(&format!(
                "result(K, N) :- node(K), N = {} {{ V | edge(K, V) }}.\n",
                op.keyword()
            ));
            let ast = crate::parser::parse(&src).expect("generated source parses");
            lower(&ast).expect("generated grouped program lowers")
        }

        /// The expected `result` relation for [`grouped_ir`], computed by a plain
        /// group-by fold that touches **neither evaluator** and does not call
        /// `fold_aggregate` — the B7/C2 pattern applied to §9.
        ///
        /// Set semantics is applied first (facts are a set, so equal `edge` rows
        /// collapse); with the group key fixed, the goal's only variable is `V`,
        /// so the witnesses are exactly the *distinct* values stored for that key.
        fn expected_groups(op: AggOp, nodes: &[u8], edges: &[(u8, Option<i8>)]) -> BTreeSet<Tuple> {
            let rows: BTreeSet<(u8, Option<i8>)> = edges.iter().copied().collect();
            nodes
                .iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(|k| {
                    let witnesses: Vec<Option<i8>> = rows
                        .iter()
                        .filter(|(key, _)| key == k)
                        .map(|(_, v)| *v)
                        .collect();
                    let present: Vec<i64> = witnesses.iter().flatten().map(|v| *v as i64).collect();
                    let value = match op {
                        // A binding is a binding: absent witnesses are counted.
                        AggOp::Count => Value::Int(witnesses.len() as i64),
                        // Everything else skips absent; an empty present-set is
                        // absent, never zero.
                        AggOp::Sum if present.is_empty() => Value::Absent,
                        AggOp::Sum => Value::Int(present.iter().sum()),
                        AggOp::Avg if present.is_empty() => Value::Absent,
                        AggOp::Avg => Value::Float(
                            F64::new(present.iter().sum::<i64>() as f64 / present.len() as f64)
                                .expect("a finite mean of small ints"),
                        ),
                        AggOp::Min => present
                            .iter()
                            .min()
                            .map_or(Value::Absent, |v| Value::Int(*v)),
                        AggOp::Max => present
                            .iter()
                            .max()
                            .map_or(Value::Absent, |v| Value::Int(*v)),
                    };
                    Tuple(vec![Value::Int(i64::from(*k)), value])
                })
                .collect()
        }

        /// A richer aggregate shape for the B1 differential: a **multi-atom
        /// goal** with a **negated literal** inside it, and a group key joined
        /// from a second relation. Differenced against the naive oracle rather
        /// than a hand-rolled one, so no disputed semantics is baked into an
        /// oracle (§17: absent × negation is open).
        ///
        /// `goal_order` permutes the goal's three literals. The goal is a body,
        /// so the evaluator schedules its positives before its negation whatever
        /// the source order (`literal_order`); writing `not blocked(V)` ahead of
        /// the atom that binds `V` must therefore still mean the same thing, and
        /// not silently read `V` as a wildcard.
        fn aggregate_goal_ir(
            op: &str,
            edges: &[(u8, u8)],
            blocked: &[u8],
            goal_order: usize,
        ) -> Program {
            let mut src = String::new();
            for (a, b) in edges {
                src.push_str(&format!("edge({a}, {}).\nnode({a}).\n", float_literal(*b)));
            }
            for b in blocked {
                src.push_str(&format!("blocked({}).\n", float_literal(*b)));
            }
            src.push_str("tag(0, 9).\ntag(1, 8).\n");
            let literals = ["edge(K, V)", "tag(_, _)", "not blocked(V)"];
            let mut pool: Vec<&str> = literals.to_vec();
            let mut ordered: Vec<&str> = Vec::with_capacity(3);
            let mut rest = goal_order;
            for radix in (1..=3).rev() {
                ordered.push(pool.remove(rest % radix));
                rest /= radix;
            }
            src.push_str(&format!(
                "result(K, N) :- node(K), N = {op} {{ V | {} }}.\n",
                ordered.join(", ")
            ));
            let ast = crate::parser::parse(&src).expect("generated source parses");
            lower(&ast).expect("generated aggregate-goal program lowers")
        }

        proptest! {
            // Evaluator properties; keep the case count modest.
            #![proptest_config(ProptestConfig::with_cases(96))]

            /// An **independent grouping oracle** for §9 (the B7/C2 pattern):
            /// random `node`/`edge` data through the canonical grouped aggregate
            /// equals a plain HashMap-style group-by fold that touches neither
            /// evaluator. The fold proptests pin `fold_aggregate` in isolation
            /// and the B1 differential pins the two evaluators against each
            /// other; this is the only check that the *grouping* — which keys
            /// exist, which witnesses land in which group — is right at all.
            ///
            /// Covers what the original single shape could not: group keys drawn
            /// from a different relation than the goal, keys with **no** matching
            /// rows (empty groups → `count` 0, everything else `absent`), and
            /// absent-valued witnesses.
            #[test]
            fn aggregation_matches_an_independent_group_by(
                nodes in prop::collection::vec(0u8..4, 0..5),
                edges in prop::collection::vec((0u8..4, prop::option::of(-9i8..9)), 0..10),
                op in prop::sample::select(vec![
                    AggOp::Count, AggOp::Sum, AggOp::Min, AggOp::Max, AggOp::Avg,
                ]),
            ) {
                let program = grouped_ir(op, &nodes, &edges);
                let model = eval(&program).unwrap();
                let result = program
                    .predicates
                    .iter()
                    .position(|p| p.name == "result")
                    .expect("the generated program defines `result`");
                prop_assert_eq!(
                    model.relation(PredId(result as u32)),
                    &expected_groups(op, &nodes, &edges)
                );
            }

            /// B1 over a **multi-atom, negated goal** — the aggregate goal shapes
            /// the original generator never produced — and, across the goal's
            /// literal orderings, the B5 invariant *inside* a goal: a goal is a
            /// body, so its positives are scheduled before its negation whatever
            /// the source order.
            ///
            /// **Float-pooled since 2026-08-20**, which is `bugs/007`'s second
            /// acceptance half: the six goal orderings were compared over ints in
            /// `-1000..1000`, where `sum` is associative and cannot overflow, so
            /// this stated the order-invariance claim for a month without being
            /// able to reach a case that breaks it. The two-atom goal is what
            /// makes the widening bite — with `tag(_, _)` outer the witnesses
            /// arrive grouped by tag, with `edge(K, V)` outer they arrive in `V`
            /// order, and the same multiset in two orders is exactly the defect.
            ///
            /// *Mutation*: deleting `fold_aggregate`'s `present.sort()` reddens
            /// this and `fold_is_permutation_invariant`, and nothing else in the
            /// suite.
            #[test]
            fn b1_aggregate_goal_shapes_agree(
                edges in prop::collection::vec((0u8..4, 0u8..6), 0..10),
                blocked in prop::collection::vec(0u8..6, 0..4),
                op in prop::sample::select(vec!["count", "sum", "min", "max", "avg"]),
            ) {
                let mut baseline: Option<BTreeSet<Fact>> = None;
                for goal_order in 0..6 {
                    let program = aggregate_goal_ir(op, &edges, &blocked, goal_order);
                    let model = eval(&program).unwrap();
                    let facts = model_facts(&model);
                    prop_assert_eq!(&facts, &naive_eval(&program).unwrap());
                    match &baseline {
                        Some(expected) => {
                            prop_assert_eq!(&facts, expected, "goal order {}", goal_order);
                        }
                        None => baseline = Some(facts),
                    }
                }
            }
        }

        proptest! {
            /// count counts every binding, absent ones included (§9).
            #[test]
            fn count_equals_number_of_bindings(
                values in prop::collection::vec(arb_value(), 0..8),
                extra_absents in 0usize..4,
            ) {
                let mut vs = values;
                for _ in 0..extra_absents {
                    vs.push(Value::Absent);
                }
                prop_assert_eq!(
                    fold_aggregate(AggOp::Count, &vs).unwrap().value,
                    Value::Int(vs.len() as i64)
                );
            }

            /// Absent inputs never change sum/avg/min/max — they are skipped, and
            /// the skip count grows by exactly the number added (§9).
            #[test]
            fn absent_inputs_are_skipped_not_folded(
                ints in prop::collection::vec(-1000i64..1000, 0..8),
                extra_absents in 0usize..4,
            ) {
                let present: Vec<Value> = ints.iter().map(|n| Value::Int(*n)).collect();
                let mut padded = present.clone();
                for _ in 0..extra_absents {
                    padded.push(Value::Absent);
                }
                for op in [AggOp::Sum, AggOp::Avg, AggOp::Min, AggOp::Max] {
                    let base = fold_aggregate(op, &present).unwrap();
                    let with_absent = fold_aggregate(op, &padded).unwrap();
                    prop_assert_eq!(base.value.clone(), with_absent.value.clone());
                    prop_assert_eq!(with_absent.skipped, base.skipped + extra_absents);
                }
            }

            /// min and max bound every present value, over one ordered type (§9).
            ///
            /// Widened past `int` (2026-08-20): §9 accepts `min`/`max` over
            /// **any single type**, so an int-only pool was stating one eighth
            /// of the sentence. The temporal arms matter most — they read the
            /// same `Ord` A4 pins and `ordered_comparison_and_minmax_agree_on_every_type`
            /// ties to `<`, and they were the types that `Ord` had most recently
            /// gained.
            #[test]
            fn min_and_max_bound_every_value(
                vs in prop::collection::vec(crate::testgen::arb_value(), 1..8)
                    .prop_filter(
                        "one type per fold (§9 has no cross-type order to fold over)",
                        |vs| vs.windows(2).all(|w| {
                            std::mem::discriminant(&w[0]) == std::mem::discriminant(&w[1])
                        }),
                    ),
            ) {
                let lo = fold_aggregate(AggOp::Min, &vs).unwrap().value;
                let hi = fold_aggregate(AggOp::Max, &vs).unwrap().value;
                for v in &vs {
                    prop_assert!(lo <= *v && *v <= hi);
                }
            }

            /// **`avg` over durations is a `duration`, not a `float`** (§9,
            /// spec.md:1077) — and it equals the sum scaled by the count, since
            /// §8 makes `duration / int` a scaling rather than a division that
            /// leaves the type behind.
            ///
            /// The rule §9 argues for at length had exactly one hand test
            /// (`tests/pipeline.rs`) before this. It is the arm a future
            /// refactor toward a uniform "avg is int → float" would silently
            /// break, because every *other* avg property is numeric.
            ///
            /// **Mutation**: making `avg_values` fall through to the numeric
            /// branch for durations reddens this and nothing else.
            #[test]
            fn avg_over_durations_is_a_duration(
                us in prop::collection::vec(-1_000_000_000i64..1_000_000_000, 1..8),
            ) {
                let vs: Vec<Value> = us
                    .iter()
                    .map(|n| Value::Duration(temporal::Duration::from_micros(*n)))
                    .collect();
                let got = fold_aggregate(AggOp::Avg, &vs).unwrap().value;
                let total: i64 = us.iter().sum();
                prop_assert_eq!(
                    got,
                    Value::Duration(temporal::Duration::from_micros(total / us.len() as i64)),
                    "avg over durations left the vector type behind"
                );
            }

            /// `sum` over a **point** type is a type error (§9, spec.md:1076) —
            /// the rejection half, which `testing.md`'s corollary asks for
            /// beside every acceptance claim. Adding two dates has no meaning,
            /// and §9 requires the message to say that rather than "not
            /// numeric".
            #[test]
            fn sum_over_a_point_type_is_rejected(
                days in prop::collection::vec(0i64..3000, 1..6),
                as_timestamp in any::<bool>(),
            ) {
                let vs: Vec<Value> = days
                    .iter()
                    .map(|d| {
                        let date = temporal::Date::from_days(
                            temporal::Date::from_ymd(2020, 1, 1).expect("a real day").days()
                                as i64
                                + d,
                        )
                        .expect("inside the range");
                        if as_timestamp { Value::Timestamp(date.at_midnight()) } else { Value::Date(date) }
                    })
                    .collect();
                if vs.len() == 1 {
                    // A one-element fold never applies `+`, so it cannot report
                    // the error — §9's claim is about *adding* two points.
                    return Ok(());
                }
                let err = fold_aggregate(AggOp::Sum, &vs).unwrap_err();
                prop_assert!(
                    err.to_string().contains("type error"),
                    "summing points should be a type error, got: {}", err
                );
            }

            /// **The fold does not depend on the order its witnesses arrive
            /// in** (§9) — permuting the multiset leaves every one of the five
            /// unchanged.
            ///
            /// This is the law two *existing* properties already assume and
            /// neither can see. `b5_aggregate_body_order_does_not_change_the_model`
            /// permutes the body, which is exactly what changes which literal is
            /// outer and so the order `enumerate_from` pushes values into the
            /// witness `Vec`; `b1_aggregate_goal_shapes_agree` lets the two
            /// evaluators enumerate a goal differently. Both were int-only, and
            /// over ints the question cannot arise: integer addition is
            /// associative and commutative, so any permutation folds alike.
            ///
            /// **Floats are where it can fail**, which is why the pool is
            /// float-valued and deliberately ill-conditioned — a large magnitude
            /// beside small ones is the classic case where `(a + b) + c` and
            /// `a + (c + b)` differ in the last bits. Determinism of output is a
            /// ratified design claim (`testing.md`, "Why PBT fits").
            ///
            /// **It did redden, and that was `bugs/007`** — two spellings of one
            /// goal giving `r(0.0)` against `r(0.1)` over floats, and
            /// answer-versus-`exit 2` over the int overflow check, which §17
            /// (2026-07-25) rules out in terms: "Two orderings of one
            /// conjunction, two answers, which no declarative reading permits."
            /// Written `#[ignore]`d and failing in the sitting the defect was
            /// found, the way `dash_q_rule_equals_the_same_rule_in_a_file` was
            /// written for `bugs/002`; `fold_aggregate` folds in §14 order now,
            /// so the answer is a function of the multiset rather than of the
            /// enumeration, and the `#[ignore]` is gone. The shrunk
            /// counterexample stays committed in
            /// `proptest-regressions/engine/mod.txt`: it is the case this has to
            /// keep passing.
            ///
            /// **Mutation-verified**: deleting the `present.sort()` reddens
            /// this and `b1_aggregate_goal_shapes_agree` — the end-to-end arm
            /// that `007`'s second acceptance half float-pooled — and nothing
            /// else in the suite. `b5_aggregate_body_order_does_not_change_the_model`
            /// stays green even float-pooled, for a reason recorded there: its
            /// goal is a single atom, so it has only one enumeration order to
            /// offer.
            #[test]
            fn fold_is_permutation_invariant(
                floats in prop::collection::vec(
                    prop_oneof![
                        (-1e6f64..1e6),
                        Just(1e16f64),
                        Just(-1e16f64),
                        Just(0.1f64),
                        Just(1e-16f64),
                    ],
                    1..8,
                ),
                rotation in 0usize..8,
            ) {
                let vs: Vec<Value> = floats
                    .iter()
                    .map(|f| Value::Float(F64::new(*f).expect("pool floats are not NaN")))
                    .collect();
                let mut permuted = vs.clone();
                permuted.rotate_left(rotation % vs.len());
                permuted.reverse();
                for op in [AggOp::Count, AggOp::Sum, AggOp::Avg, AggOp::Min, AggOp::Max] {
                    let straight = fold_aggregate(op, &vs).unwrap();
                    let shuffled = fold_aggregate(op, &permuted).unwrap();
                    prop_assert_eq!(
                        straight.value, shuffled.value,
                        "{:?} depends on witness order over {:?}", op, &floats
                    );
                }
            }

            /// **`avg` is `sum` divided by `count`** over the present values
            /// (§9). The three ops are each pinned in isolation above; nothing
            /// tied them to each other, and `avg_values` is a separate code path
            /// (`engine/mod.rs`) that could drift from `sum_values` without a
            /// single existing property noticing.
            ///
            /// Stated over `int` because that is where all three are exact.
            /// Over `float` the identity is only approximate — and over any type
            /// the *ordering* caveat of `bugs/007` applies to both sides
            /// equally, so it does not bear on this law.
            ///
            /// **Mutation-verified**: perturbing `avg_values`' own accumulator
            /// (`sum` → `sum + 1.0`) so it drifts from `sum_values` reddens
            /// **this and nothing else** — `avg_is_the_float_mean_of_present_values`
            /// and `absent_inputs_are_skipped_not_folded` both stay green,
            /// because each compares avg against avg. That is the measurement
            /// showing the law is what ties the two code paths together.
            #[test]
            fn avg_is_sum_over_count(
                ints in prop::collection::vec(-1000i64..1000, 1..8),
                extra_absents in 0usize..3,
            ) {
                let mut vs: Vec<Value> = ints.iter().map(|n| Value::Int(*n)).collect();
                for _ in 0..extra_absents {
                    vs.push(Value::Absent);
                }
                let avg = fold_aggregate(AggOp::Avg, &vs).unwrap().value;
                let sum = fold_aggregate(AggOp::Sum, &vs).unwrap().value;
                let present = ints.len() as f64;
                let Value::Int(total) = sum else {
                    return Err(TestCaseError::fail("sum of ints is an int"));
                };
                prop_assert_eq!(
                    avg,
                    Value::Float(F64::new(total as f64 / present).unwrap()),
                    "avg is not sum/count over the present values"
                );
            }

            /// **The fold is a monoid homomorphism over group partition**:
            /// splitting a group's witnesses into two halves and combining the
            /// two folds equals folding the whole. `count` adds, `sum` adds,
            /// `min`/`max` take the extreme of the two extremes.
            ///
            /// This is the law that pins **grouping** rather than folding.
            /// `aggregation_matches_an_independent_group_by` (B7) checks which
            /// witnesses land in which group, but only for one generated shape;
            /// stated as a law, partition-invariance says the answer cannot
            /// depend on how the witnesses were *divided up* at all — which is
            /// the property a future change to group-key handling would break.
            ///
            /// `avg` is deliberately absent: the mean is not a monoid
            /// homomorphism (you cannot average two averages without their
            /// counts), and asserting it would be stating a false law.
            ///
            /// Ints only, and that exclusion is `bugs/007`: over floats the
            /// combination is exactly the re-association the fold is not
            /// invariant under, so a float arm here would be measuring that
            /// defect rather than this law.
            ///
            /// **Mutation-verified**: making `extreme_value` keep the *first*
            /// present value rather than the extreme reddens the min/max halves
            /// here, alongside five sibling min/max tests — an aim that is
            /// broad rather than wrong, since any min/max defect is visible to
            /// several checks at once. `b7_ancestor_is_transitive_closure`
            /// staying green is the useful contrast: it is the one aggregate-free
            /// oracle in that group.
            #[test]
            fn folding_a_partitioned_group_combines(
                left in prop::collection::vec(-1000i64..1000, 1..6),
                right in prop::collection::vec(-1000i64..1000, 1..6),
            ) {
                let vals = |ns: &[i64]| -> Vec<Value> {
                    ns.iter().map(|n| Value::Int(*n)).collect()
                };
                let whole: Vec<Value> =
                    vals(&left).into_iter().chain(vals(&right)).collect();

                let fold = |op: AggOp, vs: &[Value]| fold_aggregate(op, vs).unwrap().value;

                prop_assert_eq!(
                    fold(AggOp::Count, &whole),
                    Value::Int((left.len() + right.len()) as i64),
                    "count is not additive over a partition"
                );
                prop_assert_eq!(
                    fold(AggOp::Sum, &whole),
                    apply_arith(
                        ArithOp::Add,
                        fold(AggOp::Sum, &vals(&left)),
                        fold(AggOp::Sum, &vals(&right)),
                    )
                    .unwrap(),
                    "sum is not additive over a partition"
                );
                prop_assert_eq!(
                    fold(AggOp::Min, &whole),
                    fold(AggOp::Min, &vals(&left)).min(fold(AggOp::Min, &vals(&right))),
                    "min is not the extreme of the two extremes"
                );
                prop_assert_eq!(
                    fold(AggOp::Max, &whole),
                    fold(AggOp::Max, &vals(&left)).max(fold(AggOp::Max, &vals(&right))),
                    "max is not the extreme of the two extremes"
                );
            }

            /// sum matches an independent integer fold; empty → absent (§9).
            #[test]
            fn sum_matches_an_independent_fold(ints in prop::collection::vec(-1000i64..1000, 0..8)) {
                let vs: Vec<Value> = ints.iter().map(|n| Value::Int(*n)).collect();
                let got = fold_aggregate(AggOp::Sum, &vs).unwrap().value;
                if vs.is_empty() {
                    prop_assert_eq!(got, Value::Absent);
                } else {
                    let expected: i64 = ints.iter().sum();
                    prop_assert_eq!(got, Value::Int(expected));
                }
            }
        }

        proptest! {
            // Evaluator differentials are the expensive properties; keep the
            // case count modest (testing.md, Tooling).
            #![proptest_config(ProptestConfig::with_cases(64))]

            /// B1 for aggregation: naive and semi-naive agree as fact sets over
            /// generated grouped-aggregate programs. The fold is shared by both
            /// evaluators, so this pins their witness enumeration and grouping.
            #[test]
            fn b1_aggregate_programs_agree(
                edges in prop::collection::vec((0u8..4, 0u8..6), 0..12),
                op in prop::sample::select(vec!["count", "sum", "min", "max", "avg"]),
            ) {
                let program = aggregate_ir(op, &edges);
                let model = eval(&program).unwrap();
                prop_assert_eq!(model_facts(&model), naive_eval(&program).unwrap());
            }
        }

        proptest! {
            /// **The `as` cast's round-trip law** (§8, 2026-08-16): rendering a
            /// value to `string` and reading it back recovers it exactly.
            ///
            /// This is D1's closure property at the *value* level, and it holds
            /// for the same reason: `V as string` is §14's canonical spelling,
            /// and `string as T` is the literal grammar that spelling is written
            /// in, so the two are inverse by construction — one classifier, one
            /// printer, no second opinion (`lexer::classify_cell`). An
            /// independent law rather than a differential: it never asks a
            /// second evaluator what it thinks, it asks whether the composition
            /// is the identity.
            ///
            /// **Mutation-verified**: making `print_f64` emit `{}` instead of
            /// `{:?}` reddens the float arm (`2.0` prints as `2`, which reads
            /// back an int, so the cast yields `absent`).
            #[test]
            fn casting_through_string_is_the_identity(
                n in any::<i64>(),
                f in proptest::num::f64::NORMAL,
                b in any::<bool>(),
                temporal in crate::testgen::arb_temporal(),
            ) {
                // The temporal arms (2026-08-20) are the half T1 cannot reach.
                // T1 round-trips a value through `temporal::parse_temporal`
                // directly; this goes through `apply_cast`, so the reading side
                // is `lexer::classify_cell` and the writing side is
                // `print::print_value` **minus the `@`** — the sigil is a
                // delimiter the printer supplies, and §8's render/read pair is
                // inverse only if it is dropped and re-supplied consistently.
                let (as_date, as_ts, as_dur) = match temporal {
                    crate::temporal::Temporal::Date(d) => (Some(Value::Date(d)), None, None),
                    crate::temporal::Temporal::Timestamp(t) => (None, Some(Value::Timestamp(t)), None),
                    crate::temporal::Temporal::Duration(d) => (None, None, Some(Value::Duration(d))),
                };
                let temporal_values = [as_date, as_ts, as_dur].into_iter().flatten();
                for value in [Value::Int(n), Value::Float(F64::new(f).unwrap()), Value::Bool(b)]
                    .into_iter()
                    .chain(temporal_values)
                {
                    let ty = match value {
                        Value::Int(_) => TypeName::Int,
                        Value::Float(_) => TypeName::Float,
                        Value::Date(_) => TypeName::Date,
                        Value::Timestamp(_) => TypeName::Timestamp,
                        Value::Duration(_) => TypeName::Duration,
                        _ => TypeName::Bool,
                    };
                    let text = apply_cast(value.clone(), TypeName::String).unwrap();
                    prop_assert!(matches!(text, Value::String(_)), "rendering is total");
                    prop_assert_eq!(
                        apply_cast(text, ty).unwrap(),
                        value.clone(),
                        "`{:?} as string as {}` lost the value",
                        value,
                        ty.keyword()
                    );
                }
            }

            /// `absent as T` is `absent` for **every** `T` (§4/§8) — annihilation
            /// ahead of every other check, so it holds even for the pairs that
            /// have no conversion at all.
            ///
            /// **Mutation-verified**: the same mutation as the table test —
            /// moving `apply_cast`'s absent short-circuit below the
            /// undefined-pair check turns three of the five arms into errors.
            #[test]
            fn absent_survives_every_cast(
                ty in prop::sample::select(vec![
                    TypeName::Int, TypeName::Float, TypeName::String,
                    TypeName::Symbol, TypeName::Bool,
                    // All eight since 2026-08-20 — "every `T`" had meant five
                    // since the temporal types landed, and the annihilation
                    // short-circuit sits ahead of the undefined-pair check
                    // precisely so the pairs with *no* conversion still hold.
                    TypeName::Date, TypeName::Timestamp, TypeName::Duration,
                ]),
            ) {
                prop_assert_eq!(apply_cast(Value::Absent, ty).unwrap(), Value::Absent);
            }

            /// A cast is **idempotent at its own type**: `V as T` where `V`
            /// already has type `T` is `V`, for every one of the **eight**.
            /// Pins the identity diagonal of §8's table, which is the part most
            /// easily broken by a reorganisation of `apply_cast`'s match arms.
            ///
            /// It reaches all eight since 2026-08-20 without an edit here,
            /// because `arb_value` was widened — which is the argument for
            /// fixing the pool rather than each property: the diagonal's
            /// temporal cells came along for free.
            ///
            /// **Mutation-verified**: dropping `TypeName::String`'s
            /// `Value::String(_) => return Ok(value)` early return reddens it —
            /// the string is re-rendered *with* its quotes.
            #[test]
            fn casting_to_a_values_own_type_is_the_identity(value in arb_value()) {
                let ty = value_type(&value);
                prop_assert_eq!(apply_cast(value.clone(), ty).unwrap(), value);
            }
        }
    }
}
