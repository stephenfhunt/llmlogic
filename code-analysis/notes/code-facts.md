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

## Open

- The intermittent `npm test` failure once listed here was P5's heap guard
  missing about one suite in 30 (fixed 2026-09-11, `../testing.md` P5), found
  by running the suite 20 times with the spec reporter. P2-py had a second.
- TypeScript 7's API, when it stabilizes: a second backend, behind P1–P8.
- Accessors, spread and instance method values in the dataflow layer — each a
  known under-approximation of `pts`, listed above.
