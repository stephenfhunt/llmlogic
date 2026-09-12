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
- **Nothing is ruled next in datalog § Performance**; column projection and magic
  sets are the bigger chunk, and relation pruning moved their floor.
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

## 2026-09-11 (later still) — the library at a million facts: it was the seek, not the aggregates

Picked up the `code_design` pack at step 1. The sizing spike blamed the
aggregates; the measurement said otherwise, and the plan's own hypothesis went
down with it.

**Done** — **datalog 598 tests**, code-facts **108**, clippy and fmt clean
- **It is the seek's leading-prefix rule.** `seek::bound_prefix` stops at the
  first unbound column, so an atom binding a column its relation does not lead
  with scans the whole relation once per outer row:
  `count { E | flow_node(id: E, fn: F, kind: entry) }` is 108,597 rows × 13,983
  functions. Every library over 30 s was over it for this and nothing else.
- **The fix is one rule per re-keying, in the library** — `lib/keys.dl` (`child`)
  plus `file_member`, `called_by`, `entry_node`, `alloc_of`, `decl_file`,
  `access_of`, `used_at`, `touched_by`, `contains`. **`checks.dl` 199.6 s → 13.7 s
  on two of them**, answers byte-identical.
- **`Provenance::Reports`** (`datalog/`): an aggregate or a cast in any rule body
  provisioned the *full* derivation store for the whole program to carry two
  counts. Now it keeps only the derivations those counts are read from.
- **`tools/code-facts/bench/`** times each library over a fact directory and
  **digests its answers** — the guard that a speed-up did not move a row. It
  did not, across every change here.
- **On `vs/base` (1.33M facts):** `checks` 196 → **9.3 s**, `coupling` 330 →
  **27.2**, `cohesion` 404 → **28.2**, `coupling_kinds` >289 → **24.6**,
  `metrics` >79 → **6.6**, `orient` 7.5 → **4.9**. Table in
  `code-analysis/notes/code-facts.md` § At a million facts.
- **`callreach.dl` is the exception at 38 s / 4.1 GB** — a whole-project call
  closure is quadratic in the graph's density and no re-keying touches it;
  `callreach_seeded.dl` is the seeded form, `taint.dl`'s idiom.

**Decided** (`datalog/spec.md` §17, `code-analysis/decisions.md`, both 2026-09-11)
- **Secondary indexes stay rejected.** The rejected case is exactly what a real
  corpus hit — and a program can re-key itself in one rule, which is cheaper than
  a second copy of every relation. `keys.dl` states the rule for a reader.
- **A re-keying is not free**; it pays only where the scan it replaces is
  quadratic. `comp_edge_to` was written, measured at no gain, removed.
- **`Reports` buys memory, not always time** — `cohesion` 35.3 → 28.2 s and
  4.2 → 2.3 GB, but `coupling` 23.0 → 27.5 s for half the residency. A 900 s,
  4 GB cell is short of the memory.
- **E9's third claim is stated over the warnings, not over the predicate under
  test** — an oracle built from `Derivation::reports` stayed green under its own
  mutation.

**Removed**
- `comp_edge_to` (measured at no gain); the static per-rule gate on the reporting
  walk (measured as noise, 9.4 → 9.3 s); from `checks.dl`, `coupling.dl`,
  `cohesion.dl`, `metrics.dl` and `coupling_kinds.dl`, every scan-shaped join.

**Then step 2, the same day** — **`checks.dl` is clean on `vs/base`**, the gate
- **Asset imports resolve through a wildcard `declare module`**, so
  `imports.target_ambient` names the pattern. Putting the declaring `.d.ts` in
  `target_file` is the obvious fix and is wrong — every stylesheet-importing file
  would gain an import edge that does not exist at run time. A new `assets`
  fixture caught a second bug on the way: `bundler!./x.css` was becoming
  `target_package` `"bundler!."`.
- **Type packages pinned by lockfile** (`experiments/corpora/vs-base-types/`, no
  vendored bytes): unresolved names **12,061 → 286**, `ref` 99,881 → 120,179,
  `diagnostic` 3,134 → 62, `any_site` 13,225 → 1,369.
- **`vs/base` is not self-contained** — 11 files outside it, without which the
  extraction is 1,336,528 facts and 463 unresolved rather than 1,363,422 and 286.
- **111 tool tests.**

**Then step 3, the same day** — the corpus in the harness, **1,667 harness tests**
- **A corpus is pinned by what its kind can promise.** `GitCorpus` pins a
  commit, because GitHub's generated tarballs guarantee no bytes; `NodePackages`
  pins a lockfile, because `npm ci` hashes the transitive tree. The pin test
  fails on a kind it does not recognise.
- **`domains/code_design/fixture.py` assembles the workspace** — `vs/base`, the
  11 files it reaches, `src/typings`, the pack's tsconfig, and
  `node_modules/@types` at the workspace root. Materialized through `arms.build`
  and extracted it is **identical to the whole checkout** (`ref` 120,179,
  `call_site` 53,741, 286 unresolved), and `checks.dl` is clean on it.
- **`REACHED_OUT` is re-derived in the tests from import text, not from
  `code-facts`** — control 1 pointed at a fixture invariant, since the extractor
  is the tool under test here. It caught its own bug: a `.css` target is a real
  file and still not in the program.
- **The type packages change one cross-file edge in 4,475** and no cycle, no
  `module_lcom4`. They are in the workspace for the *arms*, not the answer key:
  only the engine arms extract, and one reading "12,061 unresolved" off
  `orient.dl` behaves differently for a reason that is not the skill.
- **The asset catalogue now reports one line per root** — a single-root fixture
  renders byte-identically, so no archived prompt moved.
- **Installed, not in `SLATE`**; `harness domains` prints it as such.

**Then step 4, the same day** — **28 questions, and the ones that were dropped**
- **The user pushed back on a hand-rolled TypeScript parser, and was right.**
  `static_analysis`'s key uses `ast` and the extractor it grades uses `ast` too:
  control 1 forbids an oracle that calls *the thing under test*, never one that
  uses a parser. The oracle now parses with `ts.createSourceFile` — syntax only.
- **`tests/test_truth_independence.py` then caught the design**: `truth.py` may
  not `import subprocess`, bluntly, so parsing moved to `harness.corpus` beside
  `git clone` and `npm ci`.
- **A `truth.py` may reimplement a graph; it may not reimplement a measure.**
  The import graph agrees with `code-facts` **exactly** (2,039 edges); class
  LCOM4 disagrees on **47 of 187** classes with no single cause. The cohesion
  questions were built, measured, and deleted.
- **Three templates, 28 paired items** after three degeneracy rules — the last
  of which (the answer may not be the universe) cost ten items and caught
  `browser/ui/selectBox`, where every file is in a cycle.
- **The at-scale track is a pilot** — 28 items supports ~+25 points; the
  registered +10 needs 155. Addendum written before any cell ran.

**Next up**
- **Two things stand between the pack and a run**, both their own work: a
  **reference program** answering every task (`tests/test_reference_corpus.py`)
  and the **`code-analysis` arm**, which H-CA1's endpoint is defined over. The
  pack is installed and deliberately not in `SLATE` until both exist.
- **One contamination check is outstanding**: at least one question whose answer
  differs between 1.137.0 and an adjacent release. If it cannot be satisfied,
  report the caveat as unmitigated rather than dropping it.
- **The flow-layer libraries are now measured too** (`flow` 104 s,
  `dominators` 51 s, `pointsto` does not fit); the playbook says so.
- Still open from the last session: a test file's process dying under load.
