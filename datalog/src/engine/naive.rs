//! The naive reference evaluator — a permanent differential-testing oracle.
//!
//! Ratified in spec §17 (2026-07-19): this deliberately dumb evaluator stays
//! under test cfg forever, and `naive(p) == seminaive(p)` over generated
//! programs is the anchor property (`testing.md` B1). It shares nothing with
//! the semi-naive path beyond the IR types — no deltas, no views, no slot
//! arrays — so a bookkeeping bug in one cannot hide in the other.
//!
//! Iterates strata in order — the perfect-model construction (§7): each
//! stratum runs its naive fixpoint over the growing fact set, and negation is
//! checked against that set, which is sound because valid strata freeze a
//! negated predicate's extent before its readers run — the same invariant the
//! semi-naive engine relies on, expressed independently of it. For positive
//! programs every stratum order reaches the same least fixpoint, so B1's
//! anchor meaning is unchanged; with negation, B1 becomes the perfect-model
//! differential (testing.md C3).
//!
//! The **absent value (§4/§8) is in scope** (2026-07-25): annihilation in
//! arithmetic, false in every comparison, matching nothing in joins and
//! anti-joins, and the presence filter. It is written out here from the spec's
//! truth tables rather than delegating to `Value::unifies_with` /
//! the engine's `apply_compare` / `apply_arith` — an oracle that shares the code under test
//! cannot contradict it. Until then this module used plain `==` and had no
//! absent arms at all, so B1 was silently blind to the newest semantics in the
//! language; the generators emitted no absent, which is the only reason it
//! passed.

use std::collections::{BTreeSet, HashMap};

use crate::Result;
use crate::ast::{ArithOp, CmpOp};
use crate::error::Error;
use crate::ir::{
    Atom, BodyLiteral, BodyLiteralKind, Expr, F64, Fact, Program, Term, Tuple, Value, Var,
};

/// Computes the perfect model — all facts, base and derived — by brute force:
/// per stratum, apply every rule to the full fact set until nothing new
/// appears. A runtime error in a comparison/arithmetic builtin (§8) aborts, so
/// the oracle can be differenced against the engine on the error path too (B1).
pub(crate) fn naive_eval(program: &Program) -> Result<BTreeSet<Fact>> {
    let mut facts: BTreeSet<Fact> = program.facts.iter().cloned().collect();
    for stratum in &program.strata {
        loop {
            let mut derived: Vec<Fact> = Vec::new();
            for &rule_id in stratum {
                let rule = &program.rules[rule_id.0 as usize];
                for env in matches(&rule.body, &facts, &HashMap::new())? {
                    derived.push(Fact {
                        pred: rule.head.pred,
                        tuple: Tuple(rule.head.args.iter().map(|t| ground(t, &env)).collect()),
                    });
                }
            }
            let before = facts.len();
            facts.extend(derived);
            if facts.len() == before {
                break;
            }
        }
    }
    Ok(facts)
}

/// All variable environments satisfying `body` against `facts`: enumerate the
/// positive atoms first (in body order), then fold everything else —
/// comparisons, assignments, presence tests, aggregates *and* negated atoms — in
/// the scheduled order (filters prune, `=`-assignments extend the env).
///
/// Negations are **not** applied in bulk before the builtins. A negated atom
/// waits for whatever binds its arguments, which may be an `=`-assignment
/// (`not q(X + 1)` hoists to `V = X + 1, not q(V)`), so the anti-join has to run
/// at the point the schedule puts it. An argument still unbound when it runs is
/// wildcard-fresh — open, existential under the negation (§7).
fn matches(
    body: &[BodyLiteral],
    facts: &BTreeSet<Fact>,
    env: &HashMap<Var, Value>,
) -> Result<Vec<HashMap<Var, Value>>> {
    let positives: Vec<&Atom> = body
        .iter()
        .filter_map(|literal| match &literal.kind {
            BodyLiteralKind::Atom(atom) => Some(atom),
            _ => None,
        })
        .collect();
    let envs = match_positives(&positives, facts, env);
    let mut out = Vec::new();
    for env in envs {
        if let Some(env) = apply_builtins(body, facts, env)? {
            out.push(env);
        }
    }
    Ok(out)
}

/// Folds every comparison (§8), presence test (§4), aggregate (§9) and negated
/// atom (§7) over `env` in **dependency order**: a filter that fails discards
/// the environment (`Ok(None)`); an `=`-assignment binds its target; an
/// aggregate folds over its goal's witnesses and binds its result; a runtime
/// error (overflow, division by zero, NaN, type mismatch) aborts.
///
/// The order comes from [`crate::schedule::schedule_body`], the same pure
/// function lowering validates with and the engine evaluates by — shared like
/// `fold_aggregate`, because an order the two derived separately could differ
/// for reasons B1 would report as a semantic disagreement. Non-builtin indices
/// appear in the schedule too (atoms first, then negations) and are skipped
/// here: `matches` has already handled them.
fn apply_builtins(
    body: &[BodyLiteral],
    facts: &BTreeSet<Fact>,
    mut env: HashMap<Var, Value>,
) -> Result<Option<HashMap<Var, Value>>> {
    let order = crate::schedule::schedule_body(body).map_err(|failure| {
        Error::semantic(format!(
            "malformed IR: body literal {} can never run",
            failure.literal
        ))
    })?;
    for &index in &order {
        match &body[index].kind {
            BodyLiteralKind::Compare { op, lhs, rhs } => {
                // Assignment: `=` where exactly one side is a bare, currently-
                // unbound variable and the other side evaluates.
                if *op == CmpOp::Eq {
                    match (unbound_var(lhs, &env), unbound_var(rhs, &env)) {
                        (Some(v), None) => {
                            env.insert(v, eval_expr(rhs, &env)?);
                            continue;
                        }
                        (None, Some(v)) => {
                            env.insert(v, eval_expr(lhs, &env)?);
                            continue;
                        }
                        _ => {}
                    }
                }
                let l = eval_expr(lhs, &env)?;
                let r = eval_expr(rhs, &env)?;
                if !compare(*op, &l, &r)? {
                    return Ok(None);
                }
            }
            BodyLiteralKind::Aggregate {
                result,
                op,
                expr,
                goal,
                ..
            } => {
                // Evaluate the goal with the group keys (this env) fixed, collect
                // the collected expression over every witness, and fold — the
                // same fold the engine uses (`fold_aggregate`), so the two agree.
                let mut values = Vec::new();
                for witness in matches(goal, facts, &env)? {
                    values.push(eval_expr(expr, &witness)?);
                }
                let outcome = crate::engine::fold_aggregate(*op, &values)?;
                env.insert(*result, outcome.value);
            }
            BodyLiteralKind::Presence { expr, negated } => {
                // §8: holds iff the operand is absent, flipped by `negated`.
                // A filter — it binds nothing.
                let value = eval_expr(expr, &env)?;
                if matches!(value, Value::Absent) == *negated {
                    return Ok(None);
                }
            }
            // An anti-join filter, run at its scheduled position so that an
            // argument bound by an earlier `=` is closed rather than left open
            // (§7/§10, 2026-07-25). It binds nothing.
            BodyLiteralKind::NegAtom(atom) => {
                if facts
                    .iter()
                    .filter(|fact| fact.pred == atom.pred)
                    .any(|fact| refutes(atom, &fact.tuple, &env))
                {
                    return Ok(None);
                }
            }
            // Positive atoms are handled in `matches`.
            BodyLiteralKind::Atom(_) => {}
        }
    }
    Ok(Some(env))
}

/// A bare, currently-unbound variable expression, if that is what `expr` is.
fn unbound_var(expr: &Expr, env: &HashMap<Var, Value>) -> Option<Var> {
    match expr {
        Expr::Term(Term::Var(v)) if !env.contains_key(v) => Some(*v),
        _ => None,
    }
}

/// The §4 *semantic* sameness of two ground values: `absent` matches nothing —
/// not a value, and not another `absent` — while every other value matches
/// structurally.
///
/// Deliberately written out here rather than calling the engine's
/// `Value::unifies_with`: the oracle's whole value is that it can *disagree*, so it
/// re-expresses the rule (match on both operands) instead of sharing the
/// engine's expression of it. Same rule, independent code.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Absent, _) | (_, Value::Absent) => false,
        _ => a == b,
    }
}

/// Applies a comparison to two evaluated operands. Strict: cross-type operands
/// are a structured error (§4), never a silent `false`.
fn compare(op: CmpOp, lhs: &Value, rhs: &Value) -> Result<bool> {
    // §8: a comparison with an absent operand is *false* for every operator,
    // ahead of the same-type check below — so it is never a type error, and
    // `X = v` and `X != v` are both false when `X` is absent.
    if matches!(lhs, Value::Absent) || matches!(rhs, Value::Absent) {
        return Ok(false);
    }
    if std::mem::discriminant(lhs) != std::mem::discriminant(rhs) {
        return Err(Error::semantic(
            "type error: comparison requires operands of the same type".to_string(),
        ));
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

/// Evaluates an arithmetic expression under `env`.
fn eval_expr(expr: &Expr, env: &HashMap<Var, Value>) -> Result<Value> {
    match expr {
        Expr::Term(Term::Const(value)) => Ok(value.clone()),
        Expr::Term(Term::Var(var)) => env.get(var).cloned().ok_or_else(|| {
            Error::semantic("malformed IR: arithmetic operand variable is unbound".to_string())
        }),
        Expr::Binary { op, lhs, rhs } => {
            let a = eval_expr(lhs, env)?;
            let b = eval_expr(rhs, env)?;
            arith(*op, a, b)
        }
    }
}

/// Applies an arithmetic operator: strict `int op int` / `float op float`,
/// truncating integer division, checked overflow and division by zero, NaN
/// rejected.
fn arith(op: ArithOp, lhs: Value, rhs: Value) -> Result<Value> {
    // §8: absent annihilates, and does so *ahead* of the type, division-by-zero
    // and overflow checks — `5 / absent` and `absent / 0` are both absent.
    if matches!(lhs, Value::Absent) || matches!(rhs, Value::Absent) {
        return Ok(Value::Absent);
    }
    match (lhs, rhs) {
        (Value::Int(a), Value::Int(b)) => {
            let checked = match op {
                ArithOp::Add => a.checked_add(b),
                ArithOp::Sub => a.checked_sub(b),
                ArithOp::Mul => a.checked_mul(b),
                ArithOp::Div => {
                    if b == 0 {
                        return Err(Error::semantic(
                            "arithmetic error: division by zero".to_string(),
                        ));
                    }
                    a.checked_div(b)
                }
            };
            checked
                .map(Value::Int)
                .ok_or_else(|| Error::semantic("arithmetic error: integer overflow".to_string()))
        }
        (Value::Float(a), Value::Float(b)) => {
            let result = match op {
                ArithOp::Add => a.get() + b.get(),
                ArithOp::Sub => a.get() - b.get(),
                ArithOp::Mul => a.get() * b.get(),
                ArithOp::Div => a.get() / b.get(),
            };
            F64::new(result).map(Value::Float)
        }
        _ => Err(Error::semantic(
            "type error: arithmetic requires two ints or two floats".to_string(),
        )),
    }
}

/// All environments extending `env` that satisfy the positive atoms in order.
fn match_positives(
    atoms: &[&Atom],
    facts: &BTreeSet<Fact>,
    env: &HashMap<Var, Value>,
) -> Vec<HashMap<Var, Value>> {
    let Some((first, rest)) = atoms.split_first() else {
        return vec![env.clone()];
    };
    let mut out = Vec::new();
    for fact in facts.iter().filter(|f| f.pred == first.pred) {
        if let Some(extended) = match_atom(first, &fact.tuple, env) {
            out.extend(match_positives(rest, facts, &extended));
        }
    }
    out
}

/// Does `tuple` refute the negated `atom` under `env`? Constants and bound
/// variables must agree *semantically* ([`same_value`], so a slot holding
/// `absent` closes nothing and refutes nothing); unbound variables are open and
/// match anything. Deliberately non-binding — the counterpart of the engine's
/// `AbsentPattern::matches`, written against `env` instead.
fn refutes(atom: &Atom, tuple: &Tuple, env: &HashMap<Var, Value>) -> bool {
    atom.args
        .iter()
        .zip(&tuple.0)
        .all(|(term, value)| match term {
            Term::Const(c) => same_value(c, value),
            Term::Var(v) => env.get(v).is_none_or(|bound| same_value(bound, value)),
        })
}

/// Extends `env` so `atom` equals `tuple`, or `None` if it cannot. Also used
/// by the E3 replay property (testing.md) to revalidate recorded derivations
/// independently of the semi-naive join loop.
pub(crate) fn match_atom(
    atom: &Atom,
    tuple: &Tuple,
    env: &HashMap<Var, Value>,
) -> Option<HashMap<Var, Value>> {
    let mut env = env.clone();
    for (term, value) in atom.args.iter().zip(&tuple.0) {
        match term {
            // Matching an *already-known* value uses the semantic rule (§4), so
            // a repeated variable or a constant never unifies with `absent`.
            Term::Const(c) if same_value(c, value) => {}
            Term::Const(_) => return None,
            Term::Var(v) => match env.get(v) {
                Some(bound) if same_value(bound, value) => {}
                Some(_) => return None,
                // A *fresh* slot binds to whatever is stored, `absent` included —
                // that is how a missing cell flows to the head.
                None => {
                    env.insert(*v, value.clone());
                }
            },
        }
    }
    Some(env)
}

/// Substitutes `env` into a term (which must be ground under `env`).
pub(crate) fn ground(term: &Term, env: &HashMap<Var, Value>) -> Value {
    match term {
        Term::Const(value) => value.clone(),
        Term::Var(var) => env[var].clone(),
    }
}
