# Performance baseline — first measurements of the engine

Overflow for `ROADMAP.md` → *Performance* → **Profile the engine**, which is the
item these numbers exist to inform. Nothing here is a target or a regression
gate; it is the first time anything was measured, recorded so the profiling
session starts from data instead of from scratch.

**Measured 2026-07-27** at `c380435`, `cargo build --release` with no
`[profile.release]` overrides, rustc 1.96.1, AMD Ryzen 7 5700G (16 threads,
30 GB), machine otherwise idle. Wall clock and peak RSS from `/usr/bin/time`,
best of 2–3 runs; run-to-run spread was under 2%.

> An earlier pass measured several of these *while a DuckDB build was running*
> and came out 15–35% high. Those figures were corrected at source; if a number
> anywhere else disagrees with this file, this file is the later measurement.

## The workload

Source analysis of this crate: 27,957 facts across 15 JSONL tables describing
901 functions, 8,316 call sites and 397 `use` edges. The extractor, the fact
tables and the programs are **outside the repo** — they were session scratch,
preserved at `~/datalog-source-analysis-2026-07-27/`. That is the main obstacle
to turning this into a repeatable benchmark (see *What is missing*).

## Numbers

| workload | wall | peak RSS |
|---|---|---|
| import 27,957 facts, 15 JSONL files, no rules | 0.32 s | 76 MB |
| module graph: 397 edges → 120-tuple closure | 0.33 s | 78 MB |
| call graph: 5,248 edges → **173,177**-tuple closure | 11.6 s | 505 MB |
| the same, over 1,504 resolved edges (2,458-tuple closure) | 2.2 s | 134 MB |
| mutual recursion, anchored on direct edges | 18.7 s | 490 MB |
| mutual recursion, `reaches(A,B), reaches(B,A)` | **> 2 min, killed** | — |

The one prior datapoint, from the USDA dogfood (2026-07-23, unprofiled and not
re-measured here): `foundation × 170k-measurement` joins at ~11–20 s release.
The two agree that the engine handles ~10⁵ derived tuples in ~10 s.

## What the numbers say

- **Import is not the problem.** 28k facts through DuckDB in 0.32 s, and the
  process is 76 MB before any rule fires. Every second after that is evaluation.
  This closes the "import vs. eval split" question the ROADMAP item raised.
- **Derivation runs at roughly 15k tuples/s** (173k tuples, 11.6 s). That is the
  headline number to beat.
- **Memory is ~2.9 KB per derived pair** (505 MB / 173k), which is large for what
  is logically two strings. `ir::Value::String(String)` owns its bytes, so every
  tuple cell is a separate allocation and ids like
  `"engine::naive::apply_builtins"` are stored once per occurrence. **Value
  representation is the first thing to look at** — interning, or `Rc<str>`, is a
  hypothesis the profile should confirm or kill before anything else is tried.
- **The 35× cliff is a language property, not a constant factor.** Identical
  question, identical answer:

  | | wall | peak RSS |
  |---|---|---|
  | `import "lib/modgraph.dl".` | 0.33 s | 78 MB |
  | + `import "lib/callgraph.dl".` (unused by the query) | 11.5 s | 492 MB |

  Evaluation is bottom-up and materialises every derived relation whether or not
  the query touches it. A rule library that is merely *in scope* is paid for in
  full. This is the single most user-visible performance fact about the engine
  and it is not a bug — it is the absence of demand-driven evaluation
  (magic sets / SLG). Any real fix is a design item, not a tuning pass.
- **Self-joining a large derived relation does not finish.**
  `reaches(A,B), reaches(B,A)` over a 173k-tuple relation was killed at 2
  minutes; anchoring one side on the 5,248-tuple edge relation gives the same
  answer in 18.7 s. Join-order selection is doing nothing useful here — the
  planner has both relations' sizes available in principle.

## What is missing

Untested hypotheses, in the order they look worth testing:

1. **Provenance recording.** Every derivation records a `Derivation`; none of
   these workloads read it back. Its share of the 11.6 s is unknown and could be
   most of it. There is no flag to disable it, so measuring means a temporary
   build.
2. **Value representation** — see above. Cheap to test with a heap profiler.
3. **Join strategy and hashing** — nested-loop vs hash, and what the semi-naive
   delta actually saves. The naive oracle (`src/engine/naive.rs`) is a ready-made
   comparison and is already differentially tested against the real engine, so
   timing both on one program is nearly free.
4. **Where the 35× goes.** Is the unused closure's cost derivation, or the
   allocation churn of materialising it? These have different fixes.

To make this repeatable the fact base needs a home. Options, cheapest first: a
committed fixture of a few thousand rows (enough to exercise the shapes, small
enough to version); generating a synthetic graph in `testgen` and timing the
closure; or committing the extractor and regenerating from the crate itself,
which has the appeal of scaling with the codebase but makes the benchmark
non-deterministic across commits.

**Do not optimise from this file.** It says where the time goes at one scale on
one machine; it does not say why. The ROADMAP item's rule — profile before
changing anything — is what these numbers are for.
