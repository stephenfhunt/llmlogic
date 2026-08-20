//! Body scheduling: the static evaluation order of a rule or query body
//! (`spec.md` §8/§10/§15).
//!
//! A Datalog body is a *conjunction*, so its literals should mean the same
//! thing in any order. Atoms already do — the evaluator has always run all
//! positive atoms first, then negations as anti-join filters. The builtins
//! (§8 comparisons and assignments, §4 presence tests, §9 aggregates) did not:
//! they ran in **source order**, so a clause could be rejected, or worse
//! silently mean something else, purely for how it was written:
//!
//! ```datalog
//! ok(X, N)  :- q(X), Y = X + 1, N = count { C | r(Y, C) }.   % groups by Y
//! bad(X, N) :- q(X), N = count { C | r(Y, C) }, Y = X + 1.   % used to count everything
//! rev(X, M) :- a(X), M = N + 1, N = X + 1.                   % used to be rejected
//! ```
//!
//! [`schedule_body`] replaces that with a **dependency order**. Each builtin
//! declares what it reads and what it binds ([`Effect`]); the scheduler emits
//! any literal whose reads are satisfied, repeats to fixpoint, and fails only
//! when what is left is genuinely unschedulable — a variable nothing binds, or a
//! circular dependency. Both `ok` and `bad` above now mean "grouped by `Y`", and
//! `rev` is accepted.
//!
//! Two properties make this a widening rather than a semantics change:
//!
//! - **Source order is the tie-break.** Among the literals that are ready, the
//!   earliest in source order is emitted. So a body whose source order already
//!   works gets exactly its source order, and `=` keeps resolving
//!   assignment-vs-filter the way it always has. Nothing that lowered before
//!   changes meaning; some things that were rejected now lower.
//! - **The order is a pure function of the body**, so lowering (which reports
//!   the errors) and both evaluators derive the same schedule from the same
//!   input, the way they already share `fold_aggregate`.
//!
//! Negated atoms are scheduled too (§7/§10, 2026-07-25). A negated atom's
//! **reads** are the argument variables that something *else* in the body binds
//! ([`binder_vars`]) — so a wildcard-fresh slot, bound nowhere, is not a
//! dependency and stays existential under the negation, exactly as §7 says.
//! A negation whose reads are already satisfied by the positive atoms stays in
//! its own early phase, so cheap anti-joins still prune before expensive
//! aggregates; only the ones that are not yet ready fall through into the
//! dependency phase:
//!
//! ```datalog
//! r(X) :- p(X), not q(X + 1).        % hoists to `V = X + 1, not q(V)`
//! r(X) :- p(X), Y = X + 1, not q(Y).
//! r(X) :- p(X), not q(Y), Y = X + 1. % all three agree
//! ```
//!
//! This replaces the older rule that a negated atom's named variables had to be
//! bound *positively*. That rule justified itself by the phase order and the
//! phase order by itself, and it was not the uniform expressiveness limit it
//! claimed to be: the hand-hoisted spelling was rejected while the inline one
//! was silently misread as `not q(_)` (`bugs/001`). The safety requirement it
//! stood for — a negated atom must be ground when tested — is met by an
//! `=`-assignment just as well as by a positive atom. A *named* variable bound
//! nowhere at all is still unsafe, and lowering reports it (`check_body_safety`);
//! the scheduler stays free of `var_names`.

use std::collections::HashSet;

use crate::ast::CmpOp;
use crate::ir::{BodyLiteral, BodyLiteralKind, Expr, Term, Var};

/// Why a body could not be scheduled, and which literal is stuck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleError {
    /// Index into the body of a literal that never became ready. When several
    /// are stuck this is the earliest in source order.
    pub literal: usize,
    /// A variable that literal needs and never gets.
    pub variable: Var,
    pub cause: ScheduleFailure,
}

/// The two ways a body fails to schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleFailure {
    /// Nothing in the body ever binds the variable — no positive atom, no
    /// assignment, no aggregate result. Reordering cannot help.
    Unbound,
    /// The variable *is* bound by another stuck literal, which in turn waits on
    /// this one: a circular dependency with no valid order at all.
    Cycle,
}

/// What one builtin literal needs, and what it provides.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Effect {
    /// Variables that must already be bound for this literal to run.
    reads: Vec<Var>,
    /// Variables this literal binds when it runs.
    binds: Vec<Var>,
}

/// The evaluation order for `body`: positive atoms in source order, then the
/// negated atoms whose reads the positives already bind, then the builtins and
/// any remaining negations in dependency order.
///
/// The returned vector is a permutation of `0..body.len()`. See the module
/// docs for why source order is the tie-break.
pub fn schedule_body(body: &[BodyLiteral]) -> Result<Vec<usize>, ScheduleError> {
    schedule_body_with(body, &HashSet::new()).map(|(order, _)| order)
}

/// [`schedule_body`], plus the variables the body ends up binding, and with
/// `seed` treated as already bound.
///
/// `seed` carries an aggregate goal's **group keys** — bound by the enclosing
/// body before the goal's sub-join runs (§9). The returned binding set is what
/// a rule head may reference (§10): positives, assignment targets, and aggregate
/// results, in whatever order the schedule made available.
pub fn schedule_body_with(
    body: &[BodyLiteral],
    seed: &HashSet<Var>,
) -> Result<(Vec<usize>, HashSet<Var>), ScheduleError> {
    let mut order = Vec::with_capacity(body.len());
    let mut bound: HashSet<Var> = seed.clone();

    // Phase 1 — positive atoms generate bindings, so they come first and bind
    // every variable they mention.
    for (index, literal) in body.iter().enumerate() {
        if let BodyLiteralKind::Atom(atom) = &literal.kind {
            order.push(index);
            for arg in &atom.args {
                if let Term::Var(var) = arg {
                    bound.insert(*var);
                }
            }
        }
    }

    // Phase 2 — negated atoms are anti-join filters (§7/§10); they bind nothing.
    // The ones the positives already satisfy run here, so a cheap anti-join
    // prunes before an expensive aggregate. The rest wait for their binders in
    // the dependency phase.
    let binders = binder_vars(body);
    let mut deferred = Vec::new();
    for (index, literal) in body.iter().enumerate() {
        if let BodyLiteralKind::NegAtom(atom) = &literal.kind {
            if neg_reads(atom, &binders)
                .iter()
                .all(|var| bound.contains(var))
            {
                order.push(index);
            } else {
                deferred.push(index);
            }
        }
    }

    // Phase 3 — the builtins and the deferred negations, by dependency.
    let outside = outside_aggregate_vars(body);
    let mut pending: Vec<usize> = body
        .iter()
        .enumerate()
        .filter(|(index, literal)| is_builtin(literal) || deferred.contains(index))
        .map(|(index, _)| index)
        .collect();

    while !pending.is_empty() {
        // Emit the earliest ready literal, then re-scan from the start: that is
        // what makes a body whose source order already works keep it exactly.
        let ready = pending.iter().position(|&index| {
            effect_of(&body[index], &bound, &outside, &binders)
                .is_some_and(|effect| effect.reads.iter().all(|var| bound.contains(var)))
        });
        let Some(position) = ready else {
            return Err(diagnose(body, &pending, &bound, &outside, &binders));
        };
        let index = pending.remove(position);
        let effect = effect_of(&body[index], &bound, &outside, &binders).expect("just found ready");
        bound.extend(effect.binds);
        order.push(index);
    }

    Ok((order, bound))
}

/// Is this literal always scheduled in the dependency phase? Negated atoms join
/// it only when phase 2 could not satisfy them, so they are not listed here.
fn is_builtin(literal: &BodyLiteral) -> bool {
    matches!(
        literal.kind,
        BodyLiteralKind::Compare { .. }
            | BodyLiteralKind::Presence { .. }
            | BodyLiteralKind::Aggregate { .. }
    )
}

/// The variables a negated atom **waits for**: its argument variables that
/// something else in the body binds.
///
/// A slot bound nowhere is not a dependency — it is wildcard-fresh, open under
/// the negation and existential to it (§7). That is what keeps this function,
/// and so the whole scheduler, free of `var_names`: "is this a wildcard" is
/// answered by the body's structure rather than by whether the source gave the
/// slot a name. A *named* variable bound nowhere reaches the same conclusion
/// here and is caught instead by lowering's separate safety check.
fn neg_reads(atom: &crate::ir::Atom, binders: &HashSet<Var>) -> Vec<Var> {
    atom.args
        .iter()
        .filter_map(|arg| match arg {
            Term::Var(var) if binders.contains(var) => Some(*var),
            _ => None,
        })
        .collect()
}

/// Every variable the body could bind: positive-atom arguments, the bare
/// variable side of an `=` (a potential assignment target), and aggregate
/// results.
///
/// A static over-approximation, computed once and independent of order — it
/// cannot ask whether a given `=` will resolve as an assignment or a filter,
/// since that is what scheduling decides. Over-approximating is safe: a variable
/// counted here that nothing actually binds leaves its *binder* stuck, and
/// [`diagnose`] reports that literal.
fn binder_vars(body: &[BodyLiteral]) -> HashSet<Var> {
    let mut binders = HashSet::new();
    for literal in body {
        match &literal.kind {
            BodyLiteralKind::Atom(atom) => {
                for arg in &atom.args {
                    if let Term::Var(var) = arg {
                        binders.insert(*var);
                    }
                }
            }
            BodyLiteralKind::Compare {
                op: CmpOp::Eq,
                lhs,
                rhs,
            } => {
                for side in [lhs, rhs] {
                    if let Expr::Term(Term::Var(var)) = side {
                        binders.insert(*var);
                    }
                }
            }
            BodyLiteralKind::Aggregate { result, .. } => {
                binders.insert(*result);
            }
            BodyLiteralKind::Compare { .. }
            | BodyLiteralKind::Presence { .. }
            | BodyLiteralKind::NegAtom(_) => {}
        }
    }
    binders
}

/// What `literal` reads and binds, given what is bound so far.
///
/// `bound` matters for one case only: whether an `=` is an **assignment** (one
/// bare, not-yet-bound variable side, the other side evaluable) or an equality
/// **filter** (§8). That classification is genuinely order-dependent, which is
/// why it is re-asked each round rather than decided up front.
fn effect_of(
    literal: &BodyLiteral,
    bound: &HashSet<Var>,
    outside: &HashSet<Var>,
    binders: &HashSet<Var>,
) -> Option<Effect> {
    match &literal.kind {
        BodyLiteralKind::Compare { op, lhs, rhs } => {
            if *op == CmpOp::Eq {
                // Prefer the left-hand side, as the evaluator's assignment rule
                // does, so the two agree on `X = Y` with both sides unbound.
                for (target, other) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Term(Term::Var(var)) = target
                        && !bound.contains(var)
                    {
                        return Some(Effect {
                            reads: expr_vars(other),
                            binds: vec![*var],
                        });
                    }
                }
            }
            let mut reads = expr_vars(lhs);
            reads.extend(expr_vars(rhs));
            Some(Effect {
                reads,
                binds: Vec::new(),
            })
        }
        BodyLiteralKind::Presence { expr, .. } => Some(Effect {
            reads: expr_vars(expr),
            binds: Vec::new(),
        }),
        BodyLiteralKind::Aggregate {
            result,
            params,
            expr,
            goal,
            ..
        } => {
            // An aggregate reads its **group keys** — the variables it shares
            // with the enclosing body (§9) — plus its parameters. Variables
            // living only inside the goal are existential to it and are bound by
            // the goal's own atoms, so they are not dependencies.
            let mut reads: Vec<Var> = all_body_vars(goal)
                .into_iter()
                .chain(expr_vars(expr))
                .filter(|var| outside.contains(var))
                .collect();
            reads.sort_unstable_by_key(|var| var.0);
            reads.dedup();
            for param in params {
                reads.extend(expr_vars(param));
            }
            Some(Effect {
                reads,
                binds: vec![*result],
            })
        }
        // An anti-join filter: it waits for whatever else binds its arguments
        // and binds nothing itself.
        BodyLiteralKind::NegAtom(atom) => Some(Effect {
            reads: neg_reads(atom, binders),
            binds: Vec::new(),
        }),
        BodyLiteralKind::Atom(_) => None,
    }
}

/// Explains a stuck body: the earliest stuck literal, a variable it waits on,
/// and whether anything left could ever bind it.
fn diagnose(
    body: &[BodyLiteral],
    pending: &[usize],
    bound: &HashSet<Var>,
    outside: &HashSet<Var>,
    binders: &HashSet<Var>,
) -> ScheduleError {
    // Everything the remaining literals could still bind. If a missing variable
    // is in here, the literals are waiting on each other — a cycle; otherwise
    // nothing in the body ever binds it.
    let mut bindable: HashSet<Var> = HashSet::new();
    for &index in pending {
        match &body[index].kind {
            BodyLiteralKind::Compare {
                op: CmpOp::Eq,
                lhs,
                rhs,
            } => {
                for side in [lhs, rhs] {
                    if let Expr::Term(Term::Var(var)) = side
                        && !bound.contains(var)
                    {
                        bindable.insert(*var);
                    }
                }
            }
            BodyLiteralKind::Aggregate { result, .. } => {
                bindable.insert(*result);
            }
            _ => {}
        }
    }

    let index = pending[0];
    let missing = effect_of(&body[index], bound, outside, binders)
        .map(|effect| effect.reads)
        .unwrap_or_default()
        .into_iter()
        .find(|var| !bound.contains(var))
        // A literal can only be stuck on a variable it reads, so this is
        // reachable only from malformed IR.
        .unwrap_or(Var(0));
    ScheduleError {
        literal: index,
        variable: missing,
        cause: if bindable.contains(&missing) {
            ScheduleFailure::Cycle
        } else {
            ScheduleFailure::Unbound
        },
    }
}

/// The variables this body binds to a value **computed by arithmetic**, taken
/// transitively (§10, *Termination*).
///
/// The seed is an `=`-assignment whose source expression contains an arithmetic
/// operator and mentions at least one variable: that is the only construct in the
/// language that can synthesise a value absent from the program and its inputs.
/// From there the set is closed under two propagations, and getting either wrong
/// is what makes an unbounded program look bounded:
///
/// - **`=`-chains.** `K = M + 1, N = K` computes `N` just as `N = M + 1` does. The
///   assignment that binds `N` is a bare variable, so a check that looked only at
///   the binding literal would miss it.
/// - **Casts.** `Y = X as int` *propagates* taint but never creates it — a cast
///   maps a finite value set to a finite value set with no accumulation (§8), so a
///   cast over an uncomputed value is not value creation, while a cast over a
///   computed one still carries the growth.
///
/// Two things deliberately do **not** compute a value. A ground expression
/// (`N = 1 + 1`) yields one value however often it runs. An **aggregate** result
/// is a function of a relation stratified strictly below (§9), so its range is
/// already finite.
///
/// Replays [`schedule_body_with`]'s order rather than walking source order,
/// because whether an `=` *assigns* or *filters* is order-dependent and
/// [`effect_of`] is where that classification lives. An unschedulable body
/// computes nothing — its own error is reported elsewhere (the `safe_bound_vars`
/// precedent in `lower.rs`).
pub(crate) fn computed_vars(body: &[BodyLiteral]) -> HashSet<Var> {
    let Ok((order, _)) = schedule_body_with(body, &HashSet::new()) else {
        return HashSet::new();
    };
    let outside = outside_aggregate_vars(body);
    let binders = binder_vars(body);
    let mut bound: HashSet<Var> = HashSet::new();
    let mut computed: HashSet<Var> = HashSet::new();

    for &index in &order {
        let literal = &body[index];
        let Some(effect) = effect_of(literal, &bound, &outside, &binders) else {
            // A positive atom, which `effect_of` does not model: it binds its own
            // variables, and it binds them to stored values, so it computes none.
            if let BodyLiteralKind::Atom(atom) = &literal.kind {
                for arg in &atom.args {
                    if let Term::Var(var) = arg {
                        bound.insert(*var);
                    }
                }
            }
            continue;
        };
        if let BodyLiteralKind::Compare {
            op: CmpOp::Eq,
            lhs,
            rhs,
        } = &literal.kind
            && let [target] = effect.binds[..]
        {
            // `effect_of` binds whichever side is the bare unbound variable; the
            // other side is the source expression this assignment evaluates.
            let source = if matches!(lhs, Expr::Term(Term::Var(var)) if *var == target) {
                rhs
            } else {
                lhs
            };
            if creates_value(source) || expr_vars(source).iter().any(|var| computed.contains(var)) {
                computed.insert(target);
            }
        }
        bound.extend(effect.binds);
    }
    computed
}

/// Does evaluating `expr` synthesise a value that need not appear in the program
/// or its inputs? Arithmetic over at least one variable does; a cast alone does
/// not, and neither does a ground expression, which yields a single value.
fn creates_value(expr: &Expr) -> bool {
    has_arithmetic(expr) && !expr_vars(expr).is_empty()
}

/// Is there an arithmetic operator anywhere in `expr`, including under a cast?
fn has_arithmetic(expr: &Expr) -> bool {
    match expr {
        Expr::Binary { .. } => true,
        Expr::Cast { expr, .. } => has_arithmetic(expr),
        // A `std` module relation is a finite-domain map, like a cast: it
        // reads a bound value and returns one no larger, so it propagates but
        // never accumulates and §10 exempts it. Its *arguments* still count.
        Expr::Builtin { args, .. } => args.iter().any(has_arithmetic),
        Expr::Term(_) => false,
    }
}

/// Every variable occurring in `body` *outside* any aggregate's own scope — the
/// positions that share the enclosing clause's variables (§9).
///
/// An aggregate's `goal` and collected expression are its own scope, so they are
/// excluded: two aggregates in one body may each use a `C` locally without those
/// `C`s being shared (lowering gives them one slot, but each sub-join binds and
/// backtracks it independently). An aggregate's `params` and the `result` it
/// binds *do* live in the enclosing scope, so they count.
pub fn outside_aggregate_vars(body: &[BodyLiteral]) -> HashSet<Var> {
    let mut vars = HashSet::new();
    for literal in body {
        match &literal.kind {
            BodyLiteralKind::Atom(atom) | BodyLiteralKind::NegAtom(atom) => {
                for arg in &atom.args {
                    if let Term::Var(var) = arg {
                        vars.insert(*var);
                    }
                }
            }
            BodyLiteralKind::Compare { lhs, rhs, .. } => {
                vars.extend(expr_vars(lhs));
                vars.extend(expr_vars(rhs));
            }
            BodyLiteralKind::Presence { expr, .. } => vars.extend(expr_vars(expr)),
            BodyLiteralKind::Aggregate { result, params, .. } => {
                vars.insert(*result);
                for param in params {
                    vars.extend(expr_vars(param));
                }
            }
        }
    }
    vars
}

/// Every variable occurring anywhere in `body`, descending into nested
/// aggregate goals and collected expressions.
pub fn all_body_vars(body: &[BodyLiteral]) -> HashSet<Var> {
    let mut vars = outside_aggregate_vars(body);
    for literal in body {
        if let BodyLiteralKind::Aggregate { expr, goal, .. } = &literal.kind {
            vars.extend(expr_vars(expr));
            vars.extend(all_body_vars(goal));
        }
    }
    vars
}

/// The variables of an expression, in occurrence order.
pub fn expr_vars(expr: &Expr) -> Vec<Var> {
    match expr {
        Expr::Term(Term::Var(var)) => vec![*var],
        Expr::Term(Term::Const(_)) => Vec::new(),
        Expr::Binary { lhs, rhs, .. } => {
            let mut vars = expr_vars(lhs);
            vars.extend(expr_vars(rhs));
            vars
        }
        // The scheduler must see *through* a cast to the variables it consumes,
        // or `V = X as int` would look input-free and be placed before whatever
        // binds `X`.
        Expr::Cast { expr, .. } => expr_vars(expr),
        // Same reason, and it is what makes a `std` relation's input position
        // an *input*: `Y = year(D)` must be placed after whatever binds `D`.
        Expr::Builtin { args, .. } => args.iter().flat_map(expr_vars).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Span;
    use crate::ir::{Atom, BodyLiteral, PredId, Value};
    use proptest::prelude::*;

    fn atom(pred: u32, args: Vec<Term>) -> BodyLiteral {
        BodyLiteral {
            kind: BodyLiteralKind::Atom(Atom {
                pred: PredId(pred),
                args,
            }),
            span: Span::DUMMY,
        }
    }

    fn assign(target: u32, from: Expr) -> BodyLiteral {
        BodyLiteral {
            kind: BodyLiteralKind::Compare {
                op: CmpOp::Eq,
                lhs: Expr::Term(Term::Var(Var(target))),
                rhs: from,
            },
            span: Span::DUMMY,
        }
    }

    fn var(slot: u32) -> Expr {
        Expr::Term(Term::Var(Var(slot)))
    }

    fn plus_one(slot: u32) -> Expr {
        Expr::Binary {
            op: crate::ast::ArithOp::Add,
            lhs: Box::new(var(slot)),
            rhs: Box::new(Expr::Term(Term::Const(Value::Int(1)))),
        }
    }

    fn neg_atom(pred: u32, args: Vec<Term>) -> BodyLiteral {
        BodyLiteral {
            kind: BodyLiteralKind::NegAtom(Atom {
                pred: PredId(pred),
                args,
            }),
            span: Span::DUMMY,
        }
    }

    /// A negation whose argument an `=` binds waits for it, whichever side of
    /// the negation that `=` is written on (`bugs/001`, §7/§10 2026-07-25).
    #[test]
    fn a_negation_waits_for_the_assignment_that_binds_it() {
        // p(X), V = X + 1, not q(V)  — already in a workable order.
        let binder_first = vec![
            atom(0, vec![Term::Var(Var(0))]),
            assign(1, plus_one(0)),
            neg_atom(1, vec![Term::Var(Var(1))]),
        ];
        assert_eq!(schedule_body(&binder_first).unwrap(), vec![0, 1, 2]);

        // p(X), not q(V), V = X + 1  — the negation is written before its
        // binder, so only scheduling makes it run at all. It must still land
        // *after* the assignment.
        let binder_last = vec![
            atom(0, vec![Term::Var(Var(0))]),
            neg_atom(1, vec![Term::Var(Var(1))]),
            assign(1, plus_one(0)),
        ];
        assert_eq!(schedule_body(&binder_last).unwrap(), vec![0, 2, 1]);
    }

    /// A negation nothing else binds keeps its early phase: its slot is
    /// wildcard-fresh, open and existential under the negation (§7), so it has
    /// no dependency to wait for and still prunes before the builtins.
    #[test]
    fn a_wildcard_negation_stays_in_the_early_phase() {
        // p(X), not q(_), V = X + 1  — `_` is Var(2), bound nowhere.
        let body = vec![
            atom(0, vec![Term::Var(Var(0))]),
            neg_atom(1, vec![Term::Var(Var(2))]),
            assign(1, plus_one(0)),
        ];
        assert_eq!(schedule_body(&body).unwrap(), vec![0, 1, 2]);
        // And it binds nothing, so the assignment target is the only new slot.
        let (_, bound) = schedule_body_with(&body, &HashSet::new()).unwrap();
        assert!(!bound.contains(&Var(2)));
    }

    /// A body whose source order already works keeps it exactly — the tie-break
    /// that makes scheduling a widening rather than a change.
    #[test]
    fn a_satisfiable_source_order_is_preserved() {
        // p(X), N = X + 1, M = N + 1
        let body = vec![
            atom(0, vec![Term::Var(Var(0))]),
            assign(1, plus_one(0)),
            assign(2, plus_one(1)),
        ];
        assert_eq!(schedule_body(&body).unwrap(), vec![0, 1, 2]);
    }

    /// When several literals are ready at once, the earliest in source order
    /// wins. This is the load-bearing half of "scheduling is a widening": a body
    /// that already worked keeps its exact order, so nothing that lowered before
    /// can change behaviour — including the one place order is observable even
    /// for `=`, which is *pruning* (a filter that fails first stops a later
    /// literal from raising a runtime error).
    #[test]
    fn source_order_breaks_ties_among_ready_literals() {
        // p(X), X > 0, Y = X + 1  — the filter and the assignment are both ready
        // immediately, so only the tie-break decides.
        let body = vec![
            atom(0, vec![Term::Var(Var(0))]),
            BodyLiteral {
                kind: BodyLiteralKind::Compare {
                    op: CmpOp::Gt,
                    lhs: var(0),
                    rhs: Expr::Term(Term::Const(Value::Int(0))),
                },
                span: Span::DUMMY,
            },
            assign(1, plus_one(0)),
        ];
        assert_eq!(schedule_body(&body).unwrap(), vec![0, 1, 2]);

        // Swapped in the source, they swap in the schedule.
        let swapped = vec![body[0].clone(), body[2].clone(), body[1].clone()];
        assert_eq!(schedule_body(&swapped).unwrap(), vec![0, 1, 2]);
    }

    /// The same chain written backwards schedules into dependency order instead
    /// of being rejected.
    #[test]
    fn a_reversed_chain_is_reordered() {
        // p(X), M = N + 1, N = X + 1   ->  atom, then the N binder, then M's.
        let body = vec![
            atom(0, vec![Term::Var(Var(0))]),
            assign(2, plus_one(1)),
            assign(1, plus_one(0)),
        ];
        assert_eq!(schedule_body(&body).unwrap(), vec![0, 2, 1]);
    }

    /// Mutually dependent assignments have no valid order at all.
    #[test]
    fn a_cycle_is_reported_as_a_cycle() {
        // p(X), M = N + 1, N = M + 1
        let body = vec![
            atom(0, vec![Term::Var(Var(0))]),
            assign(2, plus_one(1)),
            assign(1, plus_one(2)),
        ];
        let failure = schedule_body(&body).expect_err("no order exists");
        assert_eq!(failure.cause, ScheduleFailure::Cycle);
    }

    /// A variable no literal binds is unbound, not circular — different advice.
    #[test]
    fn a_variable_nothing_binds_is_unbound() {
        // p(X), X > Z
        let body = vec![
            atom(0, vec![Term::Var(Var(0))]),
            BodyLiteral {
                kind: BodyLiteralKind::Compare {
                    op: CmpOp::Gt,
                    lhs: var(0),
                    rhs: var(1),
                },
                span: Span::DUMMY,
            },
        ];
        let failure = schedule_body(&body).expect_err("Z is never bound");
        assert_eq!(failure.cause, ScheduleFailure::Unbound);
        assert_eq!(failure.variable, Var(1));
    }

    /// The scheduler's contract, stated directly: in the emitted order, every
    /// literal's reads are already bound when it runs.
    ///
    /// This is the property the B1 differential **cannot** provide. Both
    /// evaluators consume this one function, so a scheduling bug would move them
    /// together and they would agree on a wrong answer.
    fn assert_reads_precede_binds(body: &[BodyLiteral], order: &[usize]) {
        let outside = outside_aggregate_vars(body);
        let binders = binder_vars(body);
        let mut bound: HashSet<Var> = HashSet::new();
        for &index in order {
            match &body[index].kind {
                BodyLiteralKind::Atom(atom) => {
                    for arg in &atom.args {
                        if let Term::Var(v) = arg {
                            bound.insert(*v);
                        }
                    }
                }
                // Negations are checked too: a deferred one must not run before
                // the `=` that closes its argument (§7/§10, 2026-07-25).
                _ => {
                    let effect = effect_of(&body[index], &bound, &outside, &binders)
                        .expect("a scheduled literal");
                    for read in &effect.reads {
                        assert!(
                            bound.contains(read),
                            "literal {index} reads {read:?} before anything bound it \
                             (order {order:?})"
                        );
                    }
                    bound.extend(effect.binds);
                }
            }
        }
    }

    proptest! {
        /// Every schedule the scheduler emits satisfies its own contract, over
        /// generated §8 comparison/assignment programs.
        #[test]
        fn schedules_bind_before_they_read(
            program in crate::testgen::arb_comparison_program()
        ) {
            for rule in &program.rules {
                let order = schedule_body(&rule.body).expect("a lowered rule schedules");
                prop_assert_eq!(order.len(), rule.body.len());
                assert_reads_precede_binds(&rule.body, &order);
            }
        }

        /// Permuting a body never changes the schedule's *meaning*: whatever
        /// order the literals are written in, the scheduler either rejects the
        /// body or emits an order satisfying the same contract. (That the
        /// resulting model is also unchanged is checked end-to-end by
        /// `b5_aggregate_body_order_does_not_change_the_model`.)
        #[test]
        fn permuting_a_body_still_schedules_soundly(
            program in crate::testgen::arb_comparison_program(),
            rotation in 0usize..4,
        ) {
            for rule in &program.rules {
                if rule.body.is_empty() {
                    continue;
                }
                let mut body = rule.body.clone();
                let shift = rotation % body.len();
                body.rotate_left(shift);
                if let Ok(order) = schedule_body(&body) {
                    assert_reads_precede_binds(&body, &order);
                }
            }
        }
    }
}
