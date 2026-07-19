# Datalog Language Specification

**Status:** `v0.1-draft` — living document. Sections are drafted, refined, and
prototype-validated over time. Nothing here is final until its section is marked
*Stable* and the corresponding decision is recorded in §17.

This is both the specification and the design workspace for the `datalog` engine. It
carries the current design **and** an explicit decisions/open-questions log so the
reasoning behind each choice stays visible.

The academic literature behind each section is cataloged in
[`references.md`](references.md), grouped by topic and cross-referenced to spec
sections — consult the relevant group before drafting or implementing a section.

---

## How we work through the design

The scope is large (full-featured v1), so we use a disciplined, repeatable process:

- **Example-driven.** Before finalizing the syntax/semantics of a feature, write the
  canonical example programs we want to express (collect them in §16). Let those
  examples drive the design rather than designing in the abstract.
- **Living doc + decisions log.** Every non-obvious choice gets a dated entry in §17
  with its rationale and the alternatives considered. Unresolved questions live in
  §17 until answered.
- **Spec-first, prototype-validated.** We specify first (per the project's intent),
  but we validate risky sections — grammar, negation/stratification, provenance —
  with small Rust prototypes in the crate and feed findings back before marking a
  section *Stable*. Spec and code co-evolve.
- **Versioned.** The spec header carries a version tag so we can track churn.
- **Sequencing.** Draft §1–5 (goals → grammar) to a stable-enough point first, then
  §6–11 (semantics through provenance), then §12–14 (errors, sources, API), iterating
  as needed. §15–16 accrete throughout.

Each section below starts with a **Status** line: `TBD` → `Draft` → `Stable`.

---

## 1. Overview & goals

*Status: TBD*

A Datalog engine purpose-built for LLM/agent use and for conveniently loading fact
tables from external sources. Three pillars drive every design decision:

1. **Provenance / explainability** — the engine can explain *why* a fact was derived.
2. **LLM-friendly syntax + structured errors** — a familiar, unambiguous surface
   syntax models generate reliably, with errors that are structured and actionable.
3. **Agent-native interface** — an agent drives the executable directly
   (skill-based, CLI-first); Datalog is the interchange format in both directions,
   with JSON at the machine-readable edges (errors, provenance). See §14.

*To fill in: concrete goals, non-goals, target users, success criteria.*

## 2. Design principles

*Status: TBD*

Candidate principles to ratify:
- **Prefer familiar, conventional Datalog surface syntax.** LLMs generate standard
  Prolog-/Datalog-style syntax reliably because it is well represented in training
  data — a strong reason to stay conventional rather than invent novel syntax.
- **Strict, unambiguous grammar.** No syntax whose meaning depends on subtle context.
- **Every error is structured and actionable** (machine-readable, with spans and,
  where possible, suggested fixes).
- **Explainability and the agent API are first-class**, designed in from the start.
- **Predictable evaluation** — termination and resource behavior an agent can rely on.

## 3. Lexical structure

*Status: Draft*

- **Comments** — `%` to end of line.
- **Identifiers** (relation names, symbols, field names) — start with a lowercase
  letter, continue with letters, digits, `_` (snake_case by convention): `parent`,
  `family_tree`.
- **Variables** — start with an uppercase letter or `_`: `X`, `Who`, `_Age`. A lone
  `_` is the anonymous variable; each occurrence is a fresh variable.
- **Literals**
  - *Integers* — `0`, `42`, `-7` (64-bit signed).
  - *Floats* — `3.14`, `-0.5`, `6.02e23` (64-bit IEEE 754).
  - *Strings* — `"alice"` or `'alice'` (both delimiters accepted, identical
    semantics); backslash escapes `\\`, `\"`, `\'`, `\n`, `\t`.
  - *Booleans* — `true`, `false`.
  - *Symbols* — a bare identifier in term position (`red`, `pending`) denotes a
    symbol constant, a distinct type from the string `"red"`.
- **Punctuation / operators** — `:-` (rule), `?-` (query), `.` (statement end),
  `,` (conjunction), `(` `)`, `:` (named argument), comparisons `=` `!=` `<` `<=`
  `>` `>=`, arithmetic `+` `-` `*` `/` (§8).
- **Reserved words** — `import`, `as`, `declare`, `not`, `true`, `false`. These
  cannot be used as relation or field names.
- **Whitespace** is insignificant except as a token separator.

## 4. Data model & types

*Status: Draft — validated by the AST/IR prototype (`src/ast.rs`, `src/ir.rs`),
2026-07-19*

### Values and terms

Primitive value types: **symbol**, **string**, **int**, **float**, **bool**.
Symbols and strings are distinct types and never compare equal. Terms are **flat**:
a term is a constant or a variable — no compound/function terms in v1 (deferred;
see §17).

A relation has a fixed arity, and every column has exactly one type.

### Static typing via inference

The language is statically typed **with full type inference** — annotations are
never required. Before evaluation, the engine infers every column's type from:

1. literals in program facts (`30` int, `3.14` float, `"a"` string, `true` bool,
   bare `red` symbol),
2. column types of imported sources (§13), and
3. variable flow through rule bodies (a variable unifies the types of every
   position it occupies; builtins constrain their operands, §8).

Inference is a simple unification pass over the primitive types — no polymorphism,
no type constructors, not Hindley–Milner. Any conflict — an int column joined
against a string column, or `age(X, "old")` alongside `age("bob", 30)` — is
reported as a structured **type error before evaluation** (§12), never a silent
empty result or a runtime surprise. This serves the structured-errors pillar and
makes agent-generated programs safer.

### Declarations (`declare`) — optional

`declare` is never required to run a program. It does two opt-in things:

```datalog
declare person(name, age).                 % (a) name the fields
declare person(name: string, age: int).    % (b) also assert column types
```

(a) **Field naming** enables named-argument access (below) for in-program
predicates; imported relations get field names from their source automatically.
(b) An asserted signature is **verified against the inferred types** — it buys
documentation and earlier, clearer errors, not new obligations. Fields may be named
without types; a type always follows a field name.

### Positional and named arguments

Every predicate can be used **positionally**. A predicate whose field names are
known (via `declare` or import) can also be used with **named arguments**:

```datalog
adult(N) :- person(name: N, age: A), A >= 18.
```

- A single literal is **all-positional or all-named** — never mixed.
- Named literals support **partial selection**: omitted fields are implicitly bound
  to fresh anonymous variables — essential for wide imported tables.
- Argument order in a named literal is irrelevant.
- In a rule **head**, the named form must supply *all* declared fields (a head
  cannot leave columns unbound).

## 5. Syntax

*Status: Draft — validated by the AST/IR prototype (`src/ast.rs`), 2026-07-19*

A program is a sequence of statements, each terminated by `.`:

```datalog
import "data/parents.csv" as parent.        % import (§13)
declare person(name: string, age: int).     % declaration (§4)
person("alice", 30).                        % fact
ancestor(X, Y) :- parent(X, Y).             % rule
?- ancestor("alice", Who).                  % query
```

### Grammar (EBNF)

```ebnf
program     = { statement } ;
statement   = import | declaration | clause | query ;

import      = "import" string "as" ident [ "(" field { "," field } ")" ] "." ;
declaration = "declare" ident "(" field { "," field } ")" "." ;
field       = ident [ ":" type ] ;
type        = "int" | "float" | "string" | "symbol" | "bool" ;

clause      = atom [ ":-" body ] "." ;          (* fact when no body, else rule *)
query       = "?-" body "." ;
body        = literal { "," literal } ;
literal     = [ "not" ] atom | comparison ;

atom        = ident "(" args ")" ;
args        = positional | named ;
positional  = term { "," term } ;
named       = ident ":" term { "," ident ":" term } ;

term        = constant | variable ;
constant    = integer | float | string | bool | ident ;    (* bare ident = symbol *)
variable    = VARIABLE ;                        (* uppercase- or "_"-initial, §3 *)

comparison  = expr cmp expr ;
cmp         = "=" | "!=" | "<" | "<=" | ">" | ">=" ;
expr        = term | expr ( "+" | "-" | "*" | "/" ) expr ;  (* precedence: §8, TBD *)
```

Notes:
- Whether `args` is positional or named is decided by the literal's first argument;
  mixing the two styles in one literal is a syntax error with a targeted message
  (§12).
- `not` applies to atoms only, not comparisons; semantics and safety are §7.
- Aggregate expressions (§9) are **not yet in the grammar** — their syntax is an
  open question (§17), in part because `:` now also delimits named arguments.

## 6. Declarative semantics

*Status: TBD*

*To fill in: Herbrand base, least/minimal model, and how stratification defines the
meaning of programs with negation and aggregation.*

## 7. Negation

*Status: TBD*

*To fill in: stratified negation-as-failure; stratification algorithm; safety
conditions on negated literals.*

## 8. Arithmetic & comparison builtins

*Status: TBD*

*To fill in: operators/functions, evaluation rules, type coercions, and the
mode/safety conditions (which variables must be bound before evaluation).*

## 9. Aggregation

*Status: TBD*

*To fill in: supported aggregates (count, sum, min, max, avg, …), grouping semantics,
and interaction with recursion and stratification.*

## 10. Recursion & safety

*Status: Draft (range restriction only; the rest TBD)*

**Range restriction** (enforced by front-end lowering, `src/lower.rs`): every
variable in a rule head, in a negated atom, or occurring only in comparisons
must also occur in a positive body atom; facts must be ground. Violations are
structured semantic errors reported before evaluation.

*Still to fill in: termination guarantees; safety/mode conditions for arithmetic
(§8); treatment of recursion through negation/aggregation.*

## 11. Provenance / explainability

*Status: TBD*

*To fill in: the derivation/proof-tree model, what a derivation records (rule
instance + premises), how base (source) facts anchor the leaves, and how provenance
is requested and returned. Designed alongside the evaluator (§15).*

## 12. Error model

*Status: TBD*

*To fill in: the structured error taxonomy — categories, machine-readable codes,
source spans, severities, and suggested fixes — designed for LLM consumption.*

## 13. External data / fact sources

*Status: Draft*

External tabular data is bound to a relation name with `import`:

```datalog
import "data/parents.csv" as parent.
```

- The imported relation is **extensional** (base facts): used in rules exactly like
  in-program facts, and its tuples anchor the leaves of provenance trees (§11).
- **Schema inference:** field names come from the CSV header row; column types are
  inferred from the data (all-int column → int; int/float mix → float;
  `true`/`false` → bool; anything else → string). Imported text is typed string,
  never symbol.
- **Explicit schema** overrides inference, and is required for headerless sources:

  ```datalog
  import "data/parents.csv" as parent(parent: string, child: string).
  ```

- Named-argument access works on imported relations immediately, using the header
  (or explicit) field names.
- Paths are resolved relative to the directory of the program source file.
- **Formats:** CSV is specified first. TSV/JSON/JSONL, SQLite, DuckDB, Parquet/
  Arrow, and Postgres are planned; priority order is an open question (§17) —
  DuckDB is a strong early candidate since it also provides CSV/Parquet readers.
  The engine-side abstraction is the `FactSource` trait (`src/sources.rs`).

## 14. Programmatic / agent API

*Status: Draft*

The primary usage pattern is **skill-based**: an agent drives the `datalog`
executable directly (CLI-first; no server required — a server/MCP layer can wrap
the same surface later). The interchange format in both directions is **Datalog
itself**:

- **Input** — program files and/or stdin: imports, declarations, facts, rules,
  queries.
- **Output** — query results are emitted as **ground facts in canonical Datalog
  syntax**, one per line, deterministically ordered (sorted). Output is therefore
  valid input: runs compose over pipes, and an agent can materialize an
  intermediate result to a fact file and query it again later (the closure
  property that makes jq effective for agents).

One-shot queries via a command-line flag — the jq analog:

```sh
# bare-atom query: sugar for appending `?- ...` to the loaded program
datalog family.dl -q 'ancestor("alice", X)'

# rule query: define-and-select in one flag; emits the head predicate's facts
datalog family.dl -q 'grandparent(X, Z) :- parent(X, Y), parent(Y, Z)'

# composition over pipes ("-" reads stdin)
datalog people.dl -q 'adult(N) :- person(name: N, age: A), A >= 18.' \
  | datalog - -q 'answer(N) :- adult(N), N != "bob".'
```

The motivating workflow is **token economy**: an agent issues precise, narrow
queries over large fact bases and reads back only the derived facts, instead of
loading raw data into context — the piecemeal analysis pattern agents already use
jq for over JSON, made Datalog-native.

JSON serves the machine-readable edges rather than the data path: structured
errors (§12) and provenance trees (§11), likely via `--format json` and/or stderr.

*Open (§17):* exact `-q` semantics (bare atom vs rule set; the synthesized answer
predicate for bare-body queries), output ordering rule, stdin/`-` conventions,
`--format json` scope, and the agent skill definition that documents this surface.

## 15. Evaluation strategy (non-normative)

*Status: TBD*

*To fill in: stratified, semi-naive bottom-up evaluation; how provenance is captured
during the fixpoint; magic-sets as a future optimization.*

## 16. Worked examples

*Status: Draft*

> The core syntax used below — facts, rules, queries, imports, named arguments —
> is ratified in §3–§5 and §13. The aggregate expression form (§16.4) and the
> provenance query form (§16.6) remain **provisional**; see §17. Each example calls
> out the design questions it raised and their status.

### 16.1 Recursion — ancestry / reachability

```datalog
% base facts (extensional predicates)
parent("alice", "bob").
parent("bob", "carol").
parent("carol", "dave").

% recursive rule (intensional predicate)
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).

% query
?- ancestor("alice", Who).
% expected: Who ∈ {"bob", "carol", "dave"}
```
*Raised (resolved in §3/§5):* fact vs rule vs query syntax; variable vs constant
lexing; comment syntax. *Still open:* how query results are shaped (§14).

### 16.2 Negation — stratified negation-as-failure

```datalog
person("alice"). person("bob"). person("carol").
parent("alice", "bob").
parent("bob", "carol").

% someone with no recorded parent
root(X) :- person(X), not parent(_, X).
% expected: root("alice")
```
*Raised:* `not` keyword and wildcard `_` (ratified, §3/§5). *Still open (§7):* the
safety rule that every variable in a negated literal (and the head) must be bound
by a positive body literal; how stratification is computed and reported when
violated.

### 16.3 Arithmetic & comparison builtins

```datalog
age("alice", 30).
age("bob", 15).
age("carol", 42).

adult(X)      :- age(X, A), A >= 18.
older(X, Y)   :- age(X, A), age(Y, B), A > B.
next_year(X, N) :- age(X, A), N = A + 1.
% expected: adult("alice"), adult("carol"); older pairs; next_year offsets
```
*Raises:* comparison vs arithmetic operators; is `=` unification, assignment, or an
equality builtin? mode/safety conditions (which vars must be bound before an
arithmetic term evaluates); numeric types and coercion (§4/§8).

### 16.4 Aggregation — grouping

```datalog
parent("alice", "bob").
parent("alice", "carol").
parent("bob", "dave").

% number of children per parent
child_count(P, N) :- parent(P, _), N = count { C : parent(P, C) }.
% expected: child_count("alice", 2), child_count("bob", 1)
```
*Raises (§9, open):* aggregate expression syntax (`count { Var : Goal }` is
provisional — and `:` now also delimits named arguments, so this form will likely
be revisited); how grouping keys are determined (the head vars outside the
aggregate); supported aggregates and their result types; interaction with
recursion/stratification.

### 16.5 External fact source — import

```datalog
% bind a CSV file (header: parent,child) to the relation `parent`
import "data/parents.csv" as parent.

% rules then use `parent/2` exactly like in-program facts
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).

% explicit schema variant (overrides the header; required if headerless):
% import "data/parents.csv" as parent(parent: string, child: string).
```
*Raised (resolved in §13):* declaration syntax; schema/column→arg mapping; column
typing; imported tuples are base facts and anchor provenance leaves. *Still open:*
backend priority beyond CSV (§17).

### 16.6 Provenance — "why?"

```datalog
% given 16.1, ask why a derived fact holds
?why ancestor("alice", "carol").
% expected: a proof tree, e.g.
%   ancestor("alice","carol")
%     ├─ via rule: ancestor(X,Y) :- parent(X,Z), ancestor(Z,Y)
%     ├─ parent("alice","bob")                    [base fact]
%     └─ ancestor("bob","carol")
%          ├─ via rule: ancestor(X,Y) :- parent(X,Y)
%          └─ parent("bob","carol")               [base fact]
```
*Raises:* how provenance is requested (`?why` is provisional) via both CLI and the
agent API; the proof-tree representation (§11) and its JSON encoding (§14); handling
of multiple independent derivations of the same fact.

### 16.7 Named arguments & partial selection

```datalog
% a wide imported table; header gives employee 8 fields:
%   id, name, age, dept, title, salary, city, start_date
import "data/employees.csv" as employee.

% named access selects only the fields a rule needs — no wildcard runs
manager_name(N) :- employee(name: N, title: "manager").

% `declare` gives in-program predicates the same power
declare person(name: string, age: int).
person("alice", 30).
adult(N) :- person(name: N, age: A), A >= 18.
```
*Raised (resolved in §4/§5):* positional/named duality; no mixing within one
literal; partial selection (omitted fields bind to fresh anonymous variables);
`declare` names fields, with types optional.

## 17. Decisions log & open questions

*Status: living*

### Decisions

- **2026-07-03** — Language scope for v1 is **full-featured**: facts, rules,
  recursion, stratified negation, arithmetic/comparison builtins, and aggregation.
- **2026-07-03** — LLM-targeting pillars are provenance/explainability, LLM-friendly
  syntax + structured errors, and a programmatic agent API. Determinism/safety is a
  supporting requirement, not a headline pillar.
- **2026-07-03** — The Rust project is self-contained inside `datalog/` (no root
  workspace); it is structured as a library + thin binary.
- **2026-07-03** — **Casing follows the strict Prolog convention**: lowercase /
  snake_case relation names, symbols, and field names; uppercase- or `_`-initial
  variables. (Considered SQL/Logica-style capitalized relations; rejected — strict
  Prolog casing is the convention LLMs reproduce most reliably.)
- **2026-07-03** — **Named arguments** are supported alongside positional, with `:`
  as the delimiter (`family_tree(parent: X, name: Y)`). A literal is all-positional
  or all-named, never mixed; named literals allow **partial selection** (omitted
  fields ⇒ fresh anonymous variables); heads using the named form must supply all
  fields. Named access requires known field names (import header or `declare`).
  Precedent: Google Logica; kwargs-style syntax is deeply familiar to LLMs and is
  the decisive convenience for wide imported tables.
- **2026-07-03** — **Static typing with full inference**: no annotations required;
  column types are inferred from fact literals, import schemas, and variable flow
  through rules, and **type errors are reported before evaluation**. Explicitly not
  dynamic typing — errors are flagged up front, not deferred to runtime. Inference
  is unification over primitive types only (no polymorphism / Hindley–Milner).
- **2026-07-03** — **`declare` keyword** (not Soufflé-style `.decl`) for the
  optional field-naming / signature-assertion statement; reads consistently with
  the bare `import` keyword. Neither field names nor types are ever required.
- **2026-07-03** — **Import syntax**: `import "<path>" as <relation>.` with the
  schema inferred from the source (CSV header + data), plus an optional explicit
  `as rel(field: type, …)` override. CSV is the first backend.
- **2026-07-03** — **Terms are flat in v1** (constants and variables only);
  compound/function terms deferred.
- **2026-07-03** — **String literals** accept double or single quotes with
  identical semantics; symbols (bare identifiers) are a distinct type from strings.
- **2026-07-10** — **Implementation proceeds bottom-up, evaluation-first**: AST
  design → core evaluator (facts/rules/recursion with provenance hooks) →
  stratified negation → builtins + type inference → lexer/parser → CLI/agent API.
  Rationale: the risky, novel design is in the engine (semi-naive evaluation,
  stratification, provenance capture), while parsing is well-trodden; the
  evaluator's natural interface is the AST, so semantics are unit-tested without a
  parser; the test pyramid then grows outward (AST-level unit tests → parser golden
  tests → integration → system tests over the binary). The §16 worked examples
  serve as the canonical test corpus at every level.
- **2026-07-10** — **The AST is a designed contract, and the engine core is
  positional-only.** Named-argument literals and partial selection are resolved to
  positional form during front-end lowering, using the predicate schema (import
  header or `declare`); omitted fields become fresh anonymous variables at lowering
  time. Named arguments are purely surface syntax; the evaluator never sees them.
- **2026-07-10** — **Set semantics.** Relations are sets of facts: duplicates
  collapse everywhere, including at import time. Rationale: the classical Datalog
  model — fixpoint termination and semi-naive evaluation fall out naturally, and a
  fact derived multiple ways is *one* fact with multiple derivations, matching the
  §11 provenance model. Database imports lose nothing when tables have keys;
  keyless projections deduplicate, and the idiom for multiplicity-sensitive queries
  is to import the key column. Aggregates operate over distinct tuples (§9 will
  specify the details).
- **2026-07-10** — **CLI-first, skill-driven agent usage.** The primary agent
  interface is the executable, documented for agents via a skill definition; no
  server or MCP layer required for v1 (either can wrap the same CLI surface later).
- **2026-07-10** — **Datalog-in / Datalog-out.** Query results are emitted as
  ground facts in canonical Datalog syntax, one per line, deterministically
  ordered. Output is valid input (closure), enabling jq-style piecemeal pipelines
  and the token-economy workflow: agents issue narrow queries over large fact
  bases and read back only derived facts. This reframes pillar 3 — the agent API
  speaks Datalog on the data path; JSON is reserved for the machine-readable edges
  (structured errors §12, provenance trees §11).
- **2026-07-10** — **One-shot query flag** (`-q`, jq analog) accepting a bare atom
  (sugar for `?- …`) or a rule (define-and-select: emit the head predicate's
  facts). Exact semantics — synthesized answer predicate for bare-body queries,
  multiple `-q` flags, stdin conventions — still open.
- **2026-07-19** — **Surface AST / core IR split.** The parser's output
  (`src/ast.rs`) and the evaluator's input (`src/ir.rs`) are two distinct plain
  Rust type hierarchies connected by a lowering pass (`src/lower.rs`): schema &
  predicate collection (interning, arity checks) → named→positional resolution →
  wildcard elimination + variable numbering → safety/range restriction (§10) →
  stratification. The surface tree mirrors the grammar (spans, named args,
  wildcards) for source-accurate errors; the IR is positional and
  index-resolved. Rejected: a single shared tree with phase-parameterization
  (Trees That Grow — poor track record even in GHC) or optional filled-in-later
  fields (unchecked invariants). Precedent: Soufflé AST→RAM, rustc AST→HIR,
  Flix's chain of plain ASTs; Datalog evaluation is inherently a lowering
  pipeline. Type inference is a separate pass *after* lowering (the IR retains
  spans for its errors), not part of it.
- **2026-07-19** — **Variable representation**: per-rule numbered slots
  (`ir::Var(u32)`) plus a `var_names: Vec<Option<String>>` side table (`None` =
  lowering-generated fresh variable). The evaluator gets array-indexed bindings;
  provenance and errors recover original names. Body literal order is preserved
  through lowering — `RuleId` and body indices are the stable coordinates
  derivations reference (§11).
- **2026-07-19** — **Float totality**: `ir::F64` rejects NaN at construction and
  normalizes `-0.0` to `+0.0`; ordering is `total_cmp`, which under those two
  invariants agrees exactly with `==`, and hashing is bit-based. NaN-producing
  arithmetic (e.g. `0.0 / 0.0`) becomes a structured runtime error (ties into
  the open §8 division question). Rationale: set semantics and §14's
  deterministic sorted output require total `Eq`/`Ord`/`Hash` on values, and
  NaN facts (`X != X`) are toxic in a logic database.
- **2026-07-19** — **Canonical value ordering** is `Value`'s derived `Ord`:
  symbol < string < int < float < bool, then within-type — the §14 deterministic
  output order.
- **2026-07-19** — **Spans are `u32` byte offsets** (half-open) on every AST
  node; the IR keeps clause- and literal-level spans for post-lowering errors.
  **Symbol interning deferred**: `Value::Symbol(String)` in v1; `Value` is the
  single choke point, so an interner is a later drop-in if profiling justifies.
- **2026-07-19** — **Property-based testing adopted; `proptest` is the first
  dev-dependency.** Strategy and property catalog live in `testing.md`
  (single source of truth; AGENTS.md points there). Dev-dependencies don't
  affect the shipped library's zero-dependency posture but remain §17-tracked.
  Distinction ratified: test *generators* (`src/testgen.rs`, `cfg(test)`) are
  coverage machinery and allowed; ergonomic macro DSLs/builders for
  hand-written tests remain disallowed.
- **2026-07-19** — **Naive reference evaluator ratified as a permanent
  differential-testing oracle**: a deliberately simple naive evaluator is
  written alongside the semi-naive engine and kept under test cfg forever;
  `naive(p) == seminaive(p)` over generated programs is the anchor property
  (testing.md B1; precedent: queryFuzz, references.md group 9).
- **2026-07-19** — **Engine unit tests hand-construct the IR; lowering tests
  hand-construct the AST.** ("Hand-constructed ASTs" in earlier decisions
  predates the AST/IR split and covers both.)
- **2026-07-19** — **Evaluator API shape** (roadmap step 2 contract):
  `eval(&ir::Program) -> Result<Model, Error>` where `Model` holds every
  derived fact per predicate in set storage; queries are answered as
  projections over the `Model` using each query's `var_names` (so §16.1 runs
  end-to-end including its query, and §14's canonical sorted output later
  reads straight off the `Model`). `ir::Program.facts` may contain duplicates —
  they collapse when loaded into relation storage (set semantics). A program
  with imports is a structured evaluation error until fact sources (§13) land.
- **2026-07-19** — **Provenance recording: all derivations per fact**,
  deduplicated by rule instance (`RuleId` + premise facts). Matches the
  ratified "one fact, multiple derivations" model (§11) and the
  semiring/circuit literature (references.md group 5); `?why` can later show
  alternative proofs. This constrains the semi-naive delta loop's bookkeeping
  from day one — which is exactly why provenance is designed in early rather
  than retrofitted. (Considered first-witness-only recording; rejected as a
  retrofit trap inside the fixpoint loop.)
- **2026-07-19** — **Step-2 comparison policy**: the core evaluator reports
  comparison literals as a structured "not yet supported" error (the same
  pattern lowering uses for negation and named arguments). §8 semantics —
  including the open question of whether `=` is unification, assignment, or an
  equality builtin — are decided before comparisons evaluate.

### Open questions

- **Data sources beyond CSV** (TSV/JSON/JSONL, SQLite, DuckDB, Parquet/Arrow,
  Postgres): priority order and per-backend declaration details. — §13.
- **Aggregation vs recursion:** how far to go on recursive aggregation semantics. — §9.
- **`=` semantics:** unification, assignment, or an equality builtin? (raised by §16.3) — §8.
- **Aggregate expression syntax:** `count { Var : Goal }` is provisional — and `:`
  now also delimits named arguments, so the form will likely be revisited. — §9.
- **Arithmetic details:** `/` on ints (integer vs float division), overflow
  behavior, mixed int/float arithmetic and comparison coercions, operator
  precedence. — §8.
- **Provenance query syntax:** `?why <fact>` is provisional across CLI and API; also
  decide proof-tree JSON encoding. (§16.6) — §11/§14.
- **Provenance as facts:** the Datalog-in/Datalog-out closure property suggests
  `?why` output should also have a fact-shaped form (e.g. derivation edges as
  ground facts), so provenance can itself be piped back in and queried with
  Datalog — not just rendered as a tree or JSON. Design alongside §11; exercise
  with a §16 example. — §11/§14.
- **`-q` details:** synthesized answer-predicate naming for bare-body queries;
  multiple `-q` flags; stdin/`-` conventions; output ordering rule; `--format
  json` scope; the agent skill definition documenting the CLI surface. — §14.
- **Dependency choices:** lexer/parser approach, `serde` for the API, source
  backends — decide as the relevant sections stabilize.
