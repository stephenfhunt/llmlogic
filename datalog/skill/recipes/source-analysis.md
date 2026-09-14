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
Rust, the `tsc` compiler API for TypeScript, `ast` for Python, `go/ast` for Go,
`tree-sitter` for anything. A regex extractor produces false findings of its
own: a function passed by name (`.map(parse)`) is no call, two methods with one
name merge, a constructor that looks like a call is read as one.

A parser removes that class. It leaves two you must plan for:

- **A parser parses; it does not resolve.** `Parser::new`, `Config::new` and
  `Client::new` all arrive as `new`. Emit the *raw* segments — callee name,
  qualifier, enclosing module — and resolve in Datalog (*Resolve names in
  tiers*, below), where the ambiguity stays visible and countable.
- **Macro and template bodies are opaque.** Code inside a macro invocation — a
  test-generating macro, an assertion's arguments — is absent from a plain AST
  walk unless the extractor scans into it. A blind spot like this is silent: the
  analysis answers confidently about the code it can see. Count what the
  extractor skipped, and emit the count.

Emit ids that survive collisions: a qualified path (`module::Type::name`),
disambiguated by line where conditional compilation gives two definitions one
name.

## 2. Import with named columns

The rest of this recipe assumes an extractor that wrote three tables. Yours will
differ; the rules adapt.

```datalog
% facts.dl, beside the facts/ directory
import "facts/fn_def.jsonl"  as fn_def(id: string, name: string, owner: string, module: string, is_pub: bool, is_test: bool).
import "facts/calls.jsonl"   as calls(caller: string, callee: string, qualifier: string, kind: string, line: int).
import "facts/mod_dep.jsonl" as mod_dep(from: string, to: string).
```

- `fn_def` — one row per function: its qualified `id`, its bare `name`, the
  `owner` type or module that declares it, and the `module` its file belongs to.
- `calls` — one row per call site: the `caller`'s id, the `callee` name as
  written, the `qualifier` in front of it (`Type::` or `module.`; `null` when
  there is none), a `kind` of `"function"` or `"method"`, and the `line` —
  which is what keeps two calls from one caller to one name two rows.
- `mod_dep` — one row per import of one module by another.

A question is then a small file that splices `import "facts.dl".` and adds its
own rules.

Evaluation is bottom-up and computes only what the program's goals depend on: a
rule no `?-`, `?why` or `-q` reaches is never run, so one rule library can hold
cheap rules and expensive ones side by side, and a question pays only for what it
reads. With named columns an import no goal reaches is never read, and a mistake
in the program is reported before any file is. An import without them is always
read.

JSON `null` and an empty CSV cell both arrive as `absent`, so a missing column
is a first-class value, not a parse failure. Test it with `S is absent`, never
`S = absent` (which is an error that says so).

## 3. The questions worth asking

Each of these is one rule and a query, over the tables above and a few closures —
`certain` and `likely` are the resolved call edges of section 4:

```datalog
mod_reaches(A, B) :- mod_dep(from: A, to: B).
mod_reaches(A, C) :- mod_dep(from: A, to: B), mod_reaches(B, C).
reaches(A, B)     :- certain(A, B).
reaches(A, C)     :- certain(A, B), reaches(B, C).
used(F)           :- likely(_, F).
covered(F)        :- fn_def(id: T, is_test: true), reaches(T, F).
```

| question | shape |
|---|---|
| import cycles | `mod_reaches(M, M)` |
| dead code | `fn_def(id: F, is_pub: false, is_test: false), not used(F)` |
| untested | `fn_def(id: F, is_test: false), not covered(F)` |
| mutual recursion | `certain(A, B), reaches(B, A), A != B` |
| rankings | `count`/`avg`/`max` grouped by `module` |
| never constructed / never matched | variant definitions negated against type references — two more tables |
| hidden coupling | a co-change count from version history ≥ N, `not mod_reaches(A, B)` |
| undocumented API | public items with no doc comment — one more table |

The negative results carry as much as the positives: *no* unconstructed enum
variant and *no* dead private function is a real statement about a codebase, and
prose reasoning cannot make it credibly — as long as the blind spots of section 1
are counted beside it.

## 4. Resolve names in tiers, and count what stays ambiguous

Do not let the extractor guess. Define tiers and let the engine hold the
uncertainty:

```datalog
defines(Name, Id) :- fn_def(id: Id, name: Name).
ambiguous(Name)   :- defines(Name, A), defines(Name, B), A != B.
unique(Name, Id)  :- defines(Name, Id), not ambiguous(Name).

certain(A, B) :- calls(caller: A, callee: N, qualifier: Q, kind: "function"), Q is absent, unique(N, B).
certain(A, B) :- calls(caller: A, callee: N, qualifier: Q, kind: "function"), Q is not absent, fn_def(id: B, name: N, owner: Q).
likely(A, B)  :- certain(A, B).
likely(A, B)  :- calls(caller: A, callee: N, kind: "method"), unique(N, B).
```

Then **run the analysis at both tiers**. If the conclusion is the same, the
ambiguity did not matter; if it moves, you have learned exactly where to look.

Keep the leftovers as a relation, not a shrug:

```datalog
resolved(A, N)   :- calls(caller: A, callee: N), likely(A, B), fn_def(id: B, name: N).
unresolved(A, N) :- calls(caller: A, callee: N), not resolved(A, N).
```

Rank the names by how many callers leave them unresolved
(`-q 'unresolved(_, N), K = count { A | unresolved(A, N) }'`), and the top
entries usually name the problem outright: standard-library methods and macros
(`new`, `push`, `format`) colliding with the project's own function names.
Joining on the bare name would have invented an edge for every one.

## 5. Three traps, in the order they will bite

<!-- block: source-analysis-count-trap -->
1. **`count` counts bindings, not distinct values — and the wildcard can be
   invisible.** Over a wide imported table, named-argument syntax leaves every
   unmentioned column implicitly wildcarded, and each is a witness dimension:

   ```datalog
   N = count { C | calls(caller: C, callee: "bump") }   % call sites, not callers
   ```

   The question was "how many callers", and there is no `_` anywhere to warn you.
   Project first, always:

   ```datalog
   caller_of(C, F) :- calls(caller: C, callee: F).
   N = count { C | caller_of(C, "bump") }               % callers
   ```
<!-- /block -->

2. **A join across two id-spaces is silently empty.** `calls.callee` holds bare
   names and `fn_def.id` holds qualified ids, so joining them directly derives
   nothing — and nothing is a legal answer, with no error and no warning. If a
   relation comes back empty, check that both sides speak the same vocabulary
   before believing it.

3. **No string operations.** There is no prefix, split or concat, so a nested
   module path cannot be reduced to its file's module inside the engine. Emit
   both columns from the extractor instead. Anything that needs string surgery
   has to be decided by the producer.

## 6. Verify before you believe

The engine is exact about what the facts say. Whether the facts say what you
think is the open question — a wrong answer usually comes from extraction or from
encoding, rarely from evaluation.

<!-- block: verify-before-you-believe -->
So: **Datalog proposes, source verifies.** Treat a derived claim as a location to
go read, not a result. A `dead(F)` naming a function plainly used two lines from
its definition is not a wrong query: it is a right query answering about a world
with part of the code missing, and reading that function is how the gap is found.

Read the source before writing anything down. Two modules coupled with no import
between them — by an invariant, or by what an index type means — become a
finding only once the files confirm there is no import and a comment explains why.
<!-- /block -->
