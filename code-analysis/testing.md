# Testing — `code-analysis/`

`npm test` in `tools/code-facts`: `node:test`, fixtures under `test/fixtures/`, and
fast-check properties in `test/properties/`. The engine is found at `DATALOG_BIN`
or `../../../datalog/target/release/datalog` (build it with
`cargo build --release --offline` in `datalog/`).

The discipline is the repo's: [`../datalog/testing.md`](../datalog/testing.md)'s
**four rules** are normative here too — an equivalence claim ships as a property,
every generator carries a non-vacuity guard read against its property's sentence,
a property is mutation-verified with the mutation recorded on its line, and a
widened generator needs a matching acceptance property. Every entry below states
its mutation.

## Property catalog

Added 2026-09-10 with the tool (`notes/code-facts.md`), in `datalog/testing.md` until
the tool moved here 2026-09-11. fast-check generators in
`test/properties/`; `../datalog/testing.md`'s four rules apply as they do to the engine. P1 and P5 are
**independent** oracles — Node executes the generated program — and P2 and P4
restate the language (module resolution, member lookup) rather than calling the
checker. Run counts: `CODE_FACTS_RUNS` (P1/P3 default 200, the rest 25–40).

- [x] **P1** CFG soundness against execution: every consecutive pair of probes in
  a real trace is a path through probe-free nodes of the extracted graph; guard:
  back, break, continue, throw, catch, finally, case and default edges all
  exercised. *Mutation:* `implicitThrow` a no-op → red. Found, on its way in, a
  `for…of` head that re-evaluated its iterable.
- [x] **P2** Module-graph fidelity: `imports`, `import_name` (through namespaces
  and barrels) and `call_site` equal the generated graph; guard: cross-file
  imports, and a barrel. *Mutations:* no alias resolution → red; module-level
  functions dispatched `virtual` → red.
- [x] **P3** Cyclomatic two ways: `fn.cyclomatic` = E − N + 2 on exception- and
  short-circuit-free programs; guard: if, while, do, for, for-of and case all
  occur. *Mutation:* `case` decisions uncounted → red. Found parallel branch edges
  collapsing under relabelling.
- [x] **P4** Hierarchies: `extends`, `implements`, `overrides` equal the model,
  whose oracle is TypeScript's member lookup restated; guard: overrides occur,
  including of an interface member. *Mutation:* `implements` bases skipped → red.
- [x] **P5** Points-to soundness against execution: every object or function a
  probed variable holds at run time is in its `pts`; guard: cross-function
  observations, function values, and values that can only have come through a
  field. *Mutations:* the heap-load rule removed → red (**green** before the heap
  guard and round-trip op existed — the first guard certified nothing about the
  heap); the parameter rule removed → red.
- [x] **P6** Determinism: permuting a tsconfig's `files` changes no output byte;
  guard: most runs actually reorder. *Mutation:* sources unsorted → red. Found
  `project_file` in compiler order.
- [x] **P7** Schema: the writer rejects malformed rows; every table imports, and
  the engine's row count of each equals `relation_rows`.
- [x] **P8** Import elision: `imports.runtime` is true exactly when TypeScript's
  rule, restated (side-effect kept; `import type` never; under
  `verbatimModuleSyntax` everything else kept; otherwise kept iff an unmarked
  binding is used as a value), keeps the statement; guard: a plain import elided,
  one kept, one kept by `verbatimModuleSyntax`, and an `import type`.
  *Mutation:* `runtime` from syntax (`kind != type_only`) → red.
- [x] `lib/checks.dl` finds no violation on every fixture, on code-facts itself,
  and on `~/code/tsdl` when present.
