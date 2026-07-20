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

use std::collections::{BTreeSet, HashMap};

use crate::ir::{Atom, BodyLiteral, BodyLiteralKind, Fact, Program, Term, Tuple, Value, Var};

/// Computes the perfect model — all facts, base and derived — by brute force:
/// per stratum, apply every rule to the full fact set until nothing new
/// appears.
pub(crate) fn naive_eval(program: &Program) -> BTreeSet<Fact> {
    let mut facts: BTreeSet<Fact> = program.facts.iter().cloned().collect();
    for stratum in &program.strata {
        loop {
            let mut derived: Vec<Fact> = Vec::new();
            for &rule_id in stratum {
                let rule = &program.rules[rule_id.0 as usize];
                for env in matches(&rule.body, &facts, &HashMap::new()) {
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
    facts
}

/// All variable environments satisfying `body` against `facts`: enumerate the
/// positive atoms first (in body order), then keep each environment only if
/// no fact refutes any negated atom under it. Positives-first mirrors §10
/// safety (named variables under negation are positively bound), so an
/// unbound variable in a negated atom can only be wildcard-fresh — open,
/// existential under the negation.
fn matches(
    body: &[BodyLiteral],
    facts: &BTreeSet<Fact>,
    env: &HashMap<Var, Value>,
) -> Vec<HashMap<Var, Value>> {
    let positives: Vec<&Atom> = body
        .iter()
        .filter_map(|literal| match &literal.kind {
            BodyLiteralKind::Atom(atom) => Some(atom),
            BodyLiteralKind::NegAtom(_) => None,
            BodyLiteralKind::Compare { .. } => {
                panic!("naive oracle handles atoms and negated atoms only")
            }
        })
        .collect();
    let negatives: Vec<&Atom> = body
        .iter()
        .filter_map(|literal| match &literal.kind {
            BodyLiteralKind::NegAtom(atom) => Some(atom),
            _ => None,
        })
        .collect();
    let mut envs = match_positives(&positives, facts, env);
    envs.retain(|env| {
        negatives.iter().all(|atom| {
            !facts
                .iter()
                .filter(|f| f.pred == atom.pred)
                .any(|f| refutes(atom, &f.tuple, env))
        })
    });
    envs
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
/// variables must agree; unbound variables are open and match anything.
/// Deliberately non-binding — the counterpart of the engine's
/// `AbsentPattern::matches`, written against `env` instead.
fn refutes(atom: &Atom, tuple: &Tuple, env: &HashMap<Var, Value>) -> bool {
    atom.args
        .iter()
        .zip(&tuple.0)
        .all(|(term, value)| match term {
            Term::Const(c) => c == value,
            Term::Var(v) => env.get(v).is_none_or(|bound| bound == value),
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
            Term::Const(c) if c == value => {}
            Term::Const(_) => return None,
            Term::Var(v) => match env.get(v) {
                Some(bound) if bound == value => {}
                Some(_) => return None,
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
