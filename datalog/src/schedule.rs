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
//! Negated atoms stay in their own phase, before every builtin: §10 requires a
//! negated atom's named variables to be bound *positively*, which this scheduler
//! does not relax. Note what that restriction is and is not — it is **uniform**,
//! so `not q(Y), Y = X+1` and `Y = X+1, not q(Y)` are rejected identically.
//! There is no silent mis-reading to fix, as there was for aggregate group keys;
//! it is an expressiveness limit with a working alternative (hoist the
//! computation into a helper predicate). Folding negation into the dependency
//! order is a reasonable follow-on and is sketched in §17, but it *widens*
//! negation safety, which is a §7/§10 decision rather than a consequence of
//! scheduling.

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

/// The evaluation order for `body`: positive atoms in source order, then
/// negated atoms, then the builtins in dependency order.
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

    // Phase 2 — negated atoms are anti-join filters over positively-bound
    // variables (§7/§10); they bind nothing.
    for (index, literal) in body.iter().enumerate() {
        if matches!(literal.kind, BodyLiteralKind::NegAtom(_)) {
            order.push(index);
        }
    }

    // Phase 3 — the builtins, by dependency.
    let outside = outside_aggregate_vars(body);
    let mut pending: Vec<usize> = body
        .iter()
        .enumerate()
        .filter(|(_, literal)| is_builtin(literal))
        .map(|(index, _)| index)
        .collect();

    while !pending.is_empty() {
        // Emit the earliest ready literal, then re-scan from the start: that is
        // what makes a body whose source order already works keep it exactly.
        let ready = pending.iter().position(|&index| {
            effect_of(&body[index], &bound, &outside)
                .is_some_and(|effect| effect.reads.iter().all(|var| bound.contains(var)))
        });
        let Some(position) = ready else {
            return Err(diagnose(body, &pending, &bound, &outside));
        };
        let index = pending.remove(position);
        let effect = effect_of(&body[index], &bound, &outside).expect("just found ready");
        bound.extend(effect.binds);
        order.push(index);
    }

    Ok((order, bound))
}

/// Is this literal scheduled in the dependency phase?
fn is_builtin(literal: &BodyLiteral) -> bool {
    matches!(
        literal.kind,
        BodyLiteralKind::Compare { .. }
            | BodyLiteralKind::Presence { .. }
            | BodyLiteralKind::Aggregate { .. }
    )
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
        BodyLiteralKind::Atom(_) | BodyLiteralKind::NegAtom(_) => None,
    }
}

/// Explains a stuck body: the earliest stuck literal, a variable it waits on,
/// and whether anything left could ever bind it.
fn diagnose(
    body: &[BodyLiteral],
    pending: &[usize],
    bound: &HashSet<Var>,
    outside: &HashSet<Var>,
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
    let missing = effect_of(&body[index], bound, outside)
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
                BodyLiteralKind::NegAtom(_) => {}
                _ => {
                    let effect = effect_of(&body[index], &bound, &outside).expect("a builtin");
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
