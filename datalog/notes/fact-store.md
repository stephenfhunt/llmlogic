# A fact store with one owner — design

*Drafted 2026-09-13 for review; not decided. Prompted by the reverted shared
tuples (`recorder-at-scale.md` § Shared tuples, measured; §17 2026-09-13 (later
iii), ***Falsified***). Nothing here is built. The questions at the end are the
ones the review settles before a prototype.*

## The defect this answers

No part of the engine owns a fact. A tuple is made wherever it is first needed,
and whichever allocation survives becomes the fact:

| a fact is born | as | and ends up |
|---|---|---|
| an imported row | a `Vec<Value>` per row, built in `sources/table.rs` | moved through `Program.facts` into a relation |
| a program fact | a `Vec<Value>` in `lower.rs` | the same |
| a derived fact | a `Vec<Value>` grounded mid-join (`collect_rule_matches`) | cloned into the delta and again into the relation at apply |

Once held, the same fact is also copied into the round's delta and pending set,
into every recorded premise, into the derivation store's key, into its round
stamp's key, and out again through `Model::facts()`.

Copying hides the missing owner. Sharing exposed it: every holder became a
co-owner, and the allocation that lived was the one nobody had placed. The shared
build copied 508 K imported rows and freed their buffers. Leaking them instead
took `pointsto.dl` from 12.7 s to 6.55 s.

## The principle

**Each relation owns its facts, a fact is written into its relation exactly once,
and everything else refers back to it.**

- **Owner:** the relation's store. Append-only; a fact never moves and is never
  freed before the model.
- **Reference:** a `FactRef { pred: PredId, row: u32 }`, the fact's position in its
  relation's store. Every holder other than the store keeps a reference, not
  values.
- **Identity, never order.** A row number is an identity. Whatever the user can
  see is still ordered by content, read through the store.

## Shape

### The store

One flat `Vec<Value>` per relation, `arity` values wide per row. `row r` is
`values[r * arity .. (r + 1) * arity]`.
- Arity is fixed per predicate: an atom is always at full arity, and lowering
  rejects a predicate used at two arities (`seek.rs`'s arity invariant).
- A fact costs its values and nothing else: no per-tuple allocation, no header,
  and rows sit contiguously in the order they were written.
- Arity is at least 1: a predicate with no arguments is a syntax error in v1, so
  the stride is never zero.

### Births

- **Base facts** are appended before the first round, sorted and deduplicated
  first: a fact asserted twice is one fact, which the set does for free today.
  Imports should be written straight into the relation's store at load, never as
  a `Vec` per row (open question 3).
- **Derived facts** are appended when their round is applied. The round's new
  facts are sorted and deduplicated first, so each round appends one contiguous,
  sorted block.
- **Heads under collection are scratch.** A match's head is not a fact until
  apply. Pending heads live in a per-round scratch buffer, flat like the store,
  and are freed as one block when the round is applied.

### The index: sorted runs of row numbers

A seek, a membership test and canonical iteration all need content order, which
a std `BTreeSet` can only give by owning its keys. So the relation keeps its
order separately, as sorted runs of row numbers:

- **One run per applied round**, already sorted, since each round appends a
  sorted block. Base facts are sorted once at load and are the first run.
- **Seek:** a lower-bound binary search on each run, then read forward while the
  prefix matches. Candidates from several runs are merged if order matters
  (it does not for the join; `try_match` is order-free).
- **Membership** (`contains`, a head already held): binary search per run.
- **Canonical iteration** (`relation()`, `facts()`, and the lazy answer printer,
  whose order an `api.rs` test pins): a k-way merge of runs.
- **Merging runs:** merge equal-ish sizes geometrically, so a relation holds
  O(log n) runs. The newest run stays unmerged until the next apply, because it is
  the delta. Merging reorders row numbers inside an index and never moves a fact.

The semi-naive views stop being sets:

| view | today | with rows |
|---|---|---|
| `Delta` | a `BTreeSet<Tuple>` of copies | rows `≥` the last round's first row, which is the newest run |
| `Old` | full relation, filtered by `!delta.contains(tuple)` (a content lookup per candidate) | full relation, filtered by `row <` that first row (an integer comparison) |
| `Full` | the relation | every run |

`Delta` being exactly "rows written by the last apply" rests on batched
application, the invariant E1 already guards.

**Alternatives weighed:**
- **A hand-written B-tree over row numbers**, comparing through the store:
  cheaper seeks, no merges. It costs a data structure the zero-dependency core
  would write and prove from scratch. Worth prototyping against the runs if runs
  lose on seek-heavy programs.
- **An index that owns the tuples** (`BTreeMap<Tuple, u32>`) and a store of
  nothing. This is today's owner under another name: the store cannot hand out
  references into a tree that moves its keys. Rejected, for the reason sharing
  failed.

### What refers back

| holder | today | with rows |
|---|---|---|
| `Premise::Fact` | a `Fact` (a copy of the tuple) | a `FactRef`; the premise shrinks from 32 to 16 bytes |
| the derivation store | `HashMap<Fact, BTreeSet<Derivation>>` | per relation, keyed by row (base rows too: a rediscovered base fact records derivations) |
| round stamps | `HashMap<Fact, u32>` | a `Vec<u32>` per relation, indexed by row minus the base count |
| base membership | no stamp | `row < base_count`, which is E11 restated |
| delta and old views | copies | a row watermark |
| the `?whynot` cross case | re-evaluates from a copy of `Program.facts` | re-evaluates from a copy of the base block (open question 5) |

**Proof selection is unchanged in meaning.** `ProofTree::step` picks the
`Ord`-least well-founded derivation, comparing premises by content read through
the store, never by row number. The guards are the existing ones: `?why` output
byte-identical to `7e3f9ef` on the corpus goals and both `@grafana/ui` goals, and
B5 and B13.

### The boundary

- `Fact` stays the owned value type at the API: goals, `Repair`, `ProofTree`
  nodes, and what `facts()` yields.
- `Model::relation(pred)` stops returning `&BTreeSet<Tuple>` and yields `&[Value]`
  rows in canonical order. Nothing outside `datalog/` links the library; its
  in-crate callers are the answer printer (`api.rs`), `repair_for`, and tests.
- The naive oracle keeps its own `BTreeSet<Fact>`. B1 stays an independent
  oracle, and must not come to share the store.

## Beyond this design

- **Values inside a fact are still owned by value.** A binding clones a symbol's
  `String` out of the store. `pointsto.dl` makes 85 M `String::clone`
  allocations at `7e3f9ef`, the largest site by count. 60 M remained in the shared
  build, whose premises no longer copied. The same principle says bindings should refer
  back too. That is interning (ROADMAP, post-v1), and it layers on this store.
- **One derivation per fact** (`recorder-at-scale.md`, question 1) is unaffected.
  With row-keyed stores it becomes a `Vec` indexed by row.

## What the tests become

- **Equivalence claims, already properties:** B1 (naive oracle), B5, B12 (the seek
  is the scan, restated over runs), B13, E1–E3, E6, E9, E11, and E7/E8 with the
  §16.6 goldens.
- **New properties the store needs:**
  - *a row never moves:* a `FactRef` taken in any round resolves to the same values
    in the finished model;
  - *the index is the relation:* every run is sorted, the runs together are a
    permutation of the rows, and a merge of them iterates exactly the sorted set of
    held facts;
  - *views:* `Delta` is the facts first held in the last round and `Old` the rest,
    stated from round stamps rather than watermarks.
- **Each ships with a mutation and a non-vacuity guard** (`testing.md`'s rules).
  The generator must reach a relation with several runs and a merge.

## Measurement gate for the prototype

In the scratch worktree, against a frozen `7e3f9ef` release binary:

| program | today | must hold |
|---|---|---|
| `pointsto.dl`, no goals | 8.4 s / 361 MB | no slower |
| `pointsto.dl` `?why` | 9.2 s / 836 MB | no slower; memory reported |
| `callreach.dl` `?why` | 1.44 s / 389 MB | no slower; memory reported |
| `sparse_800`, no goals | 1.83 s | no slower |
| `Deep` idx 119 / 129, recorded | 2.78 / 5.43 GB peak live | reported |

Also reported: rounds per stratum and runs per relation, which price the merge
policy. `perf stat` goes beside every time (instructions, cycles, cache misses),
since the shared-tuple failure hid behind equal instruction counts.

## Questions for the review

1. **Runs, or a B-tree over row numbers?** The recommendation is runs first. They
   fit batched rounds and give the views for free. The B-tree is the prototype's
   fallback if seeks lose.
2. **Membership:** binary search per run, or a hash from content to row beside the
   runs? The hash buys O(1) head checks and costs memory per fact.
3. **Imports:** should the loader write rows straight into a store that `eval`
   adopts? That removes the per-row `Vec` entirely, but changes `LoadedTable` and
   how `Program` carries base facts (lowering and typechecking read them).
4. **Program facts in the IR:** keep `Program.facts: Vec<Fact>` for text facts and
   route imports around it, or give the IR per-relation blocks too?
5. **The `?whynot` cross case:** copy the base block (flat, cheap), or keep the
   first model's store alive for the second evaluation?
6. **Scope of the first prototype:** the store, the index and the views, with
   premises still copying? That isolates the storage change before the provenance
   change, and shared tuples conflated the two.
