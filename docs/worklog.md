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

## 2026-09-12 (night) — `pointsto.dl` as an engine vehicle: a new fact was pending once per path

Asked to keep grinding engine performance with `pointsto.dl`. The full `vs/base`
does not finish, so a series cut from it by file (10–100%), a per-round counter in
an uncommitted worktree (`~/.cache/pointsto-wt`), heaptrack, perf.

**Done** — datalog 615 tests, clippy both feature sets, fmt; code-facts `npm test`
- **Found**: a round held a *new* fact once per path that reached it — 12.16M
  pending entries for 471k distinct `pts`; the 70% cut aborted on that `Vec`
  doubling to 4 GB. The earlier fix only skipped facts already held.
- **`563831b`** — an unkept match's fact goes into a per-round set that is the
  delta. 35% cut, evaluating: **35.5 s / 2.67 GB → 32.3 s / 0.97 GB**; 50%
  3.98 → 1.55 GB; 70% now finishes (6.8 GB). The full base still aborts at 15 GB.
- **`aefb6dc`** — `answer_lines` sorts borrowed cells: printing 3.40M rows
  2,287 → 1,974 MB. **`69aac3f`** — the binary writes line by line
  (`RunResult::write_output`): no measured change; a closed pipe exits 2, not a panic.
- **Grafana bench, trunk vs now, 23 digests identical**: frontend `coupling`
  290 → 171 s, `coupling_kinds` 148 → 120 s, `orient` 78 → 62 s, `modgraph`
  75 → 61 s; peaks 2–10% lower. `@grafana/ui` `pointsto` 8.9 s / 373 MB.
- `datalog/notes/pointsto-profile-2026-09-12.md`; `code-analysis/notes/code-facts.md`
  § Re-measured after a round held each fact once; E9's two mutations in `testing.md`.
- **`8d8df27`** — two defects passed all 615 tests, each shown by mutation: a 3+-atom
  semi-naive view, a dropped reporting derivation. `arb_shaped_program` (analysis
  shapes, sized) kills both through B1/E9; B1/E9 run under a round cap, so a hang
  fails. **`a5236f6`** — output tests. 623 tests; lib suite 5.5 → 14.2 s.

**Decided** (`datalog/spec.md` §17 2026-09-12, first two entries; the last three the user's)
- **A round holds each unkept fact once**; recorded runs are untouched.
- **Keep `write_output`** though it measured nothing: it is the copy that becomes
  the peak once answers stream.
- **Property tests grow their inputs before more refactoring or performance work**
  — `datalog/notes/growing-inputs.md`. Streaming answers waits on it.

**Removed**
- `pending`'s one entry per unkept match; `answer_lines`' cloned rows; the joined
  output `String`; E9's inline body (now `e9_holds`); the oldest worklog entry.
  The round counter never entered the repo.

**Next up**
- **Grow the property tests' inputs** — design first (datalog ROADMAP § Testing):
  tiers or drawn size, scaling oracles, budgets, a generator audit; then B5/B13.
- **Then stream a query's answer**, and **drive a delta round from its delta atom**
  (14 s of `pointsto.dl`'s 28 s on the 35% cut; a §17 design session).
- Re-learned: heaptrack names the site that *allocated* a tuple, not what holds
  it — the first reading here blamed `pending` for what was the answer.
- **Open**: `datalog/bugs/009`, `013`, `014`; `code-analysis/bugs/001`–`005`.
