# code-facts — a codebase as Datalog facts

Overflow for `../datalog/spec.md` §17 2026-09-10 and `../decisions.md`. The tool
is `tools/code-facts`, run as `skill/code-facts`; the agent-facing guide is
`skill/reference/typescript.md`, and the fact schema's one normative home is
`src/schema.ts` (every output also carries a generated `SCHEMA.md`). It was
`ts-facts`, living in the datalog skill, until 2026-09-11 (`../decisions.md`);
the history below keeps that name where it was the name. This note is the
*why*: what was decided, what was rejected, and what the tests found.

## What it is for

`recipes/source-analysis.md` taught *real parser → fact tables → import → ask*
and left every user to write the extractor. The one dogfood run (Rust,
2026-07-27) found that **every wrong answer came from extraction**: 7% of the
code invisible inside macros, and bare-name joins inventing 150 "mutually
recursive" functions where there were 3 clusters. For TypeScript the checker
resolves names, so the extractor can hand over a *resolved* graph and keep
honest about the rest (`unresolved_ref`, `dispatch: unresolved`).

The brief was maximalist — module relationships down to code flow, enough for
higher-order measures like coupling and cohesion, and for questions nobody
wrote a linter for. So the tool emits **primitives** (61 relations over seven
layers) and the rule library in `lib/` turns them into measures; the agent is
expected to write its own rules on top.

## Decisions

**TypeScript 6.0, pinned, in-process.** TypeScript 7 (the Go compiler; tsdl uses
7.0.2) ships no classic compiler API — only `typescript/unstable/*` over IPC. 6.0
is the last release with the in-process checker, and it reads projects written
for 7 (6.0 was the bridge release: same language, same deprecations). The tool
carries its own copy, so the target's version never matters. *Rejected:* the TS 7
IPC API (explicitly unstable, every checker query a round trip, little prior
art); *rejected:* an adapter over both (double the surface, and two backends
whose facts could drift apart unnoticed). Revisit when TS 7's API stabilizes.

**TypeScript source, run by Node's type stripping.** No build step; Node ≥ 22.18
runs `src/main.ts`. Erasable syntax only (`erasableSyntaxOnly` in the tool's
tsconfig), so nothing needs a transform. The tool typechecks itself and is its
own realistic corpus. *Rejected:* plain JS with JSDoc (a checker-heavy codebase
wants the checker), and committed compiled JS (a build artifact to keep in step).

**It ships with a skill** — first the datalog skill's, and from 2026-09-11 the
`code-analysis` skill's (`../decisions.md`), whose `package.sh` bundles it
without `test/` or dev packages and vendors its TypeScript. Its tests live beside
it and run with `npm test`.

**Output is JSONL plus generated import headers**, not inline `.dl` facts: the
recipe's own lesson is that bulk facts belong in imports (28k facts in 0.32 s),
and inline facts at 200k rows would be a parse-time cost on every run. Explicit
schemas in `schema/*.dl` give an empty or all-absent column a type, and declare
enum columns `symbol` so an agent writes `kind: class`. *Rejected:* CSV — it
cannot tell `""` from absent.

**Ids are keyed by declaration position and assigned in path order.** Every
tsconfig is its own program with its own AST and symbol objects, so `ts.Symbol`
identity cannot be the key: a symbol declared in one project and referenced from
another must be one id. A pre-pass walks files in path order, so a collision
suffix (`@line`) lands on the same declaration however files are listed (P6).
One id-space serves every symbol-valued column — `source-analysis.md`'s trap 2
(two id-spaces joined silently) cannot happen, and `lib/checks.dl` asserts it.
An anonymous function bound by a declaration *is* that declaration
(`const f = () => …` is `#f`), because that is the name every caller uses.

**`ref` excludes locals and parameters.** They are the flow layer's (`def`/`use`),
and at symbol level they are noise that would dominate every coupling count.

**Dispatch is classified, and expansion is left to Datalog.** `static`,
`virtual` (an instance member: `lib/callgraph.dl` expands it through `overrides`,
class-hierarchy analysis), `indirect` (a function-typed value in project code:
`lib/pointsto.dl` resolves it), `unresolved`. A *library's* function-typed value
is named as the target instead: on tsdl 2,843 of 2,870 indirect sites were
vitest's API, which no analysis of the project could ever resolve.

**Whether an import survives to run time is the emitter's answer**, not the
syntax's: TypeScript drops an import whose bindings only annotate, with or
without `import type`, keeps every unmarked one under `verbatimModuleSyntax`, and
keeps one only a JSX factory or decorator metadata uses. An after-transformer on
an in-memory emit sees the final tree, and each surviving statement — an
`import`, or the `require` CommonJS made of it — points back to its source
declaration through `ts.getOriginalNode`. That is `imports.runtime`, and
`runtime_dep` follows it. Per *binding*, CommonJS output keeps no trace, so
single names are left to `ref.kind`. *Rejected:* restating the elision rules in
the extractor (P8 restates them as an oracle instead — two copies that agree are
evidence, one copy is a guess), and the checker's internal
`isReferencedAliasDeclaration` (the rules minus the options, on an API with no
promise). It costs an emit: 1.4 s on tsdl. Raised 2026-09-11 by the user; the
first version had `runtime_dep` as "not `import type`", which was wrong for
every project not written under `verbatimModuleSyntax`.

**The CFG is statement-level and over-approximates exceptions.** Every node in a
`try` may throw to its handler; outside one, only `throw` reaches `throw_exit`.
A `finally` is entered by every jump crossing it and re-issues each outward.
Short-circuits and `?:` are decisions, not nodes. `let x;` defines nothing, so a
read it reaches has no reaching definition — that is what `undefined_use` finds.

**Dataflow is Doop-shaped**: flow-insensitive, field-based, context-insensitive
three-address facts. Named variables are symbol ids, so values cross modules
through imports with no extra fact; one `this` variable per class. Not modelled:
accessor calls hidden in property reads, object spread, method values read off
instances (reached through `call_site` instead), generator resumption.

**Module analogues, and coupling by kind** (added 2026-09-11). A class-less
codebase — tsdl has 0 classes — leaves the class rules empty, so `cohesion.dl`
gained `module_lcom4` (a file's exports grouped by what they share) and
`coupling_kinds.dl` classifies file pairs on Myers' scale, content to data, from
facts rather than counts; `packages.dl` checks package.json against imports.
Three extractor additions carry them: `member_access` covers object type
aliases and inline object types (column `class` renamed `owner`; tsdl 2,757 →
3,281 rows), the refs layer resolves a member named in brackets
(`obj["secret"]`, the way TypeScript lets code reach a `private`), and
`imports.builtin` / `package_dep.types_for` do the string work the engine
cannot. The coupling kinds lean on printed type text ("primitive" is exactly
`number`, `string`, …) — a heuristic, stated in the rule file's header.

**The rule library is split by cost** (the recipe's 35× lesson): `units.dl` and
`types.dl` are shared and small; `callreach.dl` is apart from `callgraph.dl`;
`dominators.dl` apart from `flow.dl`. The extractor *emits* and the library
*filters*: bulk commits are in the facts, and `cochange.dl`'s `bulk_limit` leaves
them out.

## What the tests found

The property layer (catalog in `../testing.md`) paid for itself on the
way in. P1 runs generated programs in Node and requires every step of the real
trace to be a CFG path: it found **a `for…of` head that re-evaluated its
iterable** each iteration. P3 (cyclomatic = E − N + 2) found **parallel branch
edges collapsing** when relabelled as `back`. P6 found **`project_file` rows in
the compiler's order**. P5's first guard certified nothing about the heap — the
load-rule mutation stayed green — and it gained a store-then-load op and a heap
guard before it counted.

Real code found more than the fixtures did: tsdl's test literals carry a lone
UTF-16 surrogate, which JSON escapes and the reader rejects (the writer now
emits well-formed Unicode), and installing the packaged bundle showed the git
layer reading a whole repository for a subdirectory root.

Two engine defects, both fixed in the same session: an empty JSONL file would
not import even under an explicit schema (`bugs/resolved/010` — the tool writes
many empty tables on a small project), and a variable type clash named its
union-find roots, a wildcard in another rule, instead of the slots that clashed
(`bugs/resolved/011` — found in a 50-rule `checks.dl`).

## Calibration

tsdl (TypeScript 7, 24.5k lines across `src` and `test`, 58 files, 130 commits),
both tsconfigs, 16-thread desktop:

| | |
|---|---|
| extract, all layers | 5.7 s — load 0.85, refs 1.8, structure 1.6 (its emit), quality 0.4, flow 0.3 |
| facts | 204,865 in 61 relations; `var` 30k, `ref` 15k, `flow_node` 15k |
| `lib/checks.dl` | 1.8 s, 319 MB, clean |
| `orient.dl` | 0.9 s |
| `modgraph.dl` cycles | 0.3 s |
| `callgraph.dl` / `callreach.dl` closure | 0.7 s / 1.2 s |
| `coupling.dl` | 1.7 s |
| `flow.dl` dead stores | 7.9 s, 182 MB |
| `dominators.dl` | 3.4 s |
| `pointsto.dl` | 3.3 s |
| `cochange.dl` | 2.3 s |
| `cohesion.dl` with `module_lcom4` | 1.8 s |
| `coupling_kinds.dl` | 3.3 s, 353 MB |
| `metrics.dl` | 1.0 s |
| `packages.dl` | 0.3 s |

Re-measured 2026-09-11 after the re-keying and `Provenance::Reports` (§ At a
million facts): every library is faster than it was and **every answer digest is
unchanged**, checked library by library against the previous spellings on this
same fact base. The largest moves were `coupling_kinds` 12 → 3.3 s,
`cohesion` 6.2 → 1.8 s and `checks` 4.3 → 1.8 s.

Findings, each verified against the source: `answer.ts ↔ eval.ts` is an import
cycle, but through `import type` on both sides (so `runtime_dep` has none);
`answer.ts` and `test/examples.test.ts` co-change 9 times with no static link —
the test pins golden output that `answer.ts` renders, reached only through
`run()`; of 41 project indirect calls, 27 resolve through points-to and the 14
left are callbacks a library invokes (a Promise's `resolve`, fast-check's `tie`).

ts-facts itself (6.5k lines): 87.6k facts in 2.4 s.

## At a million facts

VS Code 1.137.0 at `645f29cc`, `src/vs/base` — 155,801 lines across 485 files,
**1,332,798 facts in 59 relations**, extracted in 17.7 s at 1.56 GB. 6.5× tsdl's
facts. The sizing spike (`../../experiments/notes/code-design-pack.md`) found the
libraries taking **100× the time for 6.5× the data** and blamed the aggregates.
It was not the aggregates.

**It was the seek's leading-prefix rule.** The engine's only index is a
relation's own column order, and `seek::bound_prefix` stops at the first unbound
column (`../../datalog/spec.md` §17 2026-08-21). So an atom that binds a column
the relation does not lead with scans the whole relation, once per outer row.
Every library over 30 s was over it for this reason:

| rule | what it bound | what it scanned |
|---|---|---|
| `checks.dl`'s entry count | `flow_node`'s `fn`, `kind` — not `id` | 108,597 rows × 13,983 functions |
| `checks.dl`'s `alloc.site` key | `alloc`'s `site` — not `var` | 25,381² |
| `coupling.dl`'s `crossing` | `member`'s `File` — not `G` | 3,600 × 505 per ref, 99,881 refs |
| `cohesion.dl`'s `method` | `symbol`'s `parent` — not `id` | 70,310 × 662 classes |
| `cohesion.dl`'s `module_link` | `top_level`'s declaration — not its file | ~10k per ref |
| `metrics.dl`'s `fan_in` | `call_edge`'s callee — not its caller | 177,793 × 13,983 |
| `coupling_kinds.dl`'s `control_param` | `use`'s `var` — not its node | 57,312 × 12,429 params |

**The fix is one rule per re-keying**, and `lib/keys.dl` is where the reason is
written down: `child(P, S) :- symbol(id: S, parent: P).` and the rest
(`file_member`, `called_by`, `entry_node`, `alloc_of`, `decl_file`, `access_of`,
`used_at`, `touched_by`, `contains`, `extended_by`). Two of them took `checks.dl`
from 199.6 s to 13.7 s with the answers byte-identical.

**A re-keying is not free** — it materializes a copy of the relation — so it pays
only where the scan it replaces is quadratic. `comp_edge_to` was written for
`coupling.dl`'s `afferent`, measured at no gain against a scan of ~10⁷, and
removed.

Then the engine's share: an aggregate or a cast in any rule body used to
provision the **full** derivation store for the whole program, to carry §9's skip
count and §12's malformed count. `Provenance::Reports`
(`../../datalog/spec.md` §17 2026-09-11) keeps only the derivations those counts
are read from. Worth −32% time and −43% memory on `checks.dl`.

| library | spike | now | peak RSS | rows | digest |
|---|---:|---:|---:|---:|---|
| `checks.dl` | 196 s | **9.3 s** | 1,804 MB | 42 | `60e0b1a60d91` |
| `orient.dl` | 7.5 s | **4.9 s** | 1,077 MB | 15 | `34c4c8ec423e` |
| `modgraph.dl` | 1.5 s | 1.5 s | 305 MB | 670 | `ea1bb455007d` |
| `callgraph.dl` | 4.1 s | 4.9 s | 869 MB | 177,793 | `b4b166797098` |
| `callreach.dl` | — | **38.0 s** | 4,087 MB | 4,391 | `0c2ef0341aa9` |
| `coupling.dl` | 330 s | **27.2 s** | 1,073 MB | 5,973 | `ed0bce1f2667` |
| `cohesion.dl` | 404 s | **28.2 s** | 2,328 MB | 2,489 | `e1c73e914963` |
| `coupling_kinds.dl` | — | **24.6 s** | 2,029 MB | 60,399 | `cf3bffeab6d9` |
| `metrics.dl` | — | **6.6 s** | 1,165 MB | 30,614 | `bb6aa0d4262f` |
| `packages.dl` | — | 1.2 s | 288 MB | 190 | `7d103530aedf` |
| `flow.dl` | — | 104.2 s | 989 MB | 118 | `3e51cdb30022` |
| `dominators.dl` | — | 51.3 s | 1,086 MB | 2,950 | `0d84d957023c` |
| `pointsto.dl` | — | **stopped at 104 s** | **23.7 GB** | — | — |

Reproduce with `npm run bench -- --facts <dir>` (`tools/code-facts/bench/`),
which prints this table. **The digest is the guard**: it is a sha256 of the
library's answers to its own documented relations, and it did not move across any
of the changes above — a speed-up that moves a row is not a speed-up.

**Three libraries stayed expensive, and all three by construction** — each was
audited for the leading-prefix trap and has none of it:

- **`callreach.dl`, 38 s / 4.1 GB.** A whole-project call closure over 14k
  functions and 178k edges is quadratic in the graph's density.
  `callreach_seeded.dl` is the answer for a question about particular functions —
  `taint.dl`'s idiom, a `seed/1` the caller supplies.
- **`flow.dl`, 104 s**, and **`dominators.dl`, 51 s.** Reaching definitions and
  liveness over 108,597 flow nodes and 90,511 edges; `rd_in`/`rd_out` are
  node × definition × variable, which is the size of the analysis and not a
  spelling of it.
- **`pointsto.dl` does not fit: stopped at 104 s holding 23.7 GB.** A
  flow-insensitive, field-based points-to over 179,748 variables and 25,381
  allocation sites. It is the one library whose *question* has to change at this
  scale — narrowed to a subtree, or to the variables a question names.

So the playbook's own costs table has a ceiling in it now, and an agent working a
repository this size should reach for the module and design libraries (all under
30 s) before the flow ones.

## The Python frontend

`src/frontends/python/` — `py_facts.py` (structure, refs) and `py_flow.py` (flow,
quality), standard library only. Node spawns it and validates every streamed row
against `schema.ts`, so there is still one schema, one writer, one git layer and
one `lib/`; the rejected alternative, a Python writer, was a second schema home.
There is no type checker behind it: names resolve by LEGB and through imports,
attributes only on receivers whose type is known (module, class, `self`,
`super()`, an annotation, a constructor call), member lookup by C3. What a user
needs to know is `../skill/reference/python.md`.

What the tests found (`../testing.md`): P4-py, whose oracle is `python3`'s own
`__mro__`, caught a breadth-first MRO; P1-py caught a `match` guard sharing its
pattern's node, so a failed pattern "evaluated" the guard. Real code found the
rest: sqlparse's `__init__` importing its own submodules sent `from pkg import
sub` round a cycle to nothing (1,165 unresolved names), a local rebound from
itself (`x = x.next()`) recursed without end, and the experiments harness had a
lambda in a decorator's arguments that was never declared.

Calibration: sqlparse (5.6k lines) extracts in 0.7 s; the facts answer the
experiments' four `static_analysis` questions exactly as `truth.py` does.
`experiments/` (21.6k lines, 103 files): 4.3 s to 93k facts; `checks.dl` 4.0 s,
clean; with nested definitions set aside, 1116 of 1117 functions' branch counts
equal ruff's mccabe.

## Dogfooding the playbook — VS Code, whole repository

2026-09-12. The bundle installed into a `.claude/skills/` and the playbook
followed end to end, as a user would, on a repository nobody here wrote. Five
things came out of it; **none was reachable from the fixtures**, which are a few
dozen files and always extract every layer.

| what a user does | what happened |
|---|---|
| `code-facts src/tsconfig.json` | **V8 fatal OOM at 41 s**, native stack trace, no output — on a box with 20 GB free |
| the same, 12 GB heap | dies at **403 s / 13.1 GB** in `dataflow`, after five of six phases |
| `--layers refs,quality` | **8,905,689 facts, 276 s, 13.0 GB** — a 2.87M-line repository is analysable |
| `checks.dl` on that | **40 semantic errors, exit 2** — then, fixed, 115 s / 19.3 GB and **2 real violations** |
| `orient.dl` on that | 389 s / 21.8 GB, and `functions(0)` on 2.9M lines |

1. **Node's heap was never raised.** V8 caps the old space near 4 GB whatever the
   machine has. The tool was failing for want of a flag, not for want of memory.
2. **Nothing said how big the job was** until after the phase that died.
3. **A layer switched off had no schema**, so `checks.dl` — which covers every
   layer — could not run at all. The playbook told a large repository to drop
   layers and then broke on having done it, and the schema file's own comment
   claimed the opposite. A test had *pinned* the wrong behaviour.
4. **JSON modules, two facets, and the invisible one is worse.** A `require`d
   `.json` resolves on disk but never enters the program, so `target_file` named
   a file with no `file` row — the two violations. An *imported* one does enter
   the program and got a row with **`lang: ts`**, because `langOf` falls through
   to `ts` for anything it does not recognise. `json` is now a language, and a
   resolved target that is not a program file gets a synthesised row, so the row
   no longer depends on which spelling some *other* module used.
5. **`orient.dl` reported `functions(0)`** for a codebase with tens of thousands,
   because `fn` is a flow-layer relation and flow was off. The engine's
   `undefined-predicate` warning fires, but it names `fn/19` — not the number it
   made wrong — and `most_complex` simply vanished. **Every count is now gated on
   the layer it needs**, so a question these facts cannot answer gets no line at
   all, and `layer_extracted(L)` comes first.

**One failure mode, three mechanisms.** 3, 4 and 5 are the same thing: *the facts
and the summaries disagreed about what was missing, and the tool reported a
confident number over the gap instead of declining to answer.* No schema, a wrong
`lang`, an ungated count. `orient.dl` exists to prevent exactly that and was
committing it three ways — which is why fixing 4 immediately exposed a third
instance, `code_lines` summing config data as source.

**Verified end to end on the whole repository**, bundle rebuilt and reinstalled:
extraction 8,905,693 facts, `checks.dl` **exit 1, zero violations** in 104 s /
19.3 GB, `orient.dl` 360 s / 21.9 GB with no `functions` line and no
`most_complex` line. `production_files` 6,607 → **6,600** and `code_lines`
809,471 → **807,428**: nine JSON files, 2,621 lines of themes and manifests, had
been counted as TypeScript source.

**What it got right, on a codebase it had never seen**: 8,810 files, 809,471
production lines, 15,007 classes, 32,059 exported symbols;
`src/vs/base/common/lifecycle.ts` imported by **3,551** files;
`editorOptions.ts` the largest at 4,675 lines; and **zero runtime import
cycles** across the whole tree — checked against 81,056 runtime import edges, so
it is a finding and not an empty relation. The blind spots it reported honestly:
125,525 unresolved calls and 126,018 unresolved names, which is what a checkout
with no `node_modules` looks like and would be far smaller on a developer's own
machine.

## Dogfooding the playbook — Grafana, a real analysis

2026-09-12, and deliberately the other half of the VS Code run above: that one
stopped at `checks.dl` and `orient.dl`, so **the exploring half of the playbook
and the whole git layer had never been used on a real codebase**. Subject:
`grafana/grafana` at `9d9d93ee41` — 9,426 TypeScript files, 1.59M lines, 73,113
commits, `node_modules` installed, the packaged bundle dropped into its
`.claude/skills/`. Two scopes, each explored by a fresh agent whose only context
was the bundle's `SKILL.md` and `reference/`; every finding was then checked
against the Grafana source here.

| scope | extraction | `checks.dl` |
|---|---|---|
| `packages/grafana-ui`, **every layer** | 975 files / 133k lines → **1.17M facts, 67 s, 2.2 GB** | exit 1, clean, 10 s / 1.9 GB |
| root + 15 package tsconfigs, `refs,quality,git` | 8,910 files / 1.48M lines → **4.04M facts, 389 s, 16.8 GB** | exit 1, clean, 53 s / 9.2 GB |

### What broke, and what it cost

| what a user does | what happened |
|---|---|
| `datalog lib/orient.dl` on 4.0M facts — **step 3 of the method** | **killed at 76 s holding 21.1 GB** |
| the same after splitting `reach.dl` out | **207 s / 13.9 GB**, complete |
| any program importing `lib/modgraph.dl` for `dep` alone | **306 s / 12.6 GB**; 70 s / 3.3 GB with `dep` inline |
| `lib/cochange.dl`'s own question, same comparison | **351 s / 16.7 GB** against **59 s / 7.4 GB** |
| `lib/packages.dl` in a monorepo | four workspace siblings reported `unused`; 540 statements import them |
| a program with a **typo** | **11 s and 2.7 GiB** — the facts load before the program is checked |
| `lib/cohesion.dl` | 304 s / 9.0 GB, **11× its `vs/base` figure** at 3× the facts |

Two fixed here (`../decisions.md` 2026-09-12 later); the rest filed in `../bugs/`
and `../../datalog/bugs/`, and the missing capabilities on the two ROADMAPs.

### Re-measured after the engine learned to prune

Same 4.04M facts, later the same day: rule and relation pruning in the engine,
`reach.dl` folded back into `modgraph.dl`. `bench/`, one run each.

| library | time | peak RSS | recorded above |
|---|---:|---:|---|
| `orient.dl` | 179 s | 9.5 GB | 207 s / 13.9 GB |
| `modgraph.dl`, cycle questions asked | 209 s | 9.3 GB | 12.6 GB for `dep` alone |
| `coupling.dl` | 484 s | 9.3 GB | — |
| `coupling_kinds.dl` | 305 s | 9.1 GB | — |
| `checks.dl` | 42 s | 6.2 GB | 53 s / 9.2 GB |
| `cohesion.dl` | 285 s | 5.4 GB | 304 s / 9.0 GB |
| `metrics.dl` | 23 s | 4.4 GB | — |
| `callgraph.dl` | 29 s | 4.0 GB | — |
| `packages.dl` | 1.6 s | 0.17 GB | — |

**Nothing ran out of memory.** `pointsto.dl` and `callreach.dl` were not run —
neither fits even `vs/base` comfortably. What is left of `orient.dl`'s cost is its
own runtime closure, which a goal asks for; what is left of `modgraph.dl`'s is the
import closure, paid only because the bench asks about cycles. The slow ones are
now the coupling and cohesion libraries: time, not memory.

**The fix that mattered was a library-design mistake, not a rule.** `modgraph.dl`
carried the import graph's transitive closure — 17.45M pairs here — beside five
non-recursive rules, and both of its importers read only `dep`. The reason no
test caught it is worth more than the fix: the calibration corpus, VS Code's
`vs/base`, has **zero import cycles**, so the one superlinear rule in the library
was never exercised. Grafana's frontend has **915 files in 27 cycles, the largest
a 796-file SCC**. *A corpus chosen for size does not exercise shape.*

### Re-measured after the engine stopped copying

The same evening, the engine's memory work
(`../../datalog/notes/memory-profile-2026-09-12.md`): an import is no longer held
three times while it is typed, the base facts no longer twice for the whole run,
and no proof is built for a match nothing keeps. **Every digest is identical.**

On Grafana's frontend, `orient.dl` **224 s / 9.46 GB → 87 s / 4.94 GB**, answers
byte-identical, and its runtime closure alone 133 s / 6.73 GB → 44 s / 3.45 GB.
`callreach.dl`, not run here before on the grounds above, asked `recursive` and
`mutual`: **18 s / 1.37 GB**. `cochange.dl` asked `revisions` and
`hidden_coupling`: **337 s / 1.97 GB** (16.7 GB before the engine pruned).
`pointsto.dl` still does not fit: on `vs/base` it was stopped above 14 GB at
130 s — its cost is its own points-to closure, which none of this touched.

The rest of the Grafana bench, against § Re-measured after the engine learned to
prune above. Answers were compared here for `orient.dl`, `modgraph.dl` and
`packages.dl` (identical); the others rest on `vs/base`'s identical digests below.

| library | after pruning | now |
|---|---:|---:|
| `modgraph.dl`, cycle questions asked | 220 s / 9.3 GB | 75 s / **5.1 GB** |
| `packages.dl` | 1.6 s / 0.17 GB | 1.3 s / **0.09 GB** |
| `coupling.dl` | 484 s / 9.3 GB | 298 s / **3.8 GB** |
| `coupling_kinds.dl` | 305 s / 9.1 GB | 148 s / **6.1 GB** |
| `checks.dl` | 42 s / 6.2 GB | 35 s / **3.2 GB** |
| `cohesion.dl` | 285 s / 5.4 GB | 211 s / **3.1 GB** |
| `metrics.dl` | 23 s / 4.4 GB | 17 s / **2.4 GB** |
| `callgraph.dl` | 29 s / 4.0 GB | 20 s / **2.8 GB** |

On `vs/base`, `bench/` with the trunk binary and then the new one, back to back.
Other measurements were running, so the times are indicative; the peaks and
digests are not.

| library | trunk | now | digest |
|---|---:|---:|---|
| `callreach.dl` | 34.5 s / 3,492 MB | 13.4 s / **677 MB** | `0c2ef0341aa9` |
| `cohesion.dl` | 28.3 s / 1,945 MB | 17.2 s / **384 MB** | `e1c73e914963` |
| `checks.dl` | 8.7 s / 1,256 MB | 6.9 s / **686 MB** | no rows |
| `coupling_kinds.dl` | 21.0 s / 1,043 MB | 12.5 s / **677 MB** | `cf3bffeab6d9` |
| `dominators.dl` | 50.4 s / 892 MB | 33.6 s / **449 MB** | `0d84d957023c` |
| `coupling.dl` | 29.4 s / 645 MB | 16.4 s / **295 MB** | `ed0bce1f2667` |
| `flow.dl` | 68.6 s / 629 MB | 39.6 s / **492 MB** | `3e51cdb30022` |
| `callgraph.dl` | 3.1 s / 425 MB | 2.0 s / **319 MB** | `b4b166797098` |
| `metrics.dl` | 3.2 s / 401 MB | 2.1 s / **247 MB** | `bb6aa0d4262f` |
| `orient.dl` | 2.0 s / 332 MB | 1.6 s / **169 MB** | `124833fead3d` |
| `modgraph.dl`, cycle questions asked | 0.3 s / 65 MB | 0.2 s / **44 MB** | `ff07c4a2b1a3` |
| `packages.dl` | 0.1 s / 37 MB | 0.1 s / 39 MB | `7d103530aedf` |

`modgraph.dl` and `packages.dl` were measured over the repository's `lib/`: a fact
directory's own `lib/` is copied at extraction and goes stale, and the copies
under `~/.cache` predate `reach.dl` being folded back — which is how a first
Grafana run "asked about cycles" in 1.2 s.

**What a non-recursive library costs now is the table it imports**, held once:
`symbol` alone is 1.37 GB on Grafana, and a library reading 4 of its 18 columns
could load it in 0.58 GB — column projection, a datalog design item.

### What it found, verified in the source

On `@grafana/ui`: five independent measures — complexity × churn, revisions,
`module_lcom4`, a runtime import cycle, co-change spread — all land on
`components/Table/TableNG`, whose `utils.ts` is 1,934 lines that `module_component`
splits into one blob of 32 exports and **14 free-floating singletons**. A real
latent bug at `useDragAndDrop.tsx:109` (an uncaught `.then()` on a promise
memoised with `??=`, so one chunk-load failure poisons the cache for the page).
`jquery`, `@types/jquery` and `react-router-dom` declared as runtime dependencies
of a **published** package that imports none of them.

On the whole frontend, asked the maintainers' own in-flight question — decoupling
the built-in plugins, enforced by eslint for four data sources and a second rule
shipped commented-out — it produced a migration plan: two plugins can have the
rule switched on today; `public/app/features/canvas` is 30 files that **nothing
outside the canvas panel imports** (so moving it clears 68 of that plugin's 98
blocking imports in one commit); the best-ratio move is a **75-line** file that
blocks nine plugins and is the sole blocker for two; and the direction nobody
lints is the larger one — 386 core→plugin imports, plus a runtime edge into a
plugin already declared decoupled.

**`imports.runtime` was the difference between right and wrong**, not a nicety:
**88 of 490** blocking imports are type-only, and `imports.kind` says `static`
for all 490, because Grafana writes `import { type X }` and never `import type`.

### Three oracles, and the one confident negative

The git layer agrees with `git log --follow` exactly — 93 commits touching
`TableNG.tsx`, 93 `touch` rows, minus 6 bulk commits = `revisions` 87 — including
rename-following through a directory move that a plain `git log -- path` misses.
`files_in_runtime_cycles(915)` matches an independent Tarjan SCC written in
Python. And a regex import-scanner written before either agent ran agrees plugin
by plugin, reproducing 0 blocking imports for the four plugins eslint already
enforces.

Against that, the one wrong answer: `n_pkg_to_public(0)` — "nothing under
`packages/` imports `public/`", reported as the strongest structural result —
**is false**. `packages/grafana-ui/.storybook/preview.ts:22` imports
`../../../public/sass/grafana.light.scss`, and `.storybook/tsconfig.json` was not
among the tsconfigs given. The query could not have found it either way: a
`.scss` specifier resolves to `target_ambient`, never `target_file`. *A confident
negative over a blind spot* is the failure mode the playbook names, and step 5 —
open the source — is what caught it.

## Subjects — what each one exercises, and what is still unexercised

Kept because choosing the next dogfood subject by *size* is what hid the closure
bug for a month: `vs/base` was picked as the big corpus and happens to have zero
import cycles, so the library's one superlinear rule never ran. **Pick the next
subject by the shape it adds, not the line count.**

Run so far (✓ = exercised, — = present but thin, ✗ = zero rows):

| subject | lines | cycles | monorepo | classes | decorators | history |
|---|---|---|---|---|---|---|
| tsdl | 24.5k | — | ✗ | ✗ (2) | ✗ | thin |
| `experiments/` (Python) | 21.6k | — | ✗ | ✓ | ✗ | thin |
| VS Code `vs/base` | 156k | **✗ zero** | ✗ | ✓ | ? | none extracted |
| VS Code `src/` | 2.87M | ✗ zero runtime | ✗ | ✓ 15,007 | ? | none extracted |
| `@grafana/ui` | 133k | ✓ 2 | ✓ | — (96) | ✗ | ✓ 20k commits |
| Grafana frontend | 1.48M | ✓ **915 files, 27 SCCs** | ✓ 21 pkgs | ✓ 898 | ✗ | ✓ 20k commits |

**What no subject has exercised yet**, from the fact bases on disk:

- **`decorator` — zero rows, every extraction.** The whole decorator half of the
  structure layer has never seen a real subject. A NestJS, Angular or TypeORM
  codebase is the fix, and it is the largest untested surface the extractor has.
  (VS Code's DI decorators may have covered it in the 2026-09-12 run; that fact
  directory was not kept, so treat the `?` above as unknown, not as zero.)
- **`implements` is thin** — 2 rows in `@grafana/ui`, 197 across the whole
  frontend. CHA's virtual-call expansion (`callgraph.dl`, and traps 2–4) rests on
  `implements`/`extends`, so a nominally-typed, interface-heavy codebase would
  test the call graph far harder than React components do.
- **Python at any scale** — the largest Python subject is 21.6k lines. Nothing has
  stressed the Python frontend's LEGB/C3 resolution the way Grafana stressed the
  TypeScript one.
- **`flow`/`dataflow` above ~150k lines** — every layer fits at 133k and dies at
  2.87M; the interesting middle is untried.

**Candidates, surveyed 2026-09-12 but not run.** Sizes are GitHub language bytes,
so they are an order of magnitude, not a measurement; none of the shape columns
above is known for these until one is extracted.

- **excalidraw** — ~8 MB TS, ~104 MB clone, a small packages monorepo, deep
  history, many contributors. *Proposed this session and passed over for Grafana
  because it adds no scale*, but it is the obvious pick when the question is the
  exploring loop rather than the ceiling: every layer fits in a couple of GB, so
  iteration is seconds and `flow`/`dataflow`/`pointsto` are all reachable.
- **nestjs/nest** (~3.6 MB TS) or **typeorm** (~10.7 MB) — decorator-saturated
  and class-heavy. Either closes the two gaps above in one run.
- **vuejs/core** (~4.5 MB), **astro** (~7.9 MB) — mid-size, single-purpose,
  useful as a second opinion on measures that looked odd on Grafana.
- **kibana** (~450 MB TS) — the next ceiling if one is ever wanted. Roughly 3× the
  whole Grafana frontend.

## Open

- The intermittent `npm test` failure once listed here was P5's heap guard
  missing about one suite in 30 (fixed 2026-09-11, `../testing.md` P5), found
  by running the suite 20 times with the spec reporter. P2-py had a second.
- TypeScript 7's API, when it stabilizes: a second backend, behind P1–P8.
- Accessors, spread and instance method values in the dataflow layer — each a
  known under-approximation of `pts`, listed above.
