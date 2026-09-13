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

## 2026-09-12 (evening) — memory at Grafana scale: an import held three times, a proof built for nothing

Asked to profile `orient.dl` on a Grafana-sized base and find what else is
memory-gated, without fitting Grafana. heaptrack (installed this session) over a
new `profiling` cargo profile; a frozen binary per stage; two shapes of base —
Grafana's frontend (4.04M facts) and `vs/base` (1.33M, zero import cycles).

**Done** — datalog 615 tests, clippy both feature sets, fmt; code-facts `npm test`;
bench digests identical on every `vs/base` library
- **Found**: `finalize` held an import three times (`symbol`: 2.28 GB of heap for
  489 MB of JSONL); lowering and evaluation each cloned the base facts; every match
  built a `Derivation` that `Unrecorded` then dropped (~2.9 GB of the closure's 6.7).
- **Four fixes**: typing consumes the raw rows; `lower_with_sources` consumes the
  tables and `eval_pruned_moving_facts` the facts; a derivation is built only if
  kept; a match whose fact is already held is not pending.
- **Grafana `orient.dl` 224 s / 9.46 GB → 87 s / 4.94 GB**, answers byte-identical;
  `modgraph` with cycles 9.3 → 5.1 GB; `coupling` 9.3 → 3.8, `coupling_kinds`
  9.1 → 6.1, `checks` 6.2 → 3.2.
  First runs there: `callreach` 18 s / 1.4 GB, `cochange` 337 s / 2.0 GB.
  `vs/base`: `callreach` 3.5 → 0.7 GB, `cohesion` 1.9 → 0.4.
- **Still memory-gated**: `pointsto.dl` (killed above 14 GB at 130 s on `vs/base`)
  and the extractor (16.8 GB on Grafana).
- **Measured for later**: column projection (`symbol`, 4 of 18 columns: 582 MB
  against 1,374); interning (263 MB of its text, 96 MB distinct).
- `datalog/notes/memory-profile-2026-09-12.md`; `code-analysis/notes/code-facts.md`
  § Re-measured after the engine stopped copying.

**Decided** (`datalog/spec.md` §17 2026-09-12, first entry; `code-analysis/decisions.md`
2026-09-12 evening — the user's)
- **Real profilers**, not an allocator counter in the engine.
- **No size gate on `orient.dl` and no size or cost guidance in the skill**: the
  tool is made usable instead. The engine stays generic — nothing special to one
  library — and a library workaround is engine debt (`lib/keys.dl` reopened).
- **A program with an explanation keeps its copy of the facts** (`?whynot`
  evaluates twice).

**Removed**
- The three-copy load path and `type_column`; every size and cost note from
  `SKILL.md`, both language references (timing columns, the join-key rule) and
  seven library headers; the queued `orient.dl` gate; the oldest worklog entry.

**Next up**
- **Profile `pointsto.dl` as an engine vehicle** (datalog ROADMAP § Performance)
  and **profile the TypeScript extractor** (16.8 GB on Grafana) — both queued.
- **A non-leading bound column still scans** — with re-keying gone from the
  skill, the engine item that retires `lib/keys.dl`. **Column projection**, then
  **interning**: design sessions, measured.
- Re-learned: measure with the repository's `lib/`, never a fact directory's copy.
- **Open**: `datalog/bugs/009`, `013`, `014`; `code-analysis/bugs/001`–`005`.

## 2026-09-12 (later) — rule and relation pruning: a run pays for what its goals reach

Picked up the two § Performance items the Grafana dogfood ruled next. Both
shipped, and `bugs/012` closed with them — narrowed, because half its premise was
false.

**Done** — **datalog 615 tests**, clippy (both feature sets) and fmt clean
- **Rule pruning** (`0841069`): only the rules a goal depends on run. The walk
  reuses `collect_stratum_edges`, so a relation read only under `not` or inside an
  aggregate stays live. `vs/base`, `callreach.dl` beside an unrelated question:
  **37.4 s / 4.20 GB → 3.1 s / 0.70 GB**.
- **Relation pruning**: with every schema explicit the program is lowered before
  any import is read, and only reached imports are. The same question **0.01 s /
  33 MB**; an unknown field **5.5 s / 1.10 GB → 0.00 s**; counting all 179,748
  `var` rows **6.7 s / 1.64 GB → 0.71 s / 0.21 GB**.
- **B13** at three levels plus an import half; four mutations recorded, each
  reddening its test. The text guard's first draft ran *pruned* and went quiet
  under the mutation it guards.
- **`code-analysis` bench digests identical** on the 13 libraries that answer
  (`pointsto` does not fit either way); `npm test` green on the new binary.
- **`bugs/014`**: a declared temporal column fixes no type, so an empty relation
  makes `A + @1d` a type error. `012`'s syntax-error half never reproduced.
- **`reach.dl` unwound into `modgraph.dl`**: answers byte-identical; `dep` over a
  synthetic 2,000-file cycle 32.7 s unpruned → 0.03 s.
- **Grafana re-measured** (4.04M facts): nothing runs out of memory; worst peaks
  `orient.dl` 9.5 GB (was 13.9) and `modgraph.dl` with cycles 9.3 GB.

**Decided** (`datalog/spec.md` §17 2026-09-12, two entries; the user's calls)
- **Permissive**: a pruned rule cannot fail or hang a run; static warnings stay
  whole-program; a program with no goals prunes nothing.
- **The early check is lowering, not typecheck.** The approved plan argued a
  fact-free typecheck only under-rejects; the source said otherwise. The user
  asked whether sources could supply types — they can (Parquet free, JSONL/CSV a
  streaming pass), but typecheck ignores declared types for the same reason.
  Split: lowering now, *a column's type is known before its rows* designed later.
- **A type rejection over a partial load is re-checked over a full one**, so a
  pruned run accepts and rejects exactly what a full load does.
- **A skipped import is still checked short of reading** — a broken path fails.

**Removed**
- Load-then-lower for explicit-schema programs; ~90 lines of pre-build analysis
  in the two ROADMAP items (now §17); the recipe's "import the closure, not the
  library" advice and its 35× anecdote; `bugs/012` (to `resolved/`).

**Next up**
- **A column's type is known before its rows** (`bugs/014`) — a design session:
  declarations as constraints first, a source's own types second.
- **Nothing is ruled next in datalog § Performance.** On Grafana the largest cost
  left is `orient.dl`'s own closure (9.5 GB) — its size gate is a user call — and
  the coupling libraries at 5–8 min (`code-analysis/notes` § Re-measured).
- **Open**: `datalog/bugs/009`, `013`, `014`; `code-analysis/bugs/001`–`005`.
