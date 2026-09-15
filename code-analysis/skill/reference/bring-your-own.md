# Extracting facts for another language

`code-facts` reads TypeScript, Python and Go. For anything else, write the extractor
yourself — the engine, the method and most of the traps carry over unchanged.
The shape is always the same:

```
real parser → fact tables (JSONL) → import → ask
```

## 1. Extract with a parser, not a regex

Use the language's own parser and emit one JSONL file per relation: `syn` for
Rust, `javaparser` or the compiler tree API for Java,
`tree-sitter` for anything. A regex extractor produces false findings of its
own: a function passed by name (`.map(parse)`) is no call, two methods with one
name merge, a constructor that looks like a call is read as one.

A parser removes that class. It leaves two you must plan for:

- **A parser parses; it does not resolve.** `Parser::new`, `Lexer::new` and
  `Token::new` all arrive as `new`. Emit the *raw* segments — callee name,
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

```datalog
% facts.dl, beside the facts/ directory
import "facts/fn_def.jsonl" as fn_def(id: string, name: string, owner: string, is_pub: bool, line: int).
import "facts/calls.jsonl"  as calls(caller: string, callee: string, qualifier: string, kind: string, line: int).
```

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

Each of these is one rule and a query.

| question | shape |
|---|---|
| import cycles | `mod_reaches(M, M)` |
| dead code | `fn_def(id: F, is_pub: false), not used(F)` |
| never constructed / never matched | negation over variant definitions × type references |
| untested | `not covered(F)` where `covered` is reachability from test functions |
| mutual recursion | `certain(A, B), reaches(B, A), A != B` |
| hidden coupling | co-change count ≥ N, `not linked(A, B)` |
| undocumented API | `doc(is_pub: true, has_doc: false)` |
| rankings | `count`/`avg`/`max` grouped by module |

The negative results carry as much as the positives: *no* unconstructed enum
variant and *no* dead private function is a real statement about a codebase, and
prose reasoning cannot make it credibly — as long as the blind spots of §1 are
counted beside it.

## 4. Resolve names in tiers, and count what stays ambiguous

Do not let the extractor guess. Define tiers and let the engine hold the
uncertainty:

```datalog
defines(Name, Id) :- fn_def(id: Id, name: Name).
ambiguous(Name)   :- defines(Name, A), defines(Name, B), A != B.
unique(Name, Id)  :- defines(Name, Id), not ambiguous(Name).

certain(A, B) :- calls(caller: A, callee: N, qualifier: Q), Q is absent, unique(N, B).
certain(A, B) :- calls(caller: A, callee: N, qualifier: Q), Q is not absent, fn_def(id: B, name: N, owner: Q).
likely(A, B)  :- certain(A, B).
likely(A, B)  :- calls(caller: A, callee: N, kind: "method"), unique(N, B).
```

Then **run the analysis at both tiers**. If the conclusion is the same, the
ambiguity did not matter; if it moves, you have learned exactly where to look.

Keep the leftovers as a relation, not a shrug. Count unresolved call sites by
name, and the top entries usually name the problem outright: standard-library
methods and macros (`new`, `push`, `format`) colliding with the project's own
function names. Joining on the bare name would have invented an edge for every
one.

## 5. Four traps, in the order they will bite

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

2. **A join across two id-spaces is silently empty.** If `calls.callee` holds
   bare names and `fn_def.id` holds qualified ids, joining them derives nothing,
   and nothing is a legal answer — no error, no warning. If a relation comes back
   empty, check that both sides speak the same vocabulary before believing it.

3. **No string operations.** There is no prefix, split or concat, so a nested
   module path cannot be reduced to its file's module inside the engine. Emit
   both columns from the extractor instead. Anything that needs string surgery
   has to be decided by the producer.

4. **Disjunction is rule-only.** A query body rejects `;` (with a clear error).
   Define a rule with `-q 'p(X) :- a(X) ; b(X)'` and query its head.

## 6. Verify before you believe

The engine is exact about what the facts say. Whether the facts say what you
think is the open question, and a wrong answer almost always comes from extraction
or from encoding, not from evaluation.

So: **Datalog proposes, source verifies.** Treat a derived claim as a location to
go read, not a result. A `dead(F)` that names functions plainly called two lines
below their definition is the signature of a blind spot — a macro body the
extractor never entered — not of dead code: the query is right about a world
with part of the code missing.

Read the source before writing anything down. A coupling between two modules with
no import between them becomes a finding only once the files show what they
share — an invariant, a format, the meaning of an index.
