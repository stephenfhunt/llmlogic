# Worklog

A running handoff log for chaining agentic coding sessions. Each session ends by
adding an entry so the next session can get oriented in seconds — without re-reading
raw transcripts (Claude Code auto-saves those under
`~/.claude/projects/<repo-slug>/*.jsonl`; resume with `claude --resume`).

**Conventions**
- Newest entry on top (reverse-chronological).
- Keep each entry short and high-signal. Four fields:
  - **Done** — what changed this session (link commits/PRs where useful).
  - **Decided** — key decisions made (design decisions also go in `datalog/spec.md`
    §17; note them here too so the timeline is complete).
  - **Removed** — what you deleted, merged, or replaced.
  - **Next up** — the concrete next threads, so the following session starts oriented.
- This is a curated summary, not a transcript. Don't paste raw output here.
- **Entries stay under ~50 lines**, and **this file keeps the most recent three.**
  Older entries rotate verbatim into [`worklog-archive/`](worklog-archive/) by
  month, so session-start orientation stays a fixed cost instead of a growing one.
  A session needing more room than that is describing work that wants its own
  document — put the long form in `datalog/notes/` and link it from the entry.
  (50 rather than 40 because the entry that set this rule landed at 48, and a cap
  nobody meets gets ignored — cf. the `Stable` rung, deleted for the same reason.)

---

## 2026-09-13 (later) — the TypeScript extractor profiled: 19.35 → 16.37 GB on Grafana, facts identical

Asked to profile the TypeScript extractor for memory, giving up no fact data,
and take any low-hanging fruit. Planned from a by-phase profile (forced GC per
phase), then shipped in three commits, each checked against a frozen trunk
extraction by `sha256sum facts/*.jsonl`.

**Done** — code-facts `npm test`, typecheck; each intermediate tree tested on its own
- **`8adb540`** — programs share each parsed file where their settings would parse
  and bind it the same. On Grafana's 16 tsconfigs, 44,954 `SourceFile`s had held
  13,768 paths: load **7,043 → 2,710 MB** retained, 36.5 → 15 s. **P9**: output
  identical with sharing on and off.
- **`2c76fe6`** — declaration keys name a file by an integer: `ids` +742 → +529 MB.
- **`444c97b`** — numstat as parallel `git log --no-walk` jobs via a child
  process: git layer 43 → 11 s. A test compares rows across job counts.
- **End to end** (single runs): frontend `refs,quality,git` **382 s / 19.35 GB →
  361 s / 16.37 GB**; `@grafana/ui` **67.3 s / 2.26 GB → 37.0 s / 2.05 GB**. Both
  digests identical. Measurements: `code-analysis/notes/code-facts.md` § The
  extractor's own cost.
- **Mutations**: keying the share on file name alone was **green at first**. Under
  `nodenext` the default `moduleDetection` binds every `.ts` file as a module, so
  only `legacy` differs, and a randomly placed witness hit 4% of runs. Pinned, it
  is 49%, and the mutant is red 3 of 3. Dropping a chunk's last commit → red.
  Ignoring the pathspec in numstat stayed green; it is output-invisible.

**Decided** (`code-analysis/decisions.md` 2026-09-13; the first the user's)
- **The extractor represents the project as its tsconfigs configure it** — no
  setting of its own; an optimisation must equal per-tsconfig output.
- The share key is TypeScript's `DocumentRegistry` key without `pathsBasePath`.
- Numstat uses a child process, not a worker thread, because a synchronous
  caller blocked on a worker that fails to load hangs.
- Compact keys only if they measured ≥100 MB — they did.

**Removed**
- The single-pass `git log --numstat`; a worker-thread draft of it, replaced
  before commit; the oldest worklog entry.

**Next up**
- **The extractor's remaining peak is TypeScript's checkers — not queued.** The
  user's ruling: no working around TypeScript's cost, since users compile the
  project anyway (`code-analysis/decisions.md` 2026-09-13, amended).
- **`datalog/bugs/015`** — still the user's call; the § Performance gate waits on it.
- **Open**: `datalog/bugs/009`, `013`, `014`, `015`; `code-analysis/bugs/001`–`005`.

## 2026-09-13 — a query's answer printed from the model: 1,978 → 978 MB, and a deep run that does not finish

Asked to work on streaming output, from the Grafana/`vs/base` performance work.
Planned; the user asked why printing copies at all — it need not, and the plan
widened. Shipped in three commits, each verified.

**Done** — datalog 502 lib tests, integration and system, clippy both feature sets, fmt; code-facts `npm test` on the new binary
- **`699d195`** — **D6**: a run prints the eager renderer's bytes over its own
  model, at `Medium`/`Large` and over shaped programs, each also with query
  bodies reversed and with leading wildcards; a guard classifies lowered queries.
- **`affb601`** — `RunResult` holds each answer (row set + §14 shape) and renders
  in `write_output`; `answers` became `answer_lines()` / `answered()`.
- **`7c91da4`** — a bare atom walks its relation while printing: constants and
  repeated variables filter through `Value::unifies_with`, trailing wildcards
  dedup adjacent rows. Anything else keeps its row set.
- **`pointsto.dl`, 0.35 cut, 3.40M rows printed: 36.5 s / 1,978 MB → 33.3 s /
  978 MB**, sha256 identical — the run now peaks where evaluation alone did.
  `grafana-ui` bench, trunk against the change: all 13 digests identical, times
  unchanged, peaks within ±30 MB (`callgraph` 244 → 217, `pointsto` 391 → 365).
- **Five mutations, all killed** — but walking past a leading wildcard survived
  D6 until `with_leading_wildcards` and its guard case: the guard counted the
  atom answering, never a relation out of the answer's order.
- **`144bb9e`** — `bugs/015`: the deep run does not finish here. B13 at `Deep`
  passed 14.9 GB; skipped, the run was still killed with B5 at `Deep` running.

**Decided** (`datalog/spec.md` §17 2026-09-13; the first two the user's)
- **Print from the model, lazily; every error before the first byte.** Rejected:
  a per-query sink, which leaves partial stdout when a later query fails.
- **Ahead of the rest of § Testing**, guarded by its own tiered property.
- Parked: a row set of `&Value` — the join clones its bindings.

**Removed**
- `RunResult.answers` and the eager line list (its renderer survives only as
  D6's test oracle); "streaming" from the ROADMAP's round-bound risk and §14's
  *Not covered*; the scratch trunk worktree; the oldest worklog entry.

**Next up**
- **`bugs/015`** — the user's call: B13's proofs only to `Large`, a sampled
  proof clause, or a smaller `Deep`. Until then the § Performance gate cannot run.
- **Tier the rest** (B2–B4, B6, B8, C11, C13, C14, E1–E10) and the generator audit.
- **Drive a delta round from its delta atom** (a §17 design session).
- Re-learned: a "baseline" that recompiles picks up uncommitted tests — the
  second trunk deep run was not trunk. Freeze binaries before measuring.
- **Open**: `datalog/bugs/009`, `013`, `014`, `015`; `code-analysis/bugs/001`–`005`.

## 2026-09-12 (late night) — property tests that grow: tiers, a deep run, the `Old` read caught unprompted

Asked to plan and start an ambitious expansion of the property tests, sized for
program shape and scale, so they can carry the performance refactors. Plan
approved; Step 1 and the first tiered properties shipped.

**Done** — datalog 495 lib tests, full suite, clippy, fmt
- **`c9422a7`** — `testgen::Tier` (`Small` = the old `eval_bounds`, unchanged),
  `arb_program_with_edb_at` / `arb_program_text_at`, `DATALOG_PBT=deep`,
  `testgen::cases`, `RunStats` read off the recorded run, a growth guard with
  measured floors; **B1 at `Medium` and `Large`**.
- **Calibration forced two shapes**: above `Small`, bodies are connected (a
  four-atom product at `Large` did not finish) and constants come from a dense
  pool (an arity-3 recursion over the typed pools was OOM-killed at `Deep`).
- **The 2026-09-12 `Old` read now reddens B1 and B5 at `Medium` and `Large`**
  (2 of 2 runs) with no shape built for it; every untiered property stays green.
  At `Large` it shrank to 8 facts and 6 rules in 2.9 s.
- **`a03a20c`** — B5 permuting every body (`with_permuted_bodies`) and B13 over
  a query mask (`keep_queries`) at both tiers. Skipping `Dep::Negated` reddens
  B13 at `Medium` in 2 of 3 runs, at `Large` in none.
- **Lib suite 12.2 → 18.1 s**: the tiered tests ~4.7 s; `b1_shaped_programs_agree`
  alone is 12.3 s. **Deep run: 134 s, 1.5 GB peak, green**; `Deep`'s floors set from it. The
  first deep run was killed for memory in B5 at `Deep`, recording derivations a
  fact claim never reads; `Unrecorded`, it peaks at 55 MB.
- `Deep`'s slowest programs take 6–7 s (debug) for under 500 derived facts —
  five-atom self-joins over arity-3 relations: performance vehicles too.

**Decided** (`datalog/spec.md` §17 2026-09-12, *sized generators*; two the user's)
- **Per-commit `cargo test --lib` under 60 s**; **a deep run before and after
  every § Performance item**, and on demand.
- Tiers are upper bounds; one shared `*_holds` checker per claim, one
  `proptest!` entry per tier; the naive oracle runs to `Large`, not `Deep`; no
  `#[ignore]` twins.

**Removed**
- The duplicated bodies of shaped B1 and engine B13 (now `b1_holds`,
  `b13_holds`); the temporary calibration test; the oldest worklog entry.

**Next up**
- **The generator audit** (plan Step 0's table, into `testing.md` § Generator
  sizes) — measured the baseline, did not write the table.
- **Tier the rest** through `*_holds`: B2–B4, B6, B8, C11, C13, C14, E1–E10.
- **A tiered shaped generator** (mutual recursion, strata layers, arithmetic,
  named arguments, temporal), then **the scaling oracles** — all-paths
  differential, staged evaluation, renaming / disjoint union, first-round
  oracle, an Andersen points-to solver — and imports, seek and parser at size.
- The plan: `~/.claude/plans/let-s-plan-and-start-quirky-pancake.md` (local);
  its substance is in `datalog/notes/growing-inputs.md` § Sequence.
- **Open**: `datalog/bugs/009`, `013`, `014`; `code-analysis/bugs/001`–`005`.
