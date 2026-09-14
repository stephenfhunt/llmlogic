//! One predicate's facts, and the views of them the semi-naive join reads.
//!
//! A [`Relation`] owns its predicate's facts (`notes/fact-store.md`). Each fact
//! is written into it once, as a row of `arity` values appended to one flat store,
//! and a row never moves: a row id taken at any point names the same values for
//! the rest of the run (`testing.md` **B14c**). The evaluator writes only through
//! [`Relation::load_base`], before the first round, and [`Relation::apply`], once
//! per round. It reads only through [`Relation::seek`], [`Relation::contains`]
//! and [`Relation::iter`].
//!
//! # Content order is an index of sorted runs
//!
//! Rows sit in the store in the order they were written, so content order is kept
//! beside them, as runs of row ids each sorted by the rows' values. The base facts
//! are one run, sorted before they are written, and each round's block is
//! another, since a block is written in sorted order. Before a round's run is
//! added, the last two runs are merged while the older holds at most eight times
//! the newer's rows. Every run but the newest therefore holds more than eight
//! times the next, and a relation of `n` facts holds at most `log8(n) + 2` runs.
//! Eight, not two: a seek searches every run, and seek-heavy queries paid for the
//! extra runs (`notes/fact-store.md` § Seek-heavy queries, measured). Merging
//! reorders ids inside the index and never moves a row.
//!
//! A seek binary-searches each run for its prefix and merges the runs' ranges in
//! content order. That is the order every consumer sees, from the join to the
//! printed answer.
//!
//! # Membership is a hash index
//!
//! `contains` asks a [`RowIndex`] by the row's hash, not each run by search. The
//! index holds row ids, which never move, so merging runs leaves it untouched.
//!
//! # Views are read by round
//!
//! The semi-naive rewrite joins each body position against one of three views
//! ([`AtomView`]). The newest run is always the most recent apply's block. A view
//! is asked for *by the round collecting*: `Delta` is the newest run when the round
//! that wrote it is the one just before, `Old` is every other run, and `Full` is
//! all of them. A relation no round wrote last time — one a lower stratum
//! finished, or one this round's rules left unchanged — has an empty delta,
//! whatever its newest run holds (**B14a**, and **B14b** stated from round
//! stamps).
//!
//! The rewrite is sound only because application is batched: nothing is added
//! while a round collects, so every row outside the newest run was held before it
//! (**E1**).

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use super::row_index::RowIndex;
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
#[derive(Debug, Clone)]
pub struct Relation {
    arity: usize,
    /// Every fact's values, `arity` to a row, in the order the rows were written.
    values: Vec<Value>,
    /// How many rows are written; counted apart from `values`, which a relation
    /// of arity 0 leaves empty.
    rows: u32,
    /// Row ids in content order: sorted runs, the most recent apply's block last.
    runs: Vec<Vec<u32>>,
    /// Every row id, by the row's hash: membership without a search.
    index: RowIndex,
    /// The round whose apply wrote the newest run, or 0 when no round has.
    delta_round: u32,
    /// How many rows are base facts: the rows `load_base` wrote, first.
    base_rows: u32,
    /// Each applied block's first row and the round that wrote it, in row order.
    blocks: Vec<(u32, u32)>,
}

impl Relation {
    /// An empty relation whose facts are `arity` values wide.
    pub(crate) fn new(arity: usize) -> Relation {
        Relation {
            arity,
            values: Vec::new(),
            rows: 0,
            runs: Vec::new(),
            index: RowIndex::default(),
            delta_round: 0,
            base_rows: 0,
            blocks: Vec::new(),
        }
    }

    /// How many facts the relation holds.
    pub fn len(&self) -> usize {
        self.rows as usize
    }

    /// Whether the relation holds no fact.
    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }

    /// Whether `row` is a fact of this relation.
    pub fn contains(&self, row: &[Value]) -> bool {
        self.find(row).is_some()
    }

    /// The id of the row holding `row`'s values, if the relation holds them.
    pub(crate) fn find(&self, row: &[Value]) -> Option<u32> {
        self.index.find(row_hash(row), |id| self.row(id) == row)
    }

    /// Every fact's row, in canonical order.
    pub fn iter(&self) -> impl Iterator<Item = &[Value]> + '_ {
        self.merged(&self.runs, &[]).map(|(_, row)| row)
    }

    /// The values of row `id`, which are the same for the rest of the run from the
    /// moment the row is written.
    pub(crate) fn row(&self, id: u32) -> &[Value] {
        let start = id as usize * self.arity;
        &self.values[start..start + self.arity]
    }

    /// Writes the program-asserted facts, before the first round: `rows` rows of
    /// `arity` values laid end to end. A fact asserted twice is one fact.
    pub(crate) fn load_base(&mut self, values: Vec<Value>, rows: usize) {
        debug_assert!(
            self.rows == 0 && self.delta_round == 0,
            "base facts load once, before any round"
        );
        let (values, rows) = crate::ir::sort_dedup_rows(values, rows, self.arity);
        self.values = values;
        self.rows = u32::try_from(rows).expect("a relation holds fewer than 2^32 facts");
        self.index.reserve(rows);
        for id in 0..self.rows {
            let start = id as usize * self.arity;
            let hash = row_hash(&self.values[start..start + self.arity]);
            self.index.insert(hash, id);
        }
        self.base_rows = self.rows;
        if self.rows > 0 {
            self.runs.push((0..self.rows).collect());
        }
    }

    /// Adds `round`'s block of new facts, which becomes the delta the next round
    /// reads. Every fact in `block` must be one the relation does not hold.
    pub(crate) fn apply(&mut self, block: BTreeSet<Tuple>, round: u32) {
        debug_assert!(round > self.delta_round, "rounds apply in order");
        debug_assert!(block.iter().all(|tuple| !self.contains(&tuple.0)));
        while let [.., older, newer] = self.runs.as_slice()
            && older.len() <= 8 * newer.len()
        {
            let newer = self.runs.pop().expect("two runs");
            let older = self.runs.pop().expect("two runs");
            let merged = self.merge(&older, &newer);
            self.runs.push(merged);
        }
        let run = self.append(block);
        if let Some(&first) = run.first() {
            self.blocks.push((first, round));
        }
        self.runs.push(run);
        self.delta_round = round;
    }

    /// Writes sorted, distinct rows to the end of the store, returning their ids,
    /// which are therefore a sorted run.
    fn append(
        &mut self,
        rows: impl IntoIterator<Item = Tuple, IntoIter: ExactSizeIterator>,
    ) -> Vec<u32> {
        let rows = rows.into_iter();
        let first = self.rows;
        self.values.reserve_exact(rows.len() * self.arity);
        self.index.reserve(rows.len());
        for Tuple(values) in rows {
            debug_assert_eq!(values.len(), self.arity, "a fact is its predicate's arity");
            let hash = row_hash(&values);
            self.values.extend(values);
            self.index.insert(hash, self.rows);
            self.rows = self
                .rows
                .checked_add(1)
                .expect("a relation holds fewer than 2^32 facts");
        }
        (first..self.rows).collect()
    }

    /// Two disjoint sorted runs as one.
    fn merge(&self, older: &[u32], newer: &[u32]) -> Vec<u32> {
        let mut merged = Vec::with_capacity(older.len() + newer.len());
        let (mut i, mut j) = (0, 0);
        while i < older.len() && j < newer.len() {
            if self.row(older[i]) < self.row(newer[j]) {
                merged.push(older[i]);
                i += 1;
            } else {
                merged.push(newer[j]);
                j += 1;
            }
        }
        merged.extend_from_slice(&older[i..]);
        merged.extend_from_slice(&newer[j..]);
        debug_assert!(
            merged
                .windows(2)
                .all(|pair| self.row(pair[0]) < self.row(pair[1]))
        );
        merged
    }

    /// Whether row `id` is a base fact: one `load_base` wrote, before any round.
    pub(crate) fn is_base_row(&self, id: u32) -> bool {
        id < self.base_rows
    }

    /// The round that wrote row `id`: 0 for a base fact, and otherwise the round
    /// of the applied block holding it. Every row of a block shares its round,
    /// because application is batched (**E1**).
    pub(crate) fn round_of(&self, id: u32) -> u32 {
        if self.is_base_row(id) {
            return 0;
        }
        let block = self.blocks.partition_point(|&(first, _)| first <= id);
        self.blocks[block - 1].1
    }

    /// Whether the newest run is the delta for a join collecting `round`.
    fn delta_is_current(&self, round: u32) -> bool {
        self.delta_round != 0 && self.delta_round + 1 == round
    }

    /// The rows of `view` whose leading columns equal `prefix`, for a join
    /// collecting `round`, in canonical order, each beside its row id.
    pub(crate) fn seek<'a>(
        &'a self,
        view: AtomView,
        round: u32,
        prefix: &'a [Value],
    ) -> Box<dyn Iterator<Item = (u32, &'a [Value])> + 'a> {
        let current = self.delta_is_current(round);
        let newest = self.runs.len().saturating_sub(1);
        let runs = match view {
            AtomView::Full => &self.runs[..],
            AtomView::Delta if current => &self.runs[newest..],
            AtomView::Delta => &[],
            AtomView::Old if current => &self.runs[..newest],
            AtomView::Old => &self.runs[..],
        };
        Box::new(self.merged(runs, prefix))
    }

    /// The rows of `runs` that start with `prefix`, merged in content order.
    fn merged<'a>(&'a self, runs: &'a [Vec<u32>], prefix: &'a [Value]) -> Merged<'a> {
        let cursors = runs
            .iter()
            .map(|run| &run[run.partition_point(|&id| self.row(id) < prefix)..])
            .filter(|rest| {
                rest.first()
                    .is_some_and(|&id| self.row(id).starts_with(prefix))
            })
            .collect();
        Merged {
            relation: self,
            prefix,
            cursors,
        }
    }

    /// The index's runs, for the properties that check it.
    #[cfg(test)]
    pub(crate) fn runs(&self) -> &[Vec<u32>] {
        &self.runs
    }
}

/// The hash a row is indexed by: `Value`'s `Hash` through [`RowHasher`]. Equal rows
/// hash alike, since `ir::F64` hashes its normalised bits.
fn row_hash(row: &[Value]) -> u64 {
    let mut hasher = RowHasher(0);
    row.hash(&mut hasher);
    hasher.finish()
}

/// A multiply-rotate hasher with a final mix, so the high bits [`RowIndex`] reads
/// are spread. Hand-written because the core takes no dependencies, and std's
/// `DefaultHasher` spends more on each row than a membership test can afford.
struct RowHasher(u64);

impl RowHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for RowHasher {
    fn finish(&self) -> u64 {
        let mut hash = self.0;
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        hash ^ (hash >> 33)
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.add(u64::from_le_bytes(chunk.try_into().expect("eight bytes")));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }
}

/// Runs' ranges merged in content order. Each cursor is the rest of one run's
/// range, and is dropped once it is empty or its next row leaves the prefix, so
/// every cursor held starts at a row that belongs in the output.
struct Merged<'a> {
    relation: &'a Relation,
    prefix: &'a [Value],
    cursors: Vec<&'a [u32]>,
}

impl<'a> Iterator for Merged<'a> {
    type Item = (u32, &'a [Value]);

    fn next(&mut self) -> Option<(u32, &'a [Value])> {
        let relation = self.relation;
        let (index, _) = self
            .cursors
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| relation.row(a[0]).cmp(relation.row(b[0])))?;
        let id = self.cursors[index][0];
        let rest = &self.cursors[index][1..];
        if rest
            .first()
            .is_some_and(|&next| relation.row(next).starts_with(self.prefix))
        {
            self.cursors[index] = rest;
        } else {
            self.cursors.swap_remove(index);
        }
        Some((id, relation.row(id)))
    }
}

/// A relation equals a set when it holds exactly the set's tuples, compared in
/// canonical order.
#[cfg(test)]
impl PartialEq<BTreeSet<Tuple>> for Relation {
    fn eq(&self, other: &BTreeSet<Tuple>) -> bool {
        self.iter().eq(other.iter().map(|tuple| tuple.0.as_slice()))
    }
}

/// Two relations are equal when they hold the same facts; where the rows sit and
/// how the index is split is evaluation state, not content.
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

    /// One round applied to a relation: a block of rows, or nothing written.
    #[derive(Debug, Clone)]
    enum Step {
        Write(Vec<Vec<Value>>),
        Skip,
    }

    /// Mostly a tiny collision-rich pool, so blocks overlap held rows (which a
    /// block must then leave out) and prefixes select proper sub-ranges; and
    /// sometimes any fact value, so content order is exercised across every type
    /// §14 orders.
    fn arb_cell() -> impl Strategy<Value = Value> {
        prop_oneof![
            4 => prop_oneof![
                Just(Value::symbol("a")),
                Just(Value::symbol("b")),
                Just(Value::Int(1)),
                Just(Value::Int(2)),
                Just(Value::Absent),
            ],
            1 => crate::testgen::arb_fact_value(),
        ]
    }

    fn arb_row() -> impl Strategy<Value = Vec<Value>> {
        prop::collection::vec(arb_cell(), 3)
    }

    /// Base rows, then rounds 1.. of steps, then a prefix to seek.
    fn arb_history() -> impl Strategy<Value = (Vec<Vec<Value>>, Vec<Step>, Vec<Value>)> {
        let step = prop_oneof![
            4 => prop::collection::vec(arb_row(), 0..8).prop_map(Step::Write),
            1 => Just(Step::Skip),
        ];
        (
            prop::collection::vec(arb_row(), 0..10),
            prop::collection::vec(step, 0..12),
            prop::collection::vec(arb_cell(), 0..=2),
        )
    }

    /// What a relation must hold, restated from the history.
    struct Ledger {
        held: BTreeSet<Vec<Value>>,
        /// Each round that wrote, with the rows it added.
        written: Vec<(u32, BTreeSet<Vec<Value>>)>,
        /// Every row id the relation had issued after each step, with the values
        /// it named then.
        issued: Vec<(u32, Vec<Value>)>,
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

    /// Replays a history onto a relation and, independently, onto a ledger. The
    /// only thing the ledger takes from the relation is what each newly issued row
    /// id named at the time, which is the claim B14a checks at the end.
    fn replay(base: &[Vec<Value>], steps: &[Step]) -> (Relation, Ledger, u32) {
        let mut relation = Relation::new(3);
        let mut ledger = Ledger {
            held: BTreeSet::new(),
            written: Vec::new(),
            issued: Vec::new(),
        };
        let issue = |relation: &Relation, ledger: &mut Ledger| {
            for id in ledger.issued.len() as u32..relation.len() as u32 {
                ledger.issued.push((id, relation.row(id).to_vec()));
            }
        };
        relation.load_base(base.concat(), base.len());
        ledger.held.extend(base.iter().cloned());
        issue(&relation, &mut ledger);
        let mut round = 0;
        for step in steps {
            round += 1;
            if let Step::Write(rows) = step {
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
                issue(&relation, &mut ledger);
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
            .map(|(_, row)| row.to_vec())
            .collect()
    }

    proptest! {
        /// **B14a** — a relation is its facts, its index is its rows in order, and
        /// its views are read by round. After any history of base rows and rounds
        /// that write or skip:
        /// - iteration is strictly ascending and is exactly the rows held, and
        ///   `contains` agrees on every row the pool can form;
        /// - every run is strictly ascending by content, the runs together hold
        ///   every row id exactly once, and there are at most `log8(n) + 2` of
        ///   them;
        /// - every row id names, at the end, the values it named when issued;
        /// - for a join collecting the round after the last and the one after
        ///   that, `Full`, `Delta` and `Old` under any prefix are the held rows,
        ///   the block the round just before wrote, and the rest, each in
        ///   canonical order. The `Full` case is the seek against the scan it
        ///   replaces (B12a's sentence, over runs).
        ///
        /// The oracle is the ledger, which re-derives each view from the history.
        ///
        /// *Mutations (killed):* recorded on `testing.md`'s B14 line.
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
                Value::symbol("a"),
                Value::symbol("b"),
                Value::Int(1),
                Value::Int(2),
                Value::Absent,
            ];
            for x in &cells {
                for y in &cells {
                    for z in &cells {
                        let row = [x.clone(), y.clone(), z.clone()];
                        prop_assert_eq!(relation.contains(&row), ledger.held.contains(row.as_slice()));
                    }
                }
            }

            let mut ids: Vec<u32> = Vec::new();
            for run in relation.runs() {
                prop_assert!(
                    run.windows(2).all(|pair| relation.row(pair[0]) < relation.row(pair[1])),
                    "a run is not sorted"
                );
                ids.extend(run);
            }
            ids.sort_unstable();
            prop_assert_eq!(ids, (0..relation.len() as u32).collect::<Vec<_>>());
            if !relation.is_empty() {
                prop_assert!(
                    relation.runs().len() <= relation.len().ilog(8) as usize + 2,
                    "{} runs for {} rows", relation.runs().len(), relation.len()
                );
            }
            for (id, values) in &ledger.issued {
                prop_assert_eq!(relation.row(*id), values.as_slice(), "row {} moved", id);
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

    fn symbol(name: &str) -> Value {
        Value::symbol(name)
    }

    /// The cases the properties are too coarse to pin: an empty prefix, and a
    /// prefix whose range has rows of other types on both sides.
    #[test]
    fn an_empty_prefix_seeks_the_whole_relation() {
        let mut relation = Relation::new(1);
        relation.load_base(vec![symbol("b"), Value::Absent, symbol("a")], 3);
        let all: Vec<&[Value]> = relation
            .seek(AtomView::Full, 0, &[])
            .map(|(_, row)| row)
            .collect();
        assert_eq!(
            all,
            vec![&[Value::Absent][..], &[symbol("a")][..], &[symbol("b")][..]]
        );
    }

    #[test]
    fn a_prefix_seeks_a_contiguous_range_across_types_and_runs() {
        // `absent < symbol < int` is the §14 cross-type order, so the `a` rows are
        // one block with rows on both sides of them, and here they sit in two runs.
        let mut relation = Relation::new(2);
        relation.load_base(
            vec![
                Value::Absent,
                symbol("z"),
                symbol("a"),
                Value::Int(2),
                symbol("b"),
                Value::Int(1),
            ],
            3,
        );
        relation.apply(BTreeSet::from([Tuple(vec![symbol("a"), Value::Int(1)])]), 1);
        let prefix = [symbol("a")];
        let sought: Vec<&[Value]> = relation
            .seek(AtomView::Full, 2, &prefix)
            .map(|(_, row)| row)
            .collect();
        assert_eq!(
            sought,
            vec![
                &[symbol("a"), Value::Int(1)][..],
                &[symbol("a"), Value::Int(2)][..]
            ]
        );
    }

    /// **B14a's non-vacuity guard**, read against its sentence. The views are
    /// only distinguished when a delta sits beside older rows and when the newest
    /// run is *not* the delta (a skipped round after a write). The index is only
    /// tested when a history merges runs and still holds several. And a prefix
    /// must select part of a view and not all of it. Floors at about two thirds
    /// of what was measured: 266, 100, 317, 63, 139 and 395 of 400.
    #[test]
    fn b14a_generator_reaches_views_merges_and_proper_ranges() {
        use proptest::strategy::ValueTree;
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = arb_history();
        let (mut current, mut stale, mut merged, mut several, mut proper, mut typed) =
            (0, 0, 0, 0, 0, 0);
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
            if ledger
                .written
                .last()
                .is_some_and(|(written, _)| *written < last)
            {
                stale += 1;
            }
            let blocks = ledger.written.len() + usize::from(!base.is_empty());
            if relation.runs().len() < blocks {
                merged += 1;
            }
            if relation.runs().len() >= 3 {
                several += 1;
            }
            let types: std::collections::HashSet<std::mem::Discriminant<Value>> = ledger
                .held
                .iter()
                .flatten()
                .map(std::mem::discriminant)
                .collect();
            if types.len() >= 3 {
                typed += 1;
            }
            let full = sought(&relation, AtomView::Full, last + 1, &prefix).len();
            if full > 0 && full < relation.len() {
                proper += 1;
            }
        }
        eprintln!(
            "current {current}, stale {stale}, merged {merged}, 3+ runs {several}, proper {proper}, 3+ types {typed} of 400"
        );
        assert!(typed >= 263, "rows of three or more types: {typed} of 400");
        assert!(
            current >= 177,
            "a current delta beside older rows: {current} of 400"
        );
        assert!(
            stale >= 66,
            "a newest run that is not the delta: {stale} of 400"
        );
        assert!(merged >= 211, "a history that merged runs: {merged} of 400");
        assert!(several >= 42, "three or more runs: {several} of 400");
        assert!(
            proper >= 92,
            "a proper non-empty sub-range: {proper} of 400"
        );
    }
}
