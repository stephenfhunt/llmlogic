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
//! satisfied absence is recorded as a [`crate::provenance::Premise::Absent`]
//! pattern.
//!
//! Still ahead here: magic sets are a future optimization.

#[cfg(test)]
pub(crate) mod naive;

use std::collections::{BTreeSet, HashMap};

use crate::Result;
use crate::ast::{ArithOp, CmpOp};
use crate::error::Error;
use crate::ir::{
    Atom, BodyLiteral, BodyLiteralKind, Expr, F64, Fact, PredId, Program, Query, Rule, RuleId,
    Term, Tuple, Value,
};
use crate::provenance::{AbsentPattern, Derivation, Premise};

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
        validate_body(&query.body, &query.var_names)?;
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
        })?;
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
        round = eval_stratum(program, stratum, &mut model, round)?;
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
            for literal in &program.rules[rule_id.0 as usize].body {
                if let BodyLiteralKind::NegAtom(atom) = &literal.kind
                    && defining_stratum[atom.pred.0 as usize].is_some_and(|def| def >= level)
                {
                    return Err(Error::Semantic(format!(
                        "malformed IR: rule {} negates `{}`, which is not defined in a \
                         strictly lower stratum",
                        rule_id.0,
                        program.pred_info(atom.pred).name
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Enforces the negation IR contract: a negated atom's *named* variables must
/// be bound by a positive atom of the same body. Well-lowered IR satisfies this
/// via §10 safety; hand-built IR that violates it would otherwise silently
/// evaluate the named variable as a wildcard. Comparison operand safety (§8) is
/// enforced in lowering; a malformed comparison fed as hand-built IR surfaces as
/// a structured error when its operand is evaluated.
fn validate_body(body: &[BodyLiteral], var_names: &[Option<String>]) -> Result<()> {
    for literal in body {
        match &literal.kind {
            BodyLiteralKind::Atom(_)
            | BodyLiteralKind::Compare { .. }
            | BodyLiteralKind::Presence { .. } => {}
            BodyLiteralKind::NegAtom(atom) => {
                for arg in &atom.args {
                    if let Term::Var(var) = arg
                        && let Some(Some(name)) = var_names.get(var.0 as usize)
                        && !positively_bound(body, *var)
                    {
                        return Err(Error::Semantic(format!(
                            "malformed IR: named variable `{name}` in negated atom is not \
                             bound by a positive body atom"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Does `var` occur in a positive atom of `body`?
fn positively_bound(body: &[BodyLiteral], var: crate::ir::Var) -> bool {
    body.iter().any(|literal| {
        matches!(&literal.kind, BodyLiteralKind::Atom(atom)
            if atom.args.iter().any(|arg| matches!(arg, Term::Var(v) if *v == var)))
    })
}

/// Runs one stratum to fixpoint, semi-naively. Returns the updated round
/// counter (monotone across strata, for `first_round` stamping).
fn eval_stratum(
    program: &Program,
    stratum: &[RuleId],
    model: &mut Model,
    mut round: u32,
) -> Result<u32> {
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
    // Positive atoms first (they bind variables), then negated atoms as
    // anti-join filters, then comparison/assignment builtins in source order.
    // Positives-first guarantees every non-assignment operand is bound; source
    // order among comparisons preserves assignment chains (`N = A+1, M = N+1`).
    let mut order: Vec<usize> = Vec::with_capacity(cx.body.len());
    for (idx, literal) in cx.body.iter().enumerate() {
        if matches!(literal.kind, BodyLiteralKind::Atom(_)) {
            order.push(idx);
        }
    }
    for (idx, literal) in cx.body.iter().enumerate() {
        if matches!(literal.kind, BodyLiteralKind::NegAtom(_)) {
            order.push(idx);
        }
    }
    for (idx, literal) in cx.body.iter().enumerate() {
        if matches!(
            literal.kind,
            BodyLiteralKind::Compare { .. } | BodyLiteralKind::Presence { .. }
        ) {
            order.push(idx);
        }
    }
    let mut bindings: Vec<Option<Value>> = vec![None; num_vars];
    let mut premises: Vec<Option<Premise>> = vec![None; cx.body.len()];
    enumerate_from(cx, &order, 0, &mut bindings, &mut premises, on_match)
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
            let candidates: Box<dyn Iterator<Item = &Tuple>> =
                match cx.views[idx] {
                    AtomView::Full => Box::new(full.iter()),
                    AtomView::Delta => match atom_delta {
                        Some(delta) => Box::new(delta.iter()),
                        None => return Ok(()),
                    },
                    AtomView::Old => Box::new(full.iter().filter(move |tuple| {
                        atom_delta.is_none_or(|delta| !delta.contains(*tuple))
                    })),
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
            // Instantiate the absence pattern: constants and bound variables
            // close a slot; an unbound slot is wildcard-fresh (validated)
            // and stays open — existential under the negation (§7).
            let pattern = AbsentPattern {
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
            // negation; no match records the absence and moves on. Negated
            // atoms bind nothing.
            if cx
                .model
                .relation(atom.pred)
                .iter()
                .any(|tuple| pattern.matches(tuple))
            {
                return Ok(());
            }
            premises[idx] = Some(Premise::Absent(pattern));
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
                    premises[idx] = Some(Premise::Builtin { op: *op, lhs, rhs });
                    let result = enumerate_from(cx, order, depth + 1, bindings, premises, on_match);
                    premises[idx] = None;
                    result?;
                }
                CompareOutcome::Bind { slot, value } => {
                    premises[idx] = Some(Premise::Builtin {
                        op: *op,
                        lhs: value.clone(),
                        rhs: value.clone(),
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
            let is_absent = value == Value::Absent;
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
    }
    Ok(())
}

/// The **semantic** sameness of two ground values (§4): structural equality,
/// except `absent` unifies with nothing — not with a value, and not with
/// another `absent`. This is the join/anti-join notion (keeping missing foreign
/// keys from matching each other); the *structural* notion (`Value`'s derived
/// `Eq`, where `absent == absent`) is reserved for set dedup and output order.
pub(crate) fn values_unify(a: &Value, b: &Value) -> bool {
    *a != Value::Absent && a == b
}

/// Unifies an atom against a ground tuple under the current bindings.
/// Returns the slots newly bound here (for backtracking), or `None` on
/// mismatch (with any partial bindings already undone).
fn try_match(atom: &Atom, tuple: &Tuple, bindings: &mut [Option<Value>]) -> Option<Vec<usize>> {
    let mut bound: Vec<usize> = Vec::new();
    for (term, value) in atom.args.iter().zip(&tuple.0) {
        let matches = match term {
            Term::Const(constant) => values_unify(constant, value),
            Term::Var(var) => {
                let slot = var.0 as usize;
                match &bindings[slot] {
                    // An already-bound value must semantically unify with the
                    // cell — `absent` unifies with nothing, so a slot bound to
                    // `absent` (or a cell that is `absent`) never re-matches.
                    Some(existing) => values_unify(existing, value),
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
    if *lhs == Value::Absent || *rhs == Value::Absent {
        return Ok(false);
    }
    if std::mem::discriminant(lhs) != std::mem::discriminant(rhs) {
        return Err(Error::Semantic(format!(
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
            Error::Semantic(
                "malformed IR: arithmetic operand variable is not bound by the body".to_string(),
            )
        }),
        Expr::Binary { op, lhs, rhs } => {
            let l = eval_expr(lhs, bindings)?;
            let r = eval_expr(rhs, bindings)?;
            apply_arith(*op, l, r)
        }
    }
}

/// Applies an arithmetic operator. Strict (§4, spec §17 2026-07-21): `int op
/// int -> int` and `float op float -> float`; any mixed or non-numeric operand
/// is a structured type error.
fn apply_arith(op: ArithOp, lhs: Value, rhs: Value) -> Result<Value> {
    // Absent annihilates (§8), *ahead* of the type/div-by-zero/overflow checks:
    // `absent / 0` and `5 / absent` are both `absent`, never an error. This is
    // value-propagation, not a third truth value.
    if lhs == Value::Absent || rhs == Value::Absent {
        return Ok(Value::Absent);
    }
    match (&lhs, &rhs) {
        (Value::Int(a), Value::Int(b)) => arith_int(op, *a, *b),
        (Value::Float(a), Value::Float(b)) => arith_float(op, *a, *b),
        _ => Err(Error::Semantic(format!(
            "type error: arithmetic `{}` requires two ints or two floats, got {} and {}",
            arith_symbol(op),
            value_type_name(&lhs),
            value_type_name(&rhs),
        ))),
    }
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
                return Err(Error::Semantic(format!(
                    "arithmetic error: division by zero in `{a} / {b}`"
                )));
            }
            a.checked_div(b)
        }
    };
    checked.map(Value::Int).ok_or_else(|| {
        Error::Semantic(format!(
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
        assert!(values_unify(&Value::Int(1), &Value::Int(1)));
        assert!(!values_unify(&Value::Int(1), &Value::Int(2)));
        // Absent unifies with nothing — not a value, not another absent — even
        // though it is *structurally* equal to itself (set dedup relies on that).
        assert!(!values_unify(&Value::Absent, &Value::Int(1)));
        assert!(!values_unify(&Value::Int(1), &Value::Absent));
        assert!(!values_unify(&Value::Absent, &Value::Absent));
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
    /// its absence pattern, and the proof tree terminating at an `Absent`
    /// leaf. "alice" is the only person no `parent(_, x)` fact points at.
    #[test]
    fn example_16_2_evaluates_with_absence_provenance() {
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
        let absent = AbsentPattern {
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
                    Premise::Absent(absent.clone()),
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
                    ProofTree::Absent(absent),
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
    /// variable in a negated atom with no positive binder is malformed
    /// (well-lowered IR is protected by §10 safety).
    #[test]
    fn unbound_named_var_in_negated_atom_is_malformed_ir() {
        // q(X) :- p(X), not r(Y).   (Y named, never positively bound)
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

    // --- Phase B (B1–B7) and Phase E (E1–E4) properties (testing.md) ---

    mod properties {
        use std::collections::{BTreeSet, HashMap};

        use proptest::prelude::*;

        use super::super::naive::{ground, match_atom, naive_eval};
        use super::super::*;
        use crate::ast::TypeName;
        use crate::ir::fixtures::example_16_1;
        use crate::lower::lower;
        use crate::provenance::ProofTree;
        use crate::testgen::{
            arb_comparison_program, arb_extension_pair, arb_parent_edges, arb_program_with_edb,
            arb_value, arb_well_typed_program, with_duplicated_facts, with_extra_fact,
            with_swapped_body, with_swapped_stratum_rules,
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
                    (BodyLiteralKind::NegAtom(atom), Premise::Absent(pattern)) => {
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
                ProofTree::Absent(pattern) => {
                    // An absence leaf must actually be absent: no tuple of
                    // the predicate falls under the pattern.
                    prop_assert!(
                        !model
                            .relation(pattern.pred)
                            .iter()
                            .any(|tuple| pattern.matches(tuple)),
                        "absence leaf {pattern:?} is refuted by the model"
                    );
                }
                ProofTree::Builtin { .. } | ProofTree::Presence { .. } => {
                    // A satisfied comparison/assignment/presence leaf carries its
                    // own justification (the evaluated operands) — nothing to
                    // check against the model.
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
                prop_assert_eq!(values_unify(&a, &b), a == b);
                prop_assert!(!values_unify(&a, &Value::Absent));
                prop_assert!(!values_unify(&Value::Absent, &a));
            }

            /// `absent` sorts before every typed value under the canonical `Ord`.
            #[test]
            fn absent_sorts_first(v in arb_value()) {
                prop_assert!(Value::Absent < v);
            }
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

            /// E1 — every derived fact has at least one derivation, and at
            /// least one of them is *well-founded*: every fact premise first
            /// appeared in a strictly earlier round than the fact itself
            /// (absence premises exempt — they carry no round). This is the
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
                                Premise::Absent(_)
                                | Premise::Builtin { .. }
                                | Premise::Presence { .. } => true,
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
            /// fact; fact premises all hold and absence premises are genuine
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
                                Premise::Absent(pattern) => {
                                    let refuted = model
                                        .relation(pattern.pred)
                                        .iter()
                                        .any(|tuple| pattern.matches(tuple));
                                    prop_assert!(
                                        !refuted,
                                        "absence premise {pattern:?} is refuted"
                                    );
                                }
                                Premise::Builtin { .. } | Premise::Presence { .. } => {
                                    // Self-justifying; `arb_program_with_edb`
                                    // emits no comparisons or presence tests, so
                                    // these are not yet exercised here.
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
    }
}
