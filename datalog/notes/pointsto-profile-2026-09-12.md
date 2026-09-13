# `pointsto.dl` profile — 2026-09-12

`code-analysis`'s Andersen points-to (`lib/pointsto.dl`) did not fit VS Code's
`vs/base` (1.33M facts): stopped above 14 GB at 130 s, after the memory work in
`memory-profile-2026-09-12.md` that halved `orient.dl`. It is profiled here as a
vehicle for the engine (`code-analysis/decisions.md` 2026-09-12 evening), so
nothing below is special to it.

## Method

- **A scaling series cut from `vs/base`**, since the full base does not finish:
  a file is kept when `sha1(path)` falls under a fraction, and every dataflow row
  follows the file its ids name (call-site-keyed rows follow the call site). Each
  cut is a superset of every smaller one. A cut drops the edges between files, so
  the series is a growth curve, not an oracle for the full answer.
- **The repository's `lib/`**, copied into each cut.
- **A per-round counter** in a scratch worktree, never committed: for each rule
  and delta position the candidate tuples tried, matches pushed and seconds; for
  each predicate each round the `pending` length, its distinct facts, the delta
  and the relation.
- **heaptrack** over the `profiling` build on the 0.35 cut. `perf` recorded no
  usable call chains; the counter's per-rule seconds stand in for it.
- Peak RSS from `/usr/bin/time`; the answer guard is a sha256 of stdout over
  `pts`, `heap` and `target`.

## The series, on trunk (`279b64a`)

| cut | `var` | `pts` | `heap` | `target` | time | peak RSS |
|---|---:|---:|---:|---:|---:|---:|
| 0.1 | 15,774 | 12,356 | 15,157 | 7,629 | 1.7 s | 158 MB |
| 0.2 | 41,662 | 78,629 | 250,642 | 19,406 | 6.6 s | 388 MB |
| 0.35 | 72,066 | 471,074 | 2,894,515 | 36,443 | 39 s | 2,668 MB |
| 0.5 | 91,745 | 693,181 | 5,611,155 | 47,306 | 72 s | 4,191 MB |
| 0.7 | 125,064 | — | — | — | aborted at 258 s | 11,305 MB |
| 1.0 | 179,748 | — | — | — | stopped at 130 s | > 14 GB |

The 0.7 cut aborted on `memory allocation of 4294967296 bytes failed` — a
`Vec` doubling to 4 GB.

**The answer is not what does not fit.** At 0.5 the three relations hold 6.3M
tuples; `heap`'s are an int, a field name and an int. 4.19 GB is ~660 bytes a
tuple, several times what a tuple and its B-tree slot cost.

## Where it went (0.35 cut)

**`pending` held a fact once per path that reached it.** Across the run `pts`
received **12.16M** pending entries for **471k** distinct facts — 26×. Round 8
alone pushed 7.4M `pts` entries for 832k distinct. Every entry is a 64-byte
`(Fact, Derivation)` plus its tuple's heap. One rule is the source: `pts(To, O2)
:- load(to: To, base: B, field: F), pts(B, O), heap(O, F, O2)` pushed 11.6M of
the 15.4M matches, since field-based heap cells merge every object that shares
a field name (`"[]"` is the field of 5,668 stores and 4,232 loads).

The earlier fix skipped a match whose fact the model *already* held; it could not
see a fact new this round reached along many paths.

heaptrack's **2.29 GB** heap peak, by the site that allocated it — which is not
the structure that holds it: a head tuple built in `on_match` lives on in the
relation.

| at peak | calls | site | holds |
|---:|---:|---|---|
| 675 MB | 27.1M | head `Tuple` built in `on_match` | relations, delta, `pending` |
| 652 MB | 275M | `String` clone in `insert_derived` | the relation's copy of a new fact |
| 537 MB | 5 | `pending`'s `Vec` growing | `pending` itself: 8.4M × 64 B |
| 152 MB | 76.5M | `Vec` clone in `insert_derived` / `enumerate_literal` | relation copy; premise clones |
| 134 MB | 1.0M | B-tree node split | relations, delta |
| 70 MB | 0.3M | `sources::table::finalize` | base facts |

So `pending`'s spine is ~0.54 GB of it, and the duplicate tuples it points at
more; **most of the rest is the answer's own representation** — every tuple a
boxed `Vec<Value>` of 32-byte cells, every string cell its own heap block,
cloned once more into the delta for the round it is new.

**Time is the join re-walking what a delta round cannot use** — 28 s of
evaluation, 157.6M candidate tuples tried:

| rule (delta position) | candidates | pushed | seconds |
|---|---:|---:|---:|
| `load` rule (`heap` delta, 3rd atom) | 45.9M | 11.6M | 14.0 |
| `this` of an instance (`subclass_or_self` delta) | 87.2M | 640 | 6.7 |
| `load` rule (`pts` delta, 2nd atom) | 3.0M | 20k | 1.4 |
| `store` rule, both `pts` positions | 6.6M | 3.2M | 2.2 |

- The `load` rule's `heap`-delta position enumerates `load × pts(B, O)` in full
  every round before it reaches the delta — 1.1M candidates in each of the last
  twelve rounds, which pushed almost nothing. Semi-naive's positions before the
  delta are Full, and the scheduler runs atoms in source order.
- `var(id: T, fn: C, kind: this)` binds `fn`, not the leading `id`, so each of
  the few delta rounds scans all of `var` — ROADMAP *a bound column that is not
  leading still scans*.

## Stage 1: a round's unkept facts are a set

A match nothing is kept of now inserts its fact into a per-round
`BTreeSet` — which is the round's delta — instead of pushing one `pending`
entry per path. Answers identical at every cut that finishes. Timings were taken
beside other measurements, so indicative; peak RSS is not affected by that.

| cut | trunk | stage 1 |
|---|---:|---:|
| 0.35 | 39 s / 2,668 MB | 43 s / **2,287 MB** |
| 0.5 | 72 s / 4,191 MB | 68 s / 4,154 MB |
| 0.7 | aborted: a 4 GB `Vec` growth, at 11.3 GB | past a 16 GB cap at 408 s |
| 1.0 | past 14 GB at 130 s | past a 16 GB cap |

**Those runs print the three relations whole, and printing is now the peak.**
heaptrack on stage 1 moves the peak to the end of the run: at 0.5, of 3.56 GB,
~1.9 GB is `api::answer_lines` — two 539 MB copies of the 6.35M answer rows,
519 MB of line strings growing, 336 MB of `Vec` growth. So the same closure,
asked `pts("nonexistent", O)` — every rule still runs, nothing is printed:

| cut | trunk | stage 1 |
|---|---:|---:|
| 0.35 | 38 s / 2,662 MB | 34 s / **978 MB** |
| 0.5 | 68 s / 3,977 MB | 62 s / **1,551 MB** |
| 0.7 | — (printing, aborted at 11.3 GB) | 297 s / **6,779 MB** |
| 1.0 | past 14 GB | aborted at a 15 GB cap, 223 s |

The full base still does not fit. The growth from 0.5 to 0.7 is 4.4×
in memory for 1.36× the variables, so the closure's own size is what remains.

Re-timed alone, twice each, at 0.35: trunk 35.5 s / 2.67 GB, stage 1 32.3 s /
0.97 GB. **Evaluation's peak falls 2.6×, and time 9%.** What a run that prints a large answer still
pays is the answer copied into rows, then into facts, then into lines, all
held before the first line is written.

## Ranking

1. **Hold an unkept round's facts as a set** — cheap, internal; shipped as
   stage 1. Necessary — the 0.7 cut died on `pending`'s growth — not sufficient.
2. **Drive a delta round from its delta atom** — the time. Reordering is
   observable on the error path (ROADMAP *Seeking makes body order matter
   more*), so it is a design decision, not a patch.
3. **A non-leading bound column** — already queued; 6.7 s of 28 here.
4. **The per-candidate `Premise::Fact` clone** — `Vec` and `String` clones plus
   the allocator are ~10% of samples; temporary, so time, not peak.
5. **Stream a query's answer** (queued, next session) — after stage 1 and the
   two printing changes, 3.40M printed rows still hold `Model::answer`'s owned
   rows (278 MB) and `RunResult.answers`' lines (289 MB) at once, plus ~250 MB of
   their `Vec`s: ~820 MB of a 1.69 GB heap peak at 0.35.

## On Grafana

`code-analysis`'s bench, trunk `279b64a` against `69aac3f` per library, back to
back, on the repository's `lib/`: **all 23 digests identical.** `@grafana/ui`'s
`pointsto.dl` answers in 9.0 → 8.9 s / 397 → 373 MB — a closure that small is
not where this bites. On the frontend (no dataflow layer, so no `pointsto`) the
unkept set shows up in **time** on the join-heavy libraries, from single runs:
`coupling.dl` 290 → 171 s, `orient.dl` 78 → 62 s, `modgraph.dl` 75 → 61 s,
`coupling_kinds.dl` 148 → 120 s, `cohesion.dl` 208 → 178 s; peaks 2–10% lower.
Table: `../../code-analysis/notes/code-facts.md` § Re-measured after a round held
each fact once.
