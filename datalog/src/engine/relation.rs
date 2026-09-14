//! One predicate's facts, and the views of them the semi-naive join reads.
//!
//! A [`Relation`] is the owner of its predicate's facts (`notes/fact-store.md`):
//! the evaluator adds to it only through [`Relation::insert_base`] before the
//! first round and [`Relation::apply`] once per round, and reads it only through
//! [`Relation::seek`], [`Relation::contains`] and [`Relation::iter`].
//!
//! # Views are read by round
//!
//! The semi-naive rewrite joins each body position against one of three views
//! ([`AtomView`]). `Delta` is the block the previous round's apply added, and
//! `Old` is everything else. A relation keeps its most recent block and the round
//! that wrote it, and a view is asked for *by the round collecting*: the block is
//! the delta only when it was written by the round just before. A relation no
//! round wrote last time — one a lower stratum finished, or one this round's
//! rules left unchanged — has an empty delta, whatever block it holds. This is
//! `testing.md`'s views property, stated from round stamps.
//!
//! The rewrite is sound only because application is batched: nothing is added
//! while a round collects, so every row not in the previous block was held
//! before it (**E1**).

use std::collections::BTreeSet;

use super::seek;
use crate::ir::{Tuple, Value};

/// Which slice of a relation a body position joins against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AtomView {
    /// Every held fact.
    Full,
    /// The facts the previous round's apply added.
    Delta,
    /// Every held fact but the delta: the relation before the previous round.
    Old,
}

/// One predicate's extent, base and derived, in canonical (§14) order.
#[derive(Debug, Clone, Default)]
pub struct Relation {
    facts: BTreeSet<Tuple>,
    /// The block the most recent apply added, when that apply was `delta_round`.
    delta: BTreeSet<Tuple>,
    /// The round whose apply wrote `delta`, or 0 when no round has.
    delta_round: u32,
}

impl Relation {
    /// How many facts the relation holds.
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    /// Whether the relation holds no fact.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Whether `row` is a fact of this relation.
    pub fn contains(&self, row: &[Value]) -> bool {
        self.facts.contains(row)
    }

    /// Every fact's row, in canonical order.
    pub fn iter(&self) -> impl Iterator<Item = &[Value]> + '_ {
        self.facts.iter().map(|tuple| tuple.0.as_slice())
    }

    /// Loads a program-asserted fact, before the first round. A fact asserted
    /// twice is one fact.
    pub(crate) fn insert_base(&mut self, tuple: Tuple) {
        debug_assert_eq!(self.delta_round, 0, "base facts load before any round");
        self.facts.insert(tuple);
    }

    /// Adds `round`'s block of new facts, which becomes the delta the next round
    /// reads. Every fact in `block` must be one the relation does not hold.
    pub(crate) fn apply(&mut self, block: BTreeSet<Tuple>, round: u32) {
        debug_assert!(round > self.delta_round, "rounds apply in order");
        debug_assert!(block.iter().all(|tuple| !self.facts.contains(tuple)));
        self.facts.extend(block.iter().cloned());
        self.delta = block;
        self.delta_round = round;
    }

    /// Frees a block no later round reads as a delta. It changes no view, since
    /// views read by round; it only keeps a relation from holding its last block
    /// for the rest of the run.
    pub(crate) fn retire_delta(&mut self) {
        self.delta = BTreeSet::new();
    }

    /// Whether the held block is the delta for a join collecting `round`.
    fn delta_is_current(&self, round: u32) -> bool {
        self.delta_round != 0 && self.delta_round + 1 == round
    }

    /// The rows of `view` whose leading columns equal `prefix`, for a join
    /// collecting `round`, in canonical order.
    pub(crate) fn seek<'a>(
        &'a self,
        view: AtomView,
        round: u32,
        prefix: &'a [Value],
    ) -> Box<dyn Iterator<Item = &'a [Value]> + 'a> {
        let current = self.delta_is_current(round);
        match view {
            AtomView::Full => Box::new(rows_with_prefix(&self.facts, prefix)),
            AtomView::Delta if current => Box::new(rows_with_prefix(&self.delta, prefix)),
            AtomView::Delta => Box::new(std::iter::empty()),
            AtomView::Old if current => Box::new(
                rows_with_prefix(&self.facts, prefix).filter(move |row| !self.delta.contains(*row)),
            ),
            AtomView::Old => Box::new(rows_with_prefix(&self.facts, prefix)),
        }
    }
}

fn rows_with_prefix<'a>(
    set: &'a BTreeSet<Tuple>,
    prefix: &'a [Value],
) -> impl Iterator<Item = &'a [Value]> + 'a {
    seek::tuples_with_prefix(set, prefix).map(|tuple| tuple.0.as_slice())
}

/// A relation equals a set when it holds exactly the set's tuples, compared in
/// canonical order.
#[cfg(test)]
impl PartialEq<BTreeSet<Tuple>> for Relation {
    fn eq(&self, other: &BTreeSet<Tuple>) -> bool {
        self.iter().eq(other.iter().map(|tuple| tuple.0.as_slice()))
    }
}

/// Two relations are equal when they hold the same facts; the delta is
/// evaluation state, not content.
#[cfg(test)]
impl PartialEq for Relation {
    fn eq(&self, other: &Relation) -> bool {
        self.iter().eq(other.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// One step applied to a relation: a block of rows written in a round, or a
    /// round that writes nothing, which may also retire the held block.
    #[derive(Debug, Clone)]
    enum Step {
        Write(Vec<Vec<Value>>),
        Skip { retire: bool },
    }

    /// A tiny collision-rich pool, so blocks overlap held rows (which a block
    /// must then leave out) and prefixes select proper sub-ranges.
    fn arb_cell() -> impl Strategy<Value = Value> {
        prop_oneof![
            Just(Value::Symbol("a".to_string())),
            Just(Value::Symbol("b".to_string())),
            Just(Value::Int(1)),
            Just(Value::Absent),
        ]
    }

    fn arb_row() -> impl Strategy<Value = Vec<Value>> {
        prop::collection::vec(arb_cell(), 2)
    }

    /// Base rows, then rounds 1.. of steps, then a prefix to seek.
    fn arb_history() -> impl Strategy<Value = (Vec<Vec<Value>>, Vec<Step>, Vec<Value>)> {
        let step = prop_oneof![
            3 => prop::collection::vec(arb_row(), 0..6).prop_map(Step::Write),
            1 => any::<bool>().prop_map(|retire| Step::Skip { retire }),
        ];
        (
            prop::collection::vec(arb_row(), 0..6),
            prop::collection::vec(step, 0..6),
            prop::collection::vec(arb_cell(), 0..=2),
        )
    }

    /// What a view must yield, restated from the history: the rows held, the
    /// block written by the round just before `round`, and the rest.
    struct Ledger {
        held: BTreeSet<Vec<Value>>,
        /// Each round that wrote, with the rows it added.
        written: Vec<(u32, BTreeSet<Vec<Value>>)>,
    }

    impl Ledger {
        fn delta_for(&self, round: u32) -> BTreeSet<Vec<Value>> {
            self.written
                .iter()
                .find(|(written, _)| *written + 1 == round)
                .map(|(_, rows)| rows.clone())
                .unwrap_or_default()
        }
    }

    /// Replays a history onto a relation and, independently, onto a ledger.
    fn replay(base: &[Vec<Value>], steps: &[Step]) -> (Relation, Ledger, u32) {
        let mut relation = Relation::default();
        let mut ledger = Ledger {
            held: BTreeSet::new(),
            written: Vec::new(),
        };
        for row in base {
            relation.insert_base(Tuple(row.clone()));
            ledger.held.insert(row.clone());
        }
        let mut round = 0;
        for step in steps {
            round += 1;
            match step {
                Step::Write(rows) => {
                    let new: BTreeSet<Vec<Value>> = rows
                        .iter()
                        .filter(|row| !ledger.held.contains(*row))
                        .cloned()
                        .collect();
                    if new.is_empty() {
                        continue;
                    }
                    relation.apply(new.iter().cloned().map(Tuple).collect(), round);
                    ledger.held.extend(new.iter().cloned());
                    ledger.written.push((round, new));
                }
                Step::Skip { retire } => {
                    if *retire {
                        relation.retire_delta();
                    }
                }
            }
        }
        (relation, ledger, round)
    }

    fn with_prefix<'a>(
        rows: impl IntoIterator<Item = &'a Vec<Value>>,
        prefix: &[Value],
    ) -> Vec<Vec<Value>> {
        rows.into_iter()
            .filter(|row| row.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn sought(
        relation: &Relation,
        view: AtomView,
        round: u32,
        prefix: &[Value],
    ) -> Vec<Vec<Value>> {
        relation
            .seek(view, round, prefix)
            .map(|row| row.to_vec())
            .collect()
    }

    proptest! {
        /// **B14a** — a relation is its facts, and its views are read by round.
        /// After any history of base rows and rounds that write or skip:
        /// iteration is strictly ascending and is exactly the rows held;
        /// `contains` agrees on every row the pool can form; and for a join
        /// collecting the round after the last and the one after that, `Full`,
        /// `Delta` and `Old` under any prefix are the held rows, the block the
        /// round just before wrote, and the rest — each in canonical order.
        ///
        /// The oracle is the ledger, which re-derives each view from the
        /// history and never calls the relation.
        ///
        /// *Mutations (killed):* `delta_is_current` ignoring the round; `Old`
        /// not filtering the delta; `apply` leaving the previous block as the
        /// delta.
        #[test]
        fn b14a_a_relation_is_its_facts_and_its_views_are_by_round(
            (base, steps, prefix) in arb_history()
        ) {
            let (relation, ledger, last) = replay(&base, &steps);

            let rows: Vec<Vec<Value>> = relation.iter().map(|row| row.to_vec()).collect();
            prop_assert!(rows.windows(2).all(|pair| pair[0] < pair[1]), "not strictly ascending");
            prop_assert_eq!(&rows, &ledger.held.iter().cloned().collect::<Vec<_>>());
            prop_assert_eq!(relation.len(), ledger.held.len());

            let cells = [
                Value::Symbol("a".to_string()),
                Value::Symbol("b".to_string()),
                Value::Int(1),
                Value::Absent,
            ];
            for x in &cells {
                for y in &cells {
                    let row = [x.clone(), y.clone()];
                    prop_assert_eq!(relation.contains(&row), ledger.held.contains(row.as_slice()));
                }
            }

            for round in [last + 1, last + 2] {
                let delta = ledger.delta_for(round);
                let old: BTreeSet<Vec<Value>> = ledger.held.difference(&delta).cloned().collect();
                prop_assert_eq!(sought(&relation, AtomView::Full, round, &prefix), with_prefix(&ledger.held, &prefix));
                prop_assert_eq!(sought(&relation, AtomView::Delta, round, &prefix), with_prefix(&delta, &prefix));
                prop_assert_eq!(sought(&relation, AtomView::Old, round, &prefix), with_prefix(&old, &prefix));
            }
        }
    }

    /// **B14a's non-vacuity guard**, read against its sentence: the views are
    /// only distinguished when the delta is non-empty beside older rows, when a
    /// held block is *not* the delta (a skipped round after a write, retired or
    /// not), and when a prefix selects part of a view and not all of it. Floors
    /// at about two thirds of what was measured: 155, 29, 29 and 135 of 400.
    #[test]
    fn b14a_generator_reaches_current_and_stale_blocks_and_proper_ranges() {
        use proptest::strategy::ValueTree;
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = arb_history();
        let (mut current, mut stale_kept, mut stale_retired, mut proper) = (0, 0, 0, 0);
        for _ in 0..400 {
            let (base, steps, prefix) = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            let (relation, ledger, last) = replay(&base, &steps);
            let delta = ledger.delta_for(last + 1);
            if !delta.is_empty() && delta.len() < ledger.held.len() {
                current += 1;
            }
            let last_write = ledger.written.last().map(|(round, _)| *round);
            if let (Some(written), Some(Step::Skip { retire })) = (last_write, steps.last())
                && written < last
            {
                if *retire {
                    stale_retired += 1;
                } else {
                    stale_kept += 1;
                }
            }
            let full = sought(&relation, AtomView::Full, last + 1, &prefix).len();
            if full > 0 && full < relation.len() {
                proper += 1;
            }
        }
        eprintln!(
            "current {current}, stale kept {stale_kept}, stale retired {stale_retired}, proper {proper} of 400"
        );
        assert!(
            current >= 100,
            "a current delta beside older rows: {current} of 400"
        );
        assert!(
            stale_kept >= 19,
            "a stale block still held: {stale_kept} of 400"
        );
        assert!(
            stale_retired >= 19,
            "a stale block retired: {stale_retired} of 400"
        );
        assert!(
            proper >= 90,
            "a proper non-empty sub-range: {proper} of 400"
        );
    }
}
