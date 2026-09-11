# ts-facts — a TypeScript project as Datalog facts

Overflow for `spec.md` §17 2026-09-10. The tool is `skill/tools/ts-facts`, run as
`skill/ts-facts`; the agent-facing guide is `skill/recipes/typescript.md`, and the
fact schema's one normative home is `src/schema.ts` (every output also carries a
generated `SCHEMA.md`). This note is the *why*: what was decided, what was
rejected, and what the tests found.

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

**It lives in the skill** (`skill/tools/ts-facts`), because it is part of the
deliverable: `cargo package-skill` ships it (without `test/` or dev packages) and
vendors its TypeScript. Its tests live beside it and run with `npm test`.

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

The property layer (catalog in `testing.md`, *ts-facts*) paid for itself on the
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
| `lib/checks.dl` | 4.3 s, 519 MB, clean |
| `modgraph.dl` cycles | 0.3 s |
| `callgraph.dl` / `callreach.dl` closure | 0.9 s / 1.8 s |
| `coupling.dl` | 3.3 s |
| `flow.dl` dead stores | 9.4 s, 591 MB |
| `dominators.dl` | 4.3 s |
| `pointsto.dl` | 3.3 s |
| `cochange.dl` | 2.3 s |
| `cohesion.dl` with `module_lcom4` | 6.2 s |
| `coupling_kinds.dl` | 12 s, 667 MB |
| `packages.dl` | 0.3 s |

Findings, each verified against the source: `answer.ts ↔ eval.ts` is an import
cycle, but through `import type` on both sides (so `runtime_dep` has none);
`answer.ts` and `test/examples.test.ts` co-change 9 times with no static link —
the test pins golden output that `answer.ts` renders, reached only through
`run()`; of 41 project indirect calls, 27 resolve through points-to and the 14
left are callbacks a library invokes (a Promise's `resolve`, fast-check's `tie`).

ts-facts itself (6.5k lines): 87.6k facts in 2.4 s.

## Open

- One full `npm test` run showed a failure that 25 further runs and 30 runs of
  each property file did not reproduce. Unexplained; the next one should be
  captured with the default reporter rather than `dot`.
- TypeScript 7's API, when it stabilizes: a second backend, behind P1–P8.
- Accessors, spread and instance method values in the dataflow layer — each a
  known under-approximation of `pts`, listed above.
