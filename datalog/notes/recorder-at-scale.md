# The recorder at scale — what a `Provenance::Recorded` run holds, and why

*Measured 2026-09-13, in a scratch worktree (`~/.cache/recorder-wt`, not
committed), to frame a design session. Prompted by `bugs/015`. B13 at `Deep`
passed 14.9 GB. Its proof clause, compared one step per live fact with one
recorded store alive at a time, still reached 19.5 GB on one draw. Nothing here is
decided; the questions at the end are the session's agenda.*

## Method

- **Instrumentation, scratch only:**
  - a counting global allocator (live and peak bytes, reset per run);
  - a counter of every rule-body match the join enumerates, which counts in
    unrecorded runs too;
  - `Model::store_stats`, which counts derivations per fact, first-round versus
    later, premises by kind, and bytes by component (analytic);
  - drop accounting: empty each provenance map after the run and read the fall in
    live bytes (exact).
- **Programs:**
  - a fixed sample of 192 `arb_program_text_at(Tier::Deep)` programs, scanned
    unrecorded and ranked by match count; the median, idx 18, 17, 16, and the two
    outliers 119 and 129 recorded, each in its own process;
  - `sparse_400`/`sparse_800`, rebuilt from `cross-engine-benchmark.md`'s
    description (a random graph with 2n edges and a path closure; seed 42). The
    original generator was never committed;
  - `@grafana/ui` (cached base): `callreach.dl` `?why reaches(…)` and
    `pointsto.dl` `?why pts("$thrown", 364)`.
- **Caps:** `prlimit --as`. This machine's user cgroups delegate only `pids`, so
  `systemd-run -p MemoryMax` is silently not enforced.

## Findings

| program | facts (base / derived) | derivations | per derived fact | first-round share | peak live: unrecorded → recorded | ratio |
|---|---|---|---|---|---|---|
| `callreach.dl` on `@grafana/ui` | 41k / 150k | 168,793 | 1.12 | 97% | 70 MB → 387 MB | 5.5× |
| `pointsto.dl` on `@grafana/ui` | 508k / 122k | 134,778 | 1.11 | 95% | 266 MB → 1,189 MB | 4.5× |
| sparse_400 | 0.8k / 100k | 198,347 | 1.97 | 55% | 12 MB → 138 MB | 11.6× |
| sparse_800 | 1.6k / 413k | 818,493 | 1.98 | 55% | 48 MB → 567 MB | 11.9× |
| `Deep` median (idx 110) | 132 / 61 | 92 | 1.5 | 59% | 0.03 → 0.15 MB | — |
| `Deep` idx 18 | 79 / 205 | 99,760 | 487 | 2.2% | 0.05 MB → 76 MB | ~1,600× |
| `Deep` idx 17 | 145 / 335 | 726,259 | 2,168 | 4.6% | 0.07 MB → 573 MB | ~7,700× |
| `Deep` idx 16 | 128 / 517 | 936,732 | 1,812 | 7.2% | 0.14 MB → 705 MB | ~5,200× |
| `Deep` idx 119 | 61 / 439 | 4,329,178 | 9,861 (one fact: 1.4 M) | 0.7% | 0.08 MB → 3.58 GB | ~46,000× |
| `Deep` idx 129 | 34 / 2,257 | 8,208,336 | 3,637 | 5.7% | 0.4 MB → 6.96 GB (RSS 10 GB) | ~17,000× |

Byte figures are the allocator's live bytes; RSS runs 1.2–1.4× higher. Every
match is stored: the derivation count equals the match count in every run.

**Two regimes:**
1. **Real library programs** (`callreach`, `pointsto`) store about one derivation
   per derived fact, and 95–97% of those are first-round. The recorder costs
   4.5–5.5×, and none of it is count. It is representation, and bookkeeping for
   base facts: see the breakdown.
2. **Dense generated programs** (the `Deep` outliers) store hundreds to thousands
   of derivations per fact. 93–99% are later-round rediscoveries that no printed
   proof can use (below), so count dominates: 1,600–46,000×. The median `Deep`
   program is trivial, and a few draws in a few hundred are like this. B13
   records every case twice, so a deep run meets one.

**What each derivation costs.** Across all runs, 630–850 bytes. On idx 129, 93%
of the store is premise content:
- `Premise` is 80 bytes, sized for its `Builtin` variant rather than a fact
  reference;
- each `Premise::Fact` clones its tuple (32 bytes per column);
- each `Value::Symbol` clone allocates its string.

These are copies of facts the model already holds.

**Exact breakdown by map** (drop accounting):

Live bytes at the end of a recorded run, measured by emptying each map in turn:

| program | `derivations` | `first_round` | `base` | relations | rest of the run |
|---|---|---|---|---|---|
| `callreach.dl` on `@grafana/ui` | 190 MB | 68 MB | 29 MB | 66 MB | 29 MB |
| `pointsto.dl` on `@grafana/ui` | 176 MB | 270 MB | 243 MB | 257 MB | 233 MB |
| sparse_800 | 460 MB | 51 MB | 0.2 MB | 46 MB | 1.3 MB |
| `Deep` idx 129 | 6,678 MB | 0.4 MB | 0.005 MB | 0.35 MB | 9.6 MB (the harness's 192-program sample) |

- **The three provenance maps are the recorder.** The unrecorded peaks (70 MB,
  266 MB, 48 MB) are the relations alone.
- **Derivations cost 560–1,130 bytes each** once map overhead is included — 814 on idx 129, where the map is 99.8% of the run.
- **On a real import-heavy program the recorder is mostly base-fact
  bookkeeping.** On `pointsto.dl`, `first_round` plus `base` is 513 of its
  690 MB, against 176 MB for the derivations themselves.
- **A `?why` run holds every base fact four times.** Recorded `insert_base` clones
  the tuple into its relation and the fact into both `first_round` and `base`.
  Separately, `api::run` evaluates a program with explanations by
  `eval_pruned(&program, …)`, which copies the facts rather than moving them,
  because the `?whynot` cross case may evaluate the program again. That copy is
  the 233 MB rest on `pointsto.dl`. So a `?why` pays for base facts it never
  derives, four times over, before a single derivation is stored.

## Two facts established from the code

1. **The printed proof uses only derivations found in the round its fact first
   appeared.** `ProofTree::explain` picks the `Ord`-least derivation whose fact
   premises all first appeared in a strictly earlier round. Semi-naive evaluation
   finds an instance in the round after its newest premise arrived. An instance
   whose premises all predate the fact's first round F is found by F, and not
   before F, or the fact would exist earlier. Any instance found after F has a
   premise from F or later, so it fails the test. All of a fact's rules share a
   stratum, because strata are assigned by head predicate (`lower.rs`
   `stratify`). **So the printed derivation can be fixed when the fact is
   established,** provided the choice is the `Ord`-least of that round's
   candidates and not the first found (collection order depends on scheduling and
   rule order; see B5 and B13).
2. **What reads the store today:**
   - `ProofTree::explain`: one derivation per fact, plus `is_base` and
     `first_round`;
   - `api::absent_skip_warnings`: every derivation that *reports* (a skipped
     absent, a lost conversion), deduplicated by instance. `Provenance::Reports`
     already isolates exactly these;
   - tests E1–E3, E9, the Reports-mode tests, and `Model::derivations_of`, which
     is public;
   - nothing renders alternative proofs. §17 2026-07-19 kept all derivations for
     that future and for the parked semiring research.

## Directions raised, not decided

- **One derivation per derived fact, chosen when it is established.**
  - Removes the count term: idx 129 goes from 8.2 M stored to 2,257.
  - Printed proofs are unchanged by fact 1.
  - Retires `first_round`, which exists only to make the choice later.
  - `base` does not need this direction: it is retired already (§ After the first
    two cuts). A held fact with no round stamp is base.
  - **What becomes wrong:** warning counts would under-report unless the
    reporting store is kept alongside; E3's coverage narrows to one derivation
    per fact; `derivations_of` changes meaning; §11's "one fact, many proofs"
    becomes on-demand. Alternatives could be re-derived for one fact by re-solving
    its rules with the head bound against the final model, as `?whynot` does.
- **Premises as references, dancing-links style.**
  - Removes the per-derivation constant.
  - Cheap form: box the rare `Premise` variants (80 → 32-byte slots). Built; see
    § After the first two cuts.
  - Structural form: stable per-fact ids in an append-only store per relation, so
    a premise is `(PredId, u32)`. The chosen derivations then *are* the proof
    graph `explain` walks.
  - Cost: relations are `BTreeSet<Tuple>` read by the joins and the prefix seek,
    so stable ids reach into evaluator storage.
- **Adjacent:** symbol interning. Every symbol clone allocates, in relations too.

## After the first two cuts

*Measured later on 2026-09-13.* Two cuts that keep every derivation were built
before the count question, because the library programs do not pay for count:

- `91b730a` retires `base` and base facts' entries in `first_round`. A held fact
  with no round stamp is base (`testing.md` **E11**).
- `ef82398` makes a premise a fact wide: every kind but `Fact` is boxed, `NoMatch`
  included, since two inline variants of the same shape leave no niche for the
  tag (40 bytes otherwise).

`?why` output is byte-identical to `8c6bc91` on both `@grafana/ui` goals and on
43 goals over the §16 corpus.

| program | before | after `91b730a` | after `ef82398` |
|---|---|---|---|
| `callreach.dl` `?why`, release CLI RSS | 466 MB | 401 MB | 389 MB |
| `pointsto.dl` `?why`, release CLI RSS | 1,428 MB / 14.7 s | 854 MB / 9.5 s | 848 MB / 9.1 s |
| `Deep` idx 18, peak live | 76 MB | — | 57 MB |
| `Deep` idx 17 | 573 MB | — | 422 MB |
| `Deep` idx 16 | 705 MB | — | 562 MB |
| `Deep` idx 119 | 3.58 GB | — | 2.78 GB |
| `Deep` idx 129 | 6.96 GB (RSS 10 GB) | — | 5.43 GB (RSS 8.5 GB) |

The `Deep` rows use this note's harness, rebased onto `ef82398` in the same
scratch worktree.

- **The base-fact cut is the library programs' win**: `pointsto.dl` is 41% smaller
  and 38% faster.
- **The premise cut is the dense programs'**, 20–26% each. On idx 129 a
  derivation now costs 627 bytes rather than 814. What is left of its store is
  premise *content*: 3.06 GB of cloned tuples and 596 MB of cloned symbol
  strings, against 1.02 GB of premise slots. The copies are the remaining
  constant, so fact references are what would move it next.
- **The deep run still does not finish.** On `ef82398`, under
  `prlimit --as=20000000000`, it failed an allocation at 372 s and 18.1 GB peak
  RSS. A smaller constant moves the tail and does not bound it, so `bugs/015`
  still waits on question 1 below.

## Fact references: shared tuples, not ids

*Designed later on 2026-09-13* (§17 2026-09-13 (later iii)). The direction above
said stable per-fact ids. Asked why a premise could not simply be a `&`, the
answer splits in two.

**Where a `&` cannot be.** A stored premise outlives the round that found it.
- The store and the relations are fields of one `Model`, so a borrow from one
  into the other is a self-referential struct.
- Every later round inserts into the relations the premises point into.
- A `BTreeSet` keeps keys inline in its nodes and moves them on a split, so even
  a raw pointer to a `Tuple` dangles. Only a `Vec`'s heap buffer stays put, and
  leaning on that takes `unsafe` that `Model: Clone` breaks.

**Where a `&` is exactly right.** Application is batched: a round collects every
match against the model as of the previous round, and only then applies them
(`eval_stratum`, E1). So during collection a premise can borrow the relation's
tuple. Before this, every matched literal cloned its tuple, in every run,
unrecorded ones included.

**What outlives a round shares ownership.** `Tuple` is `Rc<[Value]>`. A clone is
a count, so the relation, the delta, a pending head, a kept premise, the store's
fact keys and `api::run`'s copy of the program's facts all hold one allocation.
`Derivation` compares contents as before, so the printed proof cannot change.

| | shared tuples | stable ids |
|---|---|---|
| premise | 32 bytes: a 24-byte fact and a tag, which finds no spare bits | 16 bytes |
| held fact | +8 bytes (fat pointer 16, count header 16, against a `Vec` header 24) | +4 bytes |
| copies left | none | delta, pending heads, `facts()`, `api::run`'s fact copy |
| proof order | contents, by construction | an id order that B5, B13 and §16.6 must keep out |
| readers | unchanged | an id-to-tuple resolver in `explain`, printing, the trace, the tests |
| old view | a set lookup | an id comparison |
| `Send` | no (`Arc` if parallelism lands) | yes |

The one-derivation-per-fact question is untouched: this removes the constant,
not the count.

## Questions for the design session

1. Is "all derivations" a capability to keep stored, or to recover on demand?
   (§11, §17 2026-07-19 and its amendments.)
2. Warnings in a recorded run: keep the reporting store, or count at match time?
   Counting at match time needs proof that no instance is enumerated twice;
   today's deduplication assumes it can be.
3. How far do references go: a `Premise` layout fix, or stable fact ids in
   evaluator storage?
4. Which E-properties are about the store, and which about proofs and warnings?
   What replaces E3's all-instance replay?
5. Sequencing against `bugs/015`: B13 at `Deep` records with no gap once the
   count term is gone. Until then, 015 stays open; the step comparison is ready
   and independent of the decision.
