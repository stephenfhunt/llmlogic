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
**independent** oracles — Node executes the generated program — as is P4-py,
where Python does; P2 and P4 restate the language (module resolution, member
lookup) rather than calling the checker. Run counts: `CODE_FACTS_RUNS` (P1/P3 default 200, the rest 25–40). A
non-vacuity guard that needs a shape to occur *once* is a coin flipped every
run: size its default run count from the measured per-run rate, not by eye.

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
  heap); the parameter rule removed → red. The heap case was left to chance until
  2026-09-11 and occurred in 32 of 400 runs — the guard failed about one suite in
  30, the intermittent `npm test` failure the worklog had left unexplained. Each
  function now round-trips a value through a field into `h`, which nothing else
  writes (83 of 200 runs observe it).
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
- [x] **P2-py** Module-graph fidelity for Python, over P2's model rendered as
  packages: `imports`, `import_name` and `call_site` equal the graph, with
  imports spelled absolute or relative, a namespace import as `import a.b as ns`,
  `from a import b as ns` or plain `import a.b`, a barrel via `__all__`, and
  usually each `__init__` importing its own submodules; guard: each spelling
  occurs, and a `from pkg import m` through an `__init__` that imports `m` (40
  runs by default: at 25, with that case rarer, the guard missed about one suite
  in ten — an intermittent `npm test` failure).
  *Mutations:* the `__init__` self-import row kept → red; no fall-through to the
  submodule when a package's binding cycles → red; relative levels above one
  ignored → red; plain `import a.b` binding `a.b` rather than `a` → red.
- [x] **P4-py** Hierarchies for Python, with multiple inheritance: `extends` and
  `overrides` equal what **`python3` reports** — `__bases__`, and the class each
  direct base's `__mro__` finds a member on — an independent oracle; guard:
  overrides, multiple inheritance, and a lookup where C3 and breadth-first
  disagree (the generator is shaped for it: 54 of 200 runs). *Mutations:*
  breadth-first MRO (the frontend's first version) → red; depth-first → red;
  overrides against the first base only → red; no attribute through a module
  (`mod.Base`) → red.
- [x] **P1-py** CFG soundness against real Python executions, P1's method with
  Python's constructs: loop and `try` `else:`, typed and bare `except` (so an
  exception can pass a clause that does not match), `with` whose `__exit__`
  suppresses on some inputs, `match` with guards and a wildcard, `assert`; three
  programs per run. Guard: back, break, continue, throw, finally, catch, case,
  default and a catch's `on_false` all used, each else/elif/wildcard/typed-handler
  probe reached, and a suppressing `with` followed by a normal return (the rarest,
  an executed `continue`, in 35 of 200 runs). *Mutations:* implicit throws off →
  red; `break` routed through the loop's `else` → red; `with` pushing no frame →
  red; no propagation past the last typed clause → red; a guard folded into its
  pattern's `case_test` → red — the frontend's first version, which P1-py found.
- [x] **P3-py** Cyclomatic two ways for Python, on exception-free programs.
  *Mutation:* a `match` guard's decision uncounted → red.
- [x] `lib/checks.dl` finds no violation on every fixture, on code-facts itself,
  on `~/code/tsdl` when present, and on sqlparse when the experiments harness has
  cached it. On sqlparse the four `static_analysis` answers computed from the
  facts also equal `truth.py`'s (checked by hand 2026-09-11, not in the suite).
