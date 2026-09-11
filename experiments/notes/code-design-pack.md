# The `code_design` pack — VS Code as the TypeScript corpus

The build plan for H-CA1's pack (`../hypotheses.md` 2026-09-11, `../decisions.md`
2026-09-11 later). Written 2026-09-11 after a sizing spike, to be executed in a
later session. The pre-registration names a *pinned TypeScript package of 5–20k
lines*; this plan proposes VS Code instead and says what that costs, so the
addendum in step 5 is written from measurements rather than hope.

## What the spike measured

VS Code 1.137.0, commit `645f29cc3176500b4b5762ba887cf2a7f0ffdf2c`, shallow
clone in `~/.cache/llmlogic-experiments/corpora/vscode-1.137.0` (293 MB without
`.git`; a spike tsconfig sits in `src/tsconfig.spike-base.json`).

| | lines | files |
|---|---|---|
| `src/` — everything | 2,866,405 | 9,000 |
| `src/vs/workbench` | 1,478,829 | 4,150 |
| `src/vs/platform` | 636,014 | 2,528 |
| `src/vs/editor` | 282,994 | 863 |
| **`src/vs/base`** | **155,801** | **485** |
| `src/vs/base/common` | 48,468 | 156 |
| `src/vs/base/test` | 49,242 | 162 |

Extracting `vs/base` alone (no `node_modules`, so `@types/*` are missing):
**1,332,798 facts in 18 s, 1.6 GB peak** — 6.5× tsdl's fact count. The extractor
is not the problem. The library is:

| library | vs/base (1.33M facts) | tsdl (205k) |
|---|---|---|
| `modgraph.dl` | 1.5 s | 0.3 s |
| `callgraph.dl` | 4.1 s | 0.9 s |
| `orient.dl` | 7.5 s, 1.9 GB | 1.2 s |
| `checks.dl` | 196 s, 3.2 GB | 4.3 s |
| `coupling.dl` | 330 s, 1.9 GB | 3.3 s |
| `cohesion.dl` | 404 s, 4.1 GB | 6.2 s |

**100× the time for 6.5× the data**, and a cell has 900 s for *every* query it
runs. Two other findings: `checks.dl` is not clean on VS Code — 42 violations,
all `import './thing.css'` (resolved, no target) — and 12,061 names are
unresolved for want of `@types/node` and `mocha`.

**Whole-VS-Code is out.** 2.87M lines is ~24M facts by tsdl's rate; `checks.dl`
alone would want tens of GB. The scope is `vs/base`, with `vs/base` +
`vs/platform` (792k lines) as a later ceiling case, and `vs/base/common` (48k)
as the fallback if step 1 does not land.

## Why this corpus is worth the work

- **It has classes** — 662 in `vs/base`. tsdl had none, so `lcom4`, `tcc`, `dit`,
  `wmc` and `cbo` have only fixture evidence behind them.
- **It cannot be read.** 156k lines does not fit a context window, so this is the
  pre-registration's **`at-scale`** track — where the engine's advantage should
  be, and where no grid has been run.
- **It brings its own independent checkers.** `.eslint-plugin-local/code-layering.ts`
  and `code-import-patterns.ts` encode VS Code's architecture rules in someone
  else's code — the kind of oracle control 1 demands.
- **It carries its own tests** (`vs/base/test`, 49k lines), so "what no test
  reaches" is a real question here, not a fixture's.

## Step 1 — the library at 1.3M facts (`code-analysis/`, maybe `datalog/`)

> **Done 2026-09-11.** Every library the playbook names for module and design
> questions is under 30 s on `vs/base` and under 4 GB; `checks.dl` 196 → 9.3 s,
> `coupling.dl` 330 → 27.2, `cohesion.dl` 404 → 28.2. **The suspects below were
> wrong**: it was not the aggregates but the engine's leading-prefix seek, and
> the fix was one projection rule per re-keyed join, not an index. Four
> libraries stay expensive by construction — `callreach` 38 s, `dominators`
> 51 s, `flow` 104 s, and `pointsto`, which does not fit — so the questions in
> step 4 must not need them. Table and audit:
> `../../code-analysis/notes/code-facts.md` § At a million facts; decisions in
> `../../datalog/spec.md` §17 and `../../code-analysis/decisions.md`, both
> 2026-09-11.

**Target: every library the playbook sends an agent to runs in under ~30 s on
`vs/base`**, so a cell can afford several. Do this first: with `cohesion.dl` at
404 s the `code-analysis` arm — the one whose playbook *names* it — would time
out, and H-CA1 would measure the timeout.

1. **Profile before rewriting.** The suspects are already visible:
   - `coupling.dl`'s `comp_edge_weight(G, A, B, N) :- comp_edge(G, A, B), N = count { S | crossing(...) }`
     — an aggregate evaluated once per `comp_edge` binding over a large
     `crossing`, with `T`, `K`, `L` free inside the count (the count trap as
     well as the cost).
   - `cohesion.dl`'s `lreach` / `module_reach` transitive closures, and
     `component_of` / `module_component`, which take a `min` per member.
   - `checks.dl` at 196 s: 50 rules, so find which ones rather than guessing.
   Build a small benchmark script (`tools/code-facts/bench/`) that times each
   library over a fact directory and prints a table, so every claim here is
   reproducible.
2. **Decide per case where the fix belongs.** Restructuring the rule is what
   took `orient.dl` from 16.9 s to 1.2 s (project the group key first). If the
   cost is instead the engine re-scanning an aggregate's body per binding, that
   is an **engine** fix — indexing the filtered relation — and belongs in
   `datalog/` with its own tests and a `spec.md` §17 entry. Measure which it is
   before choosing.
3. **Acceptance:** each of `checks`, `orient`, `modgraph`, `callgraph`,
   `callreach`, `coupling`, `cohesion`, `coupling_kinds`, `metrics`, `packages`
   under ~30 s on `vs/base`; tsdl's numbers no worse; peak memory under ~4 GB;
   `npm test` and `cargo test` green. Record the before/after table in
   `../../code-analysis/notes/code-facts.md`.

## Step 2 — what VS Code exposes in the extractor

1. **Asset imports.** `import './actionbar.css'` is `resolved` with no
   `target_file`, which `checks.dl` calls a violation — correctly, as the facts
   contradict themselves. Decide the rule (an asset import is its own `kind`,
   or `resolved: false` with `target_package: null`), change `schema.ts`, the
   matching half of `checks.dl`, both language references, and a test.
2. **Vendor the type packages.** Pin `@types/node`, `@types/mocha` and whatever
   else `vs/base` needs beside the corpus, so those 12k unresolved names go
   away; measure the count after. Anything left is a real blind spot and belongs
   in the pack's README so the questions avoid it.
3. **Gate:** `checks.dl` clean on `vs/base` before it is a corpus at all.

## Step 3 — the corpus in the harness

1. **Pin by commit, not by archive.** `corpus.py` pins a sha256 of a tarball;
   GitHub's generated tarballs are not guaranteed byte-stable, so a `GitCorpus`
   (shallow clone of a tag, verified with `git rev-parse HEAD` against a pinned
   commit) is the honest pin. Keep the sha256 path for sdists like sqlparse.
2. **Select a subtree.** The corpus ships `src/vs/base`, `src/typings`, the
   tsconfig the pack owns (the spike's, committed here rather than left in the
   cache), and the vendored `@types`. Everything else stays out of the
   workspace.
3. **Check the fixture path for `at-scale`.** `static_analysis` builds
   `Fixture(files=dict(sources()))` — 5.6k lines of sqlparse. 156k lines must
   not be inlined into a prompt; confirm what `task.Fixture` and the prompt do
   with an `at-scale` fixture before assuming it is only a path.
4. `harness corpus fetch`, `harness domains` availability, and the cell's sealed
   copy all work as they do for sqlparse.

## Step 4 — questions and oracles

Six to eight templates, each parametrized by target (per directory, per file,
per class) to reach the item count, and each with a plain oracle that never
imports `code-facts` or `lib/`:

| question | oracle |
|---|---|
| files in a runtime import cycle within a named directory | parse imports, drop `import type`, find cycles |
| exports no other file references, given named entry points | parse imports/exports |
| files whose exports fall into several groups by shared references (the question states the grouping rule) | build the same graph from the parse |
| classes whose methods fall into several groups (LCOM4 > 1) | same, over class members |
| functions taking a project type and reading one of its members | parse signatures and member accesses |
| imports that would break a stated layering rule | VS Code's own `code-layering` / `code-import-patterns` |
| exported functions no test in `vs/base/test` reaches | the oracle's own call graph, name-based, stated in the question |

**Contamination is the live threat.** The models have read VS Code. Rules:
answers are exact sets at the pinned commit; no question whose answer VS Code's
own CI already forces to be empty (its layering rules are enforced, so
"violations" is `∅` and guessable); and at least one question whose answer
differs between 1.137.0 and an adjacent release, so memory cannot be right by
default. Record the threat in the addendum either way.

## Step 5 — sizing, then the addendum

1. Count the items the templates actually produce; run `harness power` on that
   number. Short of 155 paired items → the run is a **pilot** under the
   provenance precedent: descriptive, no p-value, no delta claimed.
2. Write the `hypotheses.md` addendum **before any cell runs**: the corpus change
   and why (this file), the `at-scale` track for the primary endpoint, the
   contamination caveat, per-cell extraction cost, and that sqlparse
   (`in-context`) and VS Code (`at-scale`) are reported separately, never pooled.

## Step 6 — dry run, then cells

`harness run --dry-run` over the new domain; one smoke cell per arm; check that
the `engine` arm finds `code-facts` unprompted and the `code-analysis` arm loads
its skill; record cell wall clock (18 s extraction plus queries) and cost. Then
the grid.

## Stop rules

- **Step 1 does not reach 30 s** → fall back to `vs/base/common` (48k lines,
  ~400k facts) and, if that still misses, to a mid-size npm package as the
  pre-registration originally said. The corpus is not worth a false negative.
- **The prose arm answers exact sets without reading the code** → contamination
  is real; re-scope the questions and say so in the report.
- **A cell cannot hold the memory** (`cohesion.dl` peaked at 4.1 GB) → the pack
  names the libraries it expects an agent to run, and the playbook's costs table
  is updated for this scale.
