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
  a real trace is a path through probe-free nodes of the extracted graph, whether
  the statements run as a function body or as a class static block; guard: back,
  break, continue, throw, catch, finally, case and default edges all exercised,
  and inside static blocks both outcomes and back, break, throw, finally, case and
  default (continue and catch are too clumped to guard there: fewest 2 and 7 in
  ten runs). Acceptance (rule 4): Node compiles every generated static block, and
  exactly one `static_block` fn is extracted for it. *Mutations:* `implicitThrow`
  a no-op → red; `isOwner` skipping static blocks, so their calls belong to the
  class → red; `extractFlow` skipping static blocks → red, on the acceptance
  assertion. Found, on its way in, a `for…of` head that re-evaluated its iterable.
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
- [x] **P9** Shared parsing: extraction with one parsed copy of a file handed to
  every tsconfig whose settings parse and bind it the same (`program.ts`,
  `shareParsedFiles`) equals extraction with each program parsing its own, byte
  for byte, over P2's projects under 2–3 overlapping tsconfigs that differ in
  `paths` (shares), `target` and `moduleDetection` (must not). Guards: a file
  listed in two sharing configs (218/500), and the script witness — a global
  declared by a script, called from a module whose program took the script from a
  config with the other detection (246/500). *Mutation:* key on file name alone →
  red, 3 of 3 suites, shrinking to that witness. **Green at first**, and its
  witness at 20/500: under `nodenext` the default `moduleDetection` binds every
  `.ts` file as a module, so only `legacy` makes the two detections differ, and a
  randomly placed witness was a coin that rarely landed.
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
- [x] **P1-go** CFG soundness against real Go executions, P1's method with Go's
  constructs: three-clause, conditional, bare and `range` loops; labeled `break`
  and `continue`; `goto`; tagged and tagless `switch` with `fallthrough`;
  `select` with `default`; `defer` of a plain and of a recovering call; panics
  from a statement and from `panic`. Three programs per run, the last "doomed" (a
  recovering defer first, `panic(0)` last). Two harness rules: a `finally` node
  may run no deferred call, so a path may pass it; probes of one node (a
  select's channel operands) are evaluated together, so a step between two of
  them is no edge. Guards: back, break, continue, throw, finally, case, default,
  fallthrough, goto and select edges used; an elif (the rarest: 22 of 200 runs),
  a select case, a default clause and both labeled jumps reached through probes
  just before them; a recovery returning normally, in the doomed function too.
  *Mutations:* implicit panics off → red; no resume at `exit` after a recovery →
  red (**green** before the doomed function: wherever a function can end
  normally, the edge is redundant); `fallthrough` carried by edge kind → red —
  the first version, which P1-go found carrying a fallthrough through an empty
  clause into the next; select operands evaluated at the cases → red; `goto`
  edges dropped → red; labels ignored → red.
- [x] **P3-go** Cyclomatic two ways for Go, on programs without panics or
  defers (with `goto`, `fallthrough`, `select` and labeled jumps). Guard: if,
  for, for_of and case decisions occur. *Mutation:* `range` decisions uncounted
  → red.
- [x] **P2-go** Module-graph fidelity for Go, over P2's model rendered as one package
  per directory: `imports` and `import_name` equal the graph — a row per file of
  another package the importer references, an `implicit` row per other file of
  its own package, at the first reference. References between packages keep one
  direction (Go forbids import cycles). Guards: imports across packages (276 of
  400 runs), within one (204), and an import naming two files of its package
  (73 — so 40 runs by default). And `call_site` equals the generated calls, each
  `static`, within a package and across one. *Mutations:* no `implicit` rows →
  red; one row per import spec rather than per file referenced → red; function
  calls dispatched `virtual` → red.
- [x] **P4-go** Hierarchies for Go: `implements` and `overrides` equal what the
  compiled program says, and `extends` and `embeds` equal the generated
  embedding, over interfaces that embed others and structs that declare methods
  with value or pointer receivers and embed by value or pointer. The oracle is
  independent: the program runs, reflect says whether *T implements I, and each
  method called through reflect prints the declaration that ran (the satisfying
  method, or the one a declaration hides). Guards, at 400 runs: implemented
  (248) and not (388), satisfied by a promoted method (182), only through the
  pointer (321), an embedded interface (140), a hidden method (205). *Mutations:*
  no pointer method set → red; promoted methods skipped → red; no hiding
  overrides → red; no interface `extends` → red. Widened to generic types
  (`T[X any]`, used as `T[int]`), with `implements.pointer` checked against
  reflect's answer for the value type; acceptance guard: a generic type implemented
  an interface. *Mutations:* generic types skipped → red; `pointer` always false →
  red. Widened to types declared in the test file, which `go test` compiles into
  another view of the package (the oracle runs as a test); acceptance guard: a
  test-file type implemented an interface. *Mutation:* matching by the first
  view that holds both import paths, not the type's own → red at the first case.
  The widenings diluted the promoted guard: runs with each guard, at 200, are
  promoted 33, pointer-only 84, hidden 49, generic 71, test-file 77 — so 50 runs
  by default; at 25 the promoted guard missed about one suite in 90.
- [x] **P5-go** Points-to soundness against real Go executions, P5's method: the
  program (variables and fields of type `any`, objects `&O{s: N}`, stores and
  loads through a type assertion to `*O`, direct calls, function values, calls
  through a type assertion to the function type) is compiled and run, and every
  object or function a probed variable holds is in `pts`. As P5 forces a heap
  round trip, every function after the first forces a call through a function
  value into `k`, which nothing else writes. Guards, at 200 runs: an object
  observed outside the function allocating it (131), a function value (146), a
  value only a field can have carried (131), an object returned through a call
  of a function value (78). *Mutations:* field loads not emitted → red; formals
  not emitted → red; functions not allocated in `<module>` → red; interface
  conversions not copied → red; no `callee_var` for a call through a function
  value → red (**green** before the forced indirect call: the random ops rarely
  put a function where a call used it).
- [x] **P2-java** Module-graph fidelity for Java, over P2's model rendered as one
  package per directory and one class of static methods per file: `imports` and
  `import_name` equal the graph — a named import of one function is a static
  import, of several (or a namespace import, or any other reference into a package
  imported with `*`) an on-demand import with a row per file it reaches, an import
  through a barrel a single-type import, and a reference within the package an
  `implicit` row. References go both ways (Java allows cycles). Read as plain
  sources, so no build is in the loop. Guards, at 200 runs: a static import (109),
  a single-type import (46), an on-demand import (123), a reference within a
  package (99), and an on-demand import reaching two files (40 — so 40 runs by
  default). *Mutations:* no `implicit` rows → red at the first case; an on-demand
  import as one row → red; static imports with no target file → red.
- [x] **P6-java** Determinism for Java: permuting a Maven reactor's `<modules>`
  order changes no output byte, over P2-java's sources in one module and three
  more that each declare `x.T`, with overloads whose ids collide and a static
  initializer. Four runs by default: each is two Maven extractions. Guard: a run
  reorders. *Mutation:* modules left in the reactor's order → red (89 failing
  cases while shrinking).
- [x] **P6-go** Determinism for Go: permuting a go.work's `use` order changes no
  output byte in any layer, over P2-go's module and three more whose two `init`s
  collide and whose package-level variables are allocation sites.
  Guard: most runs reorder. *Mutations:* sources left in load order → red;
  package-level variables allocated in map order → red at the first case — the
  Go dataflow layer's first version, which the structure-only property missed
  (`bugs/resolved/007`).
- [x] `lib/checks.dl` finds no violation on every fixture, on code-facts itself,
  on `~/code/tsdl` when present, and on sqlparse when the experiments harness has
  cached it. On sqlparse the four `static_analysis` answers computed from the
  facts also equal `truth.py`'s (checked by hand 2026-09-11, not in the suite).
