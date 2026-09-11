# A TypeScript codebase, extracted

For TypeScript you do not write the extractor. `./code-facts` (next to
`./datalog`) reads a project with the TypeScript compiler — the type checker
resolves every name, so the call graph is resolved rather than guessed — and
writes 61 relations across seven layers, from packages and import graphs down to
control flow, def/use, points-to inputs, and git history. A rule library
computes coupling, cohesion, reachability and the rest; you write the questions.

`bring-your-own.md` is the general method and its traps (for a language no
extractor here reads); this file is what is specific to these facts, and
`../SKILL.md` is how to explore them.

## 1. Run it

```sh
./code-facts path/to/tsconfig.json [more tsconfigs] -o /tmp/facts
./datalog /tmp/facts/lib/checks.dl        # exit 1 = no violations: the facts are consistent
```

Needs Node.js ≥ 22.18. Give **every** tsconfig the project uses — tests often
live in their own (`tsconfig.test.json`), and a test file missing from the facts
makes every "untested" answer wrong. `references` are followed. The root (for
every path in the facts) is the git top level unless you pass `--root`; a root
below it reads that subtree's history only. Options: `--layers
refs,flow,dataflow,quality,git` to extract less, `--no-git`, `--git-since DATE`,
`--exclude GLOB`. A 24k-line project extracts in about 6 s to 205k facts.

```
/tmp/facts/
  schema/<layer>.dl, schema/all.dl   import these — explicit types, one line per relation
  lib/*.dl                           the rule library; each imports the schema it needs
  facts/*.jsonl                      the tables
  SCHEMA.md                          every relation and column, documented
```

Write your analysis as a file next to them (`/tmp/facts/q.dl`) that starts
`import "lib/coupling.dl".` — paths are relative to the importing file.

## 2. What is in it

| layer | the relations you will use most |
|---|---|
| structure | `file` (path, dir, package, is_test, loc), `symbol` (id, kind, file, parent, exported, …), `imports` (file → target_file / target_package, kind, and whether it survives to `runtime`), `exports`, `file_ancestor` (every enclosing dir, with depth) |
| refs | `ref(from, to, kind)` — every resolved reference between declarations; `call_site` (caller, callee, **dispatch**); `extends`, `implements`, `overrides`; `member_access` (class, interface and object-type members — for cohesion and stamp coupling); `type_ref` (with position: param, return, …) |
| flow | `fn` (one per function body, with cyclomatic, cognitive, nesting, Halstead); `flow_node`/`flow_edge` (a CFG per function); `def`/`use`; `closure`; `call_at` |
| dataflow | `assign`, `alloc`, `load`, `store`, `formal`/`actual` — value flow in three-address form |
| quality | `diagnostic`, `any_site`, `assertion`, `literal`, `floating_promise`, `ts_directive`, `lint_directive`, `comment_marker`, `throw_site`, `catch_site` |
| git | `commit`, `touch(sha, path, path_now, …)` — `path_now` follows renames to today's file |

**Ids.** Every symbol-valued column in every relation uses one id-space:
`src/eval/run.ts#Evaluator.run`; a local is `…#run.x`; an anonymous function is
named by position (`…#main.<arrow@32:14>`); a file's top-level code is
`src/a.ts#<module>`; outside the project, `ext:react#useState` and
`lib#Array.map`. A `const f = () => …` *is* `#f`. Flow nodes, call sites and
allocation sites are integers.

**Enum columns are symbols** — write them bare: `symbol(kind: class)`,
`call_site(dispatch: virtual)`. `kind: "class"` is a type error that says so.

## 3. The library

Import the one you need, not all of them — evaluation computes everything a
program imports (`bring-your-own.md` §2). Two columns: the 24k-line project
above (205k facts), and VS Code's `src/vs/base` (156k lines, **1.33M facts**) —
the second is what a big repository costs, and the ratio is not the fact ratio.
Both include the import.

| file | what it derives | tsdl | `vs/base` |
|---|---|---|---|
| `checks.dl` | `violation(Check, Subject)` — the extractor contradicting itself | 1.8 s | 9.3 s |
| `modgraph.dl` | `file_dep`, `runtime_dep` (the imports emitted JavaScript keeps), `in_cycle`, `cycle_edge`, `unit_dep` (directories at any depth), `package_edge`, `external_dep` | 0.3 s | 1.5 s |
| `callgraph.dl` | `call_edge` (virtual calls expanded to every override), `call_edge_lexical` (a callback's calls counted as its enclosing function's), `called` | 0.7 s | 4.9 s |
| `callreach.dl` | `reaches`, `recursive`, `mutual` — **the one that stays expensive**: a whole-project closure is quadratic in the call graph's density | 1.2 s | **38 s, 4.1 GB** |
| `callreach_seeded.dl` | `reaches_from` — the same closure grown only from a `seed/1` you supply. Use it instead wherever the question names particular functions | — | — |
| `coupling.dl` | per component: `efferent`, `afferent`, `instability`, `abstractness`, `distance`, `sdp_violation`, `comp_edge_weight`; per type: `cbo` | 1.7 s | 27 s |
| `cohesion.dl` | per class: `lcom4`, `tcc`, `lcom_hs`; per file: `module_lcom4`, `module_component`; per component: `relational_cohesion` | 1.8 s | 28 s |
| `coupling_kinds.dl` | Myers' scale: `content_access`, `common_state`, `shared_literal`, `control_param`, `stamp_param`, `data_call`; per file pair `module_coupling`, `worst_coupling` | 3.3 s | 25 s |
| `packages.dl` | against package.json: `undeclared`, `unused`, `dev_in_production`, `only_in_tests`, `types_only` | 0.3 s | 1.2 s |
| `metrics.dl` | `dit`, `noc`, `wmc`, `rfc`, `fan_in`, `fan_out` | 1.0 s | 6.6 s |
| `flow.dl` | `reachable`, `unreachable`, `reaches_def`, `def_use`, `undefined_use`, `live_out`, `dead_store` | 7.9 s | **104 s** |
| `dominators.dl` | `dominates`, `back_edge`, `loop_header` | 3.4 s | **51 s** |
| `pointsto.dl` | `pts`, `heap`, `target`, `call_edge_pt` (indirect and structural calls resolved), `call_edge_pt_lexical`, `unresolved_call` | 3.3 s | **does not fit** — 23.7 GB and climbing at 104 s; narrow the question first |
| `taint.dl` | `tainted`, `tainted_sink` — you supply `source/1` and `sink/1` | pointsto + | pointsto + |
| `cochange.dl` | `revisions`, `cochange`, `confidence`, `hidden_coupling`, `churn`, `author_commits`, `main_author`, `first_change`, `last_change` | 2.3 s | no git history |

**Writing your own rules? Put the join key first.** The engine indexes a relation
by its own column order and seeks a **leading** prefix, stopping at the first
unbound column, so an atom that binds a column the relation does not lead with
scans the whole relation once per outer row. `symbol` leads with `id`, so
`symbol(id: S, parent: P)` with only `P` known is a full scan of every symbol —
which on a large project is the difference between seconds and minutes. Re-key it
first: `child(P, S) :- symbol(id: S, parent: P).` and join on `child`.
`lib/keys.dl` holds `child`, and each library carries the re-keyings it needs
(`file_member`, `called_by`, `decl_file`, `access_of`, `used_at`). The copy is
not free, so it pays where the scan it replaces is quadratic and not otherwise.

**No classes? Use the module versions.** Much TypeScript has none — the project
above has 0 classes and 94 exported functions — so `lcom4`, `wmc`, `dit` and
`cbo` come back empty there. The module is the unit of design instead:
`module_lcom4` groups a file's exports by what they share (N > 1 is N modules in
one file), `coupling_kinds.dl` classifies how two files are coupled, and
`coupling.dl`'s component rules already work per file.

**Components** (coupling, cohesion) are `(G, C)` pairs: `G = -1` files, `-2`
packages, `N ≥ 0` directories at depth N. `instability(1, C, I)` is instability
between top-level directories; `lib/units.dl` has the membership rule.

## 4. Questions worth asking

The library answers the textbook measures; these are the questions it makes
cheap to ask, each a few rules. All ran on the project above.

| question | shape |
|---|---|
| what changes together but shares no code? | `hidden_coupling(A, B, N), N >= 3` — co-change with no import or reference either way |
| which modules break the Stable Dependencies Principle? | `sdp_violation(G, A, B)` |
| which classes want to be two? | `lcom4(C, N), N > 1` |
| which files are several modules sharing a name? | `module_lcom4(F, N), N > 1`, then `module_component(F, R, E)` for the groups |
| how are two modules coupled — not how much, but how? | `worst_coupling(FA, FB, K)`; `content` and `common` first |
| which functions take a whole record and read one field? (stamp coupling, ISP) | `stamp_param(F, T, Used, Total)`, lowest `Used` against `Total` |
| is package.json telling the truth? | `packages.dl`: `undeclared`, `unused`, `dev_in_production`, `types_only` |
| which exports does nothing else use? | `dead_export` below — exclude your entry points |
| what does no test reach? | `untested` below — over `call_edge_pt_lexical` |
| where do internal types leak through the public API? | `type_ref(from: F, to: T), symbol(id: F, exported: true), symbol(id: T, origin: project, exported: false)` |
| where does `any` enter, and how far does it spread? | `any_site` counted by file; to follow one, seed `taint.dl` with the `actual_ret` of the call on its line |
| which string literals encode a shared meaning? (connascence of meaning) | the same `literal(kind: string, value: V)` in several files |
| which files are hot spots? | per-file `sum` of `fn.cyclomatic` × `revisions` |
| which files does every change drag along? (shotgun surgery) | count of `cochange(A, B, N), N >= 3` partners per file |
| do module boundaries follow people? (Conway) | `runtime_dep(A, B), main_author(A, EA, _), main_author(B, EB, _), EA != EB` |
| which functions can let an exception escape? | `throw_site` closed backwards over `call_edge_lexical`, stopped by `catch_site` |
| what reads a variable before anything writes it? | `undefined_use(N, V)` |

Two spelled out, because each needs a choice the table cannot show:

```datalog
% Exports nothing in another file refers to. A public API re-exported from an
% entry point counts only as a re-export, so name your entry points.
import "schema/structure.dl".
import "schema/refs.dl".
used_elsewhere(S) :- ref(from: F, to: S), symbol(id: F, file: A), symbol(id: S, file: B), A != B.
entry_export(S) :- exports(file: "src/index.ts", symbol: S).
dead_export(P, N) :- exports(file: P, name: N, symbol: S, kind: local), file(path: P, is_test: false),
                     not used_elsewhere(S), not entry_export(S).
```

```datalog
% Exported functions no test reaches. Over plain call_edge this reports false
% positives three different ways — see §5, traps 2–4.
import "lib/pointsto.dl".
import "schema/flow.dl".
test_fn(F) :- fn(id: F, file: P), file(path: P, is_test: true).
covered(X) :- test_fn(T), call_edge_pt_lexical(T, X).
covered(Y) :- covered(X), call_edge_pt_lexical(X, Y).
untested(S) :- exports(symbol: S, kind: local), fn(id: S), not covered(S).
```

## 5. Traps specific to these facts

1. **An import may not exist at run time — even without `import type`.**
   TypeScript drops an import whose bindings are only used as types, whatever
   it is written as (and under `verbatimModuleSyntax` keeps every import not
   marked `type`). `imports.kind` is only what was *written*; `imports.runtime`
   is what the emitted JavaScript keeps, read from the compiler's own emit, and
   `runtime_dep` follows it. `in_cycle` follows every import: the project above
   has an `answer.ts ↔ eval.ts` cycle that is type-only on both sides, so check
   `runtime_dep` before calling a cycle real. For single names, ask `ref`: a
   name is a runtime dependency where some reference to it has a kind other
   than `type`, `typeof` or `implements`.
2. **A callback handed to a library is called by the library**, so no
   `call_edge` reaches it — `xs.map(x => f(x))` does not make its enclosing
   function call `f`. Use `call_edge_lexical` (or `call_edge_pt_lexical`) for
   "does A eventually run B".
3. **CHA sees only nominal types.** A virtual call expands through `overrides`,
   which comes from `implements` and `extends`. A function returning an object
   literal that satisfies an interface structurally — a factory of closures — is
   invisible to it; `call_edge_pt` follows the returned object's fields and finds
   it. On the project above, "untested exports" went 5 → 0 across traps 2 and 3,
   and each of the three checked in the source was a false positive.
4. **`indirect` and `unresolved` calls are holes in `call_edge`.** Count them
   before believing "nothing calls X": `call_site(dispatch: indirect)` for values
   (resolved by `pointsto.dl`), `unresolved` for what the checker could not
   resolve, `unresolved_call` for what points-to could not. A library's
   function-typed value (`expect(x).toBe`) is named as the target, not left
   indirect.
5. **`ref` has no locals.** References to locals and parameters are `def`/`use` in
   the flow layer. `ref.from` is the innermost *named* declaration: a
   module-level variable's initializer is that variable's, a callback's body is
   the callback's.
6. **Tests are in the facts.** Filter on `file(is_test: false)` for "production
   code" questions — and check `is_test` is right for your layout (it reads
   `*.test.*`, `*.spec.*`, `__tests__/`, `test(s)/`, and test-only tsconfigs).
7. **The count trap is wider here.** `call_site` has 13 columns and `symbol` 18;
   a named-argument atom inside an aggregate carries every unmentioned one as a
   witness. Project into a two-column rule first (`bring-your-own.md` §5).
8. **The flow graph over-approximates exceptions.** Inside a `try`, every node
   has a `throw` edge; outside one, only `throw` statements reach `throw_exit`. So
   `unreachable` is sound (what it names is dead), and an analysis that needs
   "may throw here" should use `throw_site` and `call_site`, not the CFG.
9. **Points-to is may-point-to, over the modelled subset.** Accessor calls
   hidden in property reads, object spread and method values read off class
   instances are not modelled, so `pts` can miss those; everywhere else it only
   over-approximates. `lib/pointsto.dl`'s header has the rest.
10. **Bulk commits are in `commit` and out of `cochange.dl`** (over
    `bulk_limit(50)` files). A rename-everything commit would otherwise couple
    every file to every other.

## 6. Verify before you believe

`bring-your-own.md` §6 applies unchanged: **Datalog proposes, source
verifies.** Every trap in §5 was found that way — an answer, a file opened, the
answer wrong for a reason the facts could have said. On the project above the
check also went the other way: its strongest hidden coupling (`answer.ts` and
`test/examples.test.ts`, 9 co-changes, no static link) turned out real — the
test pins golden output that `answer.ts` renders, reached only through `run()`.
