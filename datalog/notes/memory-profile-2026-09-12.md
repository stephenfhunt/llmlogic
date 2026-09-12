# Memory profile — 2026-09-12

`code-analysis` on Grafana's frontend (4.04M facts) is the first workload here
where the binding constraint is **resident memory**, not time: `orient.dl` — the
playbook's step 3 — peaked at 9.5 GB, and five libraries at 4–9 GB. The
2026-08-20 profile (`profile-2026-08-20.md`) was a CPU profile over corpus
programs and says nothing about what a run holds. This one does.

## Method

- **heaptrack 1.5.0** over a `profiling` cargo profile (release, plus line
  tables; `Cargo.toml`), so stacks name source lines without touching the
  `target/release/datalog` that `code-analysis` tests and benches.
- **Frozen binaries per stage** — trunk `2eec05b` built in a worktree, then one
  copy after each fix — so every stage is measured separately and not
  reconstructed afterwards. Peak RSS from `/usr/bin/time -f %M`; the answer guard
  is a sha256 of stdout, which was identical at every stage on every probe.
- **Two fact bases of different shape**, so nothing is fitted to Grafana:
  Grafana's frontend (4.04M facts, a 796-file import SCC) and VS Code's
  `vs/base` (1.33M facts, zero import cycles).
- **Probes as stdin programs**, to separate load from evaluation: `symbol` alone
  (`count { S | symbol(id: S) }`), the runtime import closure alone (orient's
  `runtime_reaches` and `in_runtime_cycle`), and the libraries whole.

Timings below were taken with other measurements running, so they are indicative;
peak RSS is not affected by that.

## Where it went, on trunk

**Loading a table held three copies of it.** `sources::table::finalize` kept the
raw rows reordered to schema order (`arrange` cloned every cell out of a row it
was consuming), the typed columns (`coerce` cloned every string), and the rows
rebuilt from those columns — all at once. `symbol` is 831,625 rows × 18 columns,
489 MB of JSONL and 263 MB of string text: **2.28 GB of heap**, every byte of it
inside `finalize`. DuckDB's read was not the peak.

**Evaluation held the base facts twice**, for the whole run: `lower_with_sources`
cloned every row into `program.facts`, and `eval_capped` cloned every fact into
the model. At `vs-base` `checks.dl`'s peak that is 210 MB + 210 MB of the 486 MB
of tuple clones heaptrack attributes; on `orient.dl`, 74 + 74 MB.

**A recursive rule's round held every match, with its proof.**
`collect_rule_matches` built a `Derivation` for every match — every premise tuple
cloned — and `insert_derived` then dropped it under `Provenance::Unrecorded`.
Most matches in a closure are rediscoveries of facts already held. On the Grafana
closure probe (6.73 GB peak): ~2.9 GB premise clones, ~1.1 GB head tuples,
0.54 GB the pending vector itself; the relation's own strings ~0.6 GB.

## The fixes, stage by stage

Peak RSS, MB. Each stage includes the ones before it.

| probe | trunk | 1. load moves cells | 2. base facts moved | 3. no discarded derivations | 4. no pending rediscoveries |
|---|---:|---:|---:|---:|---:|
| Grafana, `symbol` load | 2,610 | 1,928 | 1,368 | 1,374 | — |
| `vs-base` `orient.dl` | 342 | 331 | 209 | 178 | 176 |
| `vs-base` `checks.dl` | 1,265 | 1,255 | 802 | 682 | 699 |
| Grafana, runtime closure | 6,730 (133 s) | — | — | 3,777 (45 s) | **3,454 (44 s)** |
| Grafana, `orient.dl` | 9,463 (224 s) | — | — | — | **4,944 (87 s)** |

1. **`finalize` consumes the raw rows.** Every column's type is fixed first, then
   each row is typed by moving its cells; `coerce` takes a cell by value.
2. **The facts move.** `lower_with_sources` consumes its tables;
   `eval_pruned_moving_facts` moves `program.facts` into the model. A program
   with an explanation keeps the copy, since `?whynot`'s cross case evaluates it
   twice. `insert_base` clones only for `Recorded`.
3. **A derivation is built only if the model will keep it** — the test
   `insert_derived` applies. `insert_derived` returns the new tuple for the delta,
   so a rediscovered fact is no longer cloned before being found present.
4. **A match whose fact the model already holds is not pending**, when nothing of
   it is kept: the model is frozen while a round collects.

Stage 3 carries the closure; stage 2 carries the non-recursive libraries. Stage 4
is worth 323 MB on the closure and nothing on `checks.dl` (+17 MB, within what
the allocator varies run to run — not re-measured).

## What is left, and the case for the deferred items

- **Column projection.** `symbol` trimmed to the 4 of its 18 columns `orient.dl`
  reads loads in **582 MB / 3.2 s**, against 1,374 MB / ~11 s — on a table most
  libraries import. Measured with a trimmed JSONL file, since an explicit schema
  that omits a source field is rejected (§13). ROADMAP § Performance.
- **Interning.** `symbol`'s 5.8M string cells hold 263 MB of text, of which
  96 MB is distinct (2.7×); every cell is also a 24-byte `String` inside a
  32-byte `Value`, plus a heap block. Import paths average 68 bytes, and a closure
  pair holds two. Not estimated further here; §14's canonical order is by
  content, which any interner must preserve.
- **The delta** copies each round's new tuples beside the relation — bounded by
  one round, not measured separately.
- **The join clones a `Premise::Fact` per candidate** (`enumerate_literal`),
  whether or not a derivation is built. Temporary, so time rather than peak.

## Reproduce

```sh
cargo build --profile profiling --offline
heaptrack -o <trace> target/profiling/datalog <program.dl>
heaptrack_print -f <trace>.zst --print-peaks 1 --print-temporary 1
/usr/bin/time -f '%e s %M KB' target/profiling/datalog <program.dl>
```

Traces are multi-GB: write them under `$HOME`, not a tmpfs.
