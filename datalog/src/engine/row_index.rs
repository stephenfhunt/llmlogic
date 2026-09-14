//! Membership of a row by content, without a search: an open-addressing table of
//! row ids.
//!
//! A [`Relation`](super::relation::Relation) answers `contains` for every head a
//! round grounds. Binary-searching each sorted run for it was the main cost of a
//! dense recursive closure (`notes/fact-store.md` § Step 2, measured). This table
//! holds no values. A slot is a row id and the high 32 bits of the row's hash, and
//! a lookup is given the row's full hash and a test of whether a row id's values
//! are the ones sought. Rows never move, so an id in the table stays right for the
//! rest of the run, whatever the relation's runs do.
//!
//! Probing is linear from the slot the tag names, and the table grows at three
//! quarters full. Growth re-places each slot by its stored tag and never reads a
//! row. The table is a set of row ids under the caller's equality: **B15** checks
//! it against a map, with hashes chosen to collide and to wrap past the last slot.

/// A slot no row id occupies. No relation reaches 2^32 − 1 rows.
const EMPTY: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Slot {
    tag: u32,
    id: u32,
}

#[derive(Clone, Default)]
pub(crate) struct RowIndex {
    slots: Vec<Slot>,
    len: usize,
}

impl std::fmt::Debug for RowIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RowIndex {{ len: {}, capacity: {} }}",
            self.len,
            self.slots.len()
        )
    }
}

/// The bits of a hash a slot keeps, and the slot its probe starts from.
fn tag(hash: u64) -> u32 {
    (hash >> 32) as u32
}

/// The smallest power of two, at least 16, that holds `rows` at three quarters.
fn capacity_for(rows: usize) -> usize {
    let mut capacity = 16;
    while rows * 4 > capacity * 3 {
        capacity *= 2;
    }
    capacity
}

impl RowIndex {
    /// The id of the row whose hash is `hash` and for which `is_row` holds.
    pub(crate) fn find(&self, hash: u64, is_row: impl Fn(u32) -> bool) -> Option<u32> {
        if self.slots.is_empty() {
            return None;
        }
        let mask = self.slots.len() - 1;
        let tag = tag(hash);
        let mut position = tag as usize & mask;
        loop {
            let slot = self.slots[position];
            if slot.id == EMPTY {
                return None;
            }
            if slot.tag == tag && is_row(slot.id) {
                return Some(slot.id);
            }
            position = (position + 1) & mask;
        }
    }

    /// Adds row `id`, whose hash is `hash`. The caller has checked that no equal
    /// row is held.
    pub(crate) fn insert(&mut self, hash: u64, id: u32) {
        debug_assert_ne!(id, EMPTY, "u32::MAX marks an empty slot");
        self.reserve(1);
        self.place(Slot { tag: tag(hash), id });
        self.len += 1;
    }

    /// Makes room for `additional` more rows without growing again.
    pub(crate) fn reserve(&mut self, additional: usize) {
        let rows = self.len + additional;
        if rows * 4 > self.slots.len() * 3 {
            let empty = Slot { tag: 0, id: EMPTY };
            let old = std::mem::replace(&mut self.slots, vec![empty; capacity_for(rows)]);
            for slot in old {
                if slot.id != EMPTY {
                    self.place(slot);
                }
            }
        }
    }

    /// Puts `slot` in the first empty slot from the one its tag names.
    fn place(&mut self, slot: Slot) {
        let mask = self.slots.len() - 1;
        let mut position = slot.tag as usize & mask;
        while self.slots[position].id != EMPTY {
            position = (position + 1) & mask;
        }
        self.slots[position] = slot;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use proptest::prelude::*;

    use super::*;

    /// How a key's hash is made. `AllOnes` gives every key the same hash, whose
    /// tag names the last slot: every lookup compares rows, and every probe wraps.
    /// `ThreeBuckets` gives keys one of three tags, so collisions cluster and keys
    /// sharing a tag are told apart only by the row test. `Spread` spreads them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Hashing {
        AllOnes,
        ThreeBuckets,
        Spread,
    }

    fn hash_of(hashing: Hashing, key: u16) -> u64 {
        match hashing {
            Hashing::AllOnes => u64::MAX,
            Hashing::ThreeBuckets => (u64::from(key % 3) << 32) | 0xFFFF_FFFF,
            Hashing::Spread => u64::from(key).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        }
    }

    /// A hashing and a sequence of lookups, each also an insert when `true` and
    /// the key is absent. Keys come from a small range, so they repeat.
    fn arb_case() -> impl Strategy<Value = (Hashing, Vec<(bool, u16)>)> {
        (
            prop_oneof![
                Just(Hashing::AllOnes),
                Just(Hashing::ThreeBuckets),
                Just(Hashing::Spread)
            ],
            prop::collection::vec((any::<bool>(), 0u16..120), 0..300),
        )
    }

    /// Replays a case against the index and a map, returning the first
    /// disagreement, and the distinct keys inserted.
    fn replay(hashing: Hashing, ops: &[(bool, u16)]) -> (Option<String>, usize) {
        let mut index = RowIndex::default();
        let mut keys: Vec<u16> = Vec::new();
        let mut reference: HashMap<u16, u32> = HashMap::new();
        for &(insert, key) in ops {
            let found = index.find(hash_of(hashing, key), |id| keys[id as usize] == key);
            if found != reference.get(&key).copied() {
                return (
                    Some(format!(
                        "find {key} under {hashing:?}: {found:?}, map says {:?}",
                        reference.get(&key)
                    )),
                    keys.len(),
                );
            }
            if insert && found.is_none() {
                let id = keys.len() as u32;
                keys.push(key);
                index.insert(hash_of(hashing, key), id);
                reference.insert(key, id);
            }
        }
        for (&key, &id) in &reference {
            let found = index.find(hash_of(hashing, key), |i| keys[i as usize] == key);
            if found != Some(id) {
                return (
                    Some(format!(
                        "after the sequence, {key} finds {found:?}, not {id}"
                    )),
                    keys.len(),
                );
            }
        }
        (None, keys.len())
    }

    proptest! {
        /// **B15** — the row index is a set of row ids under the caller's
        /// equality. After any sequence of inserts of absent keys and lookups,
        /// under a hash that collides everywhere and wraps, one that clusters, and
        /// one that spreads, every lookup finds exactly the id a map holds for the
        /// key, and nothing for a key never inserted.
        ///
        /// The oracle is `HashMap`, which shares nothing with the table.
        ///
        /// *Mutations (killed):* recorded on `testing.md`'s B15 line.
        #[test]
        fn b15_the_row_index_is_a_set_of_row_ids((hashing, ops) in arb_case()) {
            let (disagreement, _) = replay(hashing, &ops);
            prop_assert!(disagreement.is_none(), "{}", disagreement.unwrap_or_default());
        }
    }

    /// **B15's non-vacuity guard**, read against its sentence: every hashing is
    /// drawn, and a sequence inserts enough distinct keys to grow the table past
    /// its first two sizes (16 slots hold 12, 32 hold 24), so growth re-places
    /// slots that collide and wrap.
    /// Floors at about two thirds of what was measured, of 400: AllOnes 118, ThreeBuckets 103, Spread 98.
    #[test]
    fn b15_generator_reaches_every_hashing_and_two_growths() {
        use proptest::strategy::ValueTree;
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let strategy = arb_case();
        let mut grown: HashMap<Hashing, usize> = HashMap::new();
        for _ in 0..400 {
            let (hashing, ops) = strategy
                .new_tree(&mut runner)
                .expect("strategy produces a value")
                .current();
            let (_, inserted) = replay(hashing, &ops);
            if inserted >= 25 {
                *grown.entry(hashing).or_default() += 1;
            }
        }
        eprintln!("sequences growing twice, by hashing: {grown:?}");
        for (hashing, floor) in [
            (Hashing::AllOnes, 78),
            (Hashing::ThreeBuckets, 68),
            (Hashing::Spread, 65),
        ] {
            let count = grown.get(&hashing).copied().unwrap_or(0);
            assert!(
                count >= floor,
                "{hashing:?}: {count} sequences grew twice (floor {floor})"
            );
        }
    }
}
