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

## 2026-09-11 (later) — `code-analysis/`: its own project and skill, and Python

Asked whether a domain skill should package the extractor and the engine and
guide the exploration. Decided with the user: a new project, a Python extractor
now, the measurement designed now and built next session. Built so.

**Done** — **108 tool tests**, experiments **1,661**, every commit gated
- **`code-analysis/`**: `ts-facts` moved and renamed `code-facts`; the datalog
  skill restored to `7f3e998` byte for byte, so experiment workspaces hash as
  before. The skill is a playbook (extract → `checks.dl` → `orient.dl` → explore
  → verify → report, architecture rules as exit codes) with a reference per
  language; `package.sh` builds the bundle, verified installed.
- **Python frontend** (stdlib; Node validates its stream against `schema.ts`):
  every layer but dataflow. P1-py–P4-py, mutation-verified; P1-py and P4-py take
  `python3` itself as the oracle.
- **Found on the way in:** breadth-first MRO; a `match` guard sharing its
  pattern's node; sqlparse's self-importing `__init__` (1,165 unresolved names);
  `x = x.next()` recursing forever; lambdas in decorators never declared;
  comprehensions in nested defs binding outward; and, in TypeScript, cognitive
  complexity under-counting else-if bodies.
- **Checked independently:** the facts answer `static_analysis`'s four questions
  exactly as `truth.py`; on `experiments/` (21.6k lines, 4.3 s) 1,116 of 1,117
  branch counts equal ruff's mccabe; and `anthropic`, declared in its
  `pyproject.toml`, is imported nowhere.
- **The unexplained `npm test` failure was P5's heap guard** (32 of 400 runs);
  P2-py had a second (35 of 400). Both reshaped, their rates measured.
- **H-CA1 pre-registered** — `experiments/hypotheses.md` addendum.

**Decided** (`code-analysis/decisions.md`; experiments `decisions.md` 2026-09-11 later)
- **One schema for both languages**; a Python writer would be a second home.
- **Python cyclomatic counts like ESLint**, for parity; mccabe's difference is
  documented and measured, not hidden.
- **A guard that needs a shape once is sized from its measured rate.**
- **H-CA1's `engine` arm gets `code-facts` undocumented**; the primary endpoint
  is `code-analysis` − `engine` at haiku.

**Removed**
- From the datalog skill: `ts-facts`, `tools/`, `recipes/typescript.md`. From the
  frontend: its breadth-first `mro()`, its unscoped binding walk, the silent
  fallback when `py_flow` failed to import.

**Next up**
- **Build the H-CA1 pack**: the TypeScript corpus, oracles, `node`/`python3` on a
  scrubbed PATH; run `harness power` on the real item count before any grid.
  ***Added later the same day***: VS Code can be the fixture, scoped to
  `vs/base`, once the library is faster (`coupling.dl` 330 s, `cohesion.dl`
  404 s on 1.33M facts) — plan in `experiments/notes/code-design-pack.md`.
- **A test file's process dies now and then** (`'test failed'`, no assertion):
  twice in 25 runs under heavy concurrent load, never in 35 idle ones; and one
  uncaptured failure. Capture every suite run's output until it is named.
- Parked: a Python dataflow layer; a name-tier helper in `lib/` for Python's
  unresolved calls.
