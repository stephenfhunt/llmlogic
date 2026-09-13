//! Seeking a bound prefix instead of scanning a relation.
//!
//! A relation is a `BTreeSet<Tuple>` and [`Tuple`] is `Vec<Value>` with a
//! derived `Ord`, so the set is **lexicographically ordered by column** and the
//! tuples agreeing with a given prefix of leading column values form one
//! contiguous range. Body-goal enumeration knows those leading values — they are
//! the atom's constants and its already-bound variables — so it can seek that
//! range instead of walking the relation and rejecting tuples one at a time
//! (`notes/profile-2026-08-20.md`).
//!
//! # Why this is exactly equivalent, not an approximation
//!
//! Three facts, all of them already ratified:
//!
//! - `Value`'s `Ord` **agrees with its `Eq`** — derived, over an [`crate::ir::F64`]
//!   whose `total_cmp` is made total by rejecting NaN and normalizing `-0.0`
//!   (§17 2026-07-19). So lexicographic order on a `Tuple` agrees with equality
//!   column by column, which is what makes the range contiguous *and* complete.
//! - A positive atom matches with [`Value::unifies_with`], which is `==` on
//!   non-absent values (§4). Seeking a non-absent prefix therefore applies the
//!   same predicate the scan applied, not a weaker one.
//! - `absent` unifies with **nothing**, so an atom whose bound positions hold an
//!   `absent` matches nothing at all — [`bound_prefix`] reports that as `None`
//!   rather than as a range.
//!
//! # The `absent` asymmetry
//!
//! The anti-join is the one site that matches **structurally** (§4/§7, §17
//! 2026-07-29: `not p(X)` with `X` bound to `absent` asks whether `p(absent)` is
//! in the relation, and it is). So `absent` is an *impossible* key for a join and
//! a perfectly *legal* one for a refutation, and the negated-atom arm builds its
//! prefix from [`crate::provenance::NoMatchPattern`] rather than through
//! [`bound_prefix`].
//!
//! # Over-yield is invisible; under-yield is fatal
//!
//! Both callers re-check every candidate — `try_match` for a join,
//! `NoMatchPattern::matches` for a refutation — so a seek that returns *too many*
//! tuples costs only time, while one that returns too few loses answers. That
//! asymmetry is why the differential oracle (`testing.md` B1, which compares
//! against a scanning evaluator) is not the guard that matters here: it cannot
//! see an over-yield at all, and the mutations recorded for **B12** are
//! under-yielding ones.
//!
//! # The arity invariant this rests on
//!
//! A tuple *shorter* than the prefix is outside the range, where `try_match` —
//! which `zip`s — would have matched it on the columns they share. That case is
//! unreachable: an [`Atom`] is always at the predicate's full arity (`ir.rs`)
//! and lowering rejects a predicate used at two arities, so every tuple in a
//! relation is exactly as wide as every atom over it.

use std::collections::BTreeSet;

use crate::ir::{Atom, Term, Tuple, Value};
use crate::provenance::NoMatchPattern;

/// The tuples of `set` whose leading columns equal `prefix`, in set order.
///
/// Exactly `set.iter().filter(|t| t.0.starts_with(&prefix.0))`, seeked rather
/// than scanned (`testing.md` B12a states that as a property). An empty prefix
/// needs no special case: the empty tuple sorts below every tuple, so the range
/// is the whole set, and `starts_with(&[])` holds everywhere.
pub(crate) fn tuples_with_prefix<'a>(
    set: &'a BTreeSet<Tuple>,
    prefix: &'a Tuple,
) -> impl Iterator<Item = &'a Tuple> + 'a {
    set.range(prefix..)
        .take_while(move |tuple| tuple.0.starts_with(&prefix.0))
}

/// The bound prefix of `atom` under `bindings` — its leading arguments whose
/// value is already known — or `None` when the atom can match nothing.
///
/// Every tuple `try_match` accepts starts with the returned prefix
/// (`testing.md` B12b), which is what licenses seeking it. The prefix stops at
/// the first unbound variable, because a bound column *after* an unbound one is
/// not part of any contiguous range — `try_match` still filters on it. A
/// repeated variable therefore contributes at most its first occurrence, which
/// is right: the first occurrence binds the slot and only the second compares.
///
/// The walk continues past that stopping point, because an `absent` anywhere in
/// the atom's known positions makes the whole atom empty (§4) — `p(X, absent)`
/// with `X` unbound has no matches, and used to cost a full scan to discover.
pub(crate) fn bound_prefix(atom: &Atom, bindings: &[Option<Value>]) -> Option<Tuple> {
    let mut prefix: Vec<Value> = Vec::new();
    let mut extending = true;
    for term in &atom.args {
        let known = match term {
            Term::Const(value) => Some(value),
            Term::Var(var) => bindings[var.0 as usize].as_ref(),
        };
        match known {
            // `unifies_with` can never hold against this position, at any
            // column, so the atom contributes nothing.
            Some(value) if value.is_absent() => return None,
            Some(value) if extending => prefix.push(value.clone()),
            Some(_) => {}
            None => extending = false,
        }
    }
    Some(Tuple(prefix))
}

/// The closed prefix of a refutation pattern — its leading slots that a
/// constant or a bound variable has closed.
///
/// The anti-join's counterpart to [`bound_prefix`], and it differs in exactly
/// one way: [`NoMatchPattern::matches`] compares **structurally**, because a
/// refutation is a membership test rather than a join (§4/§7, §17 2026-07-29).
/// So `absent` is a legal key here — `not p(X)` with `X` bound to `absent` asks
/// whether `p(absent)` is in the relation, and it is — where in a join it means
/// the atom cannot match at all. There is correspondingly no impossible case:
/// every pattern has a range, possibly the whole relation.
pub(crate) fn closed_prefix(pattern: &NoMatchPattern) -> Tuple {
    Tuple(pattern.args.iter().map_while(|arg| arg.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{PredId, Var};
    use proptest::prelude::*;

    fn tuple(values: &[Value]) -> Tuple {
        Tuple(values.to_vec())
    }

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    fn relation(tuples: &[&[Value]]) -> BTreeSet<Tuple> {
        tuples.iter().map(|t| tuple(t)).collect()
    }

    /// What the seek must equal: the scan it replaces.
    fn scan<'a>(set: &'a BTreeSet<Tuple>, prefix: &'a Tuple) -> Vec<&'a Tuple> {
        set.iter()
            .filter(|tuple| tuple.0.starts_with(&prefix.0))
            .collect()
    }

    /// A deliberately tiny cell pool, `absent` included.
    ///
    /// `testgen`'s pools feed whole programs and are broad on purpose; B12b
    /// needs *matches*, and a match wants the same value to turn up in an
    /// atom, a binding and a tuple at once. Four values make that common where
    /// two dozen make it rare — the narrowing `testgen`'s header licenses,
    /// with the reason stated.
    fn arb_cell() -> impl Strategy<Value = Value> {
        prop_oneof![
            Just(sym("a")),
            Just(sym("b")),
            Just(Value::Int(1)),
            Just(Value::Absent),
        ]
    }

    // ---- B12a: the seek is the scan -------------------------------------

    /// A relation and a prefix, the prefix usually drawn *from* the relation so
    /// the range is non-empty. Index selection rather than `prop_flat_map`,
    /// per `testgen`'s generator rules.
    fn arb_set_and_prefix() -> impl Strategy<Value = (BTreeSet<Tuple>, Tuple)> {
        (
            prop::collection::vec(
                prop::collection::vec(crate::testgen::arb_fact_value(), 1..=3),
                0..10,
            ),
            any::<prop::sample::Index>(),
            0usize..=3,
            prop::collection::vec(crate::testgen::arb_fact_value(), 0..=3),
            0u8..4,
        )
            .prop_map(|(rows, pick, take, free, use_free)| {
                let set: BTreeSet<Tuple> = rows.iter().map(|r| Tuple(r.clone())).collect();
                let prefix = if use_free == 0 || rows.is_empty() {
                    Tuple(free)
                } else {
                    let row = &rows[pick.index(rows.len())];
                    Tuple(row[..take.min(row.len())].to_vec())
                };
                (set, prefix)
            })
    }

    proptest! {
        /// **B12a** — `tuples_with_prefix` yields exactly the tuples a scan
        /// would have kept, in the same order.
        ///
        /// The oracle is the filter itself: an independent restatement of the
        /// claim (`testing.md`'s oracle corollary), not a call to the code
        /// under test.
        ///
        /// *Mutation:* `set.range(prefix..)` → `set.range(prefix..).skip(1)`.
        #[test]
        fn b12a_the_seek_is_the_scan((set, prefix) in arb_set_and_prefix()) {
            let sought: Vec<&Tuple> = tuples_with_prefix(&set, &prefix).collect();
            prop_assert_eq!(sought, scan(&set, &prefix));
        }
    }

    /// **B12a's non-vacuity guard.** The property is satisfied whenever both
    /// sides are empty, which proves nothing about contiguity. Checked against
    /// B12a's *sentence* — "exactly the tuples a scan would have kept" — so
    /// what the guard must see is a **proper** non-empty sub-range: some tuples
    /// kept and some rejected, in one relation.
    #[test]
    fn b12a_generator_reaches_a_proper_non_empty_sub_range() {
        use proptest::strategy::{Strategy, ValueTree};
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = arb_set_and_prefix();
        let mut proper = 0;
        for _ in 0..400 {
            let (set, prefix) = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            let kept = tuples_with_prefix(&set, &prefix).count();
            if kept > 0 && kept < set.len() {
                proper += 1;
            }
        }
        assert!(
            proper >= 20,
            "the generator reached a proper non-empty sub-range only {proper} times in 400"
        );
    }

    // ---- B12b: the prefix is sound against `try_match` -------------------

    /// An atom, the bindings it runs under, and a tuple at the atom's arity —
    /// the invariant `Atom` documents and lowering enforces (a predicate used
    /// at two arities is a semantic error).
    fn arb_match_case() -> impl Strategy<Value = (Atom, Vec<Option<Value>>, Tuple)> {
        (1usize..=3)
            .prop_flat_map(|arity| {
                (
                    prop::collection::vec(
                        prop_oneof![
                            arb_cell().prop_map(Term::Const),
                            (0u32..3).prop_map(|slot| Term::Var(Var(slot))),
                        ],
                        arity,
                    ),
                    prop::collection::vec(prop::option::of(arb_cell()), 3),
                    prop::collection::vec(arb_cell(), arity),
                )
            })
            .prop_map(|(args, bindings, cells)| {
                (
                    Atom {
                        pred: PredId(0),
                        args,
                    },
                    bindings,
                    Tuple(cells),
                )
            })
    }

    proptest! {
        /// **B12b** — every tuple `try_match` accepts starts with the atom's
        /// bound prefix, and a `None` prefix means no tuple is accepted.
        ///
        /// This is the half that actually licenses replacing the scan: B12a
        /// says the range is the tuples carrying a prefix, and this says the
        /// prefix loses no match. It never calls the seek.
        ///
        /// *Mutation:* let `bound_prefix` keep extending past an unbound
        /// variable (drop the `extending` flag), so a bound column at a
        /// non-leading position joins the key.
        #[test]
        fn b12b_the_bound_prefix_loses_no_match(
            (atom, bindings, candidate) in arb_match_case()
        ) {
            let mut scratch = bindings.clone();
            let matched = super::super::try_match(&atom, &candidate, &mut scratch).is_some();
            match bound_prefix(&atom, &bindings) {
                Some(prefix) => prop_assert!(
                    !matched || candidate.0.starts_with(&prefix.0),
                    "matched a tuple outside the sought range"
                ),
                None => prop_assert!(!matched, "matched an atom reported as impossible"),
            }
        }
    }

    /// **B12b's non-vacuity guard.** "No match is lost" is satisfied by a
    /// generator that never matches, and "impossible means impossible" by one
    /// that never reports impossible. Checked against B12b's *sentence*: the
    /// run must see matches carrying a **non-empty** prefix — the case where
    /// the range is narrower than the relation — and must see `None` arrived
    /// at both ways, from a constant `absent` and from a slot bound to it.
    #[test]
    fn b12b_generator_reaches_matches_with_a_prefix_and_both_impossibilities() {
        use proptest::strategy::{Strategy, ValueTree};
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = arb_match_case();
        let (mut matched_with_prefix, mut absent_const, mut absent_binding) = (0, 0, 0);
        for _ in 0..600 {
            let (atom, bindings, candidate) = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            let mut scratch = bindings.clone();
            let matched = super::super::try_match(&atom, &candidate, &mut scratch).is_some();
            match bound_prefix(&atom, &bindings) {
                Some(prefix) if matched && !prefix.0.is_empty() => matched_with_prefix += 1,
                Some(_) => {}
                None => {
                    if atom
                        .args
                        .iter()
                        .any(|t| matches!(t, Term::Const(v) if v.is_absent()))
                    {
                        absent_const += 1;
                    }
                    if atom.args.iter().any(|t| {
                        matches!(t, Term::Var(v)
                            if bindings[v.0 as usize].as_ref().is_some_and(Value::is_absent))
                    }) {
                        absent_binding += 1;
                    }
                }
            }
        }
        assert!(
            matched_with_prefix >= 20,
            "matches carrying a non-empty prefix: {matched_with_prefix} in 600"
        );
        assert!(
            absent_const >= 20 && absent_binding >= 20,
            "impossible atoms: {absent_const} from a constant, {absent_binding} from a binding"
        );
    }

    // ---- B12c: the closed prefix loses no refutation ---------------------

    /// A refutation pattern and a tuple at its arity — the same invariant
    /// B12b's generator honours, and the one `matches` checks for itself.
    fn arb_refutation_case() -> impl Strategy<Value = (NoMatchPattern, Tuple)> {
        (1usize..=3)
            .prop_flat_map(|arity| {
                (
                    prop::collection::vec(prop::option::of(arb_cell()), arity),
                    prop::collection::vec(arb_cell(), arity),
                )
            })
            .prop_map(|(args, cells)| {
                (
                    NoMatchPattern {
                        pred: PredId(0),
                        args,
                    },
                    Tuple(cells),
                )
            })
    }

    proptest! {
        /// **B12c** — every tuple a refutation pattern matches starts with its
        /// closed prefix, `absent` keys included. B12b for the anti-join,
        /// where the comparison is structural.
        ///
        /// *Mutation:* `map_while` → `filter_map` in `closed_prefix`, so a
        /// closed slot past an open one joins the key and breaks contiguity.
        #[test]
        fn b12c_the_closed_prefix_loses_no_refutation(
            (pattern, candidate) in arb_refutation_case()
        ) {
            let prefix = closed_prefix(&pattern);
            prop_assert!(
                !pattern.matches(&candidate) || candidate.0.starts_with(&prefix.0),
                "refuted on a tuple outside the sought range"
            );
        }
    }

    /// **B12c's non-vacuity guard.** A pattern that closes nothing satisfies
    /// the property trivially. Checked against B12c's *sentence* — "`absent`
    /// keys included" — so the run must see refutations carrying a non-empty
    /// prefix, and must see one whose prefix **contains `absent`**, which is
    /// the case a join would have called impossible.
    #[test]
    fn b12c_generator_reaches_refutations_on_a_prefix_and_on_an_absent_key() {
        use proptest::strategy::{Strategy, ValueTree};
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = arb_refutation_case();
        let (mut on_prefix, mut on_absent) = (0, 0);
        for _ in 0..600 {
            let (pattern, candidate) = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            if !pattern.matches(&candidate) {
                continue;
            }
            let prefix = closed_prefix(&pattern);
            if !prefix.0.is_empty() {
                on_prefix += 1;
            }
            if prefix.0.iter().any(Value::is_absent) {
                on_absent += 1;
            }
        }
        assert!(
            on_prefix >= 20 && on_absent >= 10,
            "refutations: {on_prefix} on a non-empty prefix, {on_absent} on an absent key"
        );
    }

    // ---- the cases the properties are too coarse to pin ------------------

    #[test]
    fn an_empty_prefix_yields_the_whole_relation() {
        let set = relation(&[&[sym("a")], &[sym("b")], &[Value::Absent]]);
        let everything = Tuple(Vec::new());
        let all: Vec<&Tuple> = tuples_with_prefix(&set, &everything).collect();
        assert_eq!(all, set.iter().collect::<Vec<_>>());
    }

    #[test]
    fn a_prefix_selects_a_contiguous_range_across_types() {
        // `absent < symbol < int` is the §14 cross-type order, so the `a` rows
        // are one block with rows on both sides of them.
        let set = relation(&[
            &[Value::Absent, sym("z")],
            &[sym("a"), Value::Int(1)],
            &[sym("a"), Value::Int(2)],
            &[sym("b"), Value::Int(1)],
        ]);
        let prefix = Tuple(vec![sym("a")]);
        let sought: Vec<&Tuple> = tuples_with_prefix(&set, &prefix).collect();
        assert_eq!(
            sought,
            vec![
                &tuple(&[sym("a"), Value::Int(1)]),
                &tuple(&[sym("a"), Value::Int(2)])
            ]
        );
    }

    #[test]
    fn a_constant_absent_makes_the_atom_impossible() {
        // `p(absent, X)` matches nothing (§4), so there is no range to seek.
        let atom = Atom {
            pred: PredId(0),
            args: vec![Term::Const(Value::Absent), Term::Var(Var(0))],
        };
        assert_eq!(bound_prefix(&atom, &[None]), None);
    }

    #[test]
    fn an_absent_past_the_prefix_still_makes_the_atom_impossible() {
        // `p(X, absent)` with `X` unbound: the prefix stops at `X`, but the
        // walk continues, so the atom is reported impossible instead of
        // costing a full scan that rejects every tuple.
        let atom = Atom {
            pred: PredId(0),
            args: vec![Term::Var(Var(0)), Term::Const(Value::Absent)],
        };
        assert_eq!(bound_prefix(&atom, &[None]), None);
    }

    #[test]
    fn a_slot_bound_to_absent_makes_the_atom_impossible() {
        // The other half of the same rule: `absent` unifies with nothing,
        // including another `absent`, so a slot holding one can never re-match.
        let atom = Atom {
            pred: PredId(0),
            args: vec![Term::Var(Var(0))],
        };
        assert_eq!(bound_prefix(&atom, &[Some(Value::Absent)]), None);
    }

    #[test]
    fn the_prefix_stops_at_the_first_unbound_variable() {
        // `p(a, X, Y)` with `Y` bound and `X` not: only `a` is contiguous —
        // `Y`'s column is not part of any range, and `try_match` filters it.
        let atom = Atom {
            pred: PredId(0),
            args: vec![Term::Const(sym("a")), Term::Var(Var(0)), Term::Var(Var(1))],
        };
        assert_eq!(
            bound_prefix(&atom, &[None, Some(Value::Int(7))]),
            Some(Tuple(vec![sym("a")]))
        );
    }

    #[test]
    fn a_refutation_seeks_an_absent_key_the_join_would_call_impossible() {
        // `not p(absent)` against a stored `p(absent)`: the anti-join is a
        // structural membership test, so this refutes (§4/§7) — and the seek
        // must therefore *find* the tuple a join would have ruled out.
        let set = relation(&[&[Value::Absent], &[sym("a")]]);
        let pattern = NoMatchPattern {
            pred: PredId(0),
            args: vec![Some(Value::Absent)],
        };
        let prefix = closed_prefix(&pattern);
        assert!(tuples_with_prefix(&set, &prefix).any(|tuple| pattern.matches(tuple)));

        // The join's side of the same asymmetry, for contrast.
        let atom = Atom {
            pred: PredId(0),
            args: vec![Term::Const(Value::Absent)],
        };
        assert_eq!(bound_prefix(&atom, &[]), None);
    }

    #[test]
    fn the_closed_prefix_stops_at_the_first_open_slot() {
        // `not p(a, _, b)`: only `a` is contiguous. The `b` slot still filters
        // in `matches`, it just cannot be sought.
        let pattern = NoMatchPattern {
            pred: PredId(0),
            args: vec![Some(sym("a")), None, Some(sym("b"))],
        };
        assert_eq!(closed_prefix(&pattern), Tuple(vec![sym("a")]));
    }

    #[test]
    fn a_repeated_variable_contributes_only_its_first_occurrence() {
        // `p(X, X)` with `X` unbound: the first occurrence binds the slot and
        // the second only compares, so the prefix is empty.
        let atom = Atom {
            pred: PredId(0),
            args: vec![Term::Var(Var(0)), Term::Var(Var(0))],
        };
        assert_eq!(bound_prefix(&atom, &[None]), Some(Tuple(Vec::new())));

        // Bound, both occurrences are known and the prefix covers both.
        assert_eq!(
            bound_prefix(&atom, &[Some(sym("a"))]),
            Some(Tuple(vec![sym("a"), sym("a")]))
        );
    }
}
