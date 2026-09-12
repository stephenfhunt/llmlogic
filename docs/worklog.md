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

## 2026-09-12 — the playbook on a codebase nobody here wrote: Grafana, end to end

Asked for a real analysis of a real project, git facts included — the exploring
half of the playbook and the git layer had never been used outside fixtures.
Subject `grafana/grafana` @ `9d9d93ee41`: 9,426 TS files, 1.59M lines, 73,113
commits, deps installed, the bundle in its `.claude/skills/`. Two scopes, each
explored by a fresh agent holding only `SKILL.md` and `reference/`.

**Done** — code-facts **118 tests**, typecheck and bench clean, digests unmoved
- **Extraction and the gate held on a codebase it had never seen**: 1.17M facts
  from `grafana-ui`, **4.04M** from the whole frontend, `checks.dl` **exit 1,
  zero violations** on both, first try.
- **`lib/reach.dl`** — the closure split out of `modgraph.dl`, which computed
  17.45M pairs for every importer while both read only `dep`. Same answer:
  **306 s / 12.6 GB → 70 s / 3.3 GB**, and `orient.dl` — *step 3 of the method* —
  went from **OOM-killed at 21 GB** to 207 s.
- **`packages.dl` works in a monorepo** — `imported_workspace` reads the
  dependency off `file.package`, since a sibling resolves to a file (44,908 of
  55,762 imports) and `unused` named four packages 540 statements import. An
  unnamed `package.json` is no longer a package.
- **New `monorepo` fixture**; `bugs/` opened for `code-analysis` (five), two more
  against the engine (`012`, `013`).
- **Findings, verified in the source**: a latent bug at `useDragAndDrop.tsx:109`;
  three unused runtime deps in a published package; a plugin-decoupling plan.

**Decided** (`code-analysis/decisions.md` 2026-09-12 later; long form
`code-analysis/notes/code-facts.md` § Dogfooding — Grafana)
- **A closure does not travel with the cheap rules** — a library costs what it
  imports; `callreach.dl`'s precedent applied here and had not been taken.
- **A corpus chosen for size does not exercise shape.** `vs/base` has *zero*
  import cycles, so the one superlinear rule was never run; Grafana has 915 files
  in 27, the largest a 796-file SCC.
- **A relation that cannot tell two cases apart claims the weaker one** —
  `imported_workspace` feeds `used`, not `undeclared`.
- **Three oracles agreed** (`git log --follow`, a Tarjan SCC, a regex import
  scanner); the one wrong answer was a *confident negative over a blind spot*,
  caught by the verify step, not by the engine.

**Removed**
- `file_reaches`/`in_cycle`/`cycle_edge` from `modgraph.dl` (moved); the dir-name
  fallback for an unnamed `package.json`; the bench's two closure queries from
  its `modgraph` entry, now their own.

**Next up**
- **`orient.dl` is fixed but still 13.9 GB** — its own `runtime_reaches` is 11.8M
  pairs; the gate that declines the cycle question runs in 13.6 s / 3.3 GB, but
  its threshold is a guess from four points — a design call.
- **The next two datalog items are ruled** (user, 2026-09-12; measurements and
  sequencing in `datalog/ROADMAP.md` § Performance): *don't evaluate a rule no
  goal depends on*, then *load only the relations the program names* — one
  reachability analysis applied twice. They close `bugs/012` and make `reach.dl`
  optional; `code-analysis/ROADMAP.md` carries the unwind item. Column projection
  and magic sets are the bigger chunk behind them, deliberately not next.
- **Open**: `code-analysis/bugs/001`–`005`, `datalog/bugs/012`–`013`. H-CA1 still
  needs its reference program and the `code-analysis` arm.
