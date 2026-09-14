# Recipe: analysing a source tree

Questions about a codebase are mostly graph and set questions — what reaches
what, what nothing reaches, what changed together, what is never matched. Those
are what this engine is for, and hand-reasoning over a few thousand functions is
where an LLM quietly drops cases.

The shape is always the same:

```
real parser → fact tables (JSONL) → import → ask
```

## 1. Extract with a parser, not a regex

Use the language's own parser and emit one JSONL file per relation: `syn` for
Rust, the `tsc` compiler API for TypeScript, `ast` for Python, `tree-sitter` for
anything. Regex extraction makes the false findings a parser does not: a
higher-order `.map(fn)` reference read as a call, struct and tuple variant syntax
confused, same-name methods merged, `Display` arms read as constructions.

A parser removes that class. It leaves two you must plan for:

- **A parser parses; it does not resolve.** `Parser::new`, `Config::new` and
  `Client::new` all arrive as `new`. Emit the *raw* segments — callee name,
  qualifier, enclosing module — and resolve in Datalog (*Resolve names in
  tiers*, below), where the ambiguity stays visible and countable.
- **Macro bodies are opaque.** Functions defined inside a macro call — a whole
  `proptest! { … }` suite, say — are absent until the extractor scans macro
  bodies itself. `assert_eq!` arguments hide call edges the same way. A blind
  spot like this is silent: the analysis answers confidently about the code it
  can see.

Emit ids that survive collisions (`module::Type::name`, disambiguated by line —
`#[cfg(unix)]` and `#[cfg(not(unix))]` give two functions one name).

## 2. Import, and split the rule library by cost

```datalog
% facts.dl — imports only
import "../facts/fn_def.jsonl" as fn_def.
import "../facts/calls.jsonl"  as calls.
...
```

Import is rarely the expensive part; closures are.

Evaluation is bottom-up, and it computes only what the program's goals depend
on: a rule no `?-`, `?why` or `-q` reaches is never run. A rule library spliced
with `import "lib/graph.dl".` therefore charges an analysis for the closures its
questions read and nothing else — so one library can hold the cheap rules and the
expensive ones side by side. A program with **no goals at all** computes
everything it defines, which is what makes `datalog p.dl` a check that every rule
runs. Imports follow the same rule when each one names its columns
(`as calls(caller: string, callee: string)`): a relation no goal reaches is never
read, and a mistake in the program is reported before any file is. A schema-less
import like the ones above is always read.

JSON `null` and an empty CSV cell both arrive as `absent`, and the same program
over the same table as JSONL and as CSV gives byte-identical output — so a
missing column is a first-class value, not a parse failure. Test it with
`S is absent`, never `S = absent` (which is an error that says so).

## 3. The questions worth asking

Each of these is one rule and a query.

| question | shape |
|---|---|
| import cycles | `mod_reaches(M, M)` |
| dead code | `fn_def(id: F, is_pub: false), not used(F)` |
| never constructed / never matched | negation over `variant` × `type_ref` |
| untested | `not covered(F)` where `covered` is reachability from `is_test: true` |
| mutual recursion | `certain(A, B), c_reaches(B, A), A != B` |
| hidden coupling | co-change count ≥ N, `not linked(A, B)` |
| undocumented API | `doc(is_pub: true, has_doc: false)` |
| rankings | `count`/`avg`/`max` grouped by module |

The negative results carry as much as the positives: *no* unconstructed enum
variant and *no* dead private function is a real statement about a codebase, and
prose reasoning cannot make it credibly.

Reachability is the expensive one: a call graph's closure holds far more pairs
than the graph has edges. The natural spelling of mutual recursion —
`reaches(A, B), reaches(B, A)` — joins that closure with itself. Anchor one side
on the direct-edge relation instead, so the join is driven by the edges:

```datalog
mutual(A, B) :- call_edge(A, B), reaches(B, A), A != B.
```

It names every function on a cycle through another, paired with its callee on
that cycle. Tightening the edge relation itself is the larger win, though:
resolved edges instead of raw ones (section 4) shrink the closure and every join
over it. Fewer, better edges beat a cleverer query.

## 4. Resolve names in tiers, and count what stays ambiguous

Do not let the extractor guess. Define tiers and let the engine hold the
uncertainty:

```datalog
defines(Name, Id) :- fn_def(id: Id, name: Name).
ambiguous(Name)   :- defines(Name, A), defines(Name, B), A != B.
unique(Name, Id)  :- defines(Name, Id), not ambiguous(Name).

certain(A, B) :- path_call(A, N, Q, _), Q is absent, unique(N, B).
certain(A, B) :- path_call(A, N, Q, _), Q is not absent, fn_def(id: B, name: N, owner: Q).
likely(A, B)  :- certain(A, B).
likely(A, B)  :- calls(caller: A, callee: N, kind: "method"), unique(N, B).
```

Then **run the analysis at both tiers**. If the conclusion is the same, the
ambiguity did not matter; if it moves, you have learned exactly where to look.

Keep the leftovers as a relation, not a shrug. Rank `unresolved` by name, and its
top entries usually name the problem outright: standard-library macros and methods
(`format`, `push`, `new`) colliding with the project's own function names. Joining
on the bare name would invent an edge for every one.

## 5. Four traps, in the order they will bite

<!-- block: source-analysis-count-trap -->
1. **`count` counts bindings, not distinct values — and the wildcard can be
   invisible.** Over a wide imported table, named-argument syntax leaves every
   unmentioned column implicitly wildcarded, and each is a witness dimension:

   ```datalog
   N = count { C | calls(caller: C, callee: "bump") }   % 36 — call sites
   ```

   The question was "how many callers", the answer is 20, and there is no `_`
   anywhere to warn you. Project first, always:

   ```datalog
   caller_of(C, F) :- calls(caller: C, callee: F).
   N = count { C | caller_of(C, "bump") }               % 20 — callers
   ```
<!-- /block -->

2. **A join across two id-spaces is silently empty.** `calls.callee` holds bare
   names; `fn_def.id` holds qualified ids. Joining them derives nothing, and
   nothing is a legal answer — no error, no warning (the undefined-predicate
   warning does not fire, since both predicates exist). If a relation comes back
   empty, check that both sides speak the same vocabulary before believing it.

3. **No string operations.** There is no prefix, split, or concat, so a submodule
   path (`parser::tests`) cannot be reduced to its file's module (`parser`) inside
   the engine. Emit both columns from the extractor instead. Generally: anything
   requiring string surgery has to be decided by the producer.

4. **Disjunction is rule-only.** A query body rejects `;` (with a clear error).
   Define a rule with `-q 'p(X) :- a(X) ; b(X)'` and query its head.

## 6. Verify before you believe

The engine is exact about what the facts say. Whether the facts say what you
think is the open question — a wrong answer usually comes from extraction or from
encoding, rarely from evaluation.

<!-- block: verify-before-you-believe -->
So: **Datalog proposes, source verifies.** Treat a derived claim as a location to
go read, not a result. A `dead(F)` naming a function plainly used two lines from
its definition is not a wrong query: it is a right query answering about a world
with part of the code missing, and reading that function is how the gap is found.

Read the source before writing anything down. Two modules coupled with no `use`
edge between them — by an invariant, or by what an index type means — become a
finding only once the files confirm there is no edge and a comment explains why.
<!-- /block -->
