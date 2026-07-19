//! The naive reference evaluator — a permanent differential-testing oracle.
//!
//! Ratified in spec §17 (2026-07-19): this deliberately dumb evaluator stays
//! under test cfg forever, and `naive(p) == seminaive(p)` over generated
//! programs is the anchor property (`testing.md` B1). It shares nothing with
//! the semi-naive path beyond the IR types — no deltas, no views, no slot
//! arrays — so a bookkeeping bug in one cannot hide in the other.
//!
//! Positive programs only (matching the step-2 evaluator's scope). Strata are
//! ignored on purpose: for positive programs every evaluation order reaches
//! the same least fixpoint, which is exactly what B1 exercises.

use std::collections::{BTreeSet, HashMap};

use crate::ir::{Atom, BodyLiteralKind, Fact, Program, Term, Tuple, Value, Var};

/// Computes the least model — all facts, base and derived — by brute force:
/// apply every rule to the full fact set until nothing new appears.
pub(crate) fn naive_eval(program: &Program) -> BTreeSet<Fact> {
    let mut facts: BTreeSet<Fact> = program.facts.iter().cloned().collect();
    loop {
        let mut derived: Vec<Fact> = Vec::new();
        for rule in &program.rules {
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
            return facts;
        }
    }
}

/// All variable environments satisfying `body` against `facts`.
fn matches(
    body: &[crate::ir::BodyLiteral],
    facts: &BTreeSet<Fact>,
    env: &HashMap<Var, Value>,
) -> Vec<HashMap<Var, Value>> {
    let Some((first, rest)) = body.split_first() else {
        return vec![env.clone()];
    };
    let BodyLiteralKind::Atom(atom) = &first.kind else {
        panic!("naive oracle handles positive atoms only");
    };
    let mut out = Vec::new();
    for fact in facts.iter().filter(|f| f.pred == atom.pred) {
        if let Some(extended) = match_atom(atom, &fact.tuple, env) {
            out.extend(matches(rest, facts, &extended));
        }
    }
    out
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
