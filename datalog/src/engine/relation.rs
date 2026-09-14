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
