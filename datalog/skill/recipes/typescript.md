# Recipe: a TypeScript codebase, already extracted

For TypeScript you do not write the extractor. `./ts-facts` (next to
`./datalog`) reads a project with the TypeScript compiler — the type checker
resolves every name, so the call graph is resolved rather than guessed — and
writes 61 relations across seven layers, from packages and import graphs down to
control flow, def/use, points-to inputs, and git history. A rule library
computes coupling, cohesion, reachability and the rest; you write the questions.

`recipes/source-analysis.md` is the general method and its traps; this file is
what is specific to these facts.

## 1. Run it

```sh
./ts-facts path/to/tsconfig.json [more tsconfigs] -o /tmp/facts
./datalog /tmp/facts/lib/checks.dl        # exit 1 = no violations: the facts are consistent
```

Needs Node.js ≥ 22.18. Give **every** tsconfig the project uses — tests often
live in their own (`tsconfig.test.json`), and a test file missing from the facts
makes every "untested" answer wrong. `references` are followed. The root (for
every path in the facts) is the git top level unless you pass `--root`; a root
below it reads that subtree's history only. Options: `--layers
refs,flow,dataflow,quality,git` to extract less, `--no-git`, `--git-since DATE`,
`--exclude GLOB`. A 24k-line project extracts in about 4 s to 205k facts.

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
| structure | `file` (path, dir, package, is_test, loc), `symbol` (id, kind, file, parent, exported, …), `imports` (file → target_file / target_package, kind), `exports`, `file_ancestor` (every enclosing dir, with depth) |
| refs | `ref(from, to, kind)` — every resolved reference between declarations; `call_site` (caller, callee, **dispatch**); `extends`, `implements`, `overrides`; `member_access` (for cohesion); `type_ref` (with position: param, return, …) |
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
program imports (`source-analysis.md` §2). Times are on the 24k-line project
above (205k facts), including the import.

| file | what it derives | time |
|---|---|---|
| `checks.dl` | `violation(Check, Subject)` — the extractor contradicting itself | 4.3 s |
| `modgraph.dl` | `file_dep`, `runtime_dep` (type-only imports excluded), `in_cycle`, `cycle_edge`, `unit_dep` (directories at any depth), `package_edge`, `external_dep` | 0.3 s |
| `callgraph.dl` | `call_edge` (virtual calls expanded to every override), `call_edge_lexical` (a callback's calls counted as its enclosing function's), `called` | 0.9 s |
| `callreach.dl` | `reaches`, `recursive`, `mutual` | 1.8 s |
| `coupling.dl` | per component: `efferent`, `afferent`, `instability`, `abstractness`, `distance`, `sdp_violation`, `comp_edge_weight`; per type: `cbo` | 3.3 s |
| `cohesion.dl` | per class: `lcom4`, `tcc`, `lcom_hs`; per component: `relational_cohesion` | 1.1 s |
| `metrics.dl` | `dit`, `noc`, `wmc`, `rfc`, `fan_in`, `fan_out` | 1.7 s |
| `flow.dl` | `reachable`, `unreachable`, `reaches_def`, `def_use`, `undefined_use`, `live_out`, `dead_store` | 8–9 s |
| `dominators.dl` | `dominates`, `back_edge`, `loop_header` | 4.3 s |
| `pointsto.dl` | `pts`, `heap`, `target`, `call_edge_pt` (indirect and structural calls resolved), `call_edge_pt_lexical`, `unresolved_call` | 3.3 s |
| `taint.dl` | `tainted`, `tainted_sink` — you supply `source/1` and `sink/1` | pointsto + |
| `cochange.dl` | `revisions`, `cochange`, `confidence`, `hidden_coupling`, `churn`, `author_commits`, `main_author`, `first_change`, `last_change` | 2.3 s |

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

1. **A `dep` cycle may not exist at run time.** `in_cycle` follows every import,
   and `import type` is erased: the project above has an `answer.ts ↔ eval.ts`
   cycle that is type-only on both sides. Ask over `runtime_dep` before calling
   it a cycle.
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
   witness. Project into a two-column rule first (`source-analysis.md` §5).
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

`source-analysis.md` §6 applies unchanged: **Datalog proposes, source
verifies.** Every trap in §5 was found that way — an answer, a file opened, the
answer wrong for a reason the facts could have said. On the project above the
check also went the other way: its strongest hidden coupling (`answer.ts` and
`test/examples.test.ts`, 9 co-changes, no static link) turned out real — the
test pins golden output that `answer.ts` renders, reached only through `run()`.
