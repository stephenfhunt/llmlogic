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
2026-07-19; the named-argument rules below implemented in lowering
(`src/lower.rs`), 2026-07-20; type inference implemented as a post-lowering pass
(`src/typecheck.rs`), 2026-07-21; `declare`-signature verification implemented
(declared types threaded onto `ir::PredicateInfo.field_types`, verified in
`typecheck`), 2026-07-21. Source (2) imported *inferred* column types lands with
§13.*

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
without types; a type always follows a field name. A declared type that contradicts
what inference derives for that column (e.g. `age` declared `string` while a fact
supplies `30`) is a structured type error naming the column; a declared type
inference never otherwise constrains is simply left unrefuted (and seeds the
column's type). Two schemas for one predicate that disagree on declared types
conflict, naming both origins.

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

*Status: Draft (positive programs; extension to negation/aggregation arrives with §7/§9)*

A program's meaning is its **least model** (references.md group 1):

- The **Herbrand universe** is the finite set of constants appearing in the
  program (facts, rule constants, and later imported values); the **Herbrand
  base** is the set of all ground atoms formable from the program's predicates
  over it.
- The **immediate-consequence operator** `T_P` maps a fact set `I` to the
  program's facts plus every ground rule head whose body atoms all hold in
  `I`.
- For positive programs `T_P` is monotone over a finite lattice, so it has a
  least fixpoint, reached in finitely many steps — the **least Herbrand
  model**. That model is what evaluation computes (§15) and what queries are
  answered against, as projections.
- **Set semantics** throughout (§17): the model is a set of facts; a fact
  derivable several ways is one fact with several derivations (§11).

Stratified negation extends this to the **perfect model** — a least fixpoint
per stratum, lower strata frozen — in §7.

*Still to fill in: extension to aggregation (§9); semantics of
comparison/arithmetic literals (§8).*

## 7. Negation

*Status: Draft (semantics ratified; lands with roadmap step 4)*

Negation is **stratified negation-as-failure**. The semantics of record is the
**perfect model** of Apt/Blair/Walker (references.md group 3); the
definitional treatment of stratification, safety, and stratified evaluation is
Abiteboul/Hull/Vianu ch. 15 and Ullman's *Principles* (groups 1–2).

**Reading.** `not atom(…)` may appear as a body literal (`not` applies to
atoms only, §5). The literal holds when *no* fact of the negated predicate
matches the atom, where constants and positively-bound variables match
positionally and wildcard slots are **existential under the negation** (§17
2026-07-19): `not parent(_, X)` holds when no `parent` fact has `X` in its
second column. Negated literals bind nothing.

**Safety.** Every *named* variable in a negated atom must occur in a positive
body atom of the same clause; wildcard-fresh variables under negation are
scoped to the negated literal and never exported (§10, §17 2026-07-19).

**Stratification.** The **predicate dependency graph** has an edge `q → p`
for every rule with head predicate `p` and a body literal over `q`, marked
**negative** when that literal is negated. A program is **stratifiable** iff
no cycle contains a negative edge. Lowering computes a stratum number per
predicate by relaxation (Ullman): start every predicate at 0, then repeat to
fixpoint: `stratum(p) = max(stratum(p), stratum(q))` over positive edges and
`max(stratum(p), stratum(q) + 1)` over negative edges. A number exceeding the
predicate count witnesses recursion through negation, reported as a
structured error (§12) naming a concrete cycle. Rules inherit their head
predicate's stratum; within a stratum, rules keep source order. Programs
without negation form a single stratum.

**Semantics.** Evaluation runs stratum by stratum (§15), each to its least
fixpoint, treating lower strata as fixed input — a negated predicate's
relation is **complete and frozen** before any rule reads it negatively,
which is what makes negation-as-failure well-defined. The result is the
perfect model. By the **independence theorem** (Apt/Blair/Walker; AHV ch. 15)
every valid stratification yields the same model, so the particular numbering
above is an implementation detail, not a semantic commitment.

**Queries** may contain negated literals under the same safety rule; they run
over the finished model, where every relation is complete.

**Provenance** (§11): the derivation premise for a negated literal is the
**absence pattern** — the atom instantiated with the rule's bindings,
wildcard slots left open: `root("alice")` holds *because no `parent(_,
"alice")` fact exists*. This is a proof-tree-level why-not record, chosen
deliberately over extending §11's semiring story: provenance semirings cover
positive programs only, and the principled negation extensions
(dual-indeterminate and absorptive polynomials, references.md group 5) are
not adopted in v1.

**Out of scope**: well-founded and stable-model semantics (references.md
group 3). Stratified programs are the predictable subset for agent-generated
code; an unstratifiable program is a structured error, never a different
semantics.

## 8. Arithmetic & comparison builtins

*Status: Draft — evaluated in the engine and the naive oracle (`src/engine/`),
lowered with the assignment-safety exception (`src/lower.rs`), 2026-07-21.*

**Operators.** Comparison `= != < <= > >=`; arithmetic `+ - * /`. Both appear
as body literals: a comparison is an anti-join *filter*; arithmetic appears
inside a comparison's operands.

**`=` is assignment or equality.** If exactly one side is a bare variable not
yet bound (by a positive atom or an earlier assignment) and the other side fully
evaluates, `=` **binds** it (`next_year(X, N) :- age(X, A), N = A + 1.` binds
`N`). Otherwise both sides evaluate and `=` is an equality **filter**
(`A = 18`). `!= < <= > >=` are always filters.

**Strict types, no coercion.** `int` and `float` are distinct; **symbol**,
**string**, and **bool** are the other three. Arithmetic requires both operands
the *same numeric* type — `int op int → int`, `float op float → float`; any
mixed or non-numeric operand is a **type error**. A comparison requires both
operands the *same* type (any type); a cross-type comparison is a type error,
never a silent `false`. Ordered comparisons use the operand type's natural
order (ints/floats numerically, strings/symbols lexicographically, `false <
true`). Post-§4, these conflicts are caught before evaluation; pre-typecheck
they are structured runtime errors — same error channel either way.

**Arithmetic edge cases** are structured errors, never a wrap or panic: integer
division truncates toward zero, division by zero and integer overflow error, and
a NaN-producing float operation (`0.0 / 0.0`) errors via `F64::new`.

**Mode / safety (§10).** Every comparison operand variable must be bound by a
positive atom, *except* an `=`-assignment target, which the assignment binds.
Assignments are evaluated in source order (after positives and negations), so a
later one may depend on an earlier (`N = A+1, M = N+1`); a negated atom's
variables must be *positively* bound (negations run before assignments).

*Operator precedence for the surface syntax is deferred to the parser (§5, Phase
D); the AST already carries whatever grouping the parser chose.*

## 9. Aggregation

*Status: TBD*

*To fill in: supported aggregates (count, sum, min, max, avg, …), grouping semantics,
and interaction with recursion and stratification.*

## 10. Recursion & safety

*Status: Draft (range restriction only; the rest TBD)*

**Range restriction** (enforced by front-end lowering, `src/lower.rs`): every
variable in a rule head, every *named* variable in a negated atom, and every
variable occurring only in comparisons must also occur in a positive body
atom; facts must be ground. Wildcard-fresh variables in negated atoms are
exempt — they are existential under the negation and never exported (§7).
Violations are structured semantic errors reported before evaluation.

Recursion through negation is rejected by stratification (§7).

*Still to fill in: termination guarantees; safety/mode conditions for arithmetic
(§8); treatment of recursion through aggregation (§9).*

## 11. Provenance / explainability

*Status: Draft (data model; the query surface and JSON encoding remain TBD)*

The data model (`src/provenance.rs`, recorded by the engine during the §15
fixpoint):

- A **derivation** is a ground rule instance: a rule identity plus one
  premise per body literal, aligned index-for-index (the IR's stable
  `RuleId`/`BodyIdx` coordinates, §17) — the matched **fact** for a positive
  literal, the **absence pattern** for a negated one (§7): the negated atom
  under the rule's bindings, wildcard slots left open.
- The engine records **all derivations of every derived fact**, deduplicated
  by rule instance (§17): one fact, many proofs. Base facts have no
  derivation; they are anchored by the program text (or, later, the import)
  that asserted them.
- A **proof tree** is one finite proof of one fact: derived nodes carry the
  fact, its rule, and child proofs for each premise; **leaves are base facts
  or absence patterns** (an absence terminates a branch — "no such fact
  exists" needs no sub-proof). Extraction picks, per fact, a derivation whose
  fact premises all first appeared strictly earlier in the fixpoint (§17
  first-round stamping; absence premises always qualify), so proofs stay
  finite even when facts support each other cyclically.
- Names for rendering recover from the IR's retained tables: predicate names,
  per-rule variable names, field names (when the predicate has a schema), and
  spans. A fact over a relation with known field names can therefore be
  rendered in named form — `employee(name: "alice", title: "manager")` — which
  matters for wide imported tables, where the positional rendering is mostly
  noise.

*Still open (§17): the `?why` query form across CLI and API, the proof-tree
JSON encoding, and the provenance-as-facts closure question.*

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

*Status: Draft (positive programs; negation joins the loop at roadmap step 4)*

`eval` (`src/engine/`) computes the least model (§6) bottom-up:

- **Load**: program facts enter per-predicate *set* storage (duplicates
  collapse, §17). Relations are sorted sets, so iteration — and hence §14's
  canonical output order — is deterministic by construction.
- **Stratified fixpoint**: the IR's strata are evaluated in order, each to
  fixpoint before the next (a single stratum until §7 lands).
- **Semi-naive iteration** (references.md group 2): each stratum begins with a
  naive seed pass over the full relations; each later round joins, per rule
  and per body position *i*, the previous round's **delta** at *i*, the full
  relations before *i*, and the pre-delta relations after *i* — so every new
  rule instance is enumerated exactly once. The stratum stops when a round
  adds no new facts.
- **Provenance is captured inside the loop**: every successful body match
  records a derivation (rule + premise facts, §11), deduplicated by rule
  instance, and every fact is stamped with the round it first appeared in
  (§17: what makes finite proof extraction possible). Recording *all*
  derivations is a constraint on the delta discipline — the reason provenance
  is designed into the fixpoint rather than retrofitted.
- **Queries** run the same join machinery over the finished model and project
  their named variables; rows are deduplicated and canonically sorted.
- A **naive reference evaluator** (test-only, permanent, §17) recomputes the
  fixpoint by brute force as the differential oracle (testing.md B1).
- **Magic sets** remain a future optimization. Join order and indexing are
  evaluator-internal and free to change — the IR never encodes them.

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
*Raised:* `not` keyword and wildcard `_` (ratified, §3/§5); the safety rule and
how stratification is computed and reported (ratified, §7/§10 — *named*
variables in negated atoms must be positively bound, wildcards are existential
under the negation, and stratification is predicate-level numbering with a
structured cycle error).

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
*Resolved (§8, 2026-07-21):* `=` is assignment when one side is a bare unbound
variable (so `N = A + 1` binds `N`), else an equality filter; comparison and
arithmetic operands must be bound by a positive atom or an earlier assignment;
numerics are strict with no coercion. Evaluated end-to-end
(`example_16_3_comparisons_and_assignment_evaluate`).

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

*Implemented 2026-07-20* as lowering pass 2 (`src/lower.rs`), with
`ast::fixtures::example_16_7` / `ir::fixtures::example_16_7` as the contract
test. One adaptation in the fixture: the `employee` import is written with an
explicit schema, because header-derived field names need fact sources (§13,
not yet implemented). Until then, named access to a schema-less import is a
structured error.

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
- **2026-07-19** — **First-round stamping for finite proof extraction.** The
  `Model` stamps every fact with the fixpoint round it first appeared in
  (base facts: round 0; the counter is monotone across strata). Proof-tree
  extraction picks, per fact, the `Ord`-least recorded derivation whose
  premises all carry strictly smaller rounds — the derivation that first
  produced the fact always qualifies, and the strictly-decreasing bound makes
  proofs finite. Rationale: all-derivations storage admits cyclic
  justifications (`a` via `b` and `b` via `a`); a well-founded selection rule
  is required for `?why` to terminate, and the round stamp is one `u32` per
  fact captured for free inside the fixpoint.
- **2026-07-19** — **The naive oracle is facts-only.** It computes the least
  model's fact set (its ~70 obviously-correct lines are the point) and does
  not record provenance; provenance correctness is instead checked by replay —
  every recorded derivation's rule instance is re-matched against its premises
  with the oracle's own matcher (testing.md E3). Rejected: a second
  provenance-recording evaluator (doubles the surface that must be
  "obviously correct" without strengthening the differential).
- **2026-07-19** — **Phase E properties E1–E4 pulled forward to step 2**
  (testing.md): provenance recording lands with the evaluator, so its
  properties are tested the session it is written. Only E5
  (provenance-as-facts closure) waits on the §11/§14 surface design.
- **2026-07-19** — **Negated premises in derivations: a `Premise` enum,
  `BodyIdx`-aligned** (pre-decision for the §7 negation session).
  `Derivation.premises` becomes one entry per body literal:
  `Premise::Fact(fact)` for positive matches, `Premise::Absent(pattern)` for
  negated literals, where the pattern is the negated atom instantiated with
  the rule's bindings (wildcard-fresh variables left open). Keeps the
  premise↔body alignment invariant and lets proof trees explain negation
  ("holds because no `parent(_, "alice")` fact exists") — the explainability
  pillar. Replay (testing.md E3) checks `Absent` entries as non-matches
  against the model. (Considered positive-premises-only; rejected — loses
  alignment and silently drops *why* the negation held.)
- **2026-07-19** — **Wildcards inside negated atoms are existential under the
  negation** (pre-decision for §7/§10): `not parent(_, X)` means "no `parent`
  fact whose second column is `X`". The §10 safety rule reads: every *named*
  variable in a negated atom must occur in a positive body atom;
  wildcard-fresh variables in negated atoms are scoped under the negation
  (never exported). This is the standard NAF reading and keeps §16.2 as
  written. Lowering's wildcard elimination must therefore tag (or scope)
  fresh variables introduced under negation rather than treating them as
  ordinary rule variables.

- **2026-07-20** — **Named-argument resolution is IR-invisible.** A named
  literal and the positional literal it denotes lower to *structurally
  identical* IR — the correctness condition for pass 2, and a property
  (testing.md A13) rather than a comment. Fields land at their schema
  positions regardless of the order written, and an omitted field becomes a
  fresh anonymous slot exactly as a positional `_` would. Consequence: the
  evaluator, provenance, and every later pass are entirely unaware that named
  arguments exist, which is what makes §4's convenience free.
- **2026-07-20** — **Schema collection is program-wide.** `Lowerer.schemas` maps
  predicate name → field names, populated in pass 1 from `declare` statements
  and *explicit* import schemas. Because pass 1 walks the whole program before
  any clause is lowered, a `declare` may appear after the rule that uses the
  named form — declaration order does not matter. Two conflict rules: a
  duplicate field name within one schema is an error, and a predicate given
  two different schemas (e.g. a `declare` and an import schema that disagree)
  is an error naming both origins; identical repeats are accepted.
- **2026-07-20 (amended same day)** — **Field names are retained on
  `ir::PredicateInfo`** as `fields: Option<Vec<String>>`, with the invariant
  that `Some(f)` implies `f.len() == arity`. Populated from the pass-1 registry
  after interning; `None` for predicates with no schema, and deliberately not
  attached when a schema disagrees with the interned arity (an arity clash is
  already reported, and the IR stays self-consistent on the error path).

  *This amends the same day's original decision that field names were purely
  `lower`-internal.* That was two decisions bundled as one. The first —
  named literals desugar to positional form, and the evaluator never sees named
  arguments — **stands**, and is now backed by property A13 (the two forms lower
  to identical IR) and by composing cleanly with negation: omitted fields become
  fresh slots through the same path as wildcards, so the wildcards-are-
  existential rule covers partial selection under negation for free. The second
  — *discarding* the names — was wrong. Three consumers need them and all run
  over the IR with no access to the AST: **type inference** (§4, a separate pass
  after lowering) must name the conflicting *column*; **provenance** (§11) should
  render a wide relation in named form rather than as eight positional columns;
  and §14's canonical output has the same need. The objection that field names
  are surface syntax does not survive scrutiny — the IR already retains
  `PredicateInfo::name`, `Rule::var_names`, and spans purely for rendering and
  errors, none of which affect evaluation. Field names are that same category,
  so retaining them follows existing precedent rather than weakening the
  surface/core split. Atoms remain positional; nothing about evaluation changes.
- **2026-07-20** — **Named access requires a *known* schema, and a schema-less
  import has none until §13 lands.** `import "f.csv" as employee.` infers its
  field names from the CSV header at load time, which lowering cannot see, so
  `employee(name: N)` against it is a structured error suggesting a `declare`
  or an explicit import schema. This is a temporary consequence of fact
  sources being unimplemented, not a language rule — §16.7's spec text stays
  as written, and its fixture uses the explicit-schema import form meanwhile.
- **2026-07-20** — **Fact grounding is checked against the lowered head**, not
  against surface positional terms, so the named and positional paths behave
  identically and only the error *wording* differs (a named fact names the
  offending field, a positional one names the index). This replaced a latent
  bug: the previous zip-against-surface-terms formulation would have silently
  dropped named facts, producing neither a fact nor an error, the moment named
  heads began lowering successfully.
- **2026-07-20** — **Stratification is Ullman relaxation numbering with
  concrete-cycle reporting** (§7 session). Lowering numbers predicates by
  relaxation over the dependency graph (positive edge: `max(s(p), s(q))`;
  negative edge: `max(s(p), s(q)+1)`); exceeding the predicate count witnesses
  recursion through negation, and the structured error names a concrete cycle
  recovered by walking back from a negative edge. Rule strata are head-predicate
  strata, bucketed in `RuleId` order — source order within each stratum, and
  all rules defining one predicate share a stratum (what freezes a negated
  relation before its readers run). On the error path lowering falls back to
  the single-stratum shape so the IR stays well-formed (the `attach_field_names`
  precedent). Chosen over Tarjan SCC: the relaxation is ~25 dependency-free
  lines and directly yields the level function §7 and testing.md C1 talk about;
  by the independence theorem the choice carries no semantic weight.
- **2026-07-20** — **No wildcard tagging needed for negation safety** (amends
  the 2026-07-19 "must tag (or scope) fresh variables" note). The §10 check
  narrows to slots with a *name*: `VarScope::fresh()` never enters the name
  map, so a fresh slot occurs at exactly one term position in the whole rule —
  a `None`-named slot inside a negated atom was necessarily created there, and
  `var_names[slot].is_some()` coincides exactly with "named". No new lowering
  mechanism; the argument is recorded at the check site.
- **2026-07-20** — **`AbsentPattern` is `PredId` plus `Vec<Option<Value>>`**:
  `Some` for constants and positively-bound variables, `None` for
  wildcard-fresh slots (existential under the negation). `Premise` and
  `AbsentPattern` carry the full `Eq`/`Hash`/`Ord` derives — `Derivation`
  remains a dedup key. Proof trees terminate at `Absent` leaves; the
  first-round guard applies only to fact premises (absences carry no round
  and always qualify).
- **2026-07-20** — **Negation evaluates as an anti-join filter, scheduled
  after the positives** — evaluator-internal ordering only; IR body order is
  untouched and premises are recorded at their true `BodyIdx`. Negated
  positions bind nothing, never take a delta view, and always read the full
  (frozen, lower-stratum) relation with a dedicated pattern-vs-tuple scan —
  deliberately not the positive matcher, which binds. The engine also
  validates the negation contract statically (named negated vars positively
  bound; negated predicates defined only in strictly lower strata), the same
  malformed-IR posture as the strata-coverage check.
- **2026-07-20** — **The naive oracle iterates strata** (extends, without
  contradicting, the facts-only decision): per-stratum naive fixpoint with
  negation checked against the growing fact set — sound because valid strata
  freeze negated extents, the same invariant expressed independently of the
  semi-naive engine. This makes B1 the perfect-model differential; a second
  full evaluator for C2 stays rejected for the same reason as before.
- **2026-07-20** — **Negation provenance is a proof-tree-level why-not
  record, not a semiring construction.** Provenance semirings (references.md
  group 5) cover positive programs; the principled extensions — Grädel–Tannen
  dual-indeterminate polynomials, Dannert–Grädel–Naaf–Tannen absorptive
  polynomials for fixed-point logic — are catalogued in group 5 and
  deliberately not adopted in v1. `Premise::Absent` records the instantiated
  pattern; that is what the explainability pillar needs (§7, §11).
- **2026-07-21** — **§8 builtins evaluate; the three open questions are
  resolved** (§8, replacing the like-named open questions): (1) `=` is
  assignment when one side is a bare unbound variable, else an equality filter;
  (2) numerics are strict — mixed `int`/`float` (and any cross-type comparison)
  is a type error, no coercion; (3) integer `/` truncates toward zero, and
  division by zero, integer overflow, and NaN-producing float ops are structured
  errors. Comparisons are scheduled after positives and negations in the join
  (evaluator-internal ordering; premises land at their true `BodyIdx`), so a
  runtime arithmetic error propagates out of evaluation as `Error::Semantic`.
  A new `Premise::Builtin { op, lhs, rhs }` records a satisfied comparison as a
  self-justifying proof leaf (no round, no sub-proof — like `Absent`). Lowering
  gains an assignment-safety exception: an `=`-target counts as bound for head
  and comparison range-restriction, computed in source order. The naive oracle
  learned the same evaluation so B1 (naive ≡ semi-naive) holds over
  comparison/arithmetic programs, including the error path (both reject a
  `/ 0`). Type *inference* (§4) — verifying these operand rules statically
  before evaluation — is the next step.
- **2026-07-21** — **Type inference is a separate pass, and `eval` stays
  type-blind.** `typecheck(&ir::Program) -> Result<TypeEnv, Vec<Error>>`
  (`src/typecheck.rs`) runs between `lower` and `eval` — a union-find over the
  five primitive types with one class per predicate column and per rule/query
  variable. Facts pin columns; a variable unifies every position it occupies;
  §8 builtins add the operand constraints (arithmetic/ordered comparison ⇒
  numeric; every comparison ⇒ same-type operands). Conflicts are collected (not
  fail-fast) and reported naming the column/variable. `eval` deliberately does
  **not** call it — the evaluator is a total function over the whole value space
  (its laws are type-independent), and coupling the two would be a category
  error. Consequently the **evaluation-property generators were migrated to
  well-typed programs**: `arb_program_with_edb`/`arb_extension_pair` relabel
  every constant to a `symbol` injectively (`monotype`), so the B/E suite runs
  over the reachable, type-checkable state space while staying isomorphic to the
  old programs (no property changed behavior); cross-type `Value` ordering stays
  covered by A1–A5, its proper home. Properties **C4** (inference soundness) and
  **C5** (typed-generator completeness) are green over a dedicated typed
  generator `arb_well_typed_program`. Still deferred: imported column types
  (§13) and `declare`-signature verification (needs declared types threaded onto
  `ir::PredicateInfo`).
- **2026-07-21** — **`declare`-signature verification, closing §4.** Declared
  column types now flow AST→IR: `ir::PredicateInfo` gains a
  `field_types: Option<Vec<Option<TypeName>>>` parallel to `fields` (invariant:
  `Some` iff `fields` is `Some`, same length; a position is `None` when a field
  was named without a type — the grammar forbids a type without a name).
  `lower::collect_schema` records `FieldDecl.ty` and `attach_field_names` threads
  it onto the IR; two schemas agreeing on names but disagreeing on types now
  conflict, naming both origins. Verification is a **dedicated pass in
  `typecheck::finish`** (not fed through `set_type` during `gather`), so declared
  types stay out of inference propagation and the message is tailored ("declared
  as X but its values are Y") rather than the generic "used as both". A declared
  type inference never constrains is left unrefuted and seeds the column's type in
  the `TypeEnv`. Rationale for reusing `ast::TypeName` in the IR: `ir` already
  imports `ArithOp`/`CmpOp`/`Span` from `ast`, so no new coupling. Only
  user-written `declare`/import-schema types are checked here; imported *inferred*
  column types remain a §13 concern. testing.md **C6** covers it.

### Open questions

- **Data sources beyond CSV** (TSV/JSON/JSONL, SQLite, DuckDB, Parquet/Arrow,
  Postgres): priority order and per-backend declaration details. — §13.
- **Aggregation vs recursion:** how far to go on recursive aggregation semantics. — §9.
- **Aggregate expression syntax:** `count { Var : Goal }` is provisional — and `:`
  now also delimits named arguments, so the form will likely be revisited. — §9.
- **Operator precedence:** the surface grammar for arithmetic/comparison
  precedence and associativity (the evaluation semantics are settled in §8; this
  is a parser concern). — §5/§8.
- **Provenance query syntax:** `?why <fact>` is provisional across CLI and API; also
  decide proof-tree JSON encoding. (§16.6) — §11/§14.
- **Semiring provenance under negation:** parked research thread with a worked
  sketch in `notes/semiring-provenance.md` — the derivation store is already a
  boolean provenance circuit, `Premise::Absent` a factored dual token; candidate
  work: `?whynot` with minimal repairs, tropical cheapest-proof selection for
  token economy. — §11.
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
