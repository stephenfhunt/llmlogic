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

*Status: Draft — validated by the hand-rolled lexer (`src/lexer.rs`), 2026-07-22.*

- **Comments** — `%` **or `#`** to end of line (the `#` alias is added for LLM
  ergonomics, §17 2026-07-22; `//` is *not* a comment — reserved against a
  future floor-division operator, and lexed to a targeted did-you-mean error).
- **Identifiers** (relation names, symbols, field names) — start with a lowercase
  letter, continue with letters, digits, `_` (snake_case by convention): `parent`,
  `family_tree`.
- **Variables** — start with an uppercase letter or `_`: `X`, `Who`, `_Age`. A lone
  `_` is the anonymous variable; each occurrence is a fresh variable.
- **Literals**
  - *Integers* — `0`, `42`, `-7` (64-bit signed). The lexer produces *unsigned*
    integers; a leading `-` is the subtraction operator, folded onto the literal
    by the parser in operand position (§17, 2026-07-22).
  - *Floats* — `3.14`, `-0.5`, `6.02e23` (64-bit IEEE 754). A `.` begins the
    fraction only when a digit follows, so `p(1).` lexes as `1` then the
    terminator.
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
- **Character set** — identifiers, variables, and operators are ASCII; string
  *contents* may be any Unicode. A non-ASCII character outside a string is a
  lexical error with a span; curly/smart quotes get a dedicated hint.
- **Near-miss recovery** — Prolog-prior spellings are recognized and rejected
  with a targeted, actionable message rather than a bare "unexpected character":
  `=<`→`<=`, `\=`→`!=`, `\+`/`!`→`not`, `//`/`/* */`→comment markers. The lexer
  substitutes the intended token so parsing continues (§17, 2026-07-22).

## 4. Data model & types

*Status: Draft — validated by the AST/IR prototype (`src/ast.rs`, `src/ir.rs`),
2026-07-19; the named-argument rules below implemented in lowering
(`src/lower.rs`), 2026-07-20; type inference implemented as a post-lowering pass
(`src/typecheck.rs`), 2026-07-21; `declare`-signature verification implemented
(declared types threaded onto `ir::PredicateInfo.field_types`, verified in
`typecheck`), 2026-07-21. Source (2) imported *inferred* column types needs no
separate inference channel: imports materialize as column-uniform facts before
lowering, and source (1) types them — decided with §13, 2026-07-23; lands with
roadmap step 7.*

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

*Status: Draft — validated by the AST/IR prototype (`src/ast.rs`), 2026-07-19,
and by the hand-rolled recursive-descent parser (`src/parser.rs`), 2026-07-22.*

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

import        = data_import | module_import ;
module_import = "import" string "." ;
data_import   = "import" string [ "table" string ] "as" ident
                [ "(" field { "," field } ")" ] "." ;
declaration = "declare" ident "(" field { "," field } ")" "." ;
field       = ident [ ":" type ] ;
type        = "int" | "float" | "string" | "symbol" | "bool" ;

clause      = atom [ ":-" body ] "." ;          (* fact when no body, else rule *)
query       = "?-" conjunction "." ;            (* queries are conjunctive *)
body        = conjunction { ";" conjunction } ;  (* rule bodies: DNF, §17 *)
conjunction = literal { "," literal } ;
literal     = [ "not" ] atom | comparison ;

atom        = ident "(" args ")" ;              (* at least one argument *)
args        = positional | named ;
positional  = expr { "," expr } ;               (* args are expressions, §17 *)
named       = ident ":" expr { "," ident ":" expr } ;

term        = constant | variable ;
constant    = integer | float | string | bool | ident ;    (* bare ident = symbol *)
variable    = VARIABLE ;                        (* uppercase- or "_"-initial, §3 *)

comparison  = expr cmp expr ;                   (* non-associative; no chaining *)
cmp         = "=" | "!=" | "<" | "<=" | ">" | ">=" ;
expr        = add ;
add         = mul { ( "+" | "-" ) mul } ;       (* left-assoc *)
mul         = primary { ( "*" | "/" ) primary } ;  (* binds tighter, left-assoc *)
primary     = [ "-" ] number | term ;           (* prefix "-" folds onto a literal *)
```

Notes:
- **Operator precedence** (was open): `*` `/` bind tighter than `+` `-`, both
  left-associative (precedence climbing); comparisons are non-associative and do
  **not** chain — `0 <= X <= 9` is a targeted error suggesting `0 <= X, X <= 9`
  (§17, 2026-07-22).
- **Atom arguments are full expressions**, so inline arithmetic parses
  (`succ(N, N+1)`); lowering hoists a compound argument to an `=`-assignment, so
  the IR is unchanged (§17, 2026-07-22). A compound argument in a *fact* is
  constant-folded (`p(1+1).` → `p(2).`).
- **Disjunction `;`** in a rule body is top-level DNF (no parentheses in v1);
  `,` binds tighter than `;`. The parser expands each disjunct into its own
  clause sharing the head, so the AST/IR stay conjunction-only. Queries stay
  conjunctive (§17, 2026-07-22).
- **Signed literals**: a prefix `-` on a numeric literal folds into a negative
  constant; a prefix `-` on anything else is an error (there is no unary-minus
  node).
- A predicate takes **at least one argument** — `p.`/`p()` is a targeted error.
- Whether `args` is positional or named is decided by the literal's first
  argument (`ident :`); mixing the two styles in one literal is a syntax error
  with a targeted message (§12).
- `not` applies to atoms only, not comparisons; semantics and safety are §7.
  Uppercase relation names, `not` before a comparison, and trailing commas each
  get a targeted did-you-mean error (the strict-grammar pillar, §2).
- Aggregate expressions (§9) are **not yet in the grammar** — their syntax is an
  open question (§17), in part because `:` now also delimits named arguments.
- **`table` is a contextual keyword, not reserved** (§17, 2026-07-23): after
  `import <string>` the only legal continuations are `.`, `as`, or
  `table <string>`, so the parser matches an identifier spelled `table` there;
  everywhere else `table` remains an ordinary identifier (relation/field name).
- **Module vs data import is decided by shape, not file type**: no `as` clause =
  module import (§13). The grammar stays context-free; extension/scheme
  *semantics* (`.dl` vs data formats vs URLs) are checked during module
  resolution with targeted errors either way (`import "x.csv".` → "importing
  data requires `as`"; `import "lib.dl" as x.` → "a `.dl` file is a module
  import; drop the `as` clause").
- **URLs need no grammar**: a path is a plain string; a scheme (`http://`,
  `https://`) makes it a URL at resolution time (§13).

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
inside a comparison's operands. **Precedence** (parser, §5, resolved
2026-07-22): `*` `/` bind tighter than `+` `-`, both left-associative;
comparisons are non-associative and do not chain. Arithmetic may also be written
**inline in an atom argument** (`succ(N, N+1)`), which lowering hoists to an
`=`-assignment — surface sugar only, the §8 semantics are unchanged.

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

*Status: Ratified 2026-07-23 (design deep-dive; decisions in §17) — implementation
is roadmap step 7.*

`import` has two forms, distinguished by shape: a **data import** binds external
tabular data to one relation (`as` clause present), and a **module import**
splices another Datalog file's statements into the program (no `as` clause).

### Data imports

```datalog
import "data/parents.csv" as parent.
```

- The imported relation is **extensional** (base facts): used in rules exactly like
  in-program facts, and its tuples anchor the leaves of provenance trees (§11).
  Tuples are deduplicated at import time (set semantics); several imports may
  target the same relation, and their facts union (schema/arity disagreements are
  the usual conflict errors).
- **Schema inference — field names** come from the source: the CSV header row, or
  the keys/columns of self-describing sources (JSONL, Parquet, databases). Field
  names must be legal identifiers (§3) and unique; otherwise the import is a
  structured error suggesting an explicit schema — never silent sanitization.
- **Schema inference — column types.** CSV cells are untyped text, so the
  language's **own literal grammar** is the rulebook (decided 2026-07-23 over
  delegating to a reader's type sniffer, §17): a cell is typed int, float, or
  bool **iff the lexer reads it as exactly that literal** (as accepted in fact
  positions, including a leading sign); anything else — including empty cells —
  is a string. A column's type is the unification of its cells: all-int → int,
  int/float mix → float, all-bool → bool, anything else (or any empty cell) →
  string. Inferred types are never symbol. This makes the anchor property exact:
  **an import means precisely the facts you would get by writing its cells as
  in-program literals**, and inference is deterministic across engine and
  dependency versions.
- **Typed sources are their own authority**: JSON, Parquet, and database columns
  carry types, which are coerced onto the five value types (a JSON `"42"` stays
  a string, never re-inferred; ints are range-checked into int; date/time-like
  types become their ISO text as strings; NULLs and nested values are structured
  errors naming row and column — the value space has no null). *(Missing/null
  handling is slated to change: an empty CSV cell currently becomes `""` while
  a JSON/Parquet null is an error — three backends, three policies. §17
  2026-07-23 decides a uniform first-class optional/absent value; until it
  lands, the current per-backend behavior stands.)*
- **Explicit schema** overrides inference, and is required for headerless CSV:

  ```datalog
  import "data/parents.csv" as parent(parent: string, child: string).
  ```

  Binding is positional for CSV and by field name (set-equality, schema order
  wins) for self-describing sources. Declared types coerce cells, and a cell
  that will not coerce is a structured error naming row, column, and file.
  `symbol` may be declared explicitly (useful for joining in-program symbol
  facts). With an explicit schema the CSV first row is treated as a header and
  skipped **iff** its cells exactly equal the schema's field names
  (case-sensitive); otherwise it is data (decided 2026-07-23).
- Named-argument access works on imported relations immediately, using the header
  (or explicit) field names.
- Paths are resolved **relative to the directory of the importing file** (each
  file resolves its own imports; a stdin/`-q`-only program resolves against the
  working directory). A path with a URL scheme (`http://`, `https://`) is
  fetched instead — URL imports are part of the format table below.
- **Formats** (decided 2026-07-23; the reader is DuckDB, see below):

  | source | notes |
  |---|---|
  | `*.csv` | untyped text; header + literal-grammar inference above |
  | `*.jsonl`, `*.ndjson` | one object per non-blank line; field order = the first record's key order; every record must supply the same key set; scalar values only |
  | `*.parquet` | natively typed |
  | `http(s)://…` | fetched via DuckDB httpfs, then treated per its extension |
  | `*.duckdb`, `*.db`, `*.sqlite*` + `table "…"` | **syntax reserved, loading deferred** — `import "analytics.duckdb" table "orders" as order.` |

  Anything else is a structured "unsupported import format" error listing the
  supported set.

### The reader: DuckDB (default feature)

The import layer is powered by **DuckDB** (`duckdb` crate, bundled), a
**default-on cargo feature**: every normal build has imports; a
`--no-default-features` build is the escape hatch, in which any data import is a
structured error naming the feature. DuckDB does transport and dialect parsing
only — CSV is read `all_varchar` so the literal-grammar inference above is the
sole typing authority. The engine-side seam is the `FactSource` trait
(`src/sources/`): backends read raw tables, and one shared `finalize` layer
applies every rule in this section. Imports are **eagerly materialized** into
ordinary in-memory facts before lowering; the evaluator never touches DuckDB
(lazy loading / filter pushdown is an explicit non-goal for v1 — §17 open
question). URL imports read **directly** over DuckDB httpfs — no local file is
written, so a read-only environment can still import from a URL. The first URL
import runs `INSTALL httpfs; LOAD httpfs;` (a runtime extension fetch; failure
is a structured error explaining the network requirement).

### Module imports

```datalog
import "lib/family.dl".
```

- Splices the imported file's statements in place (textual-inclusion semantics),
  depth-first, preserving source order.
- **Once-only inclusion by canonical path**: a file is spliced the first time it
  is reached; diamonds deduplicate and cycles terminate harmlessly.
- No namespacing in v1: predicates, facts, rules, and `declare`s land in the one
  global namespace exactly as if written in the importing file.
- A query (`?- …`) in an imported file is a structured error naming the file —
  libraries define, they don't ask. Root-file queries are unaffected.
- Module imports are local files only in v1 (no URL modules).

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
  property that makes jq effective for agents, mechanically checked by
  `testing.md` D1). Implemented by the canonical printer (`src/print.rs`) and
  the `run` pipeline (`src/api.rs`), 2026-07-22.

**Canonical output form** (resolved 2026-07-22). Values print in the §4
cross-type order (symbol < string < int < float < bool); strings are
double-quoted with the §3 escapes; a float always carries a decimal point
(`1.0`, not `1`) so it re-lexes as a float. The **query answer shape** is:

- a **single positive-atom** query re-emits that atom with the answer bindings
  substituted (`?- ancestor("alice", Who).` → `ancestor("alice", "bob").` …)
  when every variable position is a projected variable; a ground such query
  prints the atom once if it holds;
- any **other** body emits synthesized `answer/N` facts over the query's named
  variables;
- rows are deduplicated and sorted.

**Binary contract** (2026-07-22; `-q` completed 2026-07-23, roadmap step 6).
Invocation is `datalog [<file> | -] [-q <query>]…`. The positional source is a
program file, `-` for stdin, or **omitted** (empty base program); at most one is
allowed. Each query's answers print to stdout as canonical facts and the process
exits **0**; program errors (lex/parse/lower/type/eval) print to stderr, one per
line, exit **1**; a usage error (bad arguments, unreadable file, or a bare
`datalog` with no source and no `-q`) exits **2**. Argument parsing is a small
hand-rolled loop in `src/main.rs`; the logic lives in the library
(`api::run_with_queries` / `program_with_queries`), so `main` stays thin.

**One-shot `-q` queries** — the jq analog (resolved 2026-07-23):

```sh
# bare-atom query: sugar for appending `?- ...` to the loaded program
datalog family.dl -q 'ancestor("alice", X)'

# comma-body query: also just a query body (answered by the answer/N shape)
datalog people.dl -q 'person(name: N, age: A), A >= 18'

# define-and-select: append the rule, then a synthesized `?- <head>.`
datalog family.dl -q 'grandparent(X, Z) :- parent(X, Y), parent(Y, Z)'

# composition over pipes ("-" reads stdin)
datalog people.dl -q 'adult(N) :- person(name: N, age: A), A >= 18.' \
  | datalog - -q 'adult(N), N != "bob"'
```

`-q` semantics: an argument is classified by **parsing** it (never by splitting
on `:-`, which a string literal may contain). A single clause with a non-empty
body is a **rule** — appended verbatim, followed by a synthesized `?- <head>.`
over its head atom. Anything else (a bare atom, or a comma-separated body) is a
**query body** and is appended as `?- <arg>.`. A trailing `.` is optional.
Multiple `-q` apply in CLI order, so a later one may reference a predicate an
earlier one defined; each produces its own answer block, in order.

The motivating workflow is **token economy**: an agent issues precise, narrow
queries over large fact bases and reads back only the derived facts, instead of
loading raw data into context — the piecemeal analysis pattern agents already use
jq for over JSON, made Datalog-native. The agent-facing usage guide is
[`docs/agent-skill.md`](docs/agent-skill.md).

**JSON is deferred as low-value** (decided 2026-07-23), not just unimplemented.
The data path is already Datalog-native — the focused filtering an agent does
with jq over JSON is done here with *another `-q`* over facts, so JSON on the
data path works against the design. At the edges, structured errors (§12) are
already actionable *prose* with spans and did-you-mean hints (more useful to an
LLM consumer than JSON codes), and provenance (§11), if ever surfaced, should be
**provenance-as-facts** (Datalog-native, preserving the closure). `--format
json` therefore stays a documented future *edge* feature only — hand-rolled if it
ever lands, keeping the zero-runtime-dependency stance.

One shape the closure does not yet cover: a body with no named variables that is
not a substitutable single atom (a pure existence check) produces no fact-shaped
output in v1.

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
lexing; comment syntax. Query-result shaping resolved in §14 (canonical output
form, 2026-07-22).

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
typing (the language's literal grammar, 2026-07-23); imported tuples are base
facts and anchor provenance leaves; formats and reader (DuckDB, default-on
feature, 2026-07-23). *Still open:* database loading and filter pushdown (§17).

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
structured error. *(§13 was ratified 2026-07-23 — the adaptation dissolves
when roadmap step 7 lands, since imports load before lowering.)*

## 17. Decisions log & open questions

*Status: living*

### Decisions

- **2026-07-23** — **Missing values → a first-class optional/absent value**
  (direction decided; design + implementation deferred to their own session).
  Dogfooding §13 imports on real USDA FoodData Central data (`docs/worklog.md`)
  exposed two things: the three fact-source backends handle absence
  **inconsistently** — a CSV empty cell becomes `""` (which forces the whole
  column to `string`), a JSONL missing key / explicit null is an error, a
  Parquet NULL is an error — and the strict "value space has no null" rule
  makes ordinary sparse real-world data (nullable Parquet/DB columns, gapped
  scientific CSVs) either unusable for arithmetic or unimportable.
  - **Decided:** the robust response is to represent absence as a **first-class
    value**, uniformly across every source, rather than error / string-coerce /
    drop-row — each of which loses data or corrupts a column's type.
  - **Decided:** the semantics are **two-valued, not SQL's three-valued
    logic.** An operation against absent is *false* (or a structured error),
    never a third "unknown" truth value that silently propagates through
    comparisons, joins, and rules. 3VL is SQL's most-regretted design and would
    undercut the predictability that is this engine's whole pitch (a logic
    engine an LLM can trust over its own chain-of-thought).
  - This **reopens the ratified "value space has no null"** statements (§4
    value model, §13 typed sources). It is a pillar-level change touching the
    type system, every builtin, unification/joins, set semantics, surface
    syntax, and §9/§11 — hence a dedicated design session, not an inline patch.
    Open sub-questions below.

- **2026-07-23** — **§13 import deep-dive** (design session; implementation is
  roadmap step 7). Decisions ratified:
  - **DuckDB is the single reader backend, on by default.** A **default-on cargo
    feature** (`default = ["duckdb"]`, bundled): imports work in every normal
    build with zero hoops; `--no-default-features` is the opt-out escape hatch
    (fast CI lane, exotic targets) where data imports are a structured error.
    No hand-rolled std CSV/JSONL readers — one reader path. *This amends the
    same-week "feature-gated so the default build stays lightweight" decision
    (below): availability was chosen over a lightweight default after explicit
    cost review.* Accepted costs: ~5–15 min cold C++ compile (cached per target
    dir/profile), a C++ toolchain required to build, a few seconds of extra link
    time per test binary, a binary in the tens of MB, and DuckDB entering the
    supply-chain trust base of a tool that runs LLM-generated programs.
  - **CSV type inference is defined by the language's literal grammar**, not by
    DuckDB's sniffer: a cell is int/float/bool iff the lexer reads it as that
    literal; columns unify; everything else (and any empty cell) is string.
    Considered and rejected: the full sniffer (types we can't represent get
    detected then normalized through casts — e.g. `01/15/2024` → `2024-01-15`;
    rules drift with dependency upgrades) and a restricted sniffer via
    `auto_type_candidates` (empty cells become NULLs and fail sparse imports;
    SQL's text-parsing rules ≠ the language's). Deciding factor: inferred types
    change program meaning, and program meaning stays spec-defined and
    reproducible; reusing the lexer also means no new classifier exists to
    drift. DuckDB remains dialect-parsing authority (`all_varchar`).
  - **Eager materialization; the evaluator never touches DuckDB.** Imports load
    fully into `Program.facts` before lowering, behind the `FactSource` seam;
    DuckDB's footprint is one leaf module. Loading before lowering lets
    header-derived field names reach pass-1 schema collection, dissolving the
    2026-07-20 "schema-less import has no schema" limitation as predicted, and
    typecheck needs no new channel — imported rows are column-uniform facts and
    the existing "facts pin columns" rule types them (resolves §4's source (2)
    note).
  - **Module imports** (`import "lib.dl".`, no `as`): splice-in-place statement
    union, once-only by canonical path (diamonds dedup; cycles terminate), no
    namespacing, queries in imported files are errors, local files only.
    Chosen over a separate `include` keyword (no second concept/reserved word;
    the missing `as` clause already distinguishes the forms grammatically) and
    over a namespaced module system (deferred until a real consumer needs it).
  - **Database grammar ratified, loading deferred**:
    `import "analytics.duckdb" table "orders" as order.` — the table name as a
    *string literal* accommodates arbitrary SQL identifiers, reads
    left-to-right, and composes with the explicit-schema suffix; `table` is a
    contextual keyword (§5). Rejected: URI fragments (`"file.duckdb#orders"` —
    stringly, collides with legal filenames) and reordered `as … from …` forms.
  - **URLs are ungated** (no `--allow-url` flag; user call, 2026-07-23) and ride
    DuckDB httpfs with a lazy runtime `INSTALL httpfs` on first use.
  - **Explicit-schema CSV header rule**: first row skipped iff it exactly equals
    the schema's field names (case-sensitive); otherwise data.
  - **API threading**: `run_at`/`run_with_queries_at` carry the program file's
    path for per-file relative resolution; the pathless forms wrap them
    (stdin/`-q` resolve against the working directory). Cross-file error
    attribution keeps per-file spans plus a statement-origin side table — the
    hook for §12's future file:offset rendering, not built out now.

- **2026-07-23** — **Diagnostic warnings** (`Warning::UndefinedPredicate`, the
  first use of the §12 severity axis; `f7581ba`):
  - A predicate referenced in a rule/query body but never **defined** (no fact,
    rule head, or import) is valid closed-world Datalog (the empty relation) yet
    almost always a typo. It is a **warning, not an error**: printed to **stderr**
    with a nearest-defined-name suggestion (Levenshtein ≤ 2, matching arity
    preferred), **exit 0**, and stdout kept a clean canonical-fact stream so the
    pipe/closure property holds.
  - **Scope includes `-q`:** *any* referenced-but-undefined predicate warns,
    including a bare `-q` query over an empty/partial base — the warning explains
    an empty result (an undefined relation, not a false query) instead of staying
    silent.

- **2026-07-23** — **Source analysis as a driving use case; §13/§9 sequencing**
  (dogfooding session; no engine change — see `docs/worklog.md` of this date):
  - **§13 import is designed against a real consumer, not the abstract CSV.** The
    motivating case is bulk-loading a *machine-generated fact table* — a
    `syn`/`tsc`-class extractor's edges as CSV/JSONL (`import "callgraph.jsonl" as
    calls(caller, callee).`) — alongside the hand-written `parent/child` CSV; that
    fact-table shape drives the schema/type rules.
  - **§13 import embraces dependencies for breadth** (decided 2026-07-23): the
    long-term goal is broad source support — the more formats the better — so the
    import layer is *not* held to the core's zero-dependency pillar. **DuckDB and
    Parquet/Arrow are high-value and worth their deps** (DuckDB especially: one
    dependency yields CSV + Parquet + SQL, resolving the §13 format-priority open
    item). The zero-dep pillar stays for the **core engine**; import backends are
    **feature-gated** (cf. the existing `packaging` feature) so the default build
    and the skill binary stay lightweight. Deep-dive lead: consider building the
    import layer on DuckDB from the outset rather than a throwaway std-only CSV
    reader. Downstream: the "zero runtime dependencies" wording in README/AGENTS.md
    becomes "zero-dependency core, optional import backends" once this lands.
    *Amended by the same-week §13 deep-dive (above): the DuckDB-from-the-outset
    lead was confirmed, but the feature is **default-on** — availability won
    over the lightweight default, and the wording becomes "zero-dependency core
    language engine; imports powered by DuckDB (default feature)".*
  - **§9 aggregation is the paired expressivity pillar** for source analysis
    (count/sum/min/max + grouping); deferred after §13, but confirmed important —
    every ranking/"how-many" in the dogfooding session was hand-done outside the
    engine (`sort | uniq -c`).

- **2026-07-23** — **Step 6: agent CLI** (`-q` one-shot queries;
  `api::program_with_queries` / `run_with_queries`, a hand-rolled arg loop in
  `src/main.rs`, and [`docs/agent-skill.md`](docs/agent-skill.md)):
  - **`-q` semantics.** An argument is classified by **parsing** it, never by
    string-splitting on `:-` (a string literal can contain `:-`): a single clause
    with a non-empty body is a rule (appended verbatim + a synthesized
    `?- <head>.`); anything else (bare atom, comma-body) is a query body appended
    as `?- <arg>.`. Trailing `.` optional; multiple `-q` apply in CLI order (a
    later one may reference an earlier one's predicate); the positional source is
    optional (empty base when only `-q` is given), and a bare `datalog` with
    neither source nor `-q` is a usage error.
  - **CLI-only, zero new dependencies.** The logic stays in the library so the
    binary is thin and unit-testable; no clap.
  - **JSON output deferred as low-value**, not merely unimplemented. The data
    path is Datalog-native — `-q` over facts *is* the jq analog, so JSON there
    works against the design; errors (§12) are already actionable prose with
    spans/hints (better for an LLM than JSON codes); provenance (§11), if
    surfaced, should be provenance-as-facts. `--format json` remains a documented
    future *edge* feature only (hand-rolled if ever — no serde).

- **2026-07-22** — **Phase D: lexer + parser** (`src/lexer.rs`, `src/parser.rs`,
  `src/print.rs`, `src/api.rs`, thin `src/main.rs`). Decisions ratified this
  session:
  - **Hand-rolled lexer + recursive-descent parser, zero new dependencies.** The
    structured-error pillar wants full control over spans and multi-error
    recovery (statement-level, skipping to the next `.`); the grammar is
    LL(1)-ish. `logos`/`chumsky` rejected for v1.
  - **Operator precedence** (closes the §5/§8 open question): `*` `/` above
    `+` `-`, both left-associative (precedence climbing); comparisons
    non-associative, no chaining.
  - **Signed literals** fold in the parser: a prefix `-` on a numeric literal
    becomes a negative constant; no unary-minus node; prefix `-` on anything else
    is an error.
  - **Query syntax stays `?-` only** (reconfirmed): LLM priors on the
    Prolog/Datalog marker, symmetry with `:-`, sigil consistency with
    `?why`/`?whynot`; the bare-atom ergonomic is served by the future `-q` CLI.
  - **Keep the `symbol` type**: well-represented in LLM training data, useful in
    a model's own deductive rules; the symbol-vs-string type error is made
    explicit.
  - **Inline arithmetic in atom arguments** (`succ(N, N+1)`): atom args widen to
    expressions; lowering hoists a compound arg to an `=`-assignment (facts
    constant-fold). IR/engine unchanged — verified by equivalence to hand-hoisted
    IR (`api::tests`).
  - **Disjunction `;` in rule bodies** (top-level DNF, `,` over `;`): the parser
    expands each disjunct into its own clause; AST/IR stay conjunction-only.
    Queries stay conjunctive.
  - **`#` line comments** alias `%`; **no `//`** (reserved). ASCII outside
    strings; Unicode inside string contents.
  - **Strict grammar + did-you-mean errors** for the Prolog-prior near-misses
    (`=<`, `\=`, `\+`, `!`, `//`, uppercase relation, chained comparison, `not`
    before a comparison, zero-arity atom, mixed argument styles, trailing comma,
    curly quotes). One canonical spelling each — hints, not silent aliases.
  - **Canonical output form + query answer shape** (§14): single positive-atom
    queries re-emit the substituted atom, other bodies emit `answer/N`; floats
    always print a decimal point so output re-lexes as input (D1 closure).
  - **Minimal binary contract** pulled forward from step 6 to enable system
    tests: `datalog <file | ->`, answers to stdout / errors to stderr, exit
    codes 0/1/2. The full `-q`/`--format json`/skill CLI remains step 6.

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
  facts). *Semantics settled 2026-07-23 (see the step-6 entry above).*
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
  *Resolved as predicted by the 2026-07-23 §13 deep-dive: imports load before
  lowering, so header-derived field names reach pass-1 schema collection and
  named access on schema-less imports works once step 7 lands.*
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

- **Optional/absent value design** (direction decided 2026-07-23, Decisions
  above — a first-class, two-valued absent value; this is the deferred design
  session). To settle: `optional T` as a distinct column type (inference tracks
  nullability, most columns stay provably total) vs. one `absent` value any
  column may hold; the truth table for `absent`-vs-value comparisons and
  arithmetic (2-valued — false or structured error, never a third truth value);
  whether `absent` unifies/joins with `absent`; its `Eq`/`Ord`/`Hash` sort
  position for set semantics; a printable literal that round-trips (D1 closure,
  Datalog-out is Datalog-in — so the grammar gains a token); and how §9
  aggregation and §11 provenance treat it. Per-source import policy (which
  sources map absence to `absent` vs. still error) folds in here, replacing the
  current three-backend inconsistency. — §4/§13/§9/§11.
- **Database loading** (SQLite/DuckDB files via the reserved `table "…"`
  grammar; Postgres via DuckDB attach) and **TSV**: deferred until a real
  consumer appears; the format table and dispatch errors already name them. — §13.
- **Filter pushdown for large sources:** v1 eagerly materializes every import;
  a program touching ten rows of a 10 GB parquet file pays for all of it. The
  ratified path/`table` syntax leaves room to push selections down into SQL if
  a consumer hits the wall. — §13.
- **Module namespacing:** v1 module imports share one global namespace;
  qualified names/visibility deferred until a real consumer needs them. — §13.
- **Aggregation vs recursion:** how far to go on recursive aggregation semantics. — §9.
- **Aggregate expression syntax:** `count { Var : Goal }` is provisional — and `:`
  now also delimits named arguments, so the form will likely be revisited. — §9.
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
- **`--format json` scope:** the only surviving §14 CLI question — a documented
  future *edge* feature (structured errors §12, provenance §11), deferred as
  low-value 2026-07-23 with the data path staying Datalog-native. (`-q`
  semantics, multiple flags, stdin/`-`, optional source, and output ordering all
  resolved 2026-07-23; the agent skill definition is `docs/agent-skill.md`.) — §14.
- **Dependency choices:** `serde` for the API — decide as §14 stabilizes.
  (Lexer/parser + CLI: resolved 2026-07-22 / 2026-07-23 — hand-rolled, zero new
  deps. Source backends: resolved 2026-07-23 — DuckDB, default-on feature, §13.)
