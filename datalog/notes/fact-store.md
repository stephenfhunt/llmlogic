# A fact store with one owner — design

*Designed 2026-09-13 and reviewed the same day (§17 2026-09-13 (later iv)); the
answers are at the end. Building on branch `fact-store`. Prompted by the reverted
shared tuples (`recorder-at-scale.md` § Shared tuples, measured; §17 2026-09-13
(later iii), ***Falsified***).*

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
    in the finished model. **Built in step 2** as **B14c** over evaluations, and
    within a relation's history in **B14a**;
  - *the index is the relation:* every run is sorted, the runs together are a
    permutation of the rows, and a merge of them iterates exactly the sorted set of
    held facts. **Built in step 2** as `testing.md` **B14a**.
  - *views:* `Delta` is the facts first held in the last round and `Old` the rest,
    stated from round stamps rather than watermarks. **Built in step 1** as
    `testing.md` **B14b**, with **B14a** stating the index and views against a
    ledger.
- **Each ships with a mutation and a non-vacuity guard** (`testing.md`'s rules).
  The generator must reach a relation with several runs and a merge.

## Gate

Every step passes all of this before the next one starts. The tooling lives
outside the repo, in `~/.cache/fact-store/`. The baseline is a frozen `efcda71`
release binary, which is code-identical to `7e3f9ef`; `~/.cache/recorder-wt` is
its worktree.

- **Per commit:**
  - `cargo test`;
  - `cargo test --no-default-features`;
  - `cargo clippy --all-targets`;
  - `cargo fmt --check`.
- **Output identical to the baseline.** `harness/diff.py run BIN OUT`, then
  `diff -r` against the baseline's run. Each case records stdout, stderr and the
  exit code, over:
  - every `tests/programs` file;
  - every `@grafana/ui` library and query file, asked for every relation it
    defines by rule, plus both `?why` goals;
  - the 26 cross-engine programs;
  - a fixed sample of generated programs at `Medium`, `Large`, `Deep` and shaped
    sizes.

  The corpus and generated programs also get up to four `?why` goals each. They
  also get two `?whynot` goals: a crossover of two answers that does not hold (the
  failure trace) and one that does (the cross case). Every goal is fixed once, from
  the baseline's answers. The baseline diffed against itself must also be empty, or
  the oracle is flaky.
- **The deep run,** `harness/deep.sh`: `DATALOG_PBT=deep cargo test --lib` under
  `prlimit --as` (16 GB). It runs at `PROPTEST_RNG_SEED` 2 and 3, which draw the
  baseline's cases while the generators are unchanged. It skips
  `b13_pruning_changes_no_live_relation_at_large` (`bugs/015`).
  - **Baseline:** seeds 2 and 3 pass all 509 tests in 159 s and 130 s, peaking
    under 1 GB.
  - **Seeds 1 and random are not in the gate.** At `Deep`,
    `b5_body_order_is_irrelevant_at_large` draws a pathological program. Seed 1
    passes, but takes 6 hours. The random seed was stopped after 72 minutes in the
    same test, and a random seed could not pair a baseline with a branch anyway.
    The per-commit `cargo test` draws fresh cases on every run.
  - **Not chased, the user's call:** fix buggy behaviour on reasonable input, and
    leave deliberately exponential generated input alone.
- **New properties** are mutation-verified, and each mutation is written on its
  catalog line in `testing.md`.
- **Performance,** `harness/measure.py BASELINE BRANCH`. The binaries are
  interleaved run by run and the baseline is re-measured in the same sitting; this
  table's figures are history, not the bar:

| program | `7e3f9ef`, measured 2026-09-13 | must hold |
|---|---|---|
| `pointsto.dl`, no goals | 8.4 s / 361 MB | no slower |
| `pointsto.dl` `?why` | 9.2 s / 836 MB | no slower; memory reported |
| `callreach.dl` `?why` | 1.44 s / 389 MB | no slower; memory reported |
| `sparse_800`, no goals | 1.83 s | no slower |
| `Deep` idx 119 / 129, recorded | 2.78 / 5.43 GB peak live | reported |

Also reported: rounds per stratum and runs per relation, which price the merge
policy. `perf stat` goes beside every time (instructions, cycles, cache misses),
since the shared-tuple failure hid behind equal instruction counts.

## Step 2, measured (2026-09-14): correct, and 30% slower

`d9a4c5e`, the flat store and sorted runs, passes every correctness gate. The
harness diff is empty over 1,109 cases, and deep seeds 2 and 3 pass 514 of 514.
It fails the performance gate. Release CLI, medians of 3, runs interleaved with
the `efcda71` baseline:

| program | `efcda71` | `d9a4c5e` | cycles | cache misses | RSS |
|---|---|---|---|---|---|
| `pointsto.dl` | 10.54 s | 13.75 s | 46.4 → 61.1 G | 226 → 305 M | 520 → 560 MB |
| `pointsto.dl` `?why` | 9.13 s | 9.46 s | 40.2 → 41.7 G | 199 → 143 M | 828 → 790 MB |
| `callreach.dl` `?why` | 1.45 s | 1.41 s | 5.9 → 5.8 G | 34 → 32 M | 382 → 375 MB |
| `sparse_800` | 1.26 s | 1.62 s | 5.4 → 7.3 G | 14 → 19 M | 126 → 95 MB |

Instructions are flat or lower (`sparse_800` 12.8 → 11.0 G) while cycles rise: the
cost is memory access, the shape the shared tuples had.

- **Profiles.** `sparse_800` spends 31% in `Relation::contains`, plus most of 40%
  in `memcmp` under it: a head check binary-searches up to 12 runs through the
  store. `pointsto.dl` gains `partition_point` (5%), `Value::partial_cmp` (7.5%)
  and glibc's `unlink_chunk` (2% → 7%).
- **Runs are few.** `pointsto.dl` has 44 of 54 relations in one run and 9 at most.
  `callreach.dl` has 4 on `reaches`, and `sparse_800` 12 on `path` (637,603 rows;
  the bound allows 21).
- **Experiments**, each a scratch build of `d9a4c5e`:

  | variant | `pointsto.dl` | `sparse_800` | reading |
  |---|---|---|---|
  | E1: seek cursors on the stack, no per-seek `Vec` | 13.90 s | 1.63 s | not it |
  | E2: E1 with `Ord::cmp` for slice `<` | 13.86 s | 1.62 s | not it |
  | E3: E2 with all older runs merged at every apply (≤ 2 runs) | 13.36 s | 1.84 s | merging costs more than the runs it saves |
  | E4: E2 with each written row's emptied buffer leaked | **11.94 s**, misses 202 M | 1.52 s, 137 MB | **freed row buffers are most of `pointsto.dl`'s cost** |

**What E4 seemed to say, and what it did.** E4's leak sat in `append`, which
wrote base rows *and* each round's block, so it could not say which freed buffers
cost the time. Three follow-ups separate them. Each row below is measured against
`efcda71` in the same sitting; `pointsto.dl` has no goals.

| change | `pointsto.dl` | `sparse_800` | reading |
|---|---|---|---|
| `5f27f41`: imports flat end to end, never a `Vec` per row | 13.54 s, 479 MB | 1.62 s | memory −79 MB, time unchanged |
| E5: `5f27f41` with derived rows' buffers leaked | 13.37 s | 1.53 s | derived rows are not `pointsto.dl`'s cost |
| E6: `5f27f41` with the loader's *raw* row buffers leaked | **12.09 s**, 778 MB, misses 214 M | 1.61 s | **the holes that matter are the loader's** |

**What it says now.** Two costs remain, and they are separate.
- **Load-time holes (`pointsto.dl`, about 1.5 s).** `RawTable` holds one
  `Vec<RawValue>` per row, which `finalize` consumes and `arrange`'s reorder
  replaces. Both free a buffer per row among live data. In `efcda71` and step 1
  the long-lived value rows filled those holes. With flat rows nothing does, so
  evaluation's allocations land in them. The fix keeps the direction the user
  chose: the raw table flat too, so the import path allocates no row on its own.
- **Reading rows through runs** (the rest of `pointsto.dl`, and all of
  `sparse_800`'s 0.37 s). Membership and seeks binary-search each run through the
  store. This is review answer 2's condition for a content-to-row hash, taken up
  after the load-time holes are gone and re-measured.

## Rules the build must keep

- **Delta is the rows this stratum's previous apply wrote, not the newest run.** A
  relation last written in an earlier round, or by a lower stratum, has an empty
  delta. So each relation records which round wrote its newest block. Guard: the
  views property, and B1 at `Medium`/`Large`.
- **`Old` is `Full` below the delta's first row.** This holds only because nothing
  is written while a round collects (E1).
- **Content order everywhere a consumer can see it.** Seeks, `relation()`,
  `facts()` and `repair_for` merge their runs in content order, as the `BTreeSet`
  gave it. The consumers that can observe order include:
  - which near-miss `trace_failure` reports;
  - which error pruning reaches first;
  - the scan printer, which drops a repeated row by comparing it with the row
    before (`api.rs`, D6).

  A seek for the join alone that skips the merge is a later optimisation, measured
  separately, after an audit shows its consumer ignores order.
- **No `unsafe`.**

## Review answers (2026-09-13, the user's; §17 2026-09-13 (later iv))

1. **Runs.** A B-tree over rows only if seeks lose at the gate.
2. **Membership is a binary search on each run.** A hash from content to row only
   if head checks lose at the gate.
3. **Imports stay `LoadedTable` → `Program.facts`**, sorted into the store when
   evaluation starts. Loading straight into the store waits until the gate shows
   the per-row `Vec` still costs.
4. **`Program.facts: Vec<Fact>` stays**, for text facts and imports alike.
5. **`?whynot`'s cross case copies the base block.**
6. **Staged, storage first, with `bugs/015`'s skips named in the gate:**
   1. a `Relation` type with no behaviour change, and the properties stated
      against it;
   2. the flat store, runs and watermark views, with premises still copied;
   3. provenance by `FactRef`;
   4. then the cross case, and imports if the gate calls for them.

   Each step is a series of green commits on branch `fact-store`.
