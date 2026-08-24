# Recipe: analysing a source tree

Questions about a codebase are mostly graph and set questions — what reaches
what, what nothing reaches, what changed together, what is never matched. Those
are what this engine is for, and hand-reasoning over a few thousand functions is
where an LLM quietly drops cases.

The shape is always the same:

```
real parser → fact tables (JSONL) → import → ask
```

Worked end to end on a 21k-line Rust crate (2026-07-27). Every number below is
measured from that run, not estimated.

## 1. Extract with a parser, not a regex

Use the language's own parser and emit one JSONL file per relation: `syn` for
Rust, the `tsc` compiler API for TypeScript, `ast` for Python, `tree-sitter` for
anything. Regex extraction was tried first (2026-07-23) and every false finding
came from it — higher-order `.map(fn)` references, struct-vs-tuple variant
syntax, same-name methods merged, `Display` arms read as constructions.

A parser removes that class. It leaves two you must plan for:

- **A parser parses; it does not resolve.** `Model::new`, `Lexer::new` and
  `F64::new` all arrive as `new`. Emit the *raw* segments — callee name,
  qualifier, enclosing module — and resolve in Datalog (*Resolve names in
  tiers*, below), where the ambiguity stays visible and countable.
- **Macro bodies are opaque.** In the crate above, 65 of 901 functions — 7%,
  all the property tests — live inside `proptest! { … }` and were simply absent
  until the extractor dropped to a token scan for macro bodies. `assert_eq!`
  arguments hide call edges the same way. A blind spot like this is silent: the
  analysis answers confidently about the code it can see.

Emit ids that survive collisions (`module::Type::name`, disambiguated by line —
`#[cfg(unix)]` and `#[cfg(not(unix))]` give two functions one name), and emit a
**numeric id per file** alongside the path (§5 says why).

## 2. Import, and split the rule library by cost

```datalog
% facts.dl — imports only
import "../facts/fn_def.jsonl" as fn_def.
import "../facts/calls.jsonl"  as calls.
...
```

Loading 27,957 facts across 15 JSONL files takes **0.32 s**. Import is not the
expensive part.

Evaluation is bottom-up: everything a program defines gets computed, whether the
query touches it or not. A rule library spliced with `import "lib/graph.dl".`
therefore charges every analysis for every closure in it. Splitting one library
into `lib/callgraph.dl` and `lib/modgraph.dl` took the module-cycle analysis from
11.5 s to 0.33 s — same question, same answer, 35x. **Import the closure you
are about to use, not the library.**

JSON `null` and an empty CSV cell both arrive as `absent`, and the same program
over the same table as JSONL and as CSV gives byte-identical output — so a
missing column is a first-class value, not a parse failure. Test it with
`S is absent`, never `S = absent` (which is an error that says so).

## 3. The questions worth asking

Each of these is one rule and a query. All ran on the crate above.

| question | shape |
|---|---|
| import cycles | `mod_reaches(M, M)` — found `engine ↔ provenance` |
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

Reachability is the expensive one. Over 5,248 call edges it derives 173,177
pairs in 11.6 s, in 505 MB. The natural spelling of mutual recursion —
`reaches(A, B), reaches(B, A)` — is a self-join of that 173k relation and **did
not finish in two minutes**. Anchor one side on the direct-edge relation, which
drives the join from 5k tuples instead of 173k:

```datalog
mutual(A, B) :- call_edge(A, B), reaches(B, A), A != B.   % same relation, 19 s
```

Tightening the edge relation itself is the larger win, though: the same query
over 1,504 resolved edges instead of 5,248 raw ones runs in **2.2 s**. Fewer,
better edges beat a cleverer query.

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
Above: 1,504 certain edges, 2,458 likely, and the mutual-recursion answer was
stable across both.

Keep the leftovers as a relation, not a shrug — `unresolved` had 599 call sites,
and its top entries named the problem outright: `format` (140), `matches` (52),
`push` (94), `new` (93). Those are std macros and std methods colliding with
crate function names. Joining on the bare name would have invented an edge for
every one.

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
   path (`lower::tests`) cannot be reduced to its file's module (`lower`) inside
   the engine. Emit both columns from the extractor instead. Generally: anything
   requiring string surgery has to be decided by the producer.

4. **Disjunction is rule-only.** A query body rejects `;` (with a clear error).
   Define a rule with `-q 'p(X) :- a(X) ; b(X)'` and query its head.

## 6. Verify before you believe

The engine is exact about what the facts say. Whether the facts say what you
think is the open question — every wrong answer in two sessions of this came from
extraction or from encoding, never from evaluation.

<!-- block: verify-before-you-believe -->
So: **Datalog proposes, source verifies.** Treat a derived claim as a location to
go read, not a result. In the run above, `dead(F)` returned six functions that
were plainly used two lines from their definition — which is how the missing
macro bodies were found at all. The query was right; it was answering about a
world with 7% of the code missing.

Read the source before writing anything down. Both sessions' best findings —
`parser ↔ print` coupled by an invariant, `lower ↔ provenance` coupled by
`ir::BodyIdx`'s meaning — only became findings after the file confirmed there was
no `use` edge and the comment explained why.
<!-- /block -->
