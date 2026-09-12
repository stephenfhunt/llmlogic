# Datalog Language Specification

`v0.1` — a living document, and both the specification and the design workspace for
the `datalog` engine. It carries the current design **and** an explicit
decisions/open-questions log (§17) so the reasoning behind each choice stays visible.

**§§1–16 state present truth and carry no half-state.** A section describes what the
language is now and ends with ***Not covered***, which is the one home for what it
defers; a reader wanting to know when something arrived, or why, goes to §17. Which
source file validated a section is tracked in
[`notes/spec-traceability.md`](notes/spec-traceability.md) — traceability worth
keeping, at the wrong altitude for a language definition (§17, 2026-08-16).

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
  with small Rust prototypes in the crate and feed findings back. Spec and code
  co-evolve.
- **Versioned.** The spec header carries a version tag so we can track churn.
- **Sequencing.** §1–5 (goals → grammar) first, then §6–11 (semantics through
  provenance), then §12–14 (errors, sources, API), iterating as needed. §15–16
  accrete throughout.

---

## 1. Overview & goals

A Datalog engine purpose-built for LLM/agent use and for conveniently loading fact
tables from external sources. Three pillars drive every design decision:

1. **Provenance / explainability** — the engine can explain *why* a fact was derived.
2. **LLM-friendly syntax + structured errors** — a familiar, unambiguous surface
   syntax models generate reliably, with errors that are structured and actionable.
3. **Agent-native interface** — an agent drives the executable directly
   (skill-based, CLI-first); Datalog is the interchange format in both directions,
   with JSON at the machine-readable edges (errors, provenance). See §14.

### Goals

Beyond the pillars, two goals are stated because a decision has already turned on
each:

- **Answer questions over real external data.** §13 exists so a question can be
  asked of a CSV, JSONL or Parquet file as it actually is, without a preprocessing
  step. This is the goal that distinguishes the engine from a semantic layer over a
  host-supplied fact base, and it was the ground for declining `FactSource`-only
  ingest (§17, 2026-08-16).
- **Evaluator performance is a goal, not a non-goal.** Adopting it as a non-goal was
  considered and **declined**; the decline was then measured — chain closure scales
  ~n^2.4 here against ~n^3.8 in an engine that took the other branch, an exponent
  rather than a constant (§17, 2026-08-16/17, `notes/cross-engine-benchmark.md`).
  What carries no guarantee is *resource* behaviour on any one run (§2, §15).

### Non-goals

These are settled, each by a decision in §17 rather than by omission. Listing them
here is what makes the boundary visible; the entry named holds the argument.

- **String construction** — no `concat`, `substr`, `split` (2026-07-27). Filters
  yes, constructors no: a constructor is unbounded in value *size* and would put a
  generator inside a positive cycle (§8/§10).
- **User-defined scalar functions** — declined as redundant, not as unsafe: in
  Datalog a rule already *is* one (2026-07-25).
- **Implicit numeric coercion** — `int` and `float` never meet implicitly; `as` is
  the in-language escape hatch, so the widening stays visible in the source text
  (2026-08-16). One `number` type was considered and declined.
- **Proof trees as facts** — a proof tree is not a fact; it rides in `%` comments,
  which keeps Datalog-out-is-Datalog-in intact byte-for-byte (2026-08-16). Lineage
  annotations on answer rows are likewise not offered (§11).
- **A resource budget** — rejected twice: no fuel, no cap, no timeout. A slow
  program stays slow and a non-terminating one is *named before it runs* rather
  than killed during it (§10, §2). This is the one non-goal a hosted surface would
  reopen, which is why hosted surfaces are parked (§17, 2026-08-18).

### Target users

- **An agent driving the CLI** — the primary user, and pillar 3's premise. It
  writes a program, runs it, reads the output, and repairs the program from the
  diagnostics, without a human in the loop. Every surface decision is scored
  against this loop: it is why output is canonical Datalog, why stdout stays a
  pure fact stream, and why a diagnostic is data rather than prose.
- **A Rust program embedding the engine** — a real second user with a different
  reach: `pub mod engine` and `pub mod provenance` expose what the CLI does not,
  which is currently the *only* way to get at a derivation (§2, §11).
- **A human reading or writing the source** — third, and deliberately so. The
  surface is conventional Datalog (§2), so a human can read it; but where the two
  conflict, the agent's loop wins.

### Success criteria

Falsifiable, and each names how it is checked. Five are checkable today; the sixth
is the project's own hypothesis, and naming its instrument is what makes it a
criterion rather than an assumption.

| | criterion | instrument | today |
|---|---|---|---|
| **S1** | An agent answering multi-hop, recursive or constraint questions is measurably more accurate **with** the engine than reasoning in prose — at two model strengths, first program recorded before any feedback. | the `experiments/` harness | **measured 2026-08-24 — null**; read the note below before citing it |
| **S2** | The engine's stdout is valid input to the engine, byte-for-byte. | property **D2**, `print::tests::corpus_round_trips` | met |
| **S3** | A rejected program can be repaired from the diagnostic alone, without reading the spec. | §12's fields; the near-miss corpus (§3) | **met 2026-08-25** — every diagnostic carries a category, a position and a code; what stays open is *suggestion* coverage (13 of 77 semantic sites), which sharpens a repair rather than enabling one |
| **S4** | A question over a real external table is answerable end-to-end with no preprocessing step. | §13 + the USDA dogfood | met (dates included, 2026-08-19) |
| **S5** | Every fact in an answer can be explained **through the surface the caller used**. | §11's goal form (§16.6, §16.15), its system tests, **E5** | met (2026-08-21) |
| **S6** | No *exponent* worse than a comparable engine on the shared corpus. | `notes/cross-engine-benchmark.md`, re-measured in `notes/profile-2026-08-20.md` | met |

**S1's first measurement, and what it does not say.** A 112-cell grid on
2026-08-24 (`../experiments/results/run-20260824T104501Z/`) put both arms at
**41/48**, a delta of **+0 points**; striking out one domain whose questions the
run itself proved ambiguous, **38/40 against 38/40**. The negative controls came
back 8/8 in both arms, so the null is not a broken instrument.

It is a null about **supplying** the engine, not about **using** it. The engine
arm reached for the engine in **9 of 56 cells** — opus in 1, haiku in 8 — so the
independent variable was barely manipulated, and every incorrect answer on the
live slate came from a cell that wrote no program. The live slate is also at a
ceiling both arms reach without help. Two consequences: *"does the agent pick the
engine up at all?"* is upstream of anything this criterion asks, and a slate that
separates the arms has to be harder than one opus answers 20/20 in prose. Neither
is a reason to call S1 unmeasured — the criterion asks that the experiment was
run — but citing the null without them overstates it.

### What v1 means

**v1 is reached when S2–S6 hold and S1 has been measured at least once.** S1's
criterion is that the measurement was *made*, not that it came out favourably: this
repo exists to test a hypothesis, so a negative result is a finding and not a
failure to ship — whereas shipping v1 having never run the experiment would leave
§1's own pillars resting on an unmeasured premise.

**All six hold as of 2026-08-25**, so v1's criteria are met. S3 was the last, and
it closed in two steps a day apart: every diagnostic gained a **position**
(2026-08-24) and then a **code** (2026-08-25), so a rejected program says where it
is wrong and what kind of wrong it is without a consumer parsing English. The
reasoning is §17, 2026-08-24 and 2026-08-25.

Meeting the criteria is not the same as **cutting** a release, which is a separate
decision; and none of this reopens what S1 measured, since that criterion asks
only that the experiment was run.

Which open backlog items that ruling makes v1 work, and which it puts after v1, is
recorded per item in [`ROADMAP.md`](ROADMAP.md) with the argument in
[`notes/v1-scope.md`](notes/v1-scope.md).

*Not covered:* what comes **after** v1 — there is no v2 theme, and post-v1 items are
classified as *not blocking v1* rather than scheduled against anything. Nor does
this section rank the v1 items among themselves; sequence is `ROADMAP.md`'s.

## 2. Design principles

Five principles, all ratified — two as stated, three **scoped** to what the
implementation delivers. A principle is ratified against evidence, so where the
surface falls short of the sentence, the sentence says so rather than the gap
living in a footer (§17, 2026-08-18). §1 says what the engine is *for*; these say
how it is built.

- **Prefer familiar, conventional Datalog surface syntax** — *ratified, scoped to
  the core.* LLMs generate standard Prolog-/Datalog-style syntax reliably because
  it is well represented in training data, so facts, rules, `:-`, `?-`, `not` and
  the comparison operators are conventional and stay that way. The principle is
  not *never deviate*; it is that **every deviation carries a §17 entry saying
  what it bought** — the `#` comment alias, named arguments, `op { Expr | Goal }`,
  `is absent`, postfix `as`, `declare`, `import … as`, `?- name:`. Its operational
  half is §3's near-miss recovery: a Prolog prior (`=<`, `\=`, `\+`) is recognized
  and corrected rather than rejected obscurely.
- **Strict, unambiguous grammar** — *ratified.* No syntax whose meaning depends on
  subtle context. The one deliberate exception is §3's contextual keywords —
  `table`, the five type names, the five aggregate names — each legal in exactly
  one position, which keeps the grammar context-free while leaving all of them
  usable as ordinary relation and field names. This principle has been paid for
  rather than asserted: `float(A)` was declined because `ident (` cannot be told
  from an atom without scan-ahead (§8, §17).
- **Every error is structured and actionable** — *ratified, scoped to
  suggestions.* A diagnostic is data with a rendering (§12): code, category,
  message, suggestion and position are separate fields, so a consumer never
  parses English to recover them. Every diagnostic carries a **position**
  (2026-08-24) and a **code** from a pinned vocabulary of 38 (2026-08-25), so
  *where* and *what kind* both reach a consumer as data. **Suggestions are the
  scoped half**: they reach every *stage* but not every diagnostic — 13 of the 77
  semantic sites carry one and none of the 36 source sites do — and §12's own
  hazard note is why that is a coverage question rather than a sweep (a
  suggestion that cannot be acted on costs a round; one that is wrong on correct
  code is worse than none).
- **Explainability and the agent API are first-class** — *ratified, scoped to the
  engine and the goal form.* Provenance is designed in from the start and is not
  an add-on: the fixpoint records **all** derivations of every derived fact,
  deduplicated by rule instance, in a run **provisioned** to record them — which
  the program's own goals decide (§11; §17 2026-08-21). It reaches the surface as
  `?why` / `?whynot` (§5), in a file or a `-q`, and `RunResult` carries the
  explanations beside the answers. What is *not* first-class is the machine-
  readable edge: there is no JSON encoding of a proof or a trace, and §12's
  diagnostics still carry no stable code (§12, ROADMAP).
- **Predictable evaluation** — *ratified 2026-08-18, and scoped.* Termination is
  decided **statically and reported before the run**: a program the engine
  certifies terminates, and one it cannot is named, with the reason, on stderr
  before evaluation begins (§10). What an agent can rely on is knowing which of
  the two it has, not that the second never happens — the shapes in the second
  class are legitimate programs whose termination depends on their data.
  *Resource* behaviour carries no guarantee at all and deliberately none: no
  budget, no cap, and a slow program stays slow (§15, §17 2026-08-16).

*Not covered:* **compatibility across versions.** Nothing here promises that a
program valid under one release stays valid under the next, and at `v0.1` there is
no deprecation path — a deliberate omission while the surface is still moving, not
an oversight (§1's v1 criteria are what would make one affordable). Nor is
performance a principle: §15 disclaims resource guarantees outright, and the
predictability ratified above is about *knowing which class a program is in*.

## 3. Lexical structure

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
  - *Temporal* — an `@` sigil then the value, self-delimiting and whitespace-free:
    a **date** `@2026-08-19`, a **timestamp** `@2026-08-19T10:30:00` (an optional
    fractional part, `@2026-08-19T10:30:00.500`, up to microsecond precision), or
    a **duration** written as
    unit-suffixed components, largest first — `@1d`, `@36h`, `@1d12h`, `@90m`,
    `@500ms`, `@-1d12h`, `@0s`. Duration units are `d h m s ms us`; there is no
    month or year unit, which is what leaves `m` unambiguously *minutes* (§4).
    Dates and timestamps are **fixed width** — a four-digit year, two-digit
    month, day and clock fields — which is what makes the form unambiguous, and
    bounds every temporal value to years 0000–9999. A timestamp carries no zone
    offset: §4's timestamp is civil, so there is nothing for one to mean.
    ISO-8601 durations (`@P1D`, `@PT36H`, `@PT0.5S`) are accepted as an input
    alias and print in the form above. The sigil is required: an unsigilled
    `2026-08-19` is arithmetic over three integers, and stays so.
- **Punctuation / operators** — `:-` (rule), `?-` (query), `.` (statement end),
  `,` (conjunction), `(` `)`, `:` (named argument), `@` (temporal literal, above),
  comparisons `=` `!=` `<` `<=`
  `>` `>=`, arithmetic `+` `-` `*` `/` (§8).
- **Reserved words** — `import`, `as`, `declare`, `not`, `is`, `true`, `false`,
  `absent`. These cannot be used as relation or field names.
- **Contextual keywords are not reserved.** `table` (§13), the eight type names
  (`int`, `float`, `string`, `symbol`, `bool`, `date`, `timestamp`, `duration`),
  and the five aggregate operator
  names (`count`, `sum`, `min`, `max`, `avg`) are lexed as ordinary identifiers
  and recognized only in the one position each is meaningful. So `int(2).` and
  `table(2).` are legal relations.
- **Whitespace** is insignificant except as a token separator.
- **Character set** — identifiers, variables, and operators are ASCII; string
  *contents* may be any Unicode. A non-ASCII character outside a string is a
  lexical error with a span; curly/smart quotes get a dedicated hint.
- **Near-miss recovery** — Prolog-prior spellings are recognized and rejected
  with a targeted, actionable message rather than a bare "unexpected character":
  `=<`→`<=`, `\=`→`!=`, `\+`/`!`→`not`, `//`/`/* */`→comment markers. The lexer
  substitutes the intended token so parsing continues (§17, 2026-07-22).

*Not covered:* string interpolation, raw or multi-line string literals, and
non-ASCII identifiers. A string's *contents* may be any Unicode; its delimiters and
the identifier grammar are ASCII.

## 4. Data model & types

### Values and terms

Primitive value types: **symbol**, **string**, **int**, **float**, **bool**,
**date**, **timestamp**, **duration**. Symbols and strings are distinct types and
never compare equal. Terms are **flat**: a term is a constant or a variable — no
compound/function terms in v1 (deferred; see §17).

A relation has a fixed arity, and every column has exactly one type.

### Temporal values — points and vectors

The three temporal types are ordinary primitives: they have literals (§3), a
natural order, and no special status in matching, negation or aggregation. What
distinguishes them is that their **arithmetic is heterogeneous**, which §8 states
as one rule and this section's domains make precise:

| type | domain | literal | precision |
|---|---|---|---|
| `date` | a civil day, proleptic Gregorian | `@2026-08-19` | one day |
| `timestamp` | a civil date **and** time | `@2026-08-19T10:30:00.500` | one microsecond |
| `duration` | a signed elapsed quantity | `@1d12h`, `@-90m`, `@0s` | one microsecond |

- **A timestamp is civil — there are no time zones in the value model.** It
  denotes a date and a clock reading, not an instant on a global timeline, so no
  zone database is needed and `timestamp - timestamp` is exact subtraction. A
  zoned source column is normalized to UTC at import and its offset dropped
  (§13), which is where that conversion is visible and stated.
- **A duration is exact.** It counts microseconds, so every duration has a fixed
  length. There is deliberately **no month or year unit** (§17): a calendar
  duration has no fixed length, so `@1mo / @1d` has no value and `D + @1mo` needs
  a clamping rule for the 31st. Period arithmetic is `std/time`'s job instead —
  `truncate` groups by month without a month-long duration existing.
- **Range and overflow.** A microsecond count in an `i64` spans roughly ±292,000
  years; a temporal operation that leaves the range is a structured error, never a
  wrap, exactly as integer overflow is (§8).
- Temporal values order naturally (earlier before later, shorter before longer)
  and sit **after `bool`** in the cross-type order §14 prints by.

### Absent — the missing-data value

A distinguished value, **`absent`**, marks missing data (an empty CSV cell, a
JSON/Parquet null; §13). It is a *value, not a ninth type*: the eight static types
above are unchanged, and `absent` may inhabit **any** column regardless of that
column's type (an `int` column may hold `absent`, exactly as a SQL `INT` column
may be `NULL`). Inference treats `absent` as **type-neutral** — it does not
participate in a column's type unification, so a numeric column with some missing
cells still infers `int`/`float` (this is what makes sparse imports usable; §13).

Its behavior is **two-valued, never SQL's three-valued logic** (§17) — there is no
propagating "unknown" truth value:

- **In value space (arithmetic, §8), `absent` annihilates** — any arithmetic with
  an absent operand yields `absent`, flowing through computed columns. This is
  value-propagation, not a third truth value.
- **In truth space (comparisons and atom matching, §8), `absent` is false** — any
  comparison with an absent operand is false (not a type error), and a value never
  unifies with `absent`.

Because a value never equals `absent`, presence is tested only with the operator
**`X is absent` / `X is not absent`** (§8); `X = absent` would itself be false.
The literal `absent` may be *produced* (in a fact or rule head, or as an
arithmetic result) but **not matched** in a body atom — matching relies on
unification, which `absent` fails, so a literal `absent` in a body match position
is a structured error suggesting `is absent`.

Two notions of "same" coexist, exactly as in SQL (`NULL ≠ NULL` under `=`, yet
equal under `DISTINCT`/`GROUP BY`):

- **Semantic**: `absent` matches/equals nothing, including another `absent`. This
  keeps missing foreign keys from joining each other into a cartesian blowup.
- **Structural**: a ground fact `p(absent)` is identical to itself, so a relation
  holds a single copy, and `absent` sorts **first** in the value order (an output
  ordering only, distinct from the `<` operator).

**Negation and joining use different notions**, deliberately (§17, 2026-07-29).
Every site where the choice arises, so that none of them is decided by mechanism
rather than by design:

| site | notion | on `absent` |
|---|---|---|
| join / unification, `=` `!=` and ordered comparison (§8) | semantic | never matches, so missing keys do not join |
| aggregate group key (§9) | semantic | the group is real but empty — `count` of it is `0` |
| anti-join of a negated atom (§7) | **structural** | a stored `p(absent)` refutes `not p(X)` with `X` bound to `absent` |
| set membership/dedup, canonical `Ord` (§14) | structural | one stored copy; sorts first |

The anti-join is the odd one because a negated atom **binds nothing**: refuting
it is a *membership* test — "is this tuple in the relation?" — not a join, and
the blowup the semantic notion prevents is a property of joins bringing in new
bindings. Two consequences follow, both intended:

- **`p(X), not p(X)` derives nothing**, for every `X` including `absent`. Under
  one uniform notion it derived `p(absent)`'s row — P ∧ ¬P.
- **Idempotence of conjunction still fails over `absent`**: `p(X), p(X)` selects
  strictly less than `p(X)`, because the repeated occurrence is a *join* and
  keeps the semantic notion. This is the direct price of `NULL ≠ NULL`, and SQL
  pays it on a self-join for the same reason.

`absent` (the missing-data value) is unrelated to the **no-match pattern** of
negation-as-failure (§7/§11), which is a provenance record for a satisfied
negated goal. Both were called "absence" until 2026-08-16; the rename is why
this paragraph no longer has to warn about a shared word.

### Static typing via inference

The language is statically typed **with full type inference** — annotations are
never required. Before evaluation, the engine infers every column's type from:

1. literals in program facts (`30` int, `3.14` float, `"a"` string, `true` bool,
   bare `red` symbol, `@2026-08-19` date, `@2026-08-19T10:30:00` timestamp,
   `@1d12h` duration),
2. column types of imported sources (§13), and
3. variable flow through rule bodies (a variable unifies the types of every
   position it occupies; builtins constrain their operands, §8).

Inference is a simple unification pass over the primitive types — no polymorphism,
no type constructors, not Hindley–Milner. Any conflict — an int column joined
against a string column, or `age(X, "old")` alongside `age("bob", 30)` — is
reported as a structured **type error before evaluation** (§12), never a silent
empty result or a runtime surprise. This serves the structured-errors pillar and
makes agent-generated programs safer.

### Conversion — the `as` cast

Types are **strict and never implicitly coerced** (§8): `int` and `float` do not
mix in arithmetic or comparison. Conversion is therefore explicit, written
`Expr as type` (§5 grammar; ratified 2026-07-25):

```datalog
% a ratio from two int columns — `1 / 3` is integer division, so cast first
share(F, R) :- count(food: F, n: N), total(t: T), R = (N as float) / (T as float).
```

- **`X as T` has type `T` unconditionally** — the first expression form whose
  result type is independent of its operand's. (§9's reducers are the precedent:
  `avg` is `int → float` for the same reason.) So a cast both satisfies and
  *terminates* inference for its subexpression: nothing about `T` flows back into
  `X`'s column.
- **`absent as T` is `absent`** — annihilation, exactly as in arithmetic (§8). A
  cast never manufactures a value for missing data.
- **Casting is not coercion.** The strictness is deliberate and stands (§17,
  2026-07-25): when imported data carries both an int and a float column there is
  usually a reason — a count versus a measurement, an identifier versus an amount —
  and that distinction belongs to the data model rather than being dissolved
  silently. Implicit widening would also reintroduce in expressions the >2⁵³
  precision loss §13 rejects at the import boundary. The cast makes the widening
  visible in the source text instead.
- **A conversion that cannot succeed splits two ways** (§8 has the table): one
  that would be *lossy* is a structured error, and one with no value to represent
  (`"abc" as int`) is `absent`. The second is **reported** — a value existed and
  was lost, and the value model cannot say so, since the `absent` it produces is
  the same value a missing cell produces. §12 carries that report
  (*malformed, not missing*), counted per conversion site, on the run where the
  conversion failed. A conversion whose result the program **guards** — `V = X as
  int, V is [not] absent`, the idiom §16.9 shows — is silent: the program already
  asks the question the report would answer.

Conversion is also available at the **import boundary** — an explicit schema
(`import "t.csv" as t(a: float)`) coerces cells as they load (§13) — and typed
sources carry their own types. The cast is what was missing *in-language*.

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

*Not covered:* compound and function terms, and collection-valued columns — the
value model is flat, which is what blocks `collect`/`string_agg` in §9. A ROADMAP
item. Nor **calendar durations** (`@1mo`, `@1y`) or time zones, both excluded by
the temporal design rather than deferred by it (§17).

## 5. Syntax

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
statement   = import | declaration | clause | query | goal ;

import        = data_import | module_import ;
module_import = "import" string "." ;
data_import   = "import" string [ "table" string ] "as" ident
                [ "(" field { "," field } ")" ] "." ;
declaration = "declare" ident "(" field { "," field } ")" "." ;
field       = ident [ ":" type ] ;
type        = "int" | "float" | "string" | "symbol" | "bool"
            | "date" | "timestamp" | "duration" ;

clause      = atom [ ":-" body ] "." ;          (* fact when no body, else rule *)
query       = "?-" [ ident ":" ] conjunction "." ;  (* conjunctive; the ident names the answer, §14 *)
goal        = ( "?why" | "?whynot" ) atom "." ;  (* one ground fact, §11 *)
body        = conjunction { ";" conjunction } ;  (* rule bodies: DNF, §17 *)
conjunction = literal { "," literal } ;
literal     = [ "not" ] atom | comparison ;

atom        = ident "(" args ")" ;              (* at least one argument *)
args        = positional | named ;
positional  = expr { "," expr } ;               (* args are expressions, §17 *)
named       = ident ":" expr { "," ident ":" expr } ;

term        = constant | variable ;
constant    = integer | float | string | bool | temporal | "absent" | ident ;  (* bare ident = symbol; `absent` = the missing-data value, §4 *)
temporal    = TEMPORAL ;                        (* "@"-sigilled date/timestamp/duration, §3 *)
variable    = VARIABLE ;                        (* uppercase- or "_"-initial, §3 *)

comparison  = expr cmp expr
            | expr "is" [ "not" ] "absent" ;    (* presence test, §4/§8 *)
cmp         = "=" | "!=" | "<" | "<=" | ">" | ">=" ;
expr        = add ;
add         = mul { ( "+" | "-" ) mul } ;       (* left-assoc *)
mul         = cast { ( "*" | "/" ) cast } ;     (* binds tighter, left-assoc *)
cast        = primary { "as" type } ;           (* postfix conversion, §8; binds tightest *)
primary     = [ "-" ] number | aggregate | term | "(" expr ")" ;  (* prefix "-" folds onto a literal *)

aggregate   = agg_op "{" expr "|" conjunction "}" ;  (* set-builder, §9 *)
agg_op      = "count" | "sum" | "min" | "max" | "avg" ;  (* contextual: ident before "{" *)
```

Notes:
- **Operator precedence** (was open): `*` `/` bind tighter than `+` `-`, both
  left-associative (precedence climbing); comparisons are non-associative and do
  **not** chain — `0 <= X <= 9` is a targeted error suggesting `0 <= X, X <= 9`
  (§17, 2026-07-22).
- **Parentheses group an expression** and are unambiguous: an atom must start
  with an identifier, so a body literal opening with `(` can only be a
  comparison. Canonical printing (§14) emits them only where dropping them would
  re-parse to a different tree — a child binding looser than its parent, or an
  equal-precedence child on the right (`A - (B - C)` keeps them, `(A - B) - C`
  does not). Prefix `-` is unaffected: it still folds onto a numeric *literal*
  only, so `-(A + B)` remains a parse error.
- **Atom arguments are full expressions**, so inline arithmetic parses
  (`succ(N, N+1)`); lowering hoists a compound argument to an `=`-assignment, so
  the IR is unchanged (§17, 2026-07-22). A compound argument is instead
  **constant-folded** where a hoist would have no body to live in or would change
  the meaning of the surrounding form: in a *fact* (`p(1+1).` → `p(2).`), and —
  when it is **ground** — in a *query*, whose answer shape §14 reads off the body
  (§17, 2026-07-27).
- **An explanation goal** `?why <fact>.` / `?whynot <fact>.` (§11) is a statement,
  not a query: its argument is one **atom**, not a conjunction, because a goal
  names one fact. A goal that is not ground is a semantic error naming the
  variable and pointing at `?-` — enumerate with a query, then ask about one of
  the rows it returned. The two sigils are single tokens (`? why` is not one),
  and neither is a selector over the answer: which of `proof` / `underivable` /
  `unknown` comes back is a fact about the model, while the sigil decides what
  the *run* records (§11, §17 2026-08-21).
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
- **Aggregate expressions** `op { Expr | Goal }` (§9) parse as a `primary`, so
  they compose inside arithmetic and either side of a comparison. The separator is
  `|` (set-builder), chosen over `:` because `:` now delimits named arguments. The
  five operator names (`count`/`sum`/`min`/`max`/`avg`) are **contextual** —
  recognised only as an identifier immediately followed by `{` in expression
  position — so a relation or field may still be named `count` (§9, §17
  2026-07-24).
- **The `as` cast** `Expr as type` (§4/§8, ratified 2026-07-25) is the conversion
  form — `V = (A as float) / (B as float)`. It is postfix, binds tighter than
  `*` `/`, and chains left-to-right (`X as int as float`). The keyword is the
  *same reserved word* as the import clause's `as`, disambiguated by position: an
  import's `as` follows a path string at statement level, a cast's follows an
  expression operand. No lookahead is needed either way, because `as` can never
  begin a statement or a body literal. The right-hand side is the existing `type`
  production, so no new vocabulary is introduced. Chosen over `float(A)` calls and
  over juxtaposition (§17, 2026-07-25).
- **A temporal literal needs no lookahead**: `@` begins nothing else in the
  grammar, so the lexer commits on the sigil and the whole literal is one token
  (§3). A malformed component is a *lexical* error naming it — `@2026-02-30` says
  the day, not "unexpected character" — which is why the value is lexed rather
  than parsed from parts. The three type names are the existing `type` production
  (§4/§8), so the cast form introduces no new vocabulary.
- **`absent` is a reserved value literal** (§4), joining `true`/`false` as a
  keyword that is not an identifier — a relation, field, or symbol may not be
  named `absent`. It may appear wherever a constant may (facts, heads, arithmetic
  operands), but matching it in a body atom argument is a structured error (it
  cannot unify) suggesting `X is absent`.
- **`is` / `is not absent`** is the presence test (§4/§8) and parses as a
  comparison: the `not` here is part of the operator, unrelated to atom-negation
  (§7). `X = absent` is *not* the test — it is false for absent, as every
  comparison with an absent operand is.
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

*Not covered:* **one body grammar.** There are three — rule bodies are DNF, while
queries and aggregate goals are conjunction-only — so a disjunctive filter can only
be expressed by detouring through the rule form. Whether `;` extends to the other
two, or the asymmetry is deliberate and gets stated as such here, is a ROADMAP item.
Also not covered: **0-arity atoms**, which are banned, which is what removes the
workaround for a yes/no question (§14).

## 6. Declarative semantics

A program's meaning is its **least model**, and with negation and aggregation its
**perfect model** (references.md groups 1 and 3). The formal operator, the match
relation and the aggregate fold are in
[`notes/declarative-semantics.md`](notes/declarative-semantics.md).

**The Herbrand universe** is the constants appearing in the program (facts, rule
constants, imported values) together with the values §8 computes from them; the
**Herbrand base** is the ground atoms formable from the program's predicates over
it. `absent` (§4) is an ordinary element of both — a program produces it in a
fact, a rule head, an arithmetic result or an import (§13) — but it is not
reachable by *matching*, which is the next rule.

**The immediate-consequence operator `T_P`** maps a fact set `I` to the program's
facts plus `θ(head)` for every rule and every substitution `θ` satisfying its body
in `I`; range restriction (§10) is what makes that head ground. Satisfaction is
per literal:

- **A positive atom** matches a stored tuple argument-wise, and **binding is total
  while matching is semantic**: an unbound variable takes whatever the tuple holds,
  `absent` included, while a constant or an already-bound variable matches only an
  equal value that is not `absent`. §4's table is the normative statement of which
  notion applies where; two things it records follow from this split rather than
  being separate rules — `p(X), p(X)` selects strictly *less* than `p(X)`, and
  `p(X), not p(X)` derives nothing.
- **A negated atom** is a **structural** membership test against a completed
  relation, and binds nothing (§7).
- **A comparison or arithmetic literal** is an **interpreted predicate**: its
  extension is fixed by §8 rather than derived, and it is infinite. `absent`'s
  two-valued treatment (§4) — annihilation in value space, false in truth space —
  is a property of that fixed extension, not a third truth value; the operator
  stays two-valued.
- **An aggregate literal** binds its result to the fold of a multiset over the
  **witness set** of its goal, evaluated against a strictly lower stratum (§9).

**Stratification is what keeps the operator monotone.** Negated and aggregated
dependencies point strictly down (§7, §9), so each stratum runs to its least
fixpoint with the lower ones frozen, and the result is the perfect model; by the
independence theorem every valid stratification yields the same one (§7). For an
aggregate this is not bookkeeping but the whole reason it has a declarative
reading: its goal's relations are complete before its stratum begins, so the
aggregate is a **fixed function from group keys to values** for the duration of
that fixpoint, rather than a non-monotone construct sitting inside the loop.

**Two finiteness claims, and they are different** — conflating them is the error
`bugs/004` found here:

- **Every application is finite.** For finite `I` a rule has finitely many
  satisfying substitutions, because §10's range restriction grounds every builtin
  operand from a positive atom or an `=`-chain rooted in one. So `T_P(I)` is
  finite, for *every* program — this says nothing about how many times it is
  applied, and nothing about whether it succeeds (see the error rule below).
- **The fixpoint is reached in finitely many rounds exactly when the program is
  *certified terminating* (§10)**, because certification bounds the *universe*,
  which no single application does. `N = M + 1` synthesises a value appearing
  nowhere in the program; §10's rule is that no such value reaches the head of a
  positively recursive predicate, and under it the classical results return —
  **PTIME data complexity** included, with combined complexity higher as usual.
  The proof is in [`notes/termination.md`](notes/termination.md).

**Outside that fragment the least model may be infinite**, and the fixpoint is a
limit that evaluation approaches without reaching. Such a program is **not
rejected**: it is valid, it is often correct on the data it will actually see
(`path_cost` over an acyclic graph), and §10 names it before the run instead.

**A run that raises an error has no model** (§17, 2026-08-18). §8's builtins can
fail — integer overflow, division by zero, a NaN-producing float operation, a
lossy `as` conversion — so `T_P` is partial, and a failure yields no model at all
rather than a smaller one: the error is neither a truth value nor a missing fact.
This is the limiting case of the truncation contract (§15), where an incomplete
model holds missing facts and never false ones; an erroring run holds nothing to
project. *Whether* a run errors is a property of the program and its data, since a
complete application enumerates every rule instance; *which* error it reports is a
property of the schedule, and only the first is guaranteed (`testing.md` B1).

**Set semantics** throughout (§17): the model is a set of facts; a fact derivable
several ways is one fact with several derivations (§11). **Queries** are answered
against the finished model, as projections (§14).

*Not covered:* well-founded and stable-model semantics (references.md group 3) —
§7 states why stratified is the choice for agent-generated programs. A
**recursive or monotonic** aggregate has no account here and could not have one
under this operator, since what it gives up is exactly the strictly-lower-stratum
premise above (references.md group 4); so would **limit predicates**, whose
fixpoint accumulates a per-group extremum rather than a set (§17 open questions).

## 7. Negation

Negation is **stratified negation-as-failure**. The semantics of record is the
**perfect model** of Apt/Blair/Walker (references.md group 3); the
definitional treatment of stratification, safety, and stratified evaluation is
Abiteboul/Hull/Vianu ch. 15 and Ullman's *Principles* (groups 1–2).

**Reading.** `not atom(…)` may appear as a body literal (`not` applies to
atoms only, §5). The literal holds when *no* fact of the negated predicate
matches the atom, where constants and **bound** variables match positionally
and wildcard slots are **existential under the negation** (§17 2026-07-19):
`not parent(_, X)` holds when no `parent` fact has `X` in its second column.
Negated literals bind nothing.

Because they bind nothing, this is a **membership** test and matching is
**structural** — §4's notion for set membership, not the semantic one joins use.
`absent` is a member like any other, so `not p(X)` with `X` bound to `absent` is
refuted by a stored `p(absent)`. §4 owns the rule and states what it costs; the
consequence here is that a row whose key is missing drops out of a "things with
no …" query rather than surviving it.

**Safety.** Every *named* variable in a negated atom must be **bound by the same
clause**, in the sense §10 defines. Wildcard-fresh variables under negation are
scoped to the negated literal and never exported (§10, §17 2026-07-19).

An argument may therefore be **computed**, and the spelling does not matter:

```datalog
r(X) :- p(X), not q(X + 1).         % lowering hoists the argument
r(X) :- p(X), Y = X + 1, not q(Y).
r(X) :- p(X), not q(Y), Y = X + 1.  % all three are the same conjunction
```

The anti-join is scheduled after whatever binds its arguments (§8, §15), which
is what keeps it a test against a *ground* atom. Negations the positive atoms
already ground stay ahead of the builtins, so they still prune early.

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
**no-match pattern** — the atom instantiated with the rule's bindings,
wildcard slots left open: `root("alice")` holds *because no `parent(_,
"alice")` fact exists*. A closed slot reads under the same structural rule as
the anti-join above, so the pattern is refuted by exactly what refutes the
literal. This is a proof-tree-level why-not record, chosen
deliberately over extending §11's semiring story: provenance semirings cover
positive programs only, and the principled negation extensions
(dual-indeterminate and absorptive polynomials, references.md group 5) are
not adopted in v1.

*Not covered:* well-founded and stable-model semantics (references.md group 3).
Stratified programs are the predictable subset for agent-generated code; an
unstratifiable program is a structured error, never a different semantics.

## 8. Arithmetic & comparison builtins

**Operators.** Comparison `= != < <= > >=`; arithmetic `+ - * /`; the postfix
conversion `as type` (below). Both appear
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

**Strict types, no coercion.** `int` and `float` are distinct. Numeric
arithmetic requires both operands the *same numeric* type — `int op int → int`,
`float op float → float`; any mixed operand is a **type error**. **Temporal
arithmetic is the one heterogeneous case**, stated below; every other operand
type — symbol, string, bool — is a type error in arithmetic. A comparison
requires both operands the *same* type (any type); a cross-type comparison is a
type error, never a silent `false`, and its suggested fix names the cast that
would make the two agree (`D as timestamp`). Ordered comparisons use the operand
type's natural order (ints/floats numerically, strings/symbols lexicographically,
`false < true`, temporal values earliest/shortest first). Post-§4, these
conflicts are caught before evaluation; pre-typecheck they are structured runtime
errors — same error channel either way.

**Temporal arithmetic — one rule** (§4 for the domains). This is the only place
in the language where an operator's two operands have different types, so it is
stated as a rule and not only as a table:

> A `date` and a `timestamp` are **points**; a `duration` is a **vector**. Point
> minus point is a vector, point plus-or-minus a vector is a point, and adding
> two points is meaningless. Vectors add to each other, scale by numbers, and
> **cancel against each other** — which is the only way a duration becomes a
> number.

| expression | result |
|---|---|
| `date - date`, `timestamp - timestamp` | `duration` |
| `date ± duration`, `timestamp ± duration` | the same point type |
| `date + date`, `timestamp + timestamp`, `duration - date` | **type error** |
| `duration ± duration` | `duration` |
| `duration * N`, `N * duration`, `duration / N` (`N` an `int` or a `float`) | `duration` |
| `duration / duration` | `float` |
| `duration` with a number under `+` `-`, or `date`/`timestamp` with anything under `*` `/` | **type error** |

Mixed point types (`date - timestamp`) are a type error like any other cross-type
comparison; `D as timestamp` is the fix, and the error says so.

**`duration / duration` is where a program names its unit**, and it is `float`
rather than `int` deliberately: `@36h / @1d` is `1.5`, and an integer result
would truncate exactly the way the unitless conversion this rule replaces did
(§17). Scaling takes an `int` *or* a `float` — a scalar is a scalar, and
excluding one would be an exception to the rule rather than part of it — and
truncates toward zero at the microsecond, which is §8's existing
integer-division behaviour applied to the count. There is **no
conversion between `duration` and a number** in either direction (the `as` table
below) — a bare number of *what* is the whole hazard, and division by a duration
literal is the construct that answers it:

```datalog
days_open(T, N) :- ticket(id: T, opened: O, closed: C), N = (C - O) / @1d.
```

**Arithmetic edge cases** are structured errors, never a wrap or panic: integer
division truncates toward zero, division by zero and integer overflow error, and
a NaN-producing float operation (`0.0 / 0.0`) errors via `F64::new`.

**Absent (§4)** is exempt from the strict-type rules above, two-valuedly. An
**arithmetic** operand that is `absent` yields `absent` (annihilation) rather than
a type error, and this **takes precedence over the edge cases above** — `5 /
absent` and `absent / 0` are both `absent`, never a division-by-zero. A
**comparison** with an `absent` operand is **false** — not a cross-type error, and
this is the one intended exception to "never a silent `false`": it is the
two-valued semantics, not a masked type mismatch. Consequently `X = 5` and `X !=
5` are *both* false when `X` is absent, which is why presence has its own operator:

**`is` / `is not absent`.** `X is absent` is true iff `X` is `absent`; `X is not
absent` is its negation. Unlike `=`/`!=` (both false for absent) these **partition
every row**, and unlike `=` the right-hand `absent` is exempt from the same-type
requirement. It is a comparison/filter — the `not` is part of the operator, not
§7 atom-negation — so its operand must be bound by the body like any comparison
(§10) and it adds no stratum. (A general `X is Y` null-safe equality is a possible
extension, §17.)

**The literal `absent` may not be a comparison operand** (2026-07-25). Since
*every* comparison with an absent operand is false, writing the literal —
`V = absent`, `V != absent`, `V < absent` — is an always-false filter, and
`V != absent` in particular reads as "where the value exists" while selecting
nothing. So a bare `absent` literal on either side of a comparison is a
**structured error steering to `is [not] absent`**, exactly as a literal `absent`
in a body atom argument is (§4). This costs no expressiveness: the comparison it
forbids could only ever be false. Two things stay legal — the **producer form**
`X = absent` where `X` is unbound (an assignment; §4's way to produce the value),
and `absent` reached through *arithmetic*, which is annihilation, not comparison.
The runtime rule is unchanged: comparing a *variable* that happens to hold
`absent` against a value is still silently false.

**The `as` cast — explicit conversion** (ratified 2026-07-25; §4 for the type
rules, §5 for the grammar). `Expr as type` converts a value between the
primitive types. It is the *only* way `int` and `float` meet, since the strict
no-coercion rule above stands:

```datalog
r(V) :- p(A, B), V = (A as float) / (B as float).   % a real ratio, not A / B
```

- Postfix, binding tighter than `*` `/`, chaining left-to-right.
- **Result type is the named type, unconditionally** — inference does not flow `T`
  back into the operand (§4).
- **`absent as T` is `absent`** — annihilation, at the same choke point as
  arithmetic's, so it takes precedence over any conversion check.
- **Governance.** A cast creates no new *reachable* values in the sense that
  matters for termination: it maps a finite value set to a finite value set with no
  accumulation, so casts are exempt from the value-creating-recursion restriction
  (`bugs/004`, `ROADMAP.md`). This was checked before adopting the form.

**Which conversions exist.** `T as T` is the identity for all five, and text is
the universal intermediary — every value renders to `string`, and `string`
converts to anything its *literal grammar* reads:

| from ↓ / to → | `int` | `float` | `string` | `symbol` | `bool` |
|---|---|---|---|---|---|
| `int` | identity | exact or error | render | — | — |
| `float` | exact or error | identity | render | — | — |
| `string` | read or `absent` | read or `absent` | identity | read or `absent` | read or `absent` |
| `symbol` | — | — | render | identity | — |
| `bool` | — | — | render | — | identity |

A `—` is a pair with **no conversion at all**, and writing one is a structured
error naming both types — a program mistake, like arithmetic over two
non-numbers. `bool as int` is the shape this excludes; nothing in the value model
(§4) says which integer a boolean is.

**Temporal conversions** are text, and one widening. Every pair not listed here —
in particular **`duration` against `int` or `float`, in both directions** — is a
`—`, and its error names the divisor form instead (`(C - O) / @1d`), because a
duration rendered as a bare number is a number of nothing:

| conversion | result |
|---|---|
| `date`/`timestamp`/`duration` `as string` | render — §14's canonical spelling |
| `string as date`/`timestamp`/`duration` | read (§3's literal grammar, **without** the `@`) or `absent` |
| `date as timestamp` | exact — midnight of that day |
| `timestamp as date` | **lossy: a structured error** |
| any temporal against `int`, `float`, `bool`, `symbol` | `—` |

`timestamp as date` is the one conversion whose exclusion costs something real:
truncating a timestamp to its day is what a group-by-day needs. It stays an error
because `as` neither rounds nor truncates in any other case either, and the
suggested fix names the construct that does — `truncate(T, day, D)`, from
`import "std/time".` (§13). Reading a temporal from text takes the *unsigilled*
form, so `"2026-08-19" as date` works and matches what a CSV cell holds (§13);
rendering is its inverse and drops the `@` for the same reason.

"Render" is §14's canonical spelling and "read" is §3's literal grammar — the
same one §13 types an untyped CSV cell with. The two directions are therefore
inverse: `V as string as T` recovers `V`, and `"30" as int` is `30` exactly when
writing `30` would be that literal. For a temporal value, render is that
spelling **minus the `@`**: the sigil is a delimiter the printer supplies, as a
string's quotes are, and dropping it is what makes the inverse hold against the
text a CSV cell actually holds.

**Two failure modes, and the line between them** (2026-08-16). Both are reachable
only in the conversions the table defines:

- **Lossy — a structured error.** A value exists and no exact representation of
  it does, so converting would corrupt it: `X as float` on an `i64` above 2⁵³,
  and `2.5 as int`. `as` neither rounds nor truncates, in either direction. This
  mirrors §13's import rule (2026-07-25): a silently rounded identifier breaks
  every join built on it, and large integers in real data are usually
  identifiers.
- **Unrepresentable — `absent`.** There is no value to represent: `"abc" as int`.
  This is the *data* being dirty rather than the program being wrong, and the
  language offers no way to ask "does this parse?" — string operations are
  rejected (§17, 2026-07-27) — so an error would leave such a column with no
  writable query at all. Yielding `absent` keeps the guard in the program's own
  vocabulary:

  ```datalog
  clean(X, V) :- raw(X), V = X as int, V is not absent.
  ```

  The cost is real and accepted: this reclassifies *malformed* as *missing*, and
  the two are not the same thing. It is bounded by where untyped data enters —
  Parquet, JSONL and databases carry their own types, so only CSV arrives
  unclassified (§13).

**Mode / safety (§10).** Every comparison operand variable, and every named
variable of a negated atom, must be bound by the body in the sense §10 defines —
the one exception being an `=`-assignment's own target, which the assignment
itself binds. The scheduler places each literal after whatever binds its inputs,
so an anti-join runs after the assignment or aggregate that grounds it.

**Evaluation order is by dependency, not by source order** (2026-07-25). A body
is a conjunction, so where a binder is *written* does not decide what the clause
means: the engine schedules positives first, then the negations those already
ground, then the builtins **and any remaining negations** in an order where every
literal's inputs are already bound (`src/schedule.rs`). `M = N+1, N = A+1` is the
same clause as `N = A+1, M = N+1` — previously the first was rejected. Two
guarantees make this a widening rather than a change of meaning:

- among the literals that are ready, the **earliest in source order runs first**,
  so a body whose source order already worked keeps exactly that order, and `=`
  resolves assignment-vs-filter as it always did;
- a body is rejected only when *no* order works — a variable nothing binds, or a
  circular dependency (`M = N+1, N = M+1`). The two get different messages,
  because only the first can be fixed by adding a binder.

*Not covered:* **String operations** — no
prefix, split or concat, *rejected* rather than deferred (§17, 2026-07-27): string
construction fails §10's termination test. **Builtin scalar functions** (`abs`,
`length`, `lower`, `substr`) are still deferred until a consumer needs them, but
they are no longer *shapeless*: a builtin is a **relation from a `std` module**
(§13), which is how `std/time` spells extraction, so the `ident (` ambiguity that
ruled out `float(A)` never arises. User-defined ones stay declined — a rule
already is one. **Implicit int/float widening** does not happen; the import
boundary is the only coercion site. **Calendar arithmetic** — `D + @1mo`, "age in
years" as a subtraction — is excluded with the calendar duration (§4); `std/time`
is where a program reaches a month or a year.

## 9. Aggregation

**Surface syntax — set-builder pipe.** An aggregate is an **expression** of the
form `op { Expr | Goal }`, read as set-builder notation ("the `op` of `Expr`
such that `Goal`"). `Goal` is a conjunction (a rule body without disjunction).
The separator is `|`, not `:`, because `:` now delimits named arguments (§5) and
would collide inside `Goal`; `|` is otherwise unused (disjunction is `;`).

```datalog
child_count(P, N) :- parent(P, _), N = count { C | parent(P, C) }.
% expected: child_count("alice", 2), child_count("bob", 1)
```

Being an expression, an aggregate composes under §8: it may sit on either side of
a comparison and inside arithmetic (`N = count { C | parent(P, C) } + 1`).
Lowering hoists it to an `=`-assignment binding a fresh result variable, exactly
as inline arithmetic is hoisted (§8) — so the surface is sugar and the core IR
carries a single aggregate body literal. The five operators (`count`, `sum`,
`min`, `max`, `avg`) are **contextual**: recognised only as an identifier
immediately followed by `{` in expression position, so a relation or field may
still be named `count`.

**Grouping is implicit.** The aggregate is evaluated once per distinct binding of
the enclosing rule's variables that occur **outside** it — in the example, `P`,
bound by `parent(P, _)`. Variables occurring only inside `Goal` (there, `C`) are
local to it. There is no separate `group by`: the rule's other body literals
supply the group keys, and a bare `Avg = avg { A | m(_, A) }` (no outer
variables) is a single global group.

A **group key must be bound by the enclosing body**, in the sense §10 defines
(another aggregate's result counts). *Where* that binder is written is
irrelevant: the aggregate declares its group keys as inputs and is scheduled
after whatever binds them (§8, 2026-07-25), so

```datalog
g(X, N) :- q(X), Y = X + 1, N = count { C | r(Y, C) }.
g(X, N) :- q(X), N = count { C | r(Y, C) }, Y = X + 1.
```

are the same rule. (Before scheduling, the second silently enumerated `Y` as a
goal-local existential and aggregated over everything — the same conjunction
meaning two different things.) A group key nothing binds is a structured error;
so is a **circular** one, where the key is only bound by something that needs the
aggregate's own result — the case no reordering can fix. Two aggregates in one
body may reuse a goal-local name freely: each `Goal` is its own scope, so neither
name is a group key or a scheduling dependency of the other.

**The goal is a body and binds like one.** `Goal` is an ordinary conjunction, so
a variable it binds by an `=`-assignment or by a *nested* aggregate is available
to the collected expression exactly as one bound by a positive atom is:
`max { T | s(K, V), T = V * 2 }` and `max { T | s(K, _), T = sum { V | s(K, V) } }`
are both well-formed (2026-07-25). Nested aggregates hoist into the enclosing
goal, where the goal's own bindings are in scope.

**In a query**, an aggregate answers over the variables the **query body** binds
— its goal-local variables are named but exist only inside the sub-join, so
`?- N = count { C | m(T, C) }.` answers over `N` alone (one global group), while
`?- thing(T), N = count { C | m(T, C) }.` answers over `T` and `N` (§14).

**Witnesses and duplicates.** For fixed group-key bindings, the aggregate folds
the multiset `{ eval(Expr, w) : w ∈ W }`, where `W` is the set of **distinct
satisfying assignments to all of `Goal`'s variables** (set semantics dedups
facts, so `W` is a set of witness tuples). Two witnesses that agree on `Expr` but
differ elsewhere are *two* multiset elements — so `sum { S | emp(N, S) }` counts
two equal salaries twice (the "duplicates and aggregates" question, §17;
`references.md`). Deduping the projected values instead is *not* what these
aggregates do.

**The fold is over that multiset, in §14 order.** How the witnesses were
enumerated never reaches the answer: the collected values are sorted before they
are folded, so an aggregate is a function of its multiset alone. A sorted fold
still has to pick *an* association, and §14's order is by value rather than by
magnitude, so two rules keep the canonical answer the right one:

- `sum` over `float` is **compensated**, carrying the low-order bits a naive fold
  drops. The multiset `{ 1e16, -1e16, 0.1 }` sums to `0.1`.
- `sum` over `int` and `duration` **accumulates wide**: overflow is a property of
  the total, never of an intermediate. A multiset whose sum is representable has
  that sum, even where some association of it would overflow. §8's binary `+` is
  unchanged — there the operand pair is what the program wrote.

A **wildcard inside a goal is a witness dimension**, not an existential: `count
{ P | parent(P, _) }` counts *edges*, not distinct parents, because each `_` is a
fresh goal variable and distinct fillings are distinct witnesses. This matches
SQL's `COUNT(col)` and follows from the witness-set rule above, but it is the one
place `_` does not mean "don't care" (contrast §7/§10, where a wildcard under
negation *is* existential) — so it is worth stating. There is no count-distinct
in v1; project into a helper relation first if you need one.

**The five reducers and their result types** (inferred, §4):

| op | over | result type | absent |
|----|------|-------------|--------|
| `count` | any type | `int` | counts absent bindings too |
| `sum` | `int`, `float`, or `duration` | same type as `Expr` | skips |
| `avg` | `int`, `float`, or `duration` | `float` for a numeric `Expr`, `duration` for a `duration` | skips |
| `min` / `max` | any single ordered type | same type as `Expr` | skips |

`sum`/`avg` require a numeric or `duration` `Expr`; `min`/`max` accept any single
type under its natural order (numeric, string/symbol lexicographic, `false <
true`, temporal earliest/shortest first); `count` accepts anything. A cross-type
or non-numeric misuse is a §4 type error before evaluation, never a silent result.

**Durations reduce, points do not.** `sum` and `avg` fold with `+` and then
divide, and §8's algebra says a vector may do both while a point may do neither —
so `sum { D | order(date: D) }` over a `date` column is a type error, and the
message says adding two dates has no meaning rather than "not numeric". `avg`
over durations is a `duration` and not a `float`, because `duration / int` is a
duration: the general "`avg` is `int → float`" rule is about *numbers* losing
their exactness, and the vector rule already answers the question for a duration.
It truncates toward zero at the microsecond, as that division does.

### Absent inputs — skip but report

Aggregates treat the absent value (§4) uniformly:

- **`sum` / `avg` / `min` / `max` skip `absent` inputs** and aggregate the present
  values — so `avg` is the mean of the values that exist, never poisoned by
  annihilation (§8) and never divided by the missing. (`min`/`max` must skip
  regardless: `absent` has no order against values.) The engine **reports the
  count of skipped absents** through **provenance** (§11): the skip is recorded in
  the derivation, keeping the aggregate a pure single value so it still composes
  under §8. Two surfaces read that record:
  - a **warning per aggregate site** on stderr whenever a site skipped anything
    ("`avg` in `mean/2` skipped 1 absent input(s) across 1 group(s)"), summed over
    every group. This is what makes the skip non-silent today — an `avg` over a
    half-empty column is otherwise indistinguishable from one over a full column
    (added 2026-07-25; the warning channel is §12's severity axis). An aggregate
    written in a **query** reports the same way, under the query's 1-based
    position rather than a rule name — a query records no derivations, so its
    counts are read from the premises as its rows are produced;
  - a proof, which prints the fold beside the literal that produced it —
    `Avg = avg { A | … }  (12.5 over 8 values, 2 absent skipped)`, the clause
    dropped when nothing was skipped (§11's rendering). `count` reports its value
    alone: its total already includes absent bindings, so a skip count would
    describe a fold it did not do.

  A user who wants the skip count *as data* writes it directly:
  `S = count { A | Goal, A is absent }` (§17, 2026-07-24 — provenance-only chosen
  over a two-place result that could not nest in arithmetic).
- **`count { X | Goal }` counts bindings**, absent ones included (a binding is a
  binding); the count of *present* values is written explicitly as
  `count { X | Goal, X is not absent }`. (SQL-parity `COUNT(col)` skipping is the
  considered alternative, §17.)
- **Over an empty present-set** — an empty group, or one whose values are all
  absent — `count` is `0` and `sum`/`avg`/`min`/`max` are **`absent`** (there is no
  value to report; chosen over `sum = 0`, which would mask "no data" as "zero
  total", and it sidesteps an `avg` division-by-zero). That `absent` then flows on
  under the §8 rules.

Settling these two-valued rules is *why* the absent value is designed before
aggregation (§17): they pin what every aggregate means on sparse data.

### Recursion & stratification

An aggregate reads a *complete* relation, so — like negation (§7) — the
predicates in its `Goal` must be **fully evaluated before** the aggregate runs:
lowering places them in a strictly lower stratum, and recursion through an
aggregate is rejected by stratification with a structured error. Safety (§10)
mirrors negation: a group-key variable used in `Goal` must be bound by the
enclosing body (§10) — in any position, since the aggregate is scheduled after
its binder (§8); `Goal`-local variables are existential (like wildcard variables
under negation). Recursive/monotonic aggregation (the Zaniolo et al.
fixpoint semantics, `references.md`) is a deliberate future extension.

*Not covered:* statistical reducers (`median`, `stddev`, `variance`, `percentile`
— the aggregate node reserves a parameter slot for the last); collection-valued
reducers (`collect`/`string_agg`, blocked on a first-class collection value, §4);
recursive aggregation. **And distinctness has no spelling**: a wildcard inside a
goal is a witness dimension, so `count { S | e(_, S) }` returns the edge count and
not the distinct-`S` count — over an imported table the wildcard is not even
written, since named-argument syntax leaves every unmentioned column implicitly
wildcarded. Measured 2026-07-27 on a 7-column table: 36 call sites where the
question wanted 20 callers. Whether v1 gains `count distinct` is a ROADMAP item.

## 10. Recursion & safety

**Range restriction** (enforced by front-end lowering, `src/lower.rs`). **This
section is the single normative statement of what "bound" means**; §7, §8, §9 and
§14 apply it to their own constructs and refer here rather than restating it.

Every variable in a rule head, every *named* variable in a negated atom, and every
variable occurring only in comparisons must be **bound by the body** — it must
occur in a positive body atom, or be bound by an `=`-assignment or an aggregate
result (§17 2026-07-25); facts must be ground. Wildcard-fresh variables in
negated atoms are exempt — they are existential under the negation and never
exported (§7). Violations are structured semantic errors reported before
evaluation.

Safety is stated against the **schedule** (§8, `src/schedule.rs`), not source
order: a body is safe when there *exists* an order in which every literal's
inputs are bound before it runs. Binding a negated atom's argument by
assignment satisfies the requirement the rule stands for — the atom is ground
when tested — just as a positive atom does.

Recursion through negation is rejected by stratification (§7). Recursion through
an **aggregate** (§9) is likewise rejected: the predicates in an aggregate's
`Goal` are stratified strictly below the enclosing rule. Aggregate safety mirrors
negation — a group-key variable used in the `Goal` must be bound by the enclosing
body (in any position; the aggregate is scheduled after its binder, §8), while
variables occurring only inside the `Goal` are
existential (like wildcard variables under negation) and never exported. Because
they are never exported, a goal-local variable is also **not an answer variable**
of a query that contains the aggregate (§14).

### Termination

**This section is the single normative statement of what the engine guarantees
about a program stopping.** §6 states the semantic consequence, §15 the
evaluation one, and both refer here.

The engine **classifies** every program and **rejects none** for this.

> A program is **certified terminating** when no rule binds a **head** variable to
> an arithmetic-computed value while its **head predicate lies on a cycle of
> positive dependency edges**.

A **certified program terminates**: its Herbrand universe is finite, so the
fixpoint is reached in finitely many rounds (§6; the proof is
`notes/termination.md`). An uncertified one **may or may not** — the condition is
sufficient, not necessary — and the engine says so before evaluation starts, with
a warning (§12) rather than an error.

**Arithmetic-computed** is taken transitively, and the transitivity is
load-bearing rather than a detail. A variable is computed when an `=`-assignment
binds it to an expression that contains an arithmetic operator and mentions a
variable, **or** to an expression mentioning an already-computed variable. So all
of these compute `N`:

```datalog
nat(N) :- nat(M), N = M + 1.
nat(N) :- nat(M), K = M + 1, N = K.          % through a bare variable
nat(N) :- nat(M), N = (M + 1) as int.        % under a cast
```

Three constructs deliberately do **not** compute a value. An `as` **cast**
propagates but never creates — it maps a finite value set to a finite value set
with no accumulation (§8) — so `N = M as int` over an uncomputed `M` is certified.
An **aggregate** result is a function of a relation stratified strictly below
(§9), so its range is already finite. A **`std` module relation** (§13) is a
finite-domain map for the same reason the cast is: `year(D, Y)` and
`truncate(D, month, M)` read a bound input and produce one value that is no
larger, so neither accumulates. A ground expression (`N = 1 + 1`) yields one
value however often it runs.

**Temporal arithmetic computes**, and is not an exception to any of this:
`D = D0 + @1d` in a head on a positive cycle is value-creating exactly as
`N = M + 1` is, and gets the same warning. The classification keys on the
operator, so this needs no separate rule — but it is the shape a date-walking
recursion takes, so it is worth saying it is covered.

**Why a warning and not an error.** The uncertified shape is written on purpose
and is usually right:

```datalog
path_cost(X, Z, C) :- path_cost(X, Y, C1), edge(Y, Z, C2), C = C1 + C2.
```

This terminates on every acyclic `edge` and diverges on every cyclic one — a
property of the **data**, which no static rule can see. Rejecting it would be a
false positive on call graphs, build dependencies and every other DAG. So the
engine reports and runs (§17, 2026-08-18, which reversed the 2026-07-25 direction;
`notes/termination.md` carries the argument and the four rejected alternatives,
including the runtime budget rejected twice).

The warning distinguishes the two cases it can distinguish. Where the recursive
rule reads a relation from **outside** its own cycle, that relation's finiteness
is what consumes a step, and the warning names it — termination holds while it has
no cycle reachable through the rule, which is a claim the program itself can
check. Where it reads none, nothing can stop the growth on any input:

```
warning: value-creating recursion: `path_cost` grows by arithmetic (`C`) inside
the positive cycle `path_cost -> path_cost`; it terminates only while `edge` has
no cycle reachable through this rule
```

**Timing is part of the contract.** Answers print after the fixpoint, so this
warning is emitted *before* evaluation begins — a diagnostic delivered with the
results would never reach the program that needs it most (§14).

*Not covered:* safety/mode conditions for arithmetic (§8), and recursive or
monotonic aggregation semantics (§9) — the latter would reopen the
aggregate-results-are-finite exclusion above. An **escape hatch** that made
`path_cost` terminate rather than merely warned about is a ROADMAP item, with
limit predicates (§17 open questions; references.md group 1) as the candidate.

## 11. Provenance / explainability

The data model (`src/provenance.rs`, recorded by the engine during the §15
fixpoint):

- A **derivation** is a ground rule instance: a rule identity plus one
  premise per body literal, aligned index-for-index (the IR's stable
  `RuleId`/`BodyIdx` coordinates, §17) — the matched **fact** for a positive
  literal, the **no-match pattern** for a negated one (§7): the negated atom
  under the rule's bindings, wildcard slots left open.
- The engine records **all derivations of every derived fact**, deduplicated
  by rule instance (§17): one fact, many proofs. Base facts have no
  derivation; they are anchored by the program text, or by the **relation** an
  import bound — never by a source row. §13 materializes an import into ordinary
  base facts before lowering, so `ImportSpec` is the finest anchor there is: a
  proof says *because `employees.csv`*, and a row-level anchor is a memory trade
  rather than an omission (`ROADMAP.md`).
- A **proof tree** is one finite proof of one fact: derived nodes carry the
  fact, its rule, and child proofs for each premise; **leaves are base facts
  or no-match patterns** (a no-match terminates a branch — "no such fact
  exists" needs no sub-proof). Extraction picks, per fact, a derivation whose
  fact premises all first appeared strictly earlier in the fixpoint (§17
  first-round stamping; no-match premises always qualify), so proofs stay
  finite even when facts support each other cyclically.
- Names for rendering recover from the IR's retained tables: predicate names,
  per-rule variable names, field names (when the predicate has a schema), and
  spans. The record itself carries none — it is `PredId`/`RuleId` throughout — so
  a proof is rendered *against* a program, never on its own. What that buys is
  below.

An **absent value** (§4) appearing in a fact is provenance-anchored like any other
value, and `X is absent` succeeding is an ordinary positive premise. This is
distinct from the **no-match pattern** above, the why-not record for a *negated*
literal. The explanation of a *missing answer* is a third thing again — a
**failure trace** (§17, 2026-08-16, which named all three).

**Rendering** (`print_proof`, `src/print.rs`). A proof is a block of `%` comments,
one line per node, and every line carries its depth **twice**: as a leading
integer, and as two spaces of indentation per level. The integer is the
structural channel and the indentation is the human's — parent/child is the whole
content of a proof, and a reader with no column cannot recover it from
alignment (§17, 2026-08-21).

```
% why ancestor("alice", "carol")
% 0  ancestor("alice", "carol")  by ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)
% 1    parent("alice", "bob")  [fact]
% 1    ancestor("bob", "carol")  by ancestor(X, Y) :- parent(X, Y)
% 2      parent("bob", "carol")  [fact]
```

- The header wears no depth number, which is what distinguishes it from a node.
- A **derived** node cites the rule that fired; a **leaf** carries `[fact]`, or
  `[fact from "employees.csv"]` for an imported relation; a **no-match** prints
  as `no parent(_, "alice")`, `_` for the slots the negation left open.
- A fact prints in **named form wherever the predicate has field names**,
  positional otherwise. Unlike §14's answer stream a proof never has to re-parse,
  which is what makes the named form available — and it is what a wide imported
  table needs, where eight positional columns say nothing about which is which.
- A **self-justifying** premise (an §8 comparison or presence test, a §9
  aggregate) prints the literal it satisfied beside the values it satisfied it
  with — `A >= 18  (30 >= 18)` — since the record holds only the values, and a
  body may hold three comparisons of the same shape. An aggregate reports its
  fold: `(6 over 2 values, 1 absent skipped)`, which is §9's skip-count surface,
  and the value alone for `count`, whose total already includes absent bindings.
  A conversion that failed on data appends `[1 value lost converting to int]` —
  the one place §12's *missing* / *malformed* distinction reaches a proof.
- The cited rule is the **lowered** one, reconstructed from the IR rather than
  sliced from the source span: premises align index-for-index with the lowered
  body, while lowering hoists compound arguments into `=`-assignments, expands
  each `;` disjunct into its own clause, and makes named arguments positional.
  A source slice would show a body whose literal count does not match the
  premises listed under it. Arguments §5's partial selection omitted are omitted
  again; a lowering-generated slot that occurs more than once keeps an identity
  (`_g3`), since two temporaries spelled `_` would read as one variable.
- **Nothing elides and nothing is shared.** A fact proved in two branches prints
  twice and a deep proof is deep — §15's no-budget-anywhere decision applied
  here, not a second policy.

**The goal that asks** (§5): `?why <fact>.` and `?whynot <fact>.`, over a ground
atom, in a program file or inside a `-q` argument. Both answer with the same
union — `proof` / `underivable` / `unknown` — so **the sigil is not a selector**:
`?why` over a fact that does not hold is an ordinary question, and so is
`?whynot` over one that does (§17, 2026-08-16).

What the sigil *does* decide is what the run records. `?why` provisions the
derivation store; `?whynot` re-solves out of the finished model and provisions
nothing, which is why a run that only asks why-not pays no provenance cost at all
(`Provenance`, §17 2026-08-21). The cross case — `?whynot` over a fact that turns
out to hold — re-runs the **fixpoint** with recording on, reusing the lowered
program, and says so in one line rather than answering partially.

An explanation is **not a row**. It rides in the same stdout stream as `%`
comments and is invisible to §14's exit code, so appending a goal to a run
changes neither what it answers nor how a shell pipeline branches on it. Stripping
the comments leaves the fact stream byte for byte (`testing.md` **E5**).

**Underivable** is a *failure trace*: one **near-miss per rule whose head unifies**
with the goal, in rule order, carrying the premises the body satisfied, the first
literal it could not, and one **repair**. A near-miss is a rule and not a binding,
and that is the whole bound — nothing truncates and no budget is spent, the size
of the answer being the program's own rule count. Each rule's body is re-solved
through the **scheduler the fixpoint uses**, so the literal reported as blocked is
the one the run really failed; an extractor choosing its own literal order would
be a second evaluator. A repair is a **step, not a promise**: the literals past
the block were never evaluated, so supplying what it names advances that rule's
prefix and need not derive the goal. Three of the five repairs name no fact — a
blocked *derived* premise (the repair is the next question, `ask ?whynot …`), a
pattern with a slot nothing bound, and a refuted negation, which names the row
that refuted it since the language has no retraction. A fourth names none either:
a slot bound to `absent` can be matched by no row at all (§4), so no fact would
repair it (`testing.md` E10).

*Not covered:* the **JSON encoding** of a proof or a trace (§14), which is the one
piece of this surface still undecided. **Proof trees as facts are not coming**: a
proof tree is not a fact, so it rides in `%` comments, which keeps
Datalog-out-is-Datalog-in intact. Also not covered: semiring provenance under
negation and tropical cheapest-proof selection
(`notes/semiring-provenance.md`), and lineage annotations on answer rows — Tier 1
in tsdl's sense — which this engine does not have at all.

## 12. Error model

A diagnostic is **data with a rendering**, never a rendering with data attached.
The prose sentence is one field among several, so a consumer never has to parse
English to recover where the problem is or what to do about it — the
structured-errors pillar (§2), which matters most when the consumer is an agent
about to rewrite the program.

**Severity.** Two levels today. An **error** rejects the program (stderr, and the
run does not answer — §14's exit vocabulary; every stage collects *all* of its own
errors before returning, so one run reports everything at that stage rather than
the first thing). A **warning** lets the program run and answer, but flags
something that is valid yet usually a mistake — currently a
referenced-but-undefined predicate (§10), an aggregate that skipped `absent`
inputs (§9), a value-creating recursion (§10), and a conversion that failed on
data (§8, below). Warnings go to stderr, never stdout, which stays a clean fact
stream (§14).

A warning **never changes the exit code**, which answers a different question:
whether the run produced rows, not whether it was happy about them (§14).

**Missing and malformed are different reports.** An `absent` that arrived as data —
an empty CSV cell, a JSON `null` (§13) — is *missing*, and §9's aggregate skip
reports it as such. An `absent` the engine **manufactured** by failing a
conversion (`"abc" as int`, §8) is *malformed*: a value existed and could not be
represented. Both flow identically through the value model (§4) — that is the
2026-08-16 decision and it stands — so the diagnostic is the only place the
distinction survives, and it is reported at the conversion that lost it, counted
per site, only on a run where one actually failed.

**Category.** Which stage rejected the program, and so which vocabulary the
message speaks: `lex`, `parse`, `semantic` (safety, stratification, types),
`source` (§13 loading).

**Code.** A stable kebab-case name for *what kind of thing is wrong* —
`unsafe-rule`, `unstratified`, `type-clash`, `file-not-found` — so a consumer
branches on the code and never on the sentence. The category is the reader's
axis and the code is the machine's: a code belongs to exactly one category, and
knowing the code implies the category but not the reverse.

A code exists **where a fix differs in kind**, not where a message differs. Which
field is unknown is what the message is for; that the named arguments do not
match the schema is what `field-mismatch` is for. Every diagnostic carries one —
there is no generic fallback, so a new kind of wrongness gets a new code rather
than a shrug — and the full list, with the census it was derived from, is
[`notes/error-codes.md`](notes/error-codes.md).

**The stability contract.** Codes are **stable and additive**. A code is never
repurposed: it means the same thing in every later version, or it does not exist.
Adding one is a compatible change, and a consumer that does not recognise a code
**falls back to the category**, which is why every code has one. Renaming or
removing a code is a breaking change, and the set is pinned by a test so neither
can happen quietly.

A **warning** carries a code on the same terms (§12's severity axis), drawn from
the same vocabulary rules.

**Location.** An error carries the **span** it is about, and that span resolved
against the source to a 1-based **line and column** — not a byte offset, which
cannot be turned into a caret or a `file:line` an editor will follow. Columns
count characters, so a caret lands correctly under non-ASCII text. Lexer and
parser errors carry positions today; semantic and source errors carry the source
name and, for imports, the row and column, but not yet a span (below).

**Suggested fix.** A separate field, not a sentence fragment: the near-miss
hints models reach for out of a Prolog prior (`=<` → `<=`, `\=` → `!=`), an
uppercase relation name, the nearest defined predicate for a typo.

**A gated name is the fix's clearest case** (§13). A `std` module's relations are
only in scope where the module is imported, so using one without the import is
the ordinary referenced-but-undefined warning — and the suggestion turns it into
one step: `` `year` is provided by `std/time`; add `import "std/time".` ``. This
is deliberate design cost, not an accident: gating is what lets those relations
take short names that would otherwise be unusable as reserved words, and the
suggestion is what keeps the gate from costing a round of guessing.

**Rendering** is
`"{category} error [{code}]: {message} (at {line}:{column}) ({suggestion})"`,
composed from the fields — so a future `--format json` edge (§14) serializes the
same data with no message re-parsing. The code is rendered ahead of the sentence
rather than after the position, so the machine-readable part of the line never
sits behind the prose a machine is trying not to read.

A span is attached by whichever stage knows one. The lexer and parser hold the
program text and resolve as they go; every later stage works over the IR, which
retains the span of each clause, premise, query and import, and records the span
alone for the position to be resolved once at the API boundary. Naming has not
gone away — `variable Q in rule 0` still says *which* variable — the position
says where to look for it, which is what stops the message degrading with
program length.

*Not covered:* a span kept for a program that **spliced in a module** (§13): spans are per-file byte
offsets, so once there is more than one file an offset no longer identifies a
place, and the span is dropped rather than resolved against the wrong text.
Module-resolution errors name their file in the message instead. Nor does this
section say
anything about a `suggestion`'s *content*, which is the field with the sharpest
known hazard: a model acts on a suggestion literally, so one that cannot be acted
on costs a round and one that is wrong on correct code is worse than none
(`notes/tsdl-cross-project-review.md`, measured there). Nor is there a **third
severity**: a run that completes, declines to answer, and says so with a non-zero
code is what the truncation contract (§15) needs and what nothing today produces,
so it is specified there rather than given a rung here (§17, 2026-08-18). Nor
does the malformed report reach a conversion written **inside a comparison**
(`X as int > 5`): a failed conversion makes its operand `absent`, every
comparison with an absent operand is false (§8), and the row is filtered out
before there is anything to report it on — so a dirty column silently narrows a
filter where it visibly widens an assignment. Tracked in `ROADMAP.md`.

## 13. External data / fact sources

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
  positions, including a leading sign); anything else is a string, and an **empty
  cell is the absent value** (§4). **A cell in ISO-8601 extended form is typed
  temporal** — `2026-08-19` a date, `2026-08-19T10:30:00` a timestamp — which is
  the one place the rulebook reads a cell as a literal it is not spelled as: the
  `@` sigil is a delimiter the way a string's quotes are, and a CSV `alice`
  already becomes the string `"alice"` rather than the symbol `alice` on exactly
  that reasoning. Inference is **strict about the form**: the space-separated
  `2026-08-19 10:30:00` common in exports stays a string, and an explicit schema
  (`as t(d: timestamp)`) is what reads it, since a declared type *coerces* and is
  deliberately more permissive than inference. Durations are never
  inferred here either (see the typed-source rule above). A
  column's type is the unification of its **non-absent** cells: all-int → int,
  int/float mix → float, all-bool → bool, all-date → date, a date/timestamp mix →
  timestamp (exact, at midnight),
  anything else → string; missing cells become `absent` and do **not** force the
  column's type (an int column with gaps stays `int` — this unbroke the USDA
  `amount`/`food_category_id` columns, §17 2026-07-24). An int/float mix widens
  each integer cell to float, and **a widening that would lose precision is a
  structured error, never a silent rounding** (2026-07-25): above 2⁵³ an `i64`
  generally has no exact `f64`, and large integers in imported data are usually
  identifiers, where rounding would corrupt every join on them. One stray float
  cell is enough to make a whole ID column `float`, so the error names the cell
  and points at the explicit-schema escape hatch. The test is exactness, not a
  magnitude cutoff — 2⁵³ itself widens fine — and an *all*-integer column is
  never widened at all. A column whose cells are
  *all* absent has no inferable type: it is resolved by use-site variable flow
  (§4), else left unconstrained. Inferred types are never symbol. This makes the
  anchor property exact:
  **an import means precisely the facts you would get by writing its cells as
  in-program literals, each cell delimited as its type requires** — a string's
  quotes and a temporal's `@` are supplied by the reader, exactly as they would
  be by a hand writing the fact. Inference stays deterministic across engine and
  dependency versions.
- **Typed sources are their own authority**: JSON, Parquet, and database columns
  carry types, coerced onto the value types (a JSON `"42"` stays a string, never
  re-inferred; ints are range-checked into int). A `DATE` column becomes `date`
  and a timestamp column `timestamp`, at microsecond precision — a finer-grained
  source column is a precision error naming the column, on the same rule that
  rejects a lossy int→float widening below. **A zoned timestamp is converted to
  UTC and its offset dropped**, since §4's timestamp is civil: the instant is
  preserved and the displayed clock reading may change, which is why it is stated
  here rather than left to the reader. An `INTERVAL` stays **text**, with `UUID`
  and `JSON`: **a duration is never inferred, from any source**. The reason is
  the one that keeps the rest of this cheap — a source renders a duration in its
  own dialect (DuckDB's is `1 day 02:00:00`), so reading it would mean carrying
  a second duration grammar beside §3's, and one grammar is what makes reading
  and rendering inverse. Dates and timestamps raise no such question: ISO-8601
  *is* §3's grammar. A **null / missing value from any source becomes the
  absent value** (§4), uniformly — an empty CSV cell, a missing JSON key or
  explicit `null`, a Parquet/DB `NULL`; nested/compound values remain structured
  errors naming row and column. This replaces the former three-backend split (CSV
  empty → `""`, JSON/Parquet null → error) and retires the "value space has no
  null" rule (§17 2026-07-24). One table still binds to **one relation** — absence
  is a value in a cell, not a change to the import's shape.
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
  | `*.jsonl`, `*.ndjson` | one object per non-blank line; field order = the first record's key order, later-only keys appended; a key a record lacks is absent; scalar values only; a file with no records is the empty relation under an explicit schema, and an error without one (nothing names its fields) |
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
applies every rule in this section. An import that is read is materialized
whole into ordinary in-memory facts; the evaluator never touches DuckDB.
**Which imports are read follows the program's goals.** When every data import
has an explicit schema, the program is lowered first — so a lowering error
arrives before any file is opened — and only the imports a goal reaches (§15)
are read. The rest contribute no rows but are still refused for anything a read
would refuse before its first row: a reserved or unsupported format, a missing
local file. A schema-less import takes its arity from its header, so a program
with one reads every import before lowering. Typecheck runs over what was read,
and a rejection over a partial read is re-checked over a full one (§17,
2026-09-12). URL imports read **directly** over DuckDB httpfs — no local file is
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
- Module imports are local files only in v1 (no URL modules) — with one
  exception, below, which is not a file at all.

### `std` modules — the builtins the language cannot spell

```datalog
import "std/time".
```

Some operations a rule cannot express are also unspellable as *functions*: an
`ident (` in expression position is ambiguous between an atom and a call, which
is what ruled out `float(A)` (§17). As a **body literal** there is no ambiguity,
so a builtin is a **relation**, and `std` modules are how a program asks for one.

- **`std/` is a reserved virtual path prefix**, resolved before the filesystem is
  consulted. It needs no grammar: `import "std/time".` has no `as` clause, so it
  is already a module import by the shape rule above. A real `./std/` directory
  that would otherwise shadow it is a **structured error**, never a silent
  preference for one or the other.
- **The relations are in scope only where the module is imported.** That is the
  point of the gate rather than an access-control feature: it is what lets them
  take short, obvious names — `year`, `month`, `day` — which as *reserved* words
  would break every program with a column called `year`. An unimporting program
  is unaffected, and using a gated name without the import is §12's
  referenced-but-undefined warning carrying the import line as its fix.
- **A collision is an error naming both origins**, exactly as two disagreeing
  `declare` schemas are (§4): a program that imports `std/time` and also defines
  its own `year` is rejected, and never silently resolved in either direction.
- **They bind like any body literal.** Every argument a module relation reads
  must be bound by the body in §10's sense, so `day(D, 15)` is a filter and
  `day(D, N)` binds `N`; the scheduler places them like it places a comparison.
  They are finite-domain maps, so §10 exempts them from value-creating recursion.

**`std/time`** is the first and, in v1, the only one:

| relation | reads | binds |
|---|---|---|
| `year(V, N)` `month(V, N)` `day(V, N)` | a `date` or `timestamp` | an `int` component |
| `hour(V, N)` `minute(V, N)` `second(V, N)` | a `timestamp` | an `int` component |
| `truncate(V, U, V')` | a point `V`, a unit symbol `U` | the same point type, at the start of that period |

`U` is one of `year`, `quarter`, `month`, `week`, `day`, `hour`, `minute`, and
is written **literally** — the unit is static in every correct program, so a
typo is caught once with the list attached rather than once per row. A `week`
starts on **Monday**, as ISO-8601 numbers it. A symbol outside the set, or a
computed one, is a structured error. `truncate` is what
`timestamp as date` refuses to be (§8), and it is what a group-by-period joins
on — one sortable key rather than a tuple of components, and the only spelling
that reaches a week or a quarter at all.

*Not covered:* **`std/math`** (`abs`, `ceil`, `floor`) and **`std/text`**
(`length`, `lower`, `substr`) — the deferred builtin scalars (§8) now have a home
and a spelling, and stay deferred until a consumer needs them. The mechanism was
designed for them and not only for `std/time`, deliberately (§17).

*Not covered:* **database loading** — the `table "…"` grammar is reserved and
SQLite/DuckDB/Postgres loading is deferred until a consumer needs it — and **TSV**,
deferred with it. **Filter pushdown**: an import a goal reaches is read whole;
pushing selections into SQL waits on someone hitting that wall. **Module namespacing**: v1 shares one global namespace, and qualified
names and visibility are deferred — `std/` is a reserved *prefix*, not a
namespace system, and gates a module's names rather than qualifying them. A cell the reader cannot represent is a
structured error, and what that costs the *run* is §15's.

## 14. Programmatic / agent API

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
cross-type order (symbol < string < int < float < bool < date < timestamp <
duration); strings are double-quoted with the §3 escapes; a float always carries
a decimal point (`1.0`, not `1`) so it re-lexes as a float.

**A temporal value prints as its §3 literal, sigil included** — `@2026-08-19`,
`@2026-08-19T10:30:00`, `@1d12h` — which is what keeps the closure property true
of a program that computes one. Three spellings are canonical where the grammar
accepts more: a timestamp's fractional part appears **only when nonzero**, a
duration is the **largest-unit decomposition** with zero components omitted
(`@1d12h`, never `@1d12h0m`), and zero is `@0s`. An ISO-8601 duration read on
input (§3) therefore prints in the friendly form — the two spellings are one
value, and the canonical one is chosen so that printing is a function of the
value and not of how it was written.

A query that **names itself** answers under that name: `?- adult: person(N, A),
A >= 18.` prints `adult` facts, one per answer row, over exactly the columns the
projection has (`name(true).` where it has none). The name is a definition, not a
label — the form is exact sugar for a rule whose head is the projection — so §5's
grammar carries it and a rule may read the relation a query named. Naming is
optional, and it is the fix for the projection hazard below.

An **unnamed** query's answer shape keys on whether the body's positive atoms
account for **every** answer variable:

- when they do, the query re-emits those atoms with the answer bindings
  substituted (`?- ancestor("alice", Who).` → `ancestor("alice", "bob").` …).
  This covers a **single atom** carrying the whole projection, however many
  non-binding literals — comparisons, negated atoms, presence tests, an aggregate
  used only as a filter — sit beside it; and a **ground** conjunction, whose
  atoms print once if the body holds;
- a body with **no answer variables** and nothing substitutable to show answers
  `holds(true).` when it holds — a multi-atom existence check, a bare comparison
  (`?- 1 < 2.`), a negation-only body (`?- not banned("bob").`), or an atom whose
  only variable is a wildcard (`?- p(_).`);
- any **other** body emits synthesized `answer/N` facts over the query's
  **answer variables**;
- facts are deduplicated and sorted, by relation name then value — so a ground
  conjunction's output does not depend on the order its atoms were written in.

**Silence means an empty answer**, and for a body with no answer variables it
means **no**: the substituted form says yes by printing its atoms and no by
printing nothing, which is what `?- p("a").` has always done. Only a body with
nothing to substitute needs `holds/1`, and §5's ban on 0-arity atoms is why its
yes carries an argument.

Why set *equality* and not "every atom argument is projected": a body can bind a
variable no atom mentions — an aggregate's result, an `=`-assignment — and
substituting the atoms would silently drop that column, an answer with a missing
field. The one-directional test was sufficient only while the shape rule also
required a single-literal body (§17, 2026-08-03 and 2026-08-17).

This is a rule about the query *as written*, and lowering must keep it that way:
a query constant-folds a ground compound argument (§5) precisely so that
`?- p("a", 1 + 1).` is still the single atom it reads as. Hoisting it produced a
two-literal body with no named variables, which then printed nothing
(`bugs/005`, §17 2026-07-27).

The **answer variables** are the named variables the query body *binds* — which
is not the same as every named variable (clarified 2026-07-25). An aggregate's
goal-local variables are named, but they exist only inside that aggregate's
sub-join (§9) and have no value in the answer row, so they are not projected:
`?- N = count { C | m(T, C) }.` answers over `N` alone. The binding rule is the
one a rule head is checked against (§10), applied to the query body and never by
descending into an aggregate's goal.

**Answers are a projection, not the relation.** The substituted-atom form prints
a real predicate's name over only the rows the query matched, and nothing in the
output marks it as a subset. Composing it onward therefore *narrows* that
predicate: `?- ancestor("alice", W).` piped into a program reasoning about
`ancestor` presents an `ancestor` holding only alice's rows. The closure above is
unaffected — the output is valid input, and every row of it is true — but it
answers a question rather than dumping a relation. **The division of labour is
that inference is for reading one query's output and naming is for composing
it**, because an inferred name is the wrong name for composition either way — a
source relation's name collides with the relation it was projected from, and
`answer` collides with every other query's answer.

**Naming the query removes the hazard**, and is the only thing that does: named
output wears no source relation's name, so nothing is silently narrowed. It stays
a hazard for an **unnamed** query, unavoidably — re-emitting a substituted atom
is what a reader wants, and it is a subset by construction (§17, 2026-08-16 and
2026-08-17; §16.11 shows both halves).

**Binary contract** (2026-07-22; `-q` completed 2026-07-23, roadmap step 6).
Invocation is `datalog [<file> | -] [-q <query>]…`. The positional source is a
program file, `-` for stdin, or **omitted** (empty base program); at most one is
allowed. Each query's answers print to stdout as canonical facts. Argument parsing
is a small hand-rolled loop in `src/main.rs`; the logic lives in the library
(`api::run_with_queries` / `program_with_queries`), so `main` stays thin.

**The exit code answers the question**, on `grep`'s vocabulary (2026-08-18):

| code | meaning |
|---|---|
| **0** | **rows found** — at least one query printed an answer, *or* the program had no queries to ask |
| **1** | **no rows** — every query ran and none produced an answer |
| **2** | **did not answer** — a usage error (bad arguments, unreadable file, a bare `datalog` with no source and no `-q`) or a program error (lex/parse/lower/type/eval), printed to stderr, one per line |

**The vocabulary is a range, not a list: `0` and `1` are answers, `≥ 2` means the
run did not answer.** A caller branches on that boundary, which is what lets a
later code refine `2` (a §13 source failure is the standing candidate) or number
the withholding §15 describes, without reinterpreting a code that already means
something. `101` (a Rust panic) and `130` (`^C`) are the process's, not ours.

A **warning does not change the code** (§12): the code says whether the run
produced rows, not whether it was happy about them. A program with **no queries**
exits `0` — nothing was asked, so "no rows" is not an answer to anything, and
`datalog p.dl` stays usable as a plain check that a program loads and runs.

**Nor does an explanation** (§11, 2026-08-21). A `?why` / `?whynot` goal is not a
query and its answer is not a row, so the code is computed over the queries alone:
adding a goal to a run cannot change which way a shell pipeline branches on it,
and a run whose *only* statements are goals exits `0` for the same reason a run
with no queries does. This is the exit-code half of the guard that stripping an
explanation's comments leaves the fact stream byte for byte (`testing.md` E5).

**A consistency check is a query, not a construct** (2026-08-18). The language
needs nothing for it: a negation-only body answers `holds(true).` when it holds,
and §10 exempts wildcard-fresh variables under negation from range restriction, so
the check writes as an ordinary query and the exit code carries the answer.

```sh
# `&&` means what it looks like: deploy only if nothing is double-booked
datalog roster.dl -q 'not double_booked(_, _, _)' && deploy
```

**Phrase the check affirmatively**, as above. It is not a style preference: every
error code is non-zero, so `&&` cannot fire on a program that failed to compile,
while the inverted phrasing — `datalog roster.dl -q 'double_booked(P, S1, S2)' ||
deploy` — deploys on a syntax error as readily as on a clean roster, because `||`
fires on `2` exactly as it fires on `1`.

**One-shot `-q` queries** — the jq analog (resolved 2026-07-23):

```sh
# bare-atom query: sugar for appending `?- ...` to the loaded program
datalog family.dl -q 'ancestor("alice", X)'

# comma-body query: also just a query body — the filter binds nothing, so this
# answers in `person` facts
datalog people.dl -q 'person(name: N, age: A), A >= 18'

# define-and-select: append the rule, then a synthesized `?- <head>.` — and the
# way to make a result travel under a name of its own
datalog family.dl -q 'grandparent(X, Z) :- parent(X, Y), parent(Y, Z)'

# composition over pipes ("-" reads stdin)
datalog people.dl -q 'adult(N) :- person(name: N, age: A), A >= 18.' \
  | datalog - -q 'adult(N), N != "bob"'

# an explanation goal (§11) — the same statement a program file would carry
datalog family.dl -q 'ancestor("alice", X)' -q '?why ancestor("alice","dave")'
```

`-q` semantics: an argument is classified by **parsing** it (never by splitting
on `:-`, which a string literal may contain). An argument opening with **`?why`
or `?whynot`** is already a §5 statement and is appended verbatim — which is why
explanations need no flag of their own. **One rule** with a non-empty body
— however many clauses it desugars to, since a top-level `;` expands to one
clause per disjunct sharing the head (§5) — is appended verbatim, followed by a
synthesized `?- <head>.` over that head atom. Anything else (a bare atom, a
comma-separated body, or clauses with differing heads) is a **query body** and is
appended as `?- <arg>.`. A trailing `.` is optional.
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

*Not covered:* **which query a synthesized answer answers** — two *unnamed*
boolean queries in one run both print `holds(true).` with nothing to tell them
apart, and the fix is a `%` comment rather than an extra argument, sequenced with
§11's comment rendering. Naming both queries already tells them apart, so this is
now only the unnamed case. Also not covered: **naming is never required**, so an
unnameable projection is not an error (§17, 2026-08-17 — a bare `?-` stays total,
and making it strict is affordable only now that a name exists). Also not covered: a
`--format json` data path (deferred as low-value; JSON stays at the
machine-readable edges), `serde` on the API types, and streaming or cursored
results. Also not covered: **an exit code survives a pipe only if the caller asks
it to.** The code above is the one channel carrying the difference between "no
rows" and "could not answer", and a downstream `datalog - -q '…'` sees **only**
stdout — so `datalog a.dl | datalog - -q '…'` reads a run that failed exactly as it
reads one that answered nothing, unless the shell is running under `set -o
pipefail`. Nothing on stdout can close this: a marker a downstream lexer skips
changes nothing, and one it does not skip is not valid input, which S2 forbids
(§1). The closure property and the truncation contract (§15) are in genuine
tension here, and this is where it lands (§17, 2026-08-18).

## 15. Evaluation strategy (non-normative)

`eval` (`src/engine/`) computes the model (§6) — least, or perfect where the
program has negation or aggregation — bottom-up:

- **Load**: program facts enter per-predicate *set* storage (duplicates
  collapse, §17). Relations are sorted sets, so iteration — and hence §14's
  canonical output order — is deterministic by construction.
- **Stratified fixpoint**: the IR's strata are evaluated in order, each to
  fixpoint before the next (a single stratum until §7 lands).
- **Pruning**: only the rules a goal depends on are evaluated — those whose
  head is among the predicates a query body or explanation goal names, closed
  under every dependency edge, negated and aggregated ones included
  (`lower::live_predicates`). A program with no goals evaluates every rule.
  Answers are unchanged (testing.md B13); a rule that is not evaluated also
  cannot raise a runtime error or keep the run from terminating, while §10's
  and §12's static warnings still cover the whole program (§17, 2026-09-12).
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

**What makes the loop stop** is §10's finiteness argument, not anything in the
loop. For a **certified** program the Herbrand universe is finite (§6), so the
relations can only grow finitely often and a round must eventually add nothing.
For an **uncertified** one nothing does, and the loop runs until the operator
interrupts it: there is no iteration cap, fact cap or wall-clock budget anywhere,
**by decision** rather than by omission (§17, 2026-08-16 and 2026-08-18) — a static
classification is the guarantee, a `^C` is the operator's, and a slow program stays
slow. The engine's only obligation on the uncertified path is to say so first,
which §10 discharges before this loop starts.

*Also not covered, and decided rather than deferred:* **what an incomplete model is
worth**. Decided 2026-08-16 — an incomplete fixpoint holds missing facts and never
false ones, so a whole-model dump survives it while *answers* do not, because a
query solved against it can be wrong rather than missing (`not p(X)` over an
incomplete `p` succeeds). Where a relation is short by rows the discriminator is
how the program reads it: projected, answer one row short and say so; folded or
negated, **withhold** — which means no answer on stdout, the reason on stderr, and
an exit code of its own (§14).

**Nothing in this engine produces a short relation, so withholding has no trigger
and no exit code yet** (2026-08-18). The rule above is about rows the fixpoint
never derived; there is no budget, no fuel and no cap on the production path (the
round cap in `src/engine/` is a test oracle), imports fail with a structured error
rather than dropping a cell (§13), and an `absent` **value** is data the source did
not have, not a row this engine did not reach (§4). When a trigger does exist — a
hosted surface's budget, an external signal — it takes the next code above §14's
`2`, which that section's range invariant reserves for exactly this.

Also not covered: semi-naive's interaction with anything annotation-shaped, magic
sets, indexes, parallelism, incremental maintenance.

## 16. Worked examples

> These are the canonical corpus at every level of the test pyramid
> (`testing.md`), and the design was driven from them rather than in the abstract.
> Each example ends with the design questions it settled or still raises.
>
> **An example is a claim about the engine, so it should name the test that runs
> it.** §16.9 onward do — a ROADMAP item for the other eight, and the reason a
> block nobody wired up could quietly stop being true (§17, 2026-08-16).
> Everything below is ratified in §3–§5, §8, §9, §11 and §13.

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
variables in negated atoms must be bound by the body, wildcards are existential
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
child_count(P, N) :- parent(P, _), N = count { C | parent(P, C) }.
% expected: child_count("alice", 2), child_count("bob", 1)
```
*Ratified (§9, 2026-07-24):* set-builder syntax `op { Expr | Goal }`; grouping is
implicit on the rule variables outside the aggregate (here `P`); the five reducers
`count`/`sum`/`min`/`max`/`avg` with inferred result types; aggregated predicates
are stratified strictly below (recursion through an aggregate is rejected).

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
% % why ancestor("alice", "carol")
% % 0  ancestor("alice", "carol")  by ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)
% % 1    parent("alice", "bob")  [fact]
% % 1    ancestor("bob", "carol")  by ancestor(X, Y) :- parent(X, Y)
% % 2      parent("bob", "carol")  [fact]
```
(The answer is itself a block of `%` comments, so the expected output above is
doubly commented — one `%` for this example, one that is the proof's own.)

*Rendering ratified (§11, §17 2026-08-21)* and tested by
`print::tests::example_16_6_proof_renders_as_the_spec_shows`, which runs this
program and pins the block byte for byte. Depth rides in two channels — the
leading integer for a reader with no column, the indentation for one with; §11
carries the rule and its reasons.

The goal is a §5 statement, so it works in a file or inside a `-q` — run by
`tests/system.rs::a_why_goal_prints_the_proof_the_spec_shows`:

```sh
datalog family.dl -q '?why ancestor("alice","carol")'
```

How a fact with several independent derivations chooses one is governed by §11's
well-founded order and not by anything the user asks for — and the answer does not
say how many others there were. *Still open:* the JSON encoding (§14). The other
half of the surface — a goal that does **not** hold — is §16.15.

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
test. One adaptation survives in the fixture: the `employee` import is written
with an explicit schema. It is an artifact of the fixture predating §13's
implementation and no longer describes a limit — imports load before lowering,
so a header supplies the field names and named access to a schema-less import
works.

### 16.8 Absent — missing data through import, filter, aggregate, provenance

```datalog
% a sparse nutrient table (header: food, nutrient, amount); some amounts empty
import "data/food_nutrient.csv" as measurement.

% naming `amount` only DISPLAYS it — every row returns, missing ones as `absent`
recorded(F, N, A) :- measurement(food: F, nutrient: N, amount: A).

% a threshold silently excludes absents (a comparison with absent is false)
high_iron(F) :- measurement(food: F, nutrient: "iron", amount: A), A >= 5.

% explicit presence — the rows that HAVE an amount, and those that don't
has_amount(F, N)     :- measurement(food: F, nutrient: N, amount: A), A is not absent.
missing_amount(F, N) :- measurement(food: F, nutrient: N, amount: A), A is absent.

% aggregation skips absents (and reports how many); avg is over present values
avg_iron(Avg) :- Avg = avg { A | measurement(nutrient: "iron", amount: A) }.
% expected: mean over present iron amounts; ?why notes N absent values skipped
```
*Resolved (§4/§8/§9/§13, 2026-07-24):* absence is a first-class, two-valued value —
type-neutral at import (the `amount` column stays numeric despite gaps),
annihilating in arithmetic, false in comparisons, tested with `is [not] absent`,
skipped-but-reported by aggregates (the skip count via provenance, §11); the
`absent` literal round-trips (Datalog-out is Datalog-in). The aggregate surface is
the ratified set-builder `avg { A | Goal }` (§9), grouped globally here.

### 16.9 Conversion — the `as` cast

Run by `tests/system.rs::cast_program_converts_and_guards` over
`tests/programs/16_9_cast.dl`.

```datalog
% two int columns — `1 / 3` is integer division, so a ratio needs the cast
serving(oats, 3).
total(9).
share(F, R) :- serving(F, N), total(T), R = (N as float) / (T as float).

% text that is only mostly numeric (the CSV case): an unreadable cell converts
% to `absent`, so both halves stay reachable with the ordinary presence test
raw("30").  raw("7").  raw("n/a").
amount(V)   :- raw(X), V = X as int, V is not absent.
unparsed(X) :- raw(X), V = X as int, V is absent.

% rendering is total, and is §14's canonical spelling
label(S) :- total(T), S = T as string.
```
```
share(oats, 0.3333333333333333).
amount(7).
amount(30).
unparsed("n/a").
label("9").
```
*Resolved (§4/§5/§8, 2026-08-16):* conversion is postfix `Expr as type`, binding
tightest and chaining left-to-right, with the result type fixed unconditionally —
so the cast terminates inference for its operand. A conversion with no value to
read is `absent` rather than an error, which is what keeps `unparsed` writable at
all: the language has no convertibility predicate, so `is absent` *is* the guard.
A *lossy* conversion stays an error (§8's table).

### 16.10 The answer shape — what a query prints, and under what name

Run by `tests/system.rs::answer_shape_program_prints_each_form` over
`tests/programs/16_10_answer_shape.dl`. The queries are in the file here, because
they are the subject.

```datalog
% §14's answer shape: what a query prints depends on whether its positive atoms
% account for every answer variable
person("alice", 34).  person("bob", 17).  person("carol", 29).
banned("carol").

% one atom beside a filter that binds nothing — answers in `person` facts
?- person(N, A), A >= 18.

% the aggregate binds a variable no atom carries, so the columns need a name of
% their own and the answer is synthesized
?- person(N, A), C = count { X | banned(X) }.

% a ground conjunction has no matched combinations to lose, so both atoms print
% under their own names — and in name order, whichever order they were written
?- person("bob", 17), banned("carol").

% nothing to substitute and no answer variables: the body's whole content is a
% yes, and §5's ban on 0-arity atoms is why it carries an argument
?- not banned("bob").
```
```
person("alice", 34).
person("carol", 29).
answer("alice", 34, 1).
answer("bob", 17, 1).
answer("carol", 29, 1).
banned("carol").
person("bob", 17).
holds(true).
```
*Resolved (§5/§14, 2026-08-17):* the rule is set **equality** between the atoms'
variables and the answer variables. Query 2 is the boundary the one-directional
test got wrong — every atom argument is projected there too, so substituting
`person` would silently drop `C`. Query 4 is what the shape owed and did not pay
before: it printed nothing whether or not it held. What this example cannot show
is the hazard in query 1 — those are real `person` facts over a subset of
`person`, which only a name removes; **§16.11 is that half**, over the same
relation and asking the same question.

### 16.11 Naming a query where it is asked

Run by `tests/system.rs::named_query_program_publishes_its_own_relation` over
`tests/programs/16_11_named_query.dl`. The queries are the subject, so they are in
the file; the first two ask the *same question* unnamed and named.

```datalog
person("alice", 34).  person("bob", 17).  person("carol", 29).
banned("carol").

% the hazard: real `person` facts over a *subset* of `person`, and nothing
% downstream can tell them from the whole relation
?- person(N, A), A >= 18.

% the fix — the same question under a name no source relation wears
?- adult: person(N, A), A >= 18.

% a name also replaces the synthesized `answer/N` the aggregate would force
?- with_bans: person(N, A), C = count { X | banned(X) }.

% no answer variables, so no columns: the yes carries an argument instead
?- clean: not banned("bob").

% the name is a definition, not a label — a rule may read what a query named
eligible(N) :- adult(N, _), not banned(N).
?- eligible(N).
```
```
person("alice", 34).
person("carol", 29).
adult("alice", 34).
adult("carol", 29).
with_bans("alice", 34, 1).
with_bans("bob", 17, 1).
with_bans("carol", 29, 1).
clean(true).
eligible("alice").
```
*Resolved (§5/§14, 2026-08-17):* `?- name: body.` is exact sugar for a rule whose
head is the projection, desugared during lowering — which is why `eligible` can
read `adult`, and why the answer needs no new print rule. Arity is the
projection's length, so a body with no answer variables heads the ground
`name(true)` (§5 bans 0-arity atoms). The head is range-safe by construction, the
projection being what the body binds. One guard: a name the program already
**defines** is rejected, since predicates intern by name alone and the answer
would silently extend that relation — a name merely *referenced* is free to take,
because defining it is what the equivalent hand-written rule does.

*Not covered:* **a named test per example**, which is what would make this corpus
load-bearing rather than illustrative; and the expected output shown in comments is
prose, not a pinned fence compared byte-for-byte. Both are one ROADMAP item —
§§16.9, 16.10 and 16.11 are done the new way, and the pattern the other **eight**
should be retrofitted to. An example proves a claim about
the channels it compares and nothing about a channel it is silent on — which
§16.12 is the first to take seriously, half its expected output being stderr.

### 16.12 Termination — which side of the line, and what the engine says

Run by `tests/system.rs::termination_program_certifies_one_half_and_warns_on_the_other`
over `tests/programs/16_12_termination.dl`. **The first example that exercises a
diagnostic**: half its expected output is on stderr, and the claim is about which
channel each half arrives on.

```datalog
step("a", "b", 3).  step("b", "c", 4).  step("c", "d", 5).

% value creation outside every positive cycle — nothing reads `doubled` back,
% so its extent is fixed before anything recurses
doubled(X, Y, D) :- step(X, Y, C), D = C * 2.

% recursion with no value creation at all: the classical Datalog case
reach(X, Y) :- step(X, Y, _).
reach(X, Z) :- reach(X, Y), step(Y, Z, _).

% value creation *inside* a positive cycle — correct here, and on every acyclic
% graph, which is why this warns instead of being rejected
path_cost(X, Y, C) :- step(X, Y, C).
path_cost(X, Z, C) :- path_cost(X, Y, C1), step(Y, Z, C2), C = C1 + C2.

?- reach("a", To).
?- doubled("a", "b", Twice).
?- path_cost("a", "d", Total).
```
stdout:
```
reach("a", "b").
reach("a", "c").
reach("a", "d").
doubled("a", "b", 6).
path_cost("a", "d", 12).
```
stderr:
```
warning: value-creating recursion: `path_cost` grows by arithmetic (`C`) inside
the positive cycle `path_cost -> path_cost`; it terminates only while `step` has
no cycle reachable through this rule
```

*Resolved (§2/§6/§10/§15, 2026-08-18):* the certified half is silent and the
uncertified half runs, answers correctly, and exits 0 — the warning is the whole
of the engine's objection. `doubled` is the discriminator the example exists for:
it computes exactly as `path_cost` does, and says nothing, because no rule feeds
it back. What the example cannot show is the timing, since a program that answers
has necessarily terminated; `tests/programs/nonterminating.dl` carries that half,
and only a live process can observe it.

### 16.13 The caller's contract — a check whose answer is the exit code

Run by `tests/system.rs::a_consistency_check_answers_through_the_exit_code` over
`tests/programs/16_13_consistency.dl` and its violated twin. **The first example
whose subject is not the output but the code**, so what it demonstrates is only
visible to a caller that reads `$?`.

```datalog
assigned("alice", "mon", "open").
assigned("bob",   "mon", "close").
assigned("alice", "tue", "open").

% the violation pattern, an ordinary rule
double_booked(P, D) :- assigned(P, D, S1), assigned(P, D, S2), S1 != S2.

% the check: a negation-only body, which answers `holds(true).` when it holds
?- not double_booked(_, _).
```
stdout, and the code:
```
holds(true).
$ echo $?
0
```
The same program over a roster where `alice` works both `mon` shifts prints
nothing and exits **1** — the answer is *no*, and no rows is how §14 says it.

*Resolved (§12/§14/§15, 2026-08-18):* a constraint needs no construct. The
language already writes the check, and what was missing was the exit code — so
the shell idiom means what it looks like:

```sh
datalog roster.dl -q 'not double_booked(_, _)' && deploy
```

Phrase it **affirmatively**, as here. Every error code is `≥ 2`, so `&&` cannot
fire on a program that failed to compile, while the inverted `-q
'double_booked(P, D)' || deploy` deploys on a syntax error exactly as readily as
on a clean roster.

### 16.14 Temporal values — a typed date column, its arithmetic, and a period key

Run by `tests/system.rs::temporal_program_types_dates_and_groups_by_period` over
`tests/programs/16_14_temporal.dl` and `tests/programs/data/tickets.csv`:

```
id,opened,closed
1,2026-06-28,2026-06-30
2,2026-07-02,2026-07-05
3,2026-07-20,2026-07-21
```

```datalog
% §13 types both date columns with no explicit schema — the cells are ISO-8601
import "data/tickets.csv" as ticket.
% the extraction and truncation relations are gated; without this line `month`
% and `truncate` are ordinary undefined predicates (§12 says so, with the fix)
import "std/time".

% §8's algebra: point - point is a vector, and the divisor is where the unit is
% named. There is no `as number` to get this wrong with.
days_open(T, N) :- ticket(id: T, opened: O, closed: C), N = (C - O) / @1d.
slow(T)         :- days_open(T, N), N > 2.0.

% a date range filter, with no cast and no string comparison standing in for one
recent(T) :- ticket(id: T, opened: O), O >= @2026-07-01.

% extraction reads a component; the same relation is a filter when its output
% position is a constant
june(T) :- ticket(id: T, opened: O), month(O, 6).

% a period key is one sortable value, so grouping is an ordinary join
month_opened(T, M) :- ticket(id: T, opened: O), truncate(O, month, M).
per_month(M, N)    :- month_opened(_, M), N = count { T | month_opened(T, M) }.
```
```
days_open(1, 2.0).
days_open(2, 3.0).
days_open(3, 1.0).
slow(2).
recent(2).
recent(3).
june(1).
month_opened(1, @2026-06-01).
month_opened(2, @2026-07-01).
month_opened(3, @2026-07-01).
per_month(@2026-06-01, 1).
per_month(@2026-07-01, 2).
```
(One query per relation, in the order above; answers are sorted within a query.)
*Resolved (§3/§4/§8/§13, 2026-08-19):* three primitive types with `@`-sigilled
literals; one heterogeneous arithmetic rule (points and vectors) in place of a
table to memorize; `duration / duration → float` as the only path from a duration
to a number, so a unit is always written down; ISO-8601 cells inferred temporal by
§13; and extraction/truncation as **relations from a gated `std` module**, which
is what makes `month` and `truncate` usable as names at all. *Still raises:* the
output above is a projection of every derived relation — a `truncate` result
printed as `@2026-06-01` is a *date*, and a reader who wants "June 2026" is
reading a start-of-period convention rather than a month value, the price of not
adding a granularity type (§17).

### 16.15 Provenance — "why not?"

```datalog
defined("a", "id42").
callsite("id42", "b_impl").
calls(A, B) :- defined(A, Id), callsite(Id, N), defined(B, N).

?whynot calls("a", "b").
% % whynot calls("a", "b")
% % not derivable
% % 0  calls(A, B) :- defined(A, Id), callsite(Id, N), defined(B, N)
% % 1    defined("a", "id42")
% % 1    callsite("id42", "b_impl")
% % 1    blocked at defined(B, N)
% % 1    repair: add defined("b", "b_impl")
```

Run by
`tests/system.rs::a_whynot_goal_names_the_literal_that_blocked_and_the_step_that_would_pass_it`,
which pins the block byte for byte.

**Why this shape and not "no rows".** A query returning nothing and a query whose
join silently connects two id-spaces print the same thing — which is the failure
`EXPERIMENTS.md` recorded on a real workload and could only catch by re-reading
the extractor. The trace names the literal the run actually failed at, under the
bindings that reached it, and one step that would pass it.

**A query cannot ask this**, which is why the form exists (§17, 2026-08-21). The
commonest why-not is about a query that *succeeded* — rows came back and an
expected one was missing — where the expectation appears nowhere in the program,
so only a goal naming the missing fact can carry it.

## 17. Decisions log & open questions

This section is an **append-only record**: history is what it is for. Amend
entries in place, never rewrite them — a rationale that turned out wrong is the
most useful thing here, because it shows where the reasoning misleads. Everything
outside §17 states present truth instead and merely *points* here (see
`docs/rules/editing-docs.md`).

**Amendment markers.** One vocabulary, so the sweep after a rule changes is a
grep and not a judgement call. Each goes in **bold italic** at the end of the
entry it amends, with a date and a pointer:

| marker | when |
|---|---|
| ***Falsified*** | a load-bearing premise turned out untrue (usually a `bugs/` file) |
| ***Superseded by …*** | still-true reasoning, but a later decision replaced the outcome |
| ***Amended …*** | the decision stands with its scope or detail changed |
| ***Reopened …*** | the decision stands and is *under review*: nothing is overturned, but it is not to be built on until the question it points at is answered |
| ***Consequences …*** | nothing changed — what it cost, whether the rationale held, whether the rejected alternative still looks rejected |
| ***Answered …*** | **open questions only** — the question is settled; say which decision settled it, in the affirmative or the negative, and what narrower question (if any) survives |

***Consequences*** is the one that needs deliberate effort: it has no triggering
change, so `AGENTS.md`'s session-end checkpoint prompts for it. It is also the only
marker that records a decision working out *well*, which the log would otherwise
never say.

### Decisions

- **2026-09-12** — **An import no goal reaches is not read, and the program is
  lowered before any import is** (§13; `api::lower_and_load`,
  `sources::load_imports_where`). Closes `bugs/012` for what lowering finds.
  - **Lowered first only when every data import names its columns** — a
    schema-less import takes its arity from its header, so such a program still
    reads every import before lowering.
  - **The early check is lowering, not typecheck** (the user's, same day). A
    declared type is not an inference constraint, so a fact-free typecheck
    rejects `T = A + @1d` over a declared `timestamp` that one fact makes valid
    (`bugs/014`). Declarations as constraints is a ROADMAP design item.
  - **A rejection over a partial load is re-checked over a full one.** Fewer
    facts can only make typecheck reject more, so a pruned run accepts and
    rejects exactly what a full load does.
  - **A skipped import is still checked short of reading** — format, feature, a
    missing local file; a URL is not probed. What moves: a lowering error now
    precedes a source error, and a bad cell in an unread file fails nothing.

  Guarded by B13's import half; mutations: skip every import, trust the partial
  rejection. `vs/base`: an unknown field 5.5 s / 1.10 GB → **0.00 s / 8 MB**;
  `schema/all.dl` counting 179,748 `var` rows 6.7 s / 1.64 GB → **0.71 s / 0.21 GB**.

- **2026-09-12** — **A rule no goal depends on is not evaluated** (§15;
  `lower::live_predicates`, `engine::eval_pruned`). Live is every predicate a
  query body or explanation goal names, closed under `collect_stratum_edges` —
  negated and aggregated edges included, since a walk over positive atoms prunes
  a relation read only under `not` or inside a set-builder and answers wrongly
  with no diagnostic. Three rulings, the user's, same day:
  - **Permissive.** A pruned rule cannot fail or hang the run: an unreached
    `Y = 100 / X` no longer exits 2, an unreached value-creating cycle answers.
    §9/§12 skip counts from a pruned rule go with it; they are post-evaluation.
  - **Static diagnostics stay whole-program.** Pruning is at evaluation, after
    `check_program` and §10's lint, so an undefined predicate in an unreached rule
    still warns — the blind-spot signal `code-analysis`'s layer gating reads.
    Pruning before typecheck was the rejected fork: it drops that warning.
  - **No goals, no pruning** — `datalog p.dl` stays a check that every rule runs.

  `RuleId`s and strata are untouched. Guarded by **B13**; its recorded mutations
  drop each strict edge from the closure. `vs/base`, `callreach.dl` beside a
  question that never reads it: 37.4 s / 4.20 GB → **3.1 s / 0.70 GB**.

- **2026-09-11** — **A program that only *reports* through provenance records
  only the derivations the reports read** (§9/§12/§15; `Provenance::Reports`,
  `Derivation::reports`). Third mode beside `Recorded` and `Unrecorded`, and an
  amendment to 2026-08-21's *provisioned by demand* rather than a new principle:
  the store was already gated on demand, and "skip but report" was demanding all
  of it to carry two counts.
  - **What it keeps** is a derivation with a premise that skipped an `absent`
    input (§9) or lost a conversion on data (§12) — exactly what
    `api::absent_skip_warnings` walks. **The deduplication key is unchanged**
    (fact + `Derivation`), which is the whole of what 2026-07-24 and 2026-08-16
    rest on, so every reported count is identical.
  - **Measured on `code-analysis`'s libraries over VS Code's `vs/base` (1.33M
    facts):** one cast rule, whose own work is one row per file, cost **+4.6 s and
    +1.36 GB** by turning the full store on for `checks.dl`'s other 60 rules.
    `checks.dl` is **9.3 s / 1.8 GB** against 13.8 s / 3.2 GB, `cohesion.dl`
    **28.2 s / 2.3 GB** against 35.3 / 4.2.
  - **It is not free on time — `coupling.dl` pays 27.5 s against 23.0 for half
    the memory (1.07 GB against 2.10).** A kept derivation is *moved* into the
    store and never dropped; a rejected one is dropped, and dropping it frees
    seven premise tuples and their strings. So the mode trades allocator work for
    residency, and wins outright only where the store it avoids was large enough
    to pay for itself. Both directions are reported here because the memory is
    what a 900 s, 4 GB cell is actually short of.
  - **Gating the walk on a static per-rule "can this report at all"** was built
    and **measured as noise** (9.4 s → 9.3 s) and removed — the same call
    `notes/profile-2026-08-20.md` made about hoisting `literal_order`.
  - **`?why` still records everything**, and anything but `Recorded` explains as
    `Unrecorded` — a partial store must not be read as a proof. The `?whynot`
    cross case tests `!= Recorded`, not `== Unrecorded`, or a program with an
    aggregate would have built a proof out of the derivations that happened to
    skip.
  - **The guard is E9**, extended to all three modes — but its third claim is
    stated over the **warnings**, not over `Derivation::reports`: an oracle built
    from the predicate under test agrees with it forever, and mutating `reports`
    to `false` was measured leaving such a claim green (`testing.md` E9).

- **2026-09-10** — **The skill extracts TypeScript itself: `ts-facts`, on a pinned
  TypeScript 6.0, emitting primitives and leaving the measures to Datalog**
  (skill; `recipes/typescript.md`). The source-analysis dogfood found every wrong
  answer in extraction; for TypeScript the checker resolves names, so the skill
  ships a resolved extractor rather than a method. Long form, alternatives and
  calibration in [`notes/ts-facts.md`](notes/ts-facts.md).
  - **TypeScript 6.0 in-process**, because 7.0 has only an unstable IPC API; the
    tool carries its own copy and reads projects written for 7.
  - **`src/schema.ts` is the one home of the schema**; the writer validates
    every row against it and generates `schema/*.dl` and `SCHEMA.md`.
  - **Ids are keyed by declaration position, in path order**, so one id-space
    spans every relation and every tsconfig — trap 2 cannot happen by construction.
  - **Primitives, not verdicts**: dispatch kinds rather than a call graph,
    Doop-style value flow rather than points-to; `lib/` derives the rest, split
    by cost. The property layer (`testing.md`, *ts-facts*) found three extractor
    defects on its way in; real code found two more, and two engine defects
    (`bugs/resolved/010`, `011`).
  - ***Amended 2026-09-11*** — **whether an import runs is the emitter's
    answer** (`imports.runtime`, `notes/ts-facts.md`). `runtime_dep` had been
    "not `import type`", but TypeScript elides any import whose bindings only
    annotate, so it over-counted on every project not written under
    `verbatimModuleSyntax` — the user's catch, not a test's. P8 restates the
    elision rule as its oracle.
  - ***Amended 2026-09-11*** — **it moved out of the skill**, into its own project
    and skill, `../code-analysis/`, renamed `code-facts`; the datalog skill's
    `SKILL.md` and `recipes/` are back to `7f3e998`'s bytes, so the experiments'
    briefing is what it was. Why: `../code-analysis/decisions.md` 2026-09-11.

- **2026-08-25** — **A diagnostic carries a stable code, and the code exists
  where the *fix* differs in kind** (§12; `ROADMAP.md`, *Machine-readable error
  taxonomy*). The last piece of S3: an agent had four things to branch on, and
  all four named the stage that rejected the program rather than what was wrong.
  The census, the folds, and the alternatives are in
  [`notes/error-codes.md`](notes/error-codes.md); **38 codes over 153 emission
  sites**.
  - **The unit is the fix, not the message and not the site.** Which field is
    unknown is what a message is for; *the named arguments do not match the
    schema* is what a code is for. Applied both ways: `Semantic` split into
    fifteen, and four field diagnostics folded into one.
  - **The code is a required constructor argument**, which is the span work's
    lesson taken literally — an optional field is how 113 sites carried no span
    for a month while §12 called errors structured. The compiler is the only
    sweep that does not miss one, and paying it is what produced the census.
  - **No generic fallback**, which the session planned and then found no tail
    for. A generic code is where the next diagnostic goes without thinking.
  - ***Consequences 2026-08-25***, within the hour: **one fold was wrong, and the
    crate's own test found it the moment it stopped reading prose.** The
    conversion table classifies *no such conversion* and *would lose the value*
    differently, and the messages had carried that in the `"type error: "` /
    `"conversion error: "` prefixes the code was replacing — so collapsing them
    into `unsupported-conversion` lost a distinction a consumer was already
    making. `lossy-conversion` split back off. The fold rule is not
    self-applying; checking it against an actual consumer is what applies it.
  - **The category stays.** A code implies its category, so rendering both is
    redundant for a machine — but the category is what a human reads first, and
    an unrecognised code has to fall back to something.
  - *Rejected:* numbered codes (`E0499`) — a name an agent can read is worth more
    than a namespace with no collisions, and nothing here is dense enough to need
    numbering. *Rejected:* `&'static str` codes — a typo would be a new code,
    silently, and the contract is that the set is enumerable.

- **2026-08-24** — **S3 is the last criterion, and what scopes it is spans on
  113 error sites** (§1/§2/§12; the item is `ROADMAP.md`, *Machine-readable error
  taxonomy*). S1 was measured the same day, so v1 no longer turns on running an
  experiment; it turns on whether *"repairable from the diagnostic alone"* holds
  for the stage where an agent actually gets stuck.
  - **Re-measured, not re-read.** §2 and §12 both carried a 2026-08-18 count of
    46 `Error::semantic` and 33 `Error::source` sites with no span. It is now
    **77 and 36** — still 0%, against 3 of 3 for each of `Error::lex` and
    `Error::parse`. The proportion held while the absolute number grew by 43%,
    which is the useful part: **this gap widens on its own**, because every new
    semantic check is written at a site that has no span threaded to it.
  - **What stands in for a location is naming** — a relation, a variable, a rule
    index. `variable Q in rule 0` is findable by counting rules. That repairs a
    short program and degrades with length, so S3 reads *met for lex/parse* not
    because the semantic diagnostics are bad but because they point at a name
    instead of a place.
  - **The test already exists**, in the other project: three of the five
    malformed programs pinned by the harness's reference corpus are `Semantic`
    and therefore spanless, so their pinned diagnostics gaining a position is the
    observable that closes this — a criterion checked by a corpus rather than by
    reading the source again. — `../experiments/reference/malformed/`.
  - ***Amended 2026-08-24*** — **built, and the design pass this entry warned
    about turned out to be a smaller thing than the site count implied.** All
    five malformed pins now carry a position, not the three this asked for. The
    113 sites were the wrong unit: an error is raised deep and *caught* shallow,
    so the spans attach at a handful of frames that still know which clause,
    premise or import is being worked on — one wrapper around the evaluator's
    per-literal recursion covers `engine/`'s 35 sites, one stamp in
    `load_imports` covers `sources/`'s 29. Six of the 77 were `naive.rs`, which
    is `#[cfg(test)]` and reaches no user. What *was* a design pass is the
    cross-file question below, which the count did not show at all.
  - ***Consequences 2026-08-25*** — **the lesson generalised, and paid for
    itself the next day.** The code vocabulary took the opposite shape from the
    spans on purpose: a span attaches at a *frame*, so a wrapper covers dozens of
    sites, but a code is a claim about *this* diagnostic and cannot be stamped in
    bulk. Making it a required constructor argument meant reading all 153 sites —
    which is the census the vocabulary was derived from, so the cost bought the
    design rather than merely paying for it. This entry's other claim also held:
    S3 was closed by the corpus, not by re-reading the source, and the pins moved
    twice in two days.

- **2026-08-24** — **Spans resolve at the boundary, and stop at the file edge**
  (§12; `error.rs`, `api.rs`, `resolve.rs`). Lowering, inference and the
  evaluator never hold the program text, so they record a span
  (`Error::at_span`) and `run_at_reporting` — which does hold `src` — resolves
  every stage's errors to a line and column once. Threading `&str` through the
  three of them was the alternative, and it would have put a source-text
  parameter on functions that have no other use for one.
  - **A span is only a place while there is one file.** `resolve.rs` splices
    modules in and its spans stay per-file byte offsets, so resolving a
    spliced-in statement's span against the root text yields a confidently wrong
    line — the same class of defect as `bugs/008`, freshly manufactured. So the
    boundary **drops** the span when the program had more than one file, and
    claims nothing. Rejected: rebasing every imported file's spans into one
    virtual text, which is the real fix and wants the `origins`/`files` side
    tables `resolve.rs` already carries for it. Filed rather than built —
    `ROADMAP.md`, *per-file error attribution*.
  - **`Error::or_span` is why the evaluator needed no per-site edit.** A runtime
    error unwinds through every enclosing literal, and the innermost frame is the
    one that knows the place, so each frame offers its span and the first offer
    wins. Attaching unconditionally would have overwritten the good span with the
    outermost one, which is the bug this method exists to not have.
  - **Measured, not asserted:** 15 of 15 constructed diagnostics — one per family
    across type inference, safety, stratification, imports, syntax and runtime
    arithmetic — carry a line and column. It was 3 of 3 for lex/parse and 0 of
    113 elsewhere the same morning.

- **2026-08-24** — **A diagnostic may not assert what nothing read**
  (§4/§12; `bugs/resolved/008`, property **C15** in `testing.md`). The
  declared-vs-inferred sweep is skipped once inference is poisoned, and its
  message re-derives the column's type from the facts before saying *"its values
  are"*.
  - **The blanket guard over the poison bit**, as the bug file recommended:
    `set_type` leaves a class carrying the incumbent type *without* merging, so a
    bit would need maintaining at two sites, to report more in a program with a
    rule error and a genuine column error at once — a shape nothing suggests is
    common.
  - **The bug's acceptance criteria were incomplete, and fixing it found the
    rest.** Inference reaches a column from rules as well as from facts, and
    `declare p(x: int). s("a"). r(S) :- p(x: S), s(S).` claimed *"`p.x` … its
    values are string"* about a relation holding **no facts at all**. The
    suppression does not touch that one: there is no other error to suppress.
    Both halves are the fix, and reverting either reddens something different.
  - **What it cost §4:** nothing. The declared-type check is right to exist; it
    was running on input already known bad, and describing a unification result
    as a property of the fact table.

- **2026-08-21** — **The asking form, and the sigil's real job** (§5/§11/§14; the
  flows, the rejected alternatives and the two-run argument are in
  [`notes/provenance-asking-form.md`](notes/provenance-asking-form.md)). The
  three-way session `ROADMAP.md` sequenced — in which the asking form and "does
  the derivation store earn its cost" turned out to be **one decision**.
  - **Two sigils; a query cannot stand in for them.** The dominant why-not shape
    is a query that *succeeded* — rows came back with one expected fact absent
    among them — so the expectation exists nowhere in the program, and only a form
    naming the missing fact carries it. §16.13 forecloses the implicit version
    besides: silence-plus-exit-1 is the designed answer for *no*.
  - **The sigil's real job is provisioning, not the cost hint.** After the fixpoint
    the engine knows whether the fact holds and needs no hint; *before* it, the
    sigil is the only thing that says whether the run needs a derivation store. So
    `?why` records and `?whynot` does not, and the cross case re-runs the
    **fixpoint only** — reusing the lowered `ir::Program` — so 2026-08-16's union
    survives intact. Rejected: union provisioning (it overpays on the commonest
    explanation run) and per-sigil with no re-run (one form could then only answer
    one way, which is what that ruling exists to prevent).
  - **Explanations are exit-code-neutral, and ride inside `-q`.** Appending `?why`
    to a `&& deploy` pipeline changes neither its fact stream (E5) nor its branch.
  - **The store's cost is answered by proportioning it, not by replacing it**:
    gated, it is paid by the run that asks. Backwards extraction demotes to a
    post-v1 optimisation of the explaining path alone.
  - ***Consequences 2026-08-21 — built the same day, and the measurement holds.***
    On a 400-node sparse closure: no goals **0.23 s / 44 MB**, `?whynot` over a
    fact that does not hold **0.23 s / 44 MB**, `?why` **0.55 s / 202 MB**,
    `?whynot` over a fact that *does* **0.79 s / 219 MB** — 78% of peak RSS and
    58% of wall clock, matching `notes/profile-2026-08-20.md`'s projection, with
    the per-sigil precision visible as the second row. The scratch build that
    profile needed is obsolete: the A/B is now two ordinary invocations.
  - ***Amended 2026-08-21 — "the goals decide" was too narrow, and a test found
    it, not the reasoning.*** §9's absent-skip count and §12's conversion-loss
    count for a **rule** site are read back out of the recorded premises
    (deduplicated by rule instance, because a counter beside the fixpoint would
    count a rediscovered instance twice). So *skip but **report*** is a provenance
    surface that predates the asking form, and a program with an aggregate or an
    `as` in a rule body provisions the recorder whether or not it asks anything —
    `Program::reports_through_provenance`, shared with the scan that reads them so
    the two cannot drift. The claim that the three maps are "provenance-only" is
    true of *answers* (E9) and was never true of *warnings*. A **query**'s
    aggregate is unaffected: `answer_reporting` hands its premises straight to the
    caller and nothing is stored.
    - ***Amended 2026-09-11:*** it provisions `Provenance::Reports`, not the full
      store. The amendment above was right that the reports are a provenance
      surface and wrong that they need the whole recorder — they read two fields
      off the premises of the derivations that skipped, and a program can skip in
      none of them and still pay for every fact it derives. See this section's
      2026-09-11 entry.
  - ***Consequences 2026-08-21 — a repair could fail to repair.*** E10's *a repair
    must repair* clause fired on its first run: a blocked pattern with a slot
    bound to `absent` rendered as `repair: add p(…, absent)`, and asserting that
    row would not advance the rule, since `absent` unifies with nothing (§4).
    `Repair::AbsentKey` is the arm that replaced it. The design said "a repair is
    a step, not a promise"; what it had not said is that a step must at least be a
    step.

- **2026-08-21** — **A proof line carries its depth twice, because it has two
  readers** (§11 has the form, §16.6 the worked block, `src/print.rs` the code).
  Closes the *rendering* half of the 2026-08-16 surface decision; the form that
  **asks** is unbuilt, so nothing outside the library reaches it yet.
  - **The integer is the structural channel; the indentation is the human's.**
    Parent/child is the entire content of a proof, so it cannot rest on the least
    salient one. Box-drawing was the candidate and fails on the reader that
    matters: `│`/`└─` mean something to an eye tracking a column down a page and
    nothing to an agent reading a linear token stream, where no column exists.
    Bare indentation is worse again — depth becomes a whitespace-run *length*,
    compared across distant lines. **E8** is what makes the number load-bearing.
  - **The cited rule is the lowered one.** Premises align with the *lowered* body,
    so a source slice would list a body whose literal count does not match the
    premises under it.
  - **No elision, no sharing, no "1 of N"** — §15's no-budget rule applied, and the
    omission that keeps this indifferent to whether the recorder survives the
    derivation-store question (`ROADMAP.md`). It is why rendering could go first.

- **2026-08-21** — **The relation is already an index: a bound prefix is sought,
  not scanned** (§15/engine; the argument in full is `src/engine/seek.rs`'s module
  header, the numbers are `ROADMAP.md`'s *Performance* item).
  - A relation's `BTreeSet` is ordered by column, so the tuples an atom's
    constants and bound variables can match are one contiguous range. The
    positive-atom arm and the anti-join seek it; an aggregate goal inherits it,
    its sub-join running through the same arm with the group keys bound. Up to
    **13.4×**, and an *exponent* on three shapes; peak RSS unchanged.
  - **Exact, not approximate**: `Value`'s `Ord` agrees with its `Eq` (2026-07-19
    float totality) and `unifies_with` is `==` off `absent`, so the range applies
    the predicate the scan applied. Both callers still re-check every candidate,
    so an over-yield is invisible and only an under-yield loses answers — which is
    why the B1 differential is **not** the guard here. **B12b/B12c fail if
    contiguity breaks**, B12a if the range does.
  - **The `absent` asymmetry.** An atom with a known-`absent` position is
    *impossible* and gets no range; a refutation compares structurally
    (2026-07-29), so `absent` is a legal key there. Two prefix builders, one per
    §4 notion of sameness.
  - **Rejected: secondary indexes over arbitrary binding patterns.** They would
    also serve a bound column that is not *leading* — worth 29× on a 3-way join
    written in the pessimal order — at a second copy of every relation, on top of
    a recorder already 78% of peak RSS. Filed on `ROADMAP.md` with that number.
  - ***Consequences 2026-09-11 — the rejected half is what a real corpus hit, and
    the program could pay it instead.*** On VS Code's `vs/base` (1.33M facts)
    every `code-analysis` library over ~30 s was over it for this reason alone:
    `count { E | flow_node(id: E, fn: F, kind: entry) }` binds columns 1 and 2 and
    leaves column 0 free, so it scanned 108,597 rows per function, 13,983 times.
    **`checks.dl` went 199.6 s → 13.7 s on two projection rules**, answers
    byte-identical, and `coupling.dl` 330 s → 23 s on four. So the 29× estimate
    was if anything low — and the fix a program can make for itself is one rule
    per re-keying, which is why the index stays rejected and `code-analysis`'s
    `lib/keys.dl` states the rule for a reader instead.
  - ***Consequences 2026-09-12 — "the program can pay it instead" has a boundary,
    and a second workaround found it.*** A 4.04M-fact corpus hit a different
    engine gap: every rule in a program is evaluated whether or not a goal reaches
    it, so `modgraph.dl` computed a 17.45M-pair closure for importers that read
    only `dep`, and `orient.dl` was OOM-killed at 21 GB. The program *could* pay
    it — and did, by splitting `lib/reach.dl` out — but the price was not one rule:
    it changed what every importer imports, two reference documents and the bench,
    and left the file unable to hold a relation that belongs in it. **Re-keying is
    a rule a program writes; restructuring a library is a layout change forced on
    every consumer.** So this one is being fixed in the engine instead — rule
    pruning is `ROADMAP.md`'s next item — and `code-analysis` carries an item to
    unwind `reach.dl` once it lands. The 2026-09-11 note stands for re-keying; it
    does not generalise to any gap a program can technically work around.
  - ***Consequences 2026-09-12 (later still)*** — the unwind landed the same day:
    `modgraph.dl` holds the closure again, answers byte-identical on `vs/base`, and
    a program asking only `dep` over a synthetic 2,000-file import cycle costs
    0.03 s where the unpruned engine took 32.7 s. The layer change did not have to
    outlive the gap that forced it.

- **2026-08-20** — **An aggregate is a fold over a multiset, so only its result
  is defined** (§9). `fold_aggregate` sorts its present values into §14 order
  before folding, closing `bugs/007`: witnesses used to arrive in
  whatever order the goal's literals were scheduled in, which reached the answer
  wherever the fold is not associative over the value type.
  - Sorting picks *an* association, and §14's order is by value, not magnitude —
    deterministic without being good. So **floats sum with Neumaier
    compensation** and **ints and durations accumulate in `i128`**: overflow is a
    property of the total, not of an intermediate nobody wrote. §8's binary `+`
    keeps its operand-pair overflow — there the pair *is* the program.
  - **Rejected: documenting the restriction** (`007`'s cheapest candidate), which
    keeps a case where reordering a conjunction changes an exit code — ruled out
    by the 2026-07-25 entry below. **Rejected: sorting alone**, which canonises
    `0.0` for `{ 1e16, -1e16, 0.1 }`: the worse of the two answers the defect
    reported.

- **2026-08-19** — **Temporal values: three types, one algebra, and the unit at the
  divisor** (§3/§4/§8/§9/§13). The design space, the measurements and the four
  rejected alternatives are in
  [`notes/temporal-values.md`](notes/temporal-values.md).
  - **`date`, `timestamp`, `duration`**, `@`-sigilled literals. The sigil is
    **decided by §14's closure**, not by taste: output must re-parse, so a
    computed date needs a spelling. **Rejected: bare ISO**, which today lexes as
    `2026 - 08 - 19` and evaluates to `1999` — accepting it changes the meaning of
    existing arithmetic silently.
  - **Points and vectors, stated as a rule** rather than a table, and it is the
    language's first heterogeneous operator typing. `duration / duration → float`
    is the only path from a duration to a number, so a unit is always written
    down — the sibling engine's `172800000` finding excluded *by construction*
    rather than by a paragraph in a guide.
  - **`timestamp` is civil, durations are exact.** No zones, no `@1mo`/`@1y` — and
    the absence of a month unit is what leaves `m` unambiguously minutes.
  - **CSV infers temporal**, and the §13 anchor property is restated ("each cell
    delimited as its type requires") rather than broken: `alice` → `"alice"` was
    already the same rule.
  - **`timestamp as date` stays a lossy error**, with `truncate` named as the fix.
    The one tension resolved *for* an existing rule; §16.14 records what that
    costs. Named tests: `system::temporal_program_types_dates_and_groups_by_period`
    (§16.14), the **T3** unit property (`testing.md`).
  - ***Consequences 2026-08-19 — built the same day.*** Four things the entry did
    not know. **The cost landed in the type checker, not the value model**: an
    operator whose result is not its operands' class cannot be a union-find edge,
    so arithmetic and `sum`/`avg` became *deferred* constraints resolved to a
    fixpoint — and the fallback for a constraint with no temporal type in
    evidence is the old homogeneous rule, which is why 400 existing tests did not
    move. **`avg` over durations is a duration**, which the entry would have got
    wrong: it is not an exception to "`avg` is `int → float`" but the vector
    rule's other half, since dividing a fold by a count is scaling. **A date
    shifts only by whole days** — the sub-day case had no answer in the design
    and takes the same refusal `timestamp as date` does. And **`INTERVAL` no
    longer becomes a duration**: §13 said "when it is exact", but reading
    DuckDB's `1 day 02:00:00` means a second duration grammar, and one grammar
    is what makes reading and rendering inverse. A duration is now never
    inferred from any source, which is the simpler rule the design should have
    reached on its own.
  - ***Consequences 2026-08-20 — what the generators did not follow.*** The value
    layer shipped with T1–T6 and the *existing* suite stayed on five types:
    `arb_constant` was never widened, so A4's cross-type order, D1/D2/D3 and the
    whole B/C/E series certified five-eighths of the sentences they state. Not
    caught by review — measured, by swapping `Date` and `Bool` in `ir::Value`'s
    variant order, which A4 passes on the old generator and fails on the new. §8's
    temporal cast rows and §9's duration folds had no property either; adding them
    found **no defect**, so the design was right and only the coverage was thin.
    The general rule this pays for is in `testing.md`: when the language widens,
    the generator is the thing that silently does not.

- **2026-08-19** — **A builtin is a relation from a gated `std` module**
  (§8/§12/§13). Long form in
  [`notes/temporal-values.md`](notes/temporal-values.md).
  - **The gate buys the short names, not safety.** `year`, `month`, `day` are the
    names a program wants *and* the names a data column has; reserving them would
    break every program with a `year` column. Unimported they stay ordinary
    relations, so nothing existing changes meaning; imported, a collision is an
    error naming both origins. **Rejected: silent user-wins shadowing** — the
    silent-meaning-change class this repo has already paid for twice.
  - **`std/` is a reserved virtual path prefix**, needing no grammar: a module
    import is already the no-`as` shape (§13). A real `./std/` is a loud error.
  - **The mechanism was designed past `std/time`, deliberately.** It answers the
    blocking half of the deferred builtin-scalars question — a body literal
    opening with an identifier can only be an atom, so the `ident (` ambiguity
    that ruled out `float(A)` never arises — and settling only what temporal
    needed would have fixed that shape by accident.
  - **The cost is a failed first attempt**, so §12's did-you-mean carries the
    import line and ships with the module rather than after it.
  - ***Consequences 2026-08-19 — built the same day.*** The mechanism turned out
    **cheaper than its own design**: lowering a builtin to the `=`-assignment
    `Y = year(D)` meant the scheduler, §10's safety rule, the
    assignment-vs-filter decision and the termination classifier all applied
    unchanged — `day(D, 15)` is a filter for the reason `X = 5` is, with no
    separate form. The one place a new rule was needed is the *unit* argument of
    `truncate`, checked statically so a typo costs one message rather than one
    per row. The entry's claim that a relation dodges the `ident (` ambiguity
    held exactly.
  - ***Consequences 2026-08-20 — the exemption now has a test.*** §10 exempts a
    `std` relation from value-creating recursion as a finite-domain map. That is
    a termination-soundness claim carried by prose for a day: C10's ten shapes
    contained no builtin. `ArithShape::StdBuiltin` puts `truncate` in a positive
    cycle, and removing the exemption reddens C10's **non-vacuity guard and only
    it** — the property itself skips warned programs, so an exemption needs a
    check watching the *classification*, not the fixpoint. The finite-domain
    reasoning held; what it lacked was a witness.

- **2026-08-18** — **The caller's contract: the exit code answers the question**
  (§12/§14/§15). Five axes, their evidence and the alternatives that lost are in
  [`notes/callers-contract.md`](notes/callers-contract.md).
  - **A constraint is not a construct** — a negation-only body already answers
    `holds(true).` (§14), so only the code was missing.
  - **`0` rows · `1` no rows · `2` did not answer**, grep's vocabulary, **stated as
    a range** so a later code refines `2` rather than reinterpreting it. Program
    errors moved `1 → 2`; §12's `category` splits them finer than a code could.
  - **stdout stays a pure fact stream**, and **withholding is specified and
    unnumbered**: §17 2026-08-16 named three live truncation sources and,
    measured, **none is one** — so a code with no trigger would be the worse error.
  - **A failed conversion now reports** *malformed, not missing* (§4/§12); so does
    a query-level aggregate skip (§9), silent before. A **guarded** conversion
    (§16.9's idiom) stays silent — a diagnostic firing on a correct program is the
    hazard `notes/tsdl-cross-project-review.md` measured.
  - Named tests: `system::a_consistency_check_answers_through_the_exit_code`
    (§16.13), `pipeline::a_guarded_conversion_is_not_warned_about`.

- **2026-08-18** — **§1 is written, §2 is ratified, and v1 has a definition**
  (§1/§2; the per-item ruling is in [`notes/v1-scope.md`](notes/v1-scope.md)).
  Prompted by the stock-take of the same day, which found that "are we feature
  complete?" was unanswerable against an unwritten §1.
  - **v1 = S2–S6 hold and S1 has been *measured* at least once.** S1 is the
    project's own hypothesis, and the criterion is deliberately that the experiment
    was **run**, not that it came out favourably: a negative result is a finding,
    whereas shipping v1 having never measured would leave the three pillars — cited
    throughout this log as settled authority — resting on an unmeasured premise.
  - **Three of §2's four principles ratified *scoped*, not as written.** A
    principle is a claim about the implementation, so each was checked first. The
    sharp one is *explainability and the agent API are first-class*: true of the
    engine (all derivations, unconditionally, no flag) and **false at the surface**
    (`?why` unbuilt, `RunResult` carries no derivations, the CLI's only flag is
    `-q`). Ratifying it as written would have made §2 assert what §11 and
    `src/lib.rs` contradict. **Rejected: leaving them candidates** — four years of
    "candidate" is how a principle stops constraining anything.
  - **The criteria were checked *against* the ruling, not only used for it.** The
    test was: if an item's ruling cannot be derived from a stated criterion, the
    criterion is missing rather than the ruling wrong. It fired once — S4 was
    written "met" and corrected to "met **except dates**", which is what makes
    temporal types v1 rather than a preference. — §1/§2.
  - ***Consequences 2026-08-21*** — **S5 is met**, and the scoping above was the
    thing that made it actionable: naming the surface gap in §2 rather than
    ratifying the principle as written is why the asking form was a tracked
    criterion instead of an aspiration. The scoping sentence itself is now
    narrower — the engine/surface split closed, and what is left un-first-class is
    the *machine-readable edge* (no JSON for a proof or a trace, no stable
    diagnostic code). S1 remains the only unmet criterion, which is what v1 now
    turns on.
  - ***Consequences 2026-08-24*** — **S1 has been measured, and the criterion
    held up exactly as written.** The result is null (§1), and the deliberate
    choice above — that the experiment was *run*, not that it came out favourably
    — is what let it be recorded as a finding instead of a reason to keep the
    grid running until it said something. What the criterion did **not**
    anticipate: the engine arm reached for the engine in 9 of 56 cells, so a null
    on *supplying* the engine is most of what a first measurement can buy. That
    is a gap in the instrument's slate, not in the criterion, and it is where the
    next measurement goes. — `../experiments/decisions.md` 2026-08-24.

- **2026-08-18** — **§6 accounts for the whole language, and a run that errors has
  no model** (§4/§6/§8/§9; long form in
  [`notes/declarative-semantics.md`](notes/declarative-semantics.md)). The
  extension is otherwise *descriptive*: §4, §7, §8, §9 and §10 had already ratified
  every rule it states, which is what the 2026-07-29 deferral was waiting for.
  - **`T_P` needs a match relation, not substitution.** `p(absent)` is in `I`, yet
    a *bound* occurrence of `X` must never match it: **binding is total, matching
    is semantic**. Holding those two apart is what makes `p(X), p(X)` select
    strictly less than `p(X)` and `p(X), not p(X)` derive nothing — one split, not
    two rules.
  - **The decision — an error yields no model at all**, not a smaller one and not a
    hole. It is the limiting case of the truncation contract (2026-08-16), and it
    stops the semantics erasing §8's line between an *unrepresentable* conversion
    (`absent`) and a *lossy* one (an error). It describes `eval`'s `Result<Model>`
    rather than changing it, so no `src/` change; **B1**'s error path is the guard,
    which pins *whether* a run errors and deliberately not *which* error it names.
    **2026-08-16 had already measured and relied on this** — that `eval_expr`'s
    error path aborts the whole run rather than the row is what decided the cast's
    failure mode — so what was missing was only its statement as a semantics.
  - **Two finiteness claims had been one**, which is what `bugs/004` found without
    naming: every application is finite for every program, while the *fixpoint* is
    reached only inside §10's certified fragment. — §4/§6/§8/§9.

- **2026-08-18** — **Termination is *classified*, not enforced: the rule ships as
  a warning** (§2/§6/§10/§15; `notes/termination.md` for the proof and the four
  rejected alternatives). A program is **certified terminating** when no rule binds
  a head variable to an arithmetic-computed value while its head predicate lies on
  a positive cycle; a certified program has a finite Herbrand universe, which
  restores §6's finiteness argument and PTIME data complexity for that fragment.
  Everything outside it **still runs**. Closes `bugs/004`; ratifies §2's pillar.
  - **Reverses 2026-07-25's static *error*** (user call). `path_cost` accumulating
    a cost terminates on every acyclic graph, so rejecting it is a false positive
    on a property of the **data**. `bugs/004`'s criteria offered "rejected **or**
    documented as in-scope"; 2026-08-16 already discounted the DoS case; and the
    only checkable line between `nat` and `path_cost` over-accepts
    `p(N) :- p(M), q(_), N = M+1.`
  - **Taint is transitive, and the 2026-07-25 sketch was unsound without it** —
    `K = M + 1, N = K` binds the head from a bare variable and still diverges.
    Guarded by **C10** (`c10_a_certified_program_reaches_its_fixpoint`) and **C8**
    (`c8_the_taint_spellings_classify_alike`), both mutation-verified.
  - **Emission moved ahead of `eval`** (`api::run_at_reporting`): answers print
    after the fixpoint, so the diagnostic for a non-terminating program was itself
    never printed. **No budget ships** — `eval_capped` is C10's oracle and nothing
    else. — §2/§6/§10/§15.

- **2026-08-17** — **The named query desugars in *lowering*, and the guard's line
  is *defined*, not *mentioned*.** The two choices building the form above left
  open. `lower_query` synthesizes an `ir::Rule` over the body it has just lowered
  and leaves behind the one-atom query `name(<projection>)`, reusing the same
  `VarScope`. So `ir::Query` gained **no field** and `api.rs` gained **no arm**:
  the substituted-atom path settled earlier today already prints a one-atom
  query under its atom's own name, including the empty-projection `name(true)`.
  The brief's site list named `ir::Query`, which turned out not to be a site —
  carrying the name to the printer and desugaring to a rule are two different
  features, and only the second makes `eligible(N) :- adult(N, _)` legal.

  **The guard rejects a name the program defines or declares, and allows one it
  merely references.** Refusing a referenced-only name would make the sugar
  inexact: defining it is precisely what the equivalent hand-written rule does.
  Arity still clashes through `intern_checked`, giving the message a rule of the
  wrong width would give.

  Four mutations recorded (`testing.md` C8). Two are worth the log: a head one
  column short of the projection is caught by the **existing** IR well-formedness
  check, not by anything added here; and dropping the empty-projection `true`
  prints `ans().`, which §5 does not accept as an atom — so the argument that
  keeps the answer *re-parseable* is the same one §5 makes, now measured rather
  than argued.

- **2026-08-17** — **The answer shape: the positive atoms must account for
  *every* answer variable, and a body with none still owes a yes.** Settles the
  five axes reopened 2026-08-16, closing the question in the affirmative for
  synthesis (a query that cannot be named still answers). Set **equality**
  between the atoms' variables and the projection replaces the one-directional
  "every atom argument is projected" test, which was sufficient only while the
  rule also required a single-literal body. Three forms follow, stated in §14.

  Two things the question's framing did not anticipate. **The substituted form
  extends to ground conjunctions**, because with no variables there are no
  matched combinations to lose, so `?- p("a"), q("b").` can answer in real
  relation names — and multi-row multi-atom bodies cannot, since a filter that
  pruned rows leaves no trace in the atoms' tuples. And **the truth value needed
  a name of its own**: `answer(true).` is already reachable from a 1-column bool
  projection, so reusing `answer` would have made two meanings byte-identical —
  a collision tsdl avoids only because their `answer` is boolean-only. Hence
  `holds/1`, and silence keeps meaning *no* rather than gaining a `holds(false)`.

  **Rejected: a warning when a program reads `answer` facts.** Proposed in this
  same session as what would make keeping the name substantive, and withdrawn on
  the evidence: the hazard is a *two-run merge*, which inside a single run is
  indistinguishable from legitimate single-pipe composition, so the diagnostic
  would fire on correct programs and stay silent on the case that matters. That
  is the inverted `invisible-witness` warning recorded 2026-08-16, reached from
  the other side. The hazard stays documented in §14 and naming is its fix.
  `testing.md` **C8** carries the property; long form in
  `notes/query-answer-shape.md`.

- **2026-08-17** — **A query may be given a name where it is asked, and the name
  replaces the answer relation.** *Decided, not built — its own session.*
  `?- conflict: p(X), q(X).` is exact sugar for a rule whose head is the
  projection: `adult(N, A) :- person(name: N, age: A), A >= 18.` plus
  `?- adult(N, A).` So the arity is the projection's length — precisely the
  columns `answer/N` prints — and it is **always range-safe**, the projection
  being by definition the variables the body binds.

  A **language** form, not `-q` sugar: the name has to survive lowering to reach
  the printer, and recomputing the projection in `api.rs` would be a second
  classifier of the kind §13's lexer move exists to prevent — where it diverged
  the user would get a range-restriction error instead of an answer. The decisive
  argument for taking it at all is that it **fixes the projection hazard**, which
  2026-08-16 recorded as unsolved in both engines: named output wears no source
  relation's name, so nothing is silently narrowed. Naming stays **optional** —
  requiring it would make the language partial, and the unnamed filtered query
  already answers usefully — but it is the prerequisite for ever making an
  unnameable query an error, since recovery becomes "prepend a word" rather than
  "rewrite it as a rule". One guard: reject a name the program already defines,
  predicates interning by name alone.

  ***Consequences 2026-08-17*** — **built the same day, and the rationale held
  except in one place: `ir::Query` was not a site.** The name never needs to reach
  the printer, because desugaring to a rule makes the query a one-atom query and
  the shape rule already prints those under their atom's name. The "second
  classifier in `api.rs`" argument was right about *where* the projection must be
  computed and understated the conclusion — it is not merely that lowering should
  compute it, but that once lowering does, printing needs no change at all.
  §16.11, `tests/system.rs::named_query_program_publishes_its_own_relation`, and
  the entry at the top of this log.

- **2026-08-16** — **A failed `as` conversion is `absent` when there is no value
  to represent, and an error when representing it would be lossy.** The piece the
  2026-07-25 ratification left to implementation time. `"abc" as int` is
  `absent`; `2.5 as int` and `as float` above 2⁵³ are structured errors — so §8's
  already-ratified widening rule extends to the narrowing direction unchanged,
  and this adds one distinction rather than a second principle.

  The deciding argument is **expressibility, not reporting**: this language has
  no convertibility predicate — string operations were *rejected* (2026-07-27),
  not deferred — so under an error rule there is no way to write "the rows where
  `X` parses as an int", and a column with one bad cell has no valid query at
  all. `absent` puts the guard back in the existing vocabulary (`V = X as int, V
  is not absent`). Two supporting measurements: `eval_expr` is `?`-propagated, so
  an error aborts the run and emits *nothing*, not even facts already derived;
  and the exposure is one format, since Parquet/JSONL/databases carry their own
  types and only CSV arrives unclassified.

  **Rejected: a hard error throughout**, which matches §13's import `coerce`
  exactly and would have kept §8's failure rule single. It loses on the above,
  and the import precedent is weaker than it looks — an import schema is optional
  and applied once at load, where a cast is per-row and mid-fixpoint. **Rejected:
  a strict/`try_` pair**, which doubles the vocabulary for a distinction the
  `is absent` guard already expresses. **Rejected: truncating `float as int`**
  (SQL's answer) — it needs a rounding rule where exactness needs none.

  **The cost, recorded because it is real:** this reclassifies *malformed* as
  *missing*, and §17's usual mitigation does not come free — "skipped *and
  reported*" is `AggOutcome.skipped`, which rides on the **aggregate** literal;
  an `=`-assignment has no such report. Making the reclassification visible is a
  ROADMAP item, not something this decision inherited.

  The conversion **table** was settled in the same sitting (§8): text is the
  universal intermediary, `bool as int` and friends have no conversion at all,
  and the text direction reuses the literal-grammar classifier rather than
  growing a second one — so `"30" as int` and an imported `30` cell cannot
  disagree. That classifier moved from `sources/table.rs` to `lexer.rs` to make
  the sharing structural instead of a convention. — §4/§8/§13.

- **2026-08-16** — **Cross-project review of `~/code/tsdl`: six things declined**
  (survey; `notes/tsdl-cross-project-review.md`). That engine is this design run
  forward in TypeScript — it names this repo as prior art and derives its testing
  rules from our `bugs/` files — which makes it strong evidence and a poor
  authority: its forcing constraints are a browser tab, a host-supplied fact base
  and zero dependencies. Three decisions were taken (below) and one question
  reopened; these were **declined**, each because a premise of theirs is not ours:
  - **`FactSource`-only ingest**, which deletes §13 — a browser has no filesystem,
    and their own entry concedes imports are "genuinely convenient for a CLI".
  - **Dropping `ROADMAP.md` and `bugs/`** — the defect record is what taught *them*
    their four testing rules; location-is-status has closed six defects here.
  - **Naive-first evaluation** — correct where evaluator performance is a non-goal,
    which for us it is not; their own profile concedes the exponent is unchanged.
  - **Performance as an explicit non-goal**; **one `number` type** (they rejected
    our int/float split because JS has one type, which is not an argument here);
    their **zero-dependency, ES-library-only build discipline**.
  - ***Consequences 2026-08-18*** — **"performance is not a non-goal" was a
    decline, and is now a stated goal.** §1 promotes it from something this entry
    refused to adopt into one of two goals named beyond the pillars, on this
    entry's own measured evidence. Worth recording because the decline was the
    *weakest-looking* bullet here — declining someone else's non-goal asserts
    nothing on its own — and it turned out to be the one §1 could not have been
    written without.
  - ***Consequences 2026-08-17*** — **the naive-first decline is measured, and it
    was an exponent.** One corpus run on both engines
    (`notes/cross-engine-benchmark.md`): chain closure scales ~n^2.4 here against
    ~n^3.8 there, where the closure output is itself n². So "their profile concedes
    the exponent is unchanged" held, and the rejected alternative would have cost a
    factor of n rather than a constant. Two things the entry did not anticipate.
    The **front ends are within ~4×** (480k vs 110k facts/s), so nothing about the
    gap is attributable to the runtime choice outside the fixpoint — the decline was
    load-bearing exactly where it was aimed. And a comparison run for their sake
    found **two shapes where this engine is the worse one**, neither visible from
    inside this project: aggregation that does not scale with the aggregated
    relation, and a cyclic-graph cliff. Being "strong evidence and a poor authority"
    cuts both ways, and the second direction is the one that had not been used.

- **2026-08-16** — **An incomplete model does not answer, and the whole-model
  surface survives it** (§6/§9/§15; `notes/tsdl-cross-project-review.md`).
  Termination keeps the 2026-07-25 static rule as its *only* guarantee — **no
  budget, no fuel** — and gains the half that decision never stated: what the engine
  owes when the model is short anyway. Unblocks `bugs/004`.
  - **Truncation costs soundness and not only completeness**, and the route is the
    query. The store only grows, so a cut-short fixpoint holds missing facts and
    never false ones — but a query solved against it can be *wrong* rather than
    missing, because `not p(X)` over an incomplete `p` succeeds.
  - **So the split is by surface**: a whole-model dump survives truncation, answers
    do not. Where a relation is short by rows the discriminator is how the program
    reads it — **projected**, one row short and say so; **folded or negated**,
    withhold. That is §9's skip-but-report rule decided on a principle. Three live
    sources today, none a budget: an external signal, §9's skips, §13's cells.
  - **Rejected: a runtime budget** — tsdl ships one because a hung browser tab is
    its forcing case; a CLI has `^C`, so a slow program stays slow and the real
    exposure is a *hosted* surface (MCP, API harness — both parked). **Rejected:
    their five-way `verdict`**, a library's return field where ours must be an exit
    code and a stdout discipline.
  - ***Consequences 2026-08-18*** — **the three "live sources" were not live**,
    which the caller's-contract session found by measuring them: §13's cells are
    structured errors, the round cap is a test oracle, and §9's skips are absent
    *values* in present rows. The entry's rule turns on a distinction it never
    draws — **short by rows** is not **absent in a cell** — and only the first is
    truncation. So the contract is right and has nothing to fire on, which is why
    §15 now says so and the exit code stays unnumbered. What the entry did get
    exactly right is the *shape*: "an exit code and a stdout discipline, not a
    return field" is what shipped, and rejecting tsdl's `verdict` field held.
  - ***Consequences 2026-08-18*** — the no-budget argument **held, and did more
    work than it was written for**. Termination (2026-08-18) reused it verbatim
    against *rejection*, not just against fuel: if "a slow program stays slow, `^C`
    is the operator's" is right, then a non-terminating program is the operator's
    too, and refusing to run it buys nothing that a warning does not. The rejected
    alternative still looks rejected, and outside evidence arrived — Soufflé's
    `.limitsize` is documented as a *debugging* directive for inspecting a
    non-terminating program, which is this entry's reading of a budget reached
    independently (references.md group 6). The one thing the entry got slightly
    wrong: it called the static rule Termination's "only guarantee", and the
    guarantee that shipped is narrower and better stated — the rule certifies a
    fragment, and everything outside it is *classified* rather than prevented.

- **2026-08-16** — **The provenance query surface: one union, and a bounded
  why-not** (§11/§14; `notes/tsdl-cross-project-review.md`). Adopted from tsdl
  `spec.md` §13, which built both tiers. Design only — the ROADMAP item stays open.
  - **The sigil is a cost hint, not a selector**: a form that could only answer one
    way makes a reader know the answer before asking, so `?why` over a fact that does
    not hold is an ordinary question. **`unknown` keeps the other two honest** —
    over a model cut short, *not derivable* and *not derived yet* are the same
    silence, so an explanation is withheld exactly where an answer is (the entry
    above, reached from the other side).
  - **A near-miss is a rule, not a binding, and that is the whole bound.** One entry
    per rule whose head unifies, carrying the longest prefix any binding satisfies
    and the first binding reaching it; nothing truncates. It re-solves through the
    scheduler the fixpoint uses — an extractor choosing its own literal order would
    be a second evaluator (cf. 2026-07-25).
  - **A repair is a step, not a promise**, the literals past the block never having
    been evaluated; a blocked *derived* premise's repair is the next question to ask.
  - **Rejected: proof trees as facts** — closing the ROADMAP item in the negative. A
    proof tree is not a fact and joins a fact stream on no terms; it rides in `%`
    comments, so stripping them leaves byte-for-byte what the same program without
    its goals prints. That is a property, and the guard.
  - ***Amended 2026-08-21*** — the rendering it left open is settled (Decisions
    above) and built, and the split this entry drew held up: the shape decided here
    survived contact with the output unchanged. What it did not anticipate is that
    the two halves separate cleanly enough to **ship apart** — a renderer with no
    form to ask it is still testable, and still worth having, because it is the one
    piece the derivation-store question cannot invalidate. The guard named here is
    E5 and it is *still* blocked: byte-for-byte stripping needs program-level
    output, so what landed is **E7**, its lexical precondition.
  - ***Amended 2026-08-21*** — **the cost hint is doing a second job this entry did
    not know about.** Held to its own terms the sigil is decoration: the engine
    reads holds-ness off the model and needs no hint to dispatch, which is nearly
    an argument for one form. What it misses is that the distinction is needed
    *before* the fixpoint, to decide whether to record derivations at all — and
    holds-ness is not yet known there. The hint is therefore load-bearing, and the
    ruling it protects (neither form may answer only one way) is what forces the
    cross-case re-run rather than a partial answer (Decisions above).

- **2026-08-16** — **Three names, because one word doing two jobs needs a footnote
  at every use** (§4/§7/§11; adopted verbatim from tsdl `spec.md` §13). The
  missing-data value is **`absent`**; the premise justifying a negated literal is a
  **no-match pattern**; the explanation of a missing answer is a **failure trace**.
  Closes the ROADMAP rename item, which has carried §4's and §11's two standing
  disclaimers since the 2026-07-25 review.
  - **The sweep is the work, and it is a follow-on session.** Counted 2026-08-16:
    **31 sites over 8 files** — 21 code (`provenance::AbsentPattern`,
    `Premise::Absent` and their doc comments, in `provenance.rs`, `ir.rs`,
    `engine/mod.rs`, `engine/naive.rs`) and 10 prose (§4 ×1, §7 ×1, §11 ×3,
    `testing.md` ×3, `references.md` ×1, `ROADMAP.md` ×1). **Nothing frozen holds
    the old name** — no §17 entry and no `bugs/` file uses it — so the sweep is
    total, with no carve-out to argue about.
  - **Why verbatim rather than our own coinage:** both engines put the same kind of
    guide in front of the same kind of reader, so divergent vocabulary between them
    has a cost of its own. `bugs/resolved/003` is what a partial sweep costs.
  - ***Amended 2026-08-16 — done, and the count above was wrong in both
    directions.*** Measured while sweeping: **~40 non-frozen lines over 9 files**,
    not 31 over 8. More importantly, "**nothing frozen** holds the old name — no
    §17 entry ... uses it — so the sweep is total, with no carve-out to argue
    about" is **false**: six §17 entries hold it (the 2026-07-20 `AbsentPattern`
    entry, the 2026-07-19 `Premise` pre-decision, and four more). They keep it —
    an append-only record is precisely what a rename must not touch — so the
    carve-out this bullet denied is the one thing the sweep needed. The
    2026-07-20 entry carries the pointer. The lesson is not about counting: a
    scope claim asserted *from* a grep should have been checked *against* the
    document-kind table before it was written down as "nothing to argue about".

- **2026-08-03** — **The substituted-atom shape widens to one positive atom plus
  non-binding literals** (§14; *decided, not implemented* — ROADMAP item, C8
  property first). The atom form applies when a body has **exactly one positive
  atom** whose variables are **exactly** the answer variables; everything else
  still emits `answer/N`. The trigger was not the opaque name but a `bugs/005`
  re-run: `?- person("bob", 17).` prints the fact and `?- person("bob", 17), 1 <
  2.` prints **nothing** — this section's own "as written" principle, defeated by
  a filter rather than by hoisting. Variable-set **equality** is a new condition,
  not a relaxation: today's check is one-directional and suffices only because a
  one-literal body has no other binder; once an aggregate binds alongside the
  atom, the missing direction drops that column.
  - **`notes/query-answer-shape.md`** holds the boundary table, the Soufflé
    survey, and two rejected alternatives: adopting Soufflé's shape (it has no
    `?-`), and keeping `answer/N` *as* a projection marker — it never was one,
    the atom form having carried the same narrowing hazard since 2026-07-22. That
    hazard is pre-existing, is enlarged here, and cut in favour; §14 gained prose
    instead. Soufflé settled the division of labour: **inference is for reading
    one query's output, naming is for composing it** (§14).
  - ***Reopened 2026-08-16*** — user call, against the open question below. Nothing
    here is overturned and the reasoning is intact; what changed is that the surface
    this decides is the one the user is least willing to get wrong, and a second
    engine has since shipped the opposite answer (an `unnamed-answer` error, no
    invented relation name) with an argument this entry never weighed. **The
    widening is not to be implemented while the shape it widens is under review**,
    which is what this marker exists to tell a reader who arrives via the ROADMAP.
  - ***Amended 2026-08-17*** — the review closed in this widening's favour and it
    is now built, with its scope enlarged twice. "Exactly one positive atom" was
    the wrong bound: **ground conjunctions** substitute too, the single-atom limit
    having stood in for "the atoms determine the answer", which is what set
    equality actually says. And the entry's own prediction about the missing
    direction was **confirmed by test rather than by argument** — restoring the
    one-directional check is mutation M1 of C8's property and it drops a column,
    which is why the property's generator had to include a body binding a variable
    no atom mentions. What this entry did not foresee: two of its rows converge, so
    `?- V = 1 + 1, p("a", V).` and `?- p(X, 1 + 1).` now print identically, and
    `bugs/005`'s acceptance 3 moved to a case that still discriminates.

- **2026-07-29** — **The anti-join is a structural membership test** (§4/§7;
  `src/provenance.rs`). `q(X) :- p(X), not p(X).` derived `q(absent)` — P ∧ ¬P.
  Direction 1 of the three below: a negated atom **binds nothing**, so refutation
  asks whether a tuple is *in* the relation, and the blowup `absent ≠ absent`
  prevents is a property of joins bringing in new bindings. Joins keep the
  semantic notion; only the anti-join takes the structural one. Direction 2 would
  have bought idempotence by giving the blowup back; direction 3 is undecidable,
  `absent` being a value and not a type.
  - **The price, decided rather than absorbed:** a row whose key is absent drops
    out of "things with no …" — SQL's answer. Measured on the §16.8 shape
    (`tests/programs/absent_negation.dl`): `unmeasured(absent)` before, gone
    after, because `measurement(absent, 3)` now refutes it.
  - **The two laws were one item but not one phenomenon.** Idempotence stays
    broken over `absent` deliberately, and the proof is mechanical:
    `repeating_a_body_literal_drops_absent_rows` passes identically with the fix
    reverted, while all three negation properties flip.
  - **The differential caught the divergence but could not have caught the law**
    — `b1_absent_programs_agree` goes red under a one-sided revert (the oracle
    re-expresses refutation), yet two evaluators sharing a *wrong* semantics
    agree. Hence **C9**, asserted against the model. Same shape as the entry
    below, different reason.

- **2026-07-27** — **Ordered comparison is widened to every primitive** (§4/§8;
  `src/typecheck.rs`; fixes `bugs/006`). `<` `<=` `>` `>=` now type-check on
  symbols, strings and bools as §8 has always specified, leaving `union(l, r)` —
  the same-type rule — in place, so `1 < "a"` is still an error. **No normative
  change:** the spec was right and the checker was wrong, and both evaluators
  already implemented §8, so the fix deleted three lines and added no behaviour.
  - **A differential property cannot catch a type-checker defect.** The
    2026-07-21 entry below makes `eval` type-blind on purpose, so B1 over
    comparison programs is structurally blind to what `typecheck` accepts. The
    bug file called extending B1's generator "the whole job"; measured, the
    extended generator left B1 **green with the fix reverted**. What fails is
    `comparison_generator_is_well_typed` — the property that calls `typecheck`.
    A generator widening needs a matching *acceptance* property, or it only
    broadens a differential that was never asking the question.
  - **§4's order now has one guard across all three of its homes** — `<`,
    `min`/`max`, and the printer's sort, each deriving from `Ord` on `Value`.
    `ordered_comparison_and_minmax_agree_on_every_type` (testing.md C8) fixes
    the pair `bugs/006` found out of step; it was unwritable until the widening.
  - **Cost of not having it:** found on real data, not by reading §8. The
    unordered-pair idiom `A < B` was unwritable over string keys, so a numeric
    `file_id` column was added to the *extractor* purely to sort by — pushing
    work into the layer a Datalog user is least able to change.

- **2026-07-27** — **String operations are rejected, not deferred** (§8/§10; user
  call). No `concat`, `substr`, `split`, `starts_with`; `ROADMAP.md` records the
  absence as closed rather than queued.
  - **The test already exists** — §5's cast governance: a builtin is safe when it
    "maps a finite value set to a finite value set with no accumulation". Casts
    pass. String construction fails it, unbounded in value *size*, not just count.
  - **Worse than arithmetic, which is already the open hole.** `nat(N) :- nat(M),
    N = M + 1.` hangs (`bugs/004`, Termination session), but i64 bounds its values
    and overflow errors, so it fails loudly at a known edge. `concat` has no edge —
    it exhausts memory, sooner. And §8's assignment rule would make `Y = concat(X,
    "-s")` a *binder*, so the function is immediately a generator inside a positive
    cycle: the shape Termination classifies as rejected.
  - **The line is filters yes, constructors no.** `<` constructs nothing, so
    widening it (`bugs/006`) is untouched by any of this.
  - **What would reopen it:** Termination landing a rule that admits value creation
    soundly — then string functions are re-arguable *under that rule*, not before.
    Until then the fact producer does it, which two analyses already had to.

- **2026-07-27** — **A query constant-folds a ground compound argument** (§5/§14;
  `src/lower.rs` `ArgMode::FoldGround`; fixes `bugs/005`). Hoisting made
  `?- p("a", 1 + 1).` a two-literal body with no named variables — the one shape
  §14 emits nothing for — so it printed nothing where `?- p("a", 2).` printed the
  fact. Folding keeps the query the single atom it reads as; `api.rs` unchanged.
  - **Rejected: plumbing hoist-origin into the IR** (the bug file's own sketch).
    Costs a new IR field *and* the claim that inline and hand-hoisted arguments
    lower to the same program, which A15 asserts over generated inputs. Trading a
    ratified equivalence property for an output-shape fix is the wrong exchange.
  - **Rejected: an existence-check case in `api.rs`**, the bug file's "cheaper"
    option. It is not cheaper — the atom to print is `p("a", V)` with `V`
    unprojected, so the row must be reconstructed — and it fixes only the fully
    ground case. Folding also fixes `?- p(X, 1 + 1).`, the same defect one
    variable short of ground, which printed the weaker `answer("a")`.
  - **Scoped to queries**, so A15's IR-identity claim over rule bodies needs no
    weakening. Folding everywhere is more uniform and is the road not taken.
    Cost: an ill-typed ground query argument is now reported by folding rather
    than typecheck, with more specific wording; `1 / 0` and overflow unchanged.

- **2026-07-25** — **Negated atoms join the dependency schedule** (§7/§10;
  `src/schedule.rs`, `src/lower.rs`, `src/engine/`; resolves the open question of
  the same name below, and fixes `bugs/001`). A negated atom's named variables no
  longer have to be bound *positively* — a positive atom, an `=`-assignment or an
  aggregate result all satisfy the requirement the rule stood for, which is that
  the atom be **ground when tested**. The scheduler places the anti-join after
  whatever binds its arguments.
  - **What the rule cost.** It was not the uniform expressiveness limit the open
    question claimed. `not q(X + 1)` hoists its argument to a generated slot, and
    the safety check tested `var_names[slot].is_some()` — "does this slot have a
    name" standing in for "is this a wildcard". A hoisted slot has no name, so the
    check skipped it and the anti-join read it as an open wildcard: the rule
    silently became `not q(_)`, "no `q` fact at all". The hand-hoisted spelling of
    the *same* conjunction was rejected. One conjunction, three spellings, three
    different answers.
  - **The scheduling rule.** A negated atom **reads** the argument variables that
    something else in the body binds (`binder_vars`). A slot bound nowhere is not
    a dependency — it stays open and existential under the negation (§7) — so the
    scheduler needs no notion of "wildcard" and stays free of `var_names`. A
    *named* variable bound nowhere reaches the same conclusion here, so lowering
    keeps a separate check for it; that is the one thing scheduling cannot decide.
  - **Early pruning is preserved.** Negations the positive atoms already ground
    stay in their own phase ahead of the builtins, so a cheap anti-join still runs
    before an expensive aggregate. Only the not-yet-ready ones defer.
  - **Chosen over tagging hoist-generated slots**, which would have kept the
    restriction and merely reported it honestly. Rejected because the restriction
    had no justification left to enforce — it justified itself by the phase order
    and the phase order by itself — and because naming the offending source
    expression in the error would have required an IR-expression renderer built
    solely to explain something we intended to delete.
  - **Sequenced ahead of `absent` × negation** (ROADMAP negation item 1), which
    the roadmap had put first. Verified that the interaction is not *created*
    here: `m(K, X), not q(X)` with a stored `q(absent)` already returns every row
    today, so the absent-under-negation hole is reachable through an ordinary
    positive binding. This change adds spellings that reach an already-broken
    cell. Item 1 still owns the anti-join's matching rule, and its two `#[ignore]`d
    tests fail exactly as before.
  - **Cost.** The rule was stated in three places — the scheduler's phase order,
    lowering's safety check, and the engine's `validate_body` contract for
    hand-built IR — and all three had to move together; `validate_body` now shares
    the scheduler's binding set instead of walking positives itself. The naive
    oracle filtered *every* negation before running any builtin, so it had to
    interleave them too: a differential oracle that hard-codes the old order would
    have agreed with a wrong engine. Covered by testing.md **C7** plus
    `CompRule::NegShift` carrying the shape into B1.

  ***Amended 2026-07-27*** (`bugs/003`). **The "three places" count was wrong, and
  what it missed was the prose, not the code.** All three *code* sites moved
  together as recorded. But the rule is also stated in the **spec**, and there it
  lived in six sections — §7, §8 twice, §9 twice, §10, §14. This entry's sweep
  updated §7 and §10 and left the rest, so two of them went on asserting the
  pre-relaxation rule: §8's `is [not] absent` operand ("positively bound") and
  §8's mode/safety paragraph ("bound by a positive atom"). The 2026-07-25
  `bugs/003` sweep, run the same day with that very file open, found neither; the
  2026-07-27 re-verification found the first and not the second.

  The lesson is not "count more carefully". Two sweeps by two sessions each found
  a different subset, which is what a rule with six homes does regardless of
  diligence. §10 is now the single normative statement and the other five refer to
  it — the structural fix that makes the *next* relaxation a one-line edit.

  ***Consequences 2026-07-29 — the resequencing was free.*** This entry took item
  1's slot on the argument that it only added spellings reaching an
  already-broken cell. Item 1 landed four days later and the two never met: this
  one changed *when* the anti-join runs, that one changed *what refutes it*. The
  "two `#[ignore]`d tests fail exactly as before" line above is now false in the
  good direction — one passes, one was converted (2026-07-29 entry).
- **2026-07-25** — **Conversion is the `as` cast; user-defined scalar functions are
  declined; and arithmetic already broke termination** (design session following the
  spec review; no engine change — implementation is a ROADMAP item). The review had
  left "does v1 have a scalar-function call form?" open, because the strict
  `int`/`float` separation has no in-language fix without one. Workshopping the
  syntax settled it and turned up something larger.
  - **Conversion is `Expr as type`** (§4/§5/§8), not `float(A)`. Both halves already
    existed — `as` is a reserved word and `type` is an existing production — so no
    new vocabulary is introduced, and it is unambiguous everywhere because `as` can
    begin neither a statement nor a body literal. It is familiar from two
    directions models reproduce reliably (SQL `CAST(x AS type)`, Rust `x as f64`),
    which serves the conventional-syntax pillar (§2) rather than straining it.
    - **Rejected: `float(A)` calls.** The conventional Datalog spelling, and the
      general answer, but at body-literal start `ident (` is ambiguous between an
      atom and a call — `float(A) > 0.5` versus `p(X)` — so it needs scan-ahead past
      the balanced paren group, costing LL(1). The §9 aggregate precedent does *not*
      transfer: aggregates are unambiguous precisely because `{` appears nowhere
      else, whereas `(` is the atom delimiter.
    - **Rejected: `float A` juxtaposition** (Haskell-style), despite the real
      attraction that it dissolves that ambiguity and keeps LL(1). Without currying,
      `f X Y` is `f(X,Y)` or an error depending on arity, so the parser must consult
      a signature table — adding a two-argument builtin would change how existing
      text parses. It also makes `abs -1` ambiguous, hard-depends on parenthesised
      grouping (which does not exist yet) for nesting, and is novel syntax no
      Datalog uses, against the pillar that LLMs reproduce conventional syntax most
      reliably.
    - **Rejected: `float[A]`.** `[`/`]` are entirely unlexed, so it is
      ambiguity-free, but equally novel without `as`'s familiarity.
    - **Strict numerics stand** (reaffirming the earlier same-day call): no implicit
      `int + float` widening. An int column beside a float column usually reflects a
      real distinction in the data, and implicit widening would reintroduce in
      expressions the >2⁵³ precision loss §13 refuses at the import boundary.
    - **Deliberately left open:** whether a failed conversion errors or yields
      `absent` (below). Decided with the implementation, not here.
      - ***Amended 2026-08-16*** — built, and the deferral was the right call:
        deciding it needed a fact this entry could not have had, that
        `eval_expr`'s error path aborts the whole run rather than the row. The
        answer is **both** — `absent` where there is no value to represent, an
        error where representing it would be lossy — which is a distinction this
        bullet's either/or framing did not admit (2026-08-16 entry). Two things
        this entry did not anticipate: the conversion **table** was as open as the
        failure mode and needed settling in the same sitting, and the text
        direction is not new code at all — §13's literal-grammar classifier
        already *was* the conversion, and now lives in `lexer.rs` where both
        callers reach it.
  - **User-defined scalar functions: declined — but not for the obvious reason.**
    The question arose from a governance concern: functions might make the language
    Turing-complete, losing the guaranteed bounds that make a logic engine safe to
    point at LLM-generated programs. That reasoning does not hold. A *non-recursive*
    user-defined scalar function is a definitional abbreviation — inlineable at
    lowering, adding exactly zero power over the existing expression language — and
    a recursive one would simply be forbidden, as stratification already forbids
    recursion through negation (§7) and aggregation (§9). The real objection is
    **redundancy: in Datalog a rule already *is* a user-defined function.**
    `double(X, Y) :- Y = X * 2.` is a relation used functionally, and that is the
    language's whole idiom; a scalar-function syntax would add only composability
    inside an expression (`p(double(X))` over `double(X, Y), p(Y)`) — thin
    ergonomics for a second way to do one thing. Precedent: the module-import
    decision (2026-07-23) chose the `as`-shaped form over a separate `include`
    keyword on exactly these grounds, "no second concept/reserved word". What stays
    open is narrower: *builtin* scalars with no relational spelling (`abs`,
    `length`, `lower`, `substr`), deferred until a consumer needs them.
  - **The governance premise was already false, and arithmetic is why.** Pure
    Datalog's guarantee comes from a finite Herbrand universe — no way to synthesise
    values absent from the input, so the fixpoint is reached in finitely many steps
    at PTIME data complexity. §8 arithmetic ended that at milestone 4: `nat(0).
    nat(N) :- nat(M), N = M + 1.` is accepted (the `=`-assignment is a §10 binder,
    the recursion is positive) and **runs forever**, with no iteration cap, fact cap,
    or time budget anywhere in the engine. §6 still asserts the finite universe and
    the finite-step fixpoint, so that is a doc defect (`bugs/004`); the behavioural
    fix is a ROADMAP design item. Stated precisely: with `i64` and overflow-as-error
    the state space is finite, so the language is not *literally* Turing-complete —
    but "terminates after 2⁶³ iterations" is not a guarantee, and the finite-lattice
    argument §6 actually makes is unavailable regardless.
    - **Direction chosen (user call): a static semantic error, not runtime fuel.**
      Reject an arithmetic-computed value flowing to the head of a positively
      recursive predicate. Fuel was considered and rejected as hacky — a budget is
      not a guarantee, and the point of this property is that it should be a
      theorem. The sketch, the soundness argument, and its cost (it rejects
      cost-accumulating transitive closure) are in `ROADMAP.md`; it needs its own
      session and is explicitly not to be patched ahead of one.
      - ***Amended 2026-08-16*** — the static rule stands and the budget stays
        rejected (2026-08-16 entry). What this bullet did not say is what the engine
        owes when the model is short **anyway**, which a static rule never
        addresses: an incomplete fixpoint is missing facts and never false ones, but
        a *query* solved against it can be wrong rather than missing. The question
        "should there be a budget" was answered here; "what does an incomplete run
        print" was not asked, and it is the half with live instances today.
      - ***Amended 2026-08-18*** — **the analysis stands; the *rejection* does
        not** (2026-08-18 entry, user call). The rule ships as a **warning**. The
        reasoning above survives intact where it argues against fuel and for a
        theorem, and the theorem is what shipped — what it got wrong is treating
        "not provably terminating" as "wrong". `path_cost` terminates on every
        acyclic graph, so rejection is a false positive on a property of the
        **data**; and this bullet's own pointer at "its cost (it rejects
        cost-accumulating transitive closure)" is the finding, recorded here and
        then not weighed. Note also that the sketch it points at was **unsound**:
        it reads only the literal binding the head variable, so
        `K = M + 1, N = K` passes it and still diverges.
    - **Casts are exempt**, checked before `as` was adopted: a cast maps a finite
      value set to a finite value set with no accumulation.
      - ***Consequences 2026-08-18*** — **the check paid, and it was the only part
        of this bullet that survived implementation unchanged.** `as` shipped three
        weeks before the termination rule, and had the exemption been wrong the
        cast would have had to be re-litigated with a language feature already in
        the field. It needed one refinement rather than a reversal: a cast is
        exempt as a *creator* and not as a *carrier*, so `N = K as int` over a
        computed `K` still counts (§10). Nothing in this bullet implied the
        distinction, and only writing the transitive rule surfaced it.
    - Note the actual **exfiltration** surface is unrelated to any of this: URL
      imports are ungated by explicit decision (2026-07-23), so an
      LLM-generated program can reach the network through §13, not through §8.

- **2026-07-25** — **Spec design & style review** (no engine change; findings
  queued in `bugs/` and `ROADMAP.md`). Read §1–§17 as a specification and probed
  every candidate finding against the built binary rather than reasoning from the
  prose. Three defects, several unqueued language gaps, and a document-hygiene
  backlog. Decisions taken during the review:
  - **Defects get their own tracker: `bugs/`, one file per defect.** `ROADMAP.md`
    holds what is *missing*; `bugs/` holds what is *wrong*. **Location is the
    status** — `bugs/*.md` is exactly the open set, resolving one is `git mv` to
    `bugs/resolved/` — so there is no `status:` field and no index file, either of
    which would be a second copy of what the directory already says. That matters
    here specifically: stale cross-references are this project's demonstrated
    failure mode (this review found five), so a scheme whose correctness depends on
    hand-maintaining an index would repeat the mistake it is meant to catch. A
    resolved file must carry a `## Resolution` note *appended* (never rewriting the
    original diagnosis) saying which candidate fix was taken and why the others
    were dropped — the part a later session cannot recover from the diff.
    Conventions in `bugs/README.md`. Rejected: GitHub Issues (the remote is a
    private SSH server), and `git-bug`/Fossil (both move issue text out of plain
    files, backwards when the primary reader is an agent that greps markdown).
  - **Strict numerics stay; `int + float` is not implicitly widened** (user call).
    Verified that `int` and `float` never meet in-language and that the only
    conversion escape hatch is the import boundary. Two reasons to keep it that
    way: this is a data-analysis language, so an int column beside a float column
    usually reflects a real distinction in the data model (a count vs. a
    measurement, an identifier vs. an amount) that the engine should respect
    rather than dissolve; and implicit widening would reintroduce in expressions
    the precision hazard §13 closed at the import path the same day (`i64` → `f64`
    above 2⁵³, where large integers are usually identifiers). The fix is therefore
    an **explicit** conversion — `float(X)`, keeping the widening visible in the
    source — which folds the numeric gap into the call-form question rather than
    making it a separate item. Note `avg`'s `int → float` result type already shows
    the language widens where a *declared result type* says so, as distinct from
    coercing operands silently.
  - **Parenthesized expressions were never a design decision** — an implementation
    gap that acquired a good error message and read as intentional. §17 has no
    entry on them; the "deliberately flat" claim exists only in commit a7d1ff1's
    message, and the Phase D entry below that ratified precedence, signed literals,
    and inline arithmetic never mentions grouping. Nothing semantic depends on it
    (`ast::ExprKind::Binary` and `ir::Expr::Binary` are already general trees) and
    there is no ambiguity (an atom must start with an identifier, so a body literal
    beginning with `(` can only be a comparison). So they are queued as a **small
    task, deliberately not bundled** with the scalar-call form — sharing a grammar
    production is not a reason to make a parser-plus-printer fix wait on an
    open-ended design question. The one real cost is that `print.rs` must become
    precedence-aware; D2/D3 are already the properties that check it.
  - **§6 is the largest substantive gap, and it belongs to the negation session.**
    Declarative semantics still stops at positive programs — no model-theoretic
    account of aggregation and none of `absent` — while §6's own note lists both as
    "still to fill in". "What does `p(X), not p(X)` mean" is a §6 question wearing
    an implementation costume, so the open absent × negation item should settle it
    there rather than in the anti-join.

  ***Amended 2026-07-29 — §6 was deliberately left out of the negation session.***
  The pairing above was not taken: the anti-join question was settled in §4/§7
  first, on the reasoning that §6 should describe a ratified semantics rather
  than one being decided as it is written, and that §6 also owes an account of
  aggregation plus a finiteness claim blocked on `bugs/004`. So §6 is now a
  session of its own with one fewer unknown, not a gap this one closed. The
  entry's *diagnosis* stands — it is a §6 question — only its sequencing changed.

  ***Consequences 2026-08-18.*** The sequencing call held twice over: §6 now has
  **two** fewer unknowns than when it was deferred, not one. The truncation
  contract (2026-08-16) settled what an incomplete model is worth, and Termination
  (2026-08-18) settled the finiteness claim, which §6 now states conditionally on
  §10's certified fragment. Had §6 been written into the negation session it would
  have committed to a finiteness premise that was false at the time and would have
  had to be rewritten twice. Its remaining unknown is aggregation alone.

  ***Consequences 2026-08-18 (§6 shipped, same day).*** The three gaps §6's footer
  listed were **not equal, and the ranking was inverted**. Aggregation — named here
  as the last unknown — cost one sentence: its goal reads a strictly lower stratum,
  so the aggregate is a fixed function from group keys to values and `T_P` stays
  monotone without an argument. `absent`, listed as a peer, forced a change to the
  **operator itself** — satisfaction is a match relation, not substitution — which
  nothing had flagged as a gap at all. The lesson is not that the deferral was
  wrong; it is that a *Not covered* footer ranks by what was noticed, and the item
  that rewrites the definition is the one nobody listed.

- **2026-07-25** — **Correctness review of milestones 8–9** (the absent value and
  aggregation, both shipped 2026-07-24). Re-derived their semantics against
  §4/§8/§9/§10 and probed the built binary. The happy paths held; four things did
  not, and are now decided:
  - **Query answer variables are the ones the body *binds*, not every named
    slot** (§14). An aggregate's goal-local variables are named but live only in
    the sub-join, so projecting them left an unbound slot — every query
    containing an aggregate with a named goal variable *panicked*, including the
    `-q` form the agent skill teaches. Lowering now computes the projection
    (`ir::Query::projection`) with the same `safe_bound_vars` notion a rule head
    is checked against, and the engine's residual unbound case is a structured
    error, not a panic. The panic escaped notice because the §9 tests only ever
    exercised the *rule* form.
  - **An aggregate goal binds like any body** (§9). The safety check used
    "occurs in a positive atom of the goal" for the collected expression, which
    rejected `max { T | s(K,V), T = V*2 }` and every **nested** aggregate — forms
    lowering already supports (it hoists a nested aggregate into the goal for
    exactly this reason). Now `safe_bound_vars(goal)`.
  - **The literal `absent` is not a comparison operand** (§8). `V != absent`
    reads as "where the value exists" and silently selects nothing, because every
    comparison with absent is false. It is now a structured error steering to
    `is [not] absent` — the same steer §4 already applies to a literal `absent`
    in a body atom argument, and it forbids only comparisons that could not have
    been true. The producer form `X = absent` (assignment) is unaffected.
  - **The §9 skip is reported now, by a warning** (§9/§12). "Skip but report"
    was chosen over a two-place aggregate result on the strength of `?why`
    rendering it — but `?why` does not exist yet, so `avg` over a half-empty
    column was indistinguishable from `avg` over a full one. A
    `Warning::AbsentSkippedInAggregate` per aggregate site now reads the count
    back out of the recorded `Premise::Aggregate` and prints it on stderr, which
    also demonstrates the provenance record is real. `?why` remains the eventual
    surface; queries are not covered (they record no derivations).

  - **Body evaluation order is by dependency, not source order** (§8/§9/§10,
    `src/schedule.rs`). The evaluator ran builtins in source order, so a group
    key whose `=`-assignment sat *after* its aggregate was still unbound when the
    aggregate ran and got enumerated as a goal-local existential — the grouping
    silently vanished. Two orderings of one conjunction, two answers, which no
    declarative reading permits.

    The first fix rejected the out-of-order form, making aggregates consistent
    with the pre-existing rule that `=`-chains must be written in dependency
    order. That restored soundness (reordering could produce an error, never a
    different answer) but kept a wart, and its message was actively wrong for the
    one case reordering *cannot* fix — a genuine cycle, where "move the
    assignment earlier" is impossible advice.

    So the discipline itself was removed instead. Each builtin declares what it
    **reads** and **binds** — an aggregate reads its group keys (identified
    syntactically: a variable used both inside it and outside it) and binds its
    result — and the body is scheduled to a fixpoint over those edges. Both
    orderings above now mean "grouped by `Y`", and `M = N+1, N = A+1` is accepted.
    Rejection is reserved for bodies where *no* order works, split into
    **unbound** (nothing binds the variable) and **circular** (the binder needs
    this literal), which want different advice.

    Two things keep this a widening rather than a change of meaning: among ready
    literals the **earliest in source order** runs first, so any body that
    already worked keeps its exact schedule (including how `=` resolves
    assignment-vs-filter, and including pruning order, which is observable on the
    error path); and the schedule is a pure function of the body, so lowering —
    which reports the errors — and both evaluators derive the same one, shared
    the way `fold_aggregate` is. That sharing means B1 cannot see a scheduling
    bug, so the scheduler carries its own contract property
    (`schedules_bind_before_they_read`) alongside the end-to-end invariance
    (`b5_aggregate_body_order_does_not_change_the_model`,
    `b5_a_computed_group_key_is_order_independent`).

    ***Falsified 2026-08-20 — `bugs/007`.*** "Two orderings of one conjunction,
    two answers, which no declarative reading permits" is still the right
    standard, and the scheduler still meets it. What was load-bearing and untrue
    is the **generalisation**: fixing the schedule does not make body order
    unobservable, because an *aggregate* fold consumes its witnesses in
    enumeration order, and that order follows the goal's literal order. Over
    `int` in a small range the question cannot arise — addition is associative
    and cannot overflow — which is exactly why the properties named above
    (`b5_aggregate_body_order_does_not_change_the_model`,
    `b1_aggregate_goal_shapes_agree`) stated the claim and stayed green for a
    month. Over `float` two spellings give `r(0.0)` and `r(0.1)`; on the int
    overflow check, one answers and the other exits 2. The guarantee is
    **conditional on the fold being associative over the value type**, and
    `bugs/007` holds the four candidate fixes.

    ***Amended 2026-08-20 — the generalisation is restored, by a second
    mechanism.*** `bugs/007` is fixed, so body order is unobservable
    again — but because *two* things are order-independent now, not one. The
    scheduler settles which literal is outer; the fold no longer cares, having
    become a function of its witness multiset (2026-08-20, above). The fix taken
    was **not** among the four candidates named here; the resolution note says
    which and why.

    Negated atoms stay in their own phase, before every builtin: §10 requires
    their named variables to be bound *positively*, and folding them into the
    dependency order would widen negation safety — a §7/§10 decision, not a
    consequence of scheduling. It is the one remaining place where where-you-
    write-it decides whether a program is accepted, though unlike the aggregate
    case the restriction is **uniform** (both orderings are refused alike), so it
    is an expressiveness limit rather than a silent mis-reading. Carried as an
    open question below.

    ***Superseded later the same day — see "Negated atoms join the dependency
    schedule" above.*** The "uniform … expressiveness limit rather than a silent
    mis-reading" claim was false when written: `not q(X + 1)` was already
    silently returning the wrong rows (`bugs/001`). The restriction was checked
    only for *named* slots, and the hoist that inline arithmetic performs mints
    an unnamed one — so the two orderings were not refused alike, and one of them
    was not refused at all.

  - **The semantic sameness rule moved onto `Value`** as
    `Value::unifies_with` (with `Value::is_absent` for the structural absence
    test), replacing the free `engine::values_unify`. Considered and **rejected**:
    moving the derived `Eq`/`Ord`/`Hash` off `Value` onto a storage-key newtype
    so that `==` at a match site would not compile. It reads well in the
    abstract, but ~146 sites depend on the derive — `Tuple`/`Fact` need it for
    `BTreeSet` storage, canonical output order needs it, and every test assertion
    over answers wants exactly structural equality — so the cost falls almost
    entirely on the *legitimate* uses in order to guard four semantic ones. It
    would also not have prevented the bug it was proposed for: `engine::naive`
    used `==` because absent was not in the author's view at all, and would have
    written `Key(a) == Key(b)` just as readily. What catches that class is the
    differential (`b1_absent_programs_agree`); what makes it less likely is one
    discoverable method on the type. Both are now in place.

  - **§12 errors became structured data** (§12 now Draft). Every error was a
    bare `String` with its location `format!`ed in as `"(at byte 217)"` — a
    stringly-typed diagnostic in the one part of the system whose stated job is
    to be machine-consumable, and a byte offset at that, which cannot become a
    caret or a `file:line`. `Error` is now a struct — category, message, span,
    **line/column** position, suggestion — with `Display` composed *from* the
    fields rather than the fields being recovered from `Display`. Suggested fixes
    (the Prolog-prior near-misses, the lowercase-relation hint) moved out of the
    message text into their own field. The parser's `Result<T, ()>` became
    `Result<T, Reported>`, where `Reported` has no constructor outside the error
    helpers: "you may only return `Err` after recording a diagnostic" is now
    checked by the compiler instead of by convention (rustc's `ErrorGuaranteed`,
    in miniature). Deferred deliberately: the machine-readable **code**
    vocabulary, and spans for semantic errors — both need decisions, not
    mechanics.
  - **A lossy int→float widening on the import path is an error** (§13). A
    mixed int/float column widens its integer cells; above 2⁵³ that rounds
    silently, on untrusted data, where the large integers are typically
    identifiers. Now a structured source error naming the cell. (The neighbouring
    concern, DuckDB's `UBigInt` → `i64`, was already checked — it widens through
    `i128` and uses `i64::try_from`.)

  Also recorded: a **wildcard inside an aggregate goal is a witness dimension**,
  not an existential (§9) — `count { P | parent(P, _) }` counts edges. Consistent
  with the witness-set rule and with SQL, but it is the one place `_` does not
  mean "don't care", so it is now stated rather than implied.
  - ***Amended 2026-08-24*** — the first of the two deferrals is done: semantic
    and source errors carry spans (Decisions, 2026-08-24). *"Needs decisions, not
    mechanics"* held — the decision was **where to resolve them**, not which span
    each site names. The code vocabulary is still deferred.

- **2026-07-24** — **Aggregation (§9): full design ratified** (design session;
  implementation is a follow-on roadmap item). v1 ships the canonical five —
  `count`, `sum`, `min`, `max`, `avg` — evaluated in the native engine over
  materialized facts (DuckDB is import-only, never at query time).
  - **Scope: the five, no more.** `count` is type-agnostic; `min`/`max` extend to
    any single ordered type (string/symbol/bool as well as numeric) at no cost
    because the value `Ord` already exists; `sum`/`avg` are numeric-only.
    Statistical (`median`/`stddev`/`variance`/`percentile`) and collection-valued
    (`collect`/`string_agg`) reducers are deferred (below). The reducer is
    orthogonal to the evaluator — adding statistical ones later is a registration
    + a typecheck arm, not a restructure — so the node reserves a **parameter
    slot** (for `percentile(p)`) from day one.
  - **Syntax: set-builder pipe** `op { Expr | Goal }`, an expression that composes
    under §8 and lowers (hoists) to an `=`-assignment. `|` over `:` avoids the
    named-argument colon collision; operator names are contextual (ident before
    `{`), so `count`/… stay usable as relation names. Rejected: keeping `:`
    (overloads named args in the same expression); a `where` keyword (verbose,
    off house style).
  - **Grouping is implicit** on the rule variables outside the aggregate — no
    `group by`. The fold is over the multiset of `Expr` across the **distinct
    witness tuples** of `Goal` (so equal projected values from distinct witnesses
    both count — the "duplicates and aggregates" resolution, `references.md`).
  - **Skip count via provenance, not the value** (resolves the open item below).
    The aggregate stays a single value that nests in arithmetic; the skipped-absent
    count is recorded in the derivation and shown by `?why`. Wanting it as data,
    the user writes `count { A | Goal, A is absent }`. Rejected: a two-place
    result (breaks expression composition).
  - **Recursion through an aggregate is rejected via stratification** (like
    negation, §7); safety mirrors negation (§10). Recursive/monotonic aggregation
    is a deliberate future item.
  - **Deferred:** statistical reducers (into the reserved param slot); collection
    reducers (blocked on a first-class collection value, §4); recursive
    aggregation (Zaniolo et al.).
  - ***Consequences 2026-07-27*** (source-analysis dogfood; `docs/worklog.md`):
    - **"`min`/`max` at no cost because the value `Ord` already exists" is now
      the argument in `bugs/006`.** That `Ord` is §4's canonical order; `<`
      refuses the strings `min`/`max` happily order, and `src/typecheck.rs` cites
      §8 for both halves. The clause was right; it just proved more than it
      claimed.
    - **The count-distinct trap is worse than §9 documents.** §9 warns about a
      visible `_` in a goal. Over an imported table the wildcard is *invisible*:
      named-argument syntax leaves every unmentioned column implicitly
      wildcarded, and each is a witness dimension. `count { C | calls(caller: C,
      callee: "bump") }` returned 36 — call sites — where the question wanted 20
      callers, with no `_` written anywhere. §13's wide machine-generated tables
      are exactly where this bites, and §13 arrived after §9 was documented.
    - **Aggregation made three tree-walkers mutually recursive**, because
      `op { Expr | Goal }` nests a *body* inside an *expression*:
      `parse_primary → parse_aggregate → parse_expr`, `naive::matches ↔
      apply_builtins`, `testgen::monotype_expr ↔ monotype_body`. This falsifies
      the 2026-07-23 dogfood's "zero mutual recursion; parser expression grammar
      acyclic" — a structural cost of the syntax that the design session did not
      anticipate and that nothing recorded until it was measured.

- **2026-07-24** — **Absent value: full design ratified** (design session;
  supersedes the 2026-07-23 direction below and resolves its open question;
  implementation is a follow-on roadmap item). The missing-data value is a
  **single first-class `absent`** — a *value that inhabits any column*, not a sixth
  static type and not a nullable-column type system. `optional T` was rejected:
  sparse data is common, so "most columns provably total" buys little; a single
  value keeps the engine value-oriented and stays reversible (`optional T` could be
  layered on later as static analysis, not vice-versa).
  - **Two-valued** (§4/§8): annihilates in arithmetic (`absent + x = absent`, ahead
    of the div-by-zero/overflow checks), false in every comparison, and never
    unifies with a value or with another `absent`.
  - **`absent ≠ absent` semantically** (joins, `=`/`!=`) so missing foreign keys
    don't cartesian-blow-up; **structurally identical** for set dedup and the
    canonical `Ord` (sorts first). Two notions of "same", as in SQL.
  - **Presence via `X is absent` / `X is not absent`** — a new comparison operator
    (SQL's `IS [NOT] NULL`), chosen over an `is_absent(X)` builtin for ergonomics
    and because `=`/`!=` are both false for absent and so cannot partition rows.
    The literal `absent` is producible (facts/heads/arithmetic) but **not matchable**
    in a body (unification fails); a body-match `absent` is a structured error
    steering to `is absent`.

    ***Amended 2026-07-27*** (`bugs/003`). The operator added a **reserved word**,
    `is`, and neither this entry nor the implementation that followed it recorded
    that anywhere: §3 carries the canonical reserved list and never learned about
    `is` (or about `absent`, reserved here too and stated only in §5). A keyword is
    a surface-syntax consequence that outlives the feature deciding it — this is
    the class of thing a decision entry has to route back to §3, because the lexer
    will not. §3 now names all eight, and a table-driven parser test asserts each
    one, so the next keyword fails a test rather than a spec review.
  - **Aggregates skip-but-report** (§9): sum/avg/min/max skip absents and report
    the skip count; `count` counts bindings; an empty present-set gives `count = 0`
    and the others `absent`.
  - **Uniform import** (§13): every source's null/missing → `absent`, type-neutral
    in inference. This fixes the USDA dogfood failures — 33 empty `amount` cells no
    longer force the column to `string` and kill arithmetic, and an empty
    `food_category_id` stays an int that joins its dimension. One table still →
    one relation.
  - **Why a first-class value over the alternatives** (session; worklog 2026-07-24):
    the *import-decomposition* model (nullable columns → present-only companion
    relations, missing = absence-of-tuple — the Datomic/Soufflé-native path) was
    stress-tested and rejected *for an exploration tool*: it breaks the "one table,
    all named columns, all rows" model an agent expects, turns naming a column to
    *see* it into a silent row-dropping *filter*, and any virtual wide view
    collapses back into needing a fill value. Zero-fill/sentinels were rejected
    outright (silently corrupt aggregates and joins; no system, DuckDB included,
    does it). Research: DuckDB carries NULL into query time with 3VL +
    `IS NOT DISTINCT FROM`; Datomic and Soufflé have no null (missing = no fact, or
    an explicit `Option` ADT); DDlog uses `Option<T>` — the rejected `optional T`.
  - This **retires the ratified "value space has no null"** (§4/§13) and amends the
    2026-07-23 CSV-inference clause "any empty cell → string" to "empty cell →
    absent, type-neutral" (below).
  - **Still open:** generalizing `is` to a full `X is Y` null-safe equality; the
    type of an all-absent import column (currently: resolved by use-site flow, else
    unconstrained). (The aggregate skip-count report surface was resolved
    2026-07-24 — provenance-only, §9.)

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
    Open sub-questions below. *(Superseded by the 2026-07-24 full design above,
    which resolved the sub-questions.)*

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
    *(Amended 2026-07-24: an empty cell is the absent value — type-neutral, not
    string — see the absent-value decision above.)*
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
    note). ***Amended 2026-09-12*** — *before lowering* now holds only for a
    program with a schema-less import; with every schema explicit, lowering runs
    first and only the imports a goal reaches are read. "No new channel" held,
    and turned out to be the cost: a declared column type is not a constraint, so
    a partial load can reject a program a full one accepts (`bugs/014`), and the
    run re-checks such a rejection over a full load rather than trust it.
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
  - ***Consequences 2026-07-27*** (the same use case re-run with §13 and §9 in
    place; `docs/worklog.md`, recipe in `skill/recipes/source-analysis.md`):
    - **The sequencing was right and the two features carried the run.** 27,957
      facts across 15 JSONL tables import in 0.4 s, and every ranking that was
      `sort | uniq -c` last time is now a `count`. Nothing about the ordering
      looks wrong in hindsight.
    - **"Extraction is the weak link" held; "so use a real parser" was half an
      answer.** `syn` did remove the entire regex error class, and introduced its
      own: macro bodies are opaque (7% of this crate's functions invisible) and
      bare callee names merge `Model::new` with `Lexer::new`. The other half is
      **resolving in Datalog, in confidence tiers, with the leftovers kept as a
      counted relation** — the engine holding the uncertainty rather than the
      extractor guessing. No extractor removes ambiguity it cannot see.
    - **A prediction confirmed, and worse than stated** — see the count-distinct
      note on the 2026-07-24 §9 entry above.

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

  ***Consequences 2026-07-25.*** "Classify by **parsing**, never by
  string-splitting on `:-`" was and is right — the string-splitting failure it
  avoids is real. What it cost is subtler: the *implementation* of that rule
  matches a single statement (`[statement]`), which silently assumes one clause
  in yields one statement out. The previous day's disjunction desugaring had
  already made that false, so `-q` rejects a rule the same file accepts
  (`bugs/002`). Neither decision is wrong; their composition was never checked,
  and §14's prose describes the `[statement]` match while reading like a
  language-level statement. The general claim — `-q` is sugar for appending to
  the loaded program — is now a property rather than prose (testing.md C8).

  ***Amended 2026-07-26.*** A rule is **one rule however many clauses it
  desugars to**: the classifier accepts N clauses with non-empty bodies and the
  same printed head, taking the query from the first. Differing heads stay on the
  query-body path, so two unrelated rules remain a parse error rather than a
  silent partial answer. `bugs/002` closed; §14's prose restated. The prose was
  the carrier, not the code — it read as normative, so nothing flagged it when
  the parser started desugaring one clause into several. Cost: seven lines in
  `api::query_source`. The C8 property written the day before was the entire
  acceptance criterion; fixing the defect was deleting its `#[ignore]`.

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

  ***Consequences 2026-07-25.*** The two widenings this session ratified turned
  out to be the two that later broke something, in the same way: each created a
  surface form whose equivalence to an existing one was asserted and not tested.
  **Inline arithmetic** hoists to a generated slot, which falsified the
  2026-07-20 negation-safety invariant two days later and returned wrong rows
  until `bugs/001`; the "same IR as the hand-written form" claim had a unit test,
  not a property. **Disjunction** desugars one clause into N statements, which
  the next day's `-q` classifier assumed away — `bugs/002`. Both are now
  properties (testing.md C8). Two smaller residues: ratifying **precedence** here
  did not remove §8's trailing "deferred to the parser" note, so the section
  contradicted itself for three days (`bugs/003`); and the entry's silence on
  **grouping** later read as a deliberate flat-expression design, when the
  ROADMAP established it was simply never considered.

  ***Amended 2026-07-27.*** "Lowering hoists a compound arg to an
  `=`-assignment (facts constant-fold)" is now three-way: a **query** folds a
  *ground* one (`bugs/005`, decision of the same date). The two-way split was
  never a language rule — it recorded where an assignment had somewhere to live —
  but §14 had meanwhile started reading a query's output shape off its body, so
  hoisting silently changed the answer. The lesson is the one this entry's
  *Consequences* note already draws twice: what this entry ratified was checked
  against the IR, and both later defects were about what a *different* section
  reads off the same structure.

  ***Amended 2026-08-03.*** The **query answer shape** ratified here keys on a
  body of one *literal*; it widens to one positive *atom* plus non-binding
  literals (decision of that date). Third time this entry's output-shape rule has
  moved, and the third time the defect was the same one: `?- p("a"), 1 < 2.`
  answers nothing for the reason `bugs/005`'s hoisted query did. What this entry
  got right was the *principle* — the shape follows the query as written — and
  wrong was encoding it as a literal count, which only coincided with the
  principle while bodies had one literal. Also recorded here because the
  *Consequences* note above is the reason the widening ships as a property first.

  ***Consequences 2026-08-17.*** The output-shape rule moved a **fourth** time,
  and the part of this entry that has never moved is *"a ground such query prints
  the atom once if it holds"* — reviewed deliberately this session and kept, twice
  over. It is why silence still means no rather than gaining a `holds(false)`, and
  it is the precedent that let ground *conjunctions* answer in real relation names
  instead of a synthesized one. So the entry's instinct about the ground case
  outlived three revisions of the rule wrapped around it; what kept failing was
  every attempt to state the *general* condition, which is now set equality
  between the atoms' variables and the projection.

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
  - ***Consequences 2026-08-16*** — a third option existed and was not on the table:
    **record nothing, and extract a proof backwards from the retained model.** tsdl
    ships that (`notes/tsdl-cross-project-review.md`), so the cost of a proof lands
    on the question and a run nobody interrogates pays one integer per row. It is
    not a free swap — this decision buys *all* derivations, and backwards extraction
    yields one — but the rejected alternative here was the wrong one to weigh
    against, and `notes/performance-baseline.md` names this recorder as its top
    hypothesis for the 35× cliff while never having measured it. **The profiling
    item now has a concrete architecture to profile against**, which is what changes.
  - ***Consequences 2026-08-17*** — **the third option has now been priced, from the
    outside.** tsdl run under its lineage semiring against its own boolean default
    costs **13×** on a cyclic graph, 6× on a negated stratum and ~1.3× on flat
    shapes (`notes/cross-engine-benchmark.md`). That is not our number — different
    engine, and theirs annotates where ours records edges — but it is the first
    evidence of any kind that the recorder is expensive in the shape that matters,
    and it lands on the same workload where this engine's own cliff appeared
    (`sparse_800`: 65 s and 1.2 GB, against 3.8 s for a chain of the same size).
    The 2026-07-19 rationale is undisturbed — all-derivations is still what `?why`
    needs — but "one integer per row for a run nobody interrogates" now has a price
    tag on the other side of it. Ours stays unmeasured, and still needs a temporary
    build: **there is no flag to disable recording**, which is now the single
    cheapest thing standing between the profiling item and its top hypothesis.
  - ***Consequences 2026-08-20*** — **ours is measured**, by exactly that temporary
    build ([`notes/profile-2026-08-20.md`](notes/profile-2026-08-20.md)). The
    recorder is **70–78% of peak RSS** on every recursive shape and only **2–24% of
    wall clock**, with its time share *falling* as the workload grows — the opposite
    of the sibling engine's 13×, and the 2026-08-17 corroboration above does not
    transfer. It is a **memory** decision, not a speed one.
    **But the number to decide against is not this one.** The recorder's time share
    is small only because an unindexed relation scan is hiding it; with that scan
    fixed (a prototype, measured, green) the same recorder is **50–60% of a run**.
    A decision taken today would price pillar 1 at 2% of a run that will not exist.
    The rejected alternative is unchanged in kind and now costed: backwards
    extraction would return ~78% of peak memory and, post-fix, half the wall clock,
    in exchange for one proof instead of all of them.
  - ***Consequences 2026-08-21*** — **that engine now exists.** The prefix seek
    landed (2026-08-21 above), so the run this decision has to be priced against
    is the current one, not the scanned one: peak RSS is unchanged by the seek,
    so the recorder's ~78% of it stands, and the profile's cut RB put its
    wall-clock share at 50–60% of a seeking engine against 2–24% of a scanning
    one. That was measured on a prototype, not on this code, so the number to
    decide against wants re-running. Nothing here is decided yet — the
    sequencing note has simply run out of things to wait for.
  - ***Amended 2026-08-21*** — **recording is now conditional on the run's goals**,
    which is the scope change this entry never had: *all* derivations, of every
    derived fact, **in a run that asked for a proof**. The question four
    amendments have circled was the wrong shape — the store did not need replacing,
    it needed proportioning, and it was costing 70–78% of peak RSS on every run
    while earning it on none, since nothing could ask. The rejected alternative
    (backwards extraction) stays rejected for v1 and is now *smaller*: it optimises
    only the explaining path, gives up all-derivations, needs its own cycle guard
    in place of the round stamp, and could change which proof prints — which §16.6
    and E7/E8 pin byte for byte. Decisions above, 2026-08-21.
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
  - ***Amended 2026-08-16*** — **first *appearance* is the sharper order, at the
    same price.** A round cannot separate two facts derived in the same one, so a
    derivation whose premises all arrived earlier *within* the round is rejected and
    a proof that exists is not found. A monotone per-fact sequence number is also one
    `u32` captured inside the fixpoint, and is a genuine topological order of
    support: the solver reads the store live, so every premise was stored before its
    head went in. Adopted from tsdl (`notes/tsdl-cross-project-review.md`), where it
    replaced this same round-based rule for this same reason.
  - ***Falsified 2026-08-16*** — **the premise above does not hold for this
    engine, and the amendment was adopted without checking it.** "The solver reads
    the store live" is tsdl's evaluator, not ours. `eval_stratum` *batches*: a
    round's matches are all collected into `pending` against the previous round's
    model, and only then applied, so nothing inserted in a round is visible to the
    collection that produced it. The derivation that first produces a fact
    therefore **always** has strictly-earlier premises, and the round bound never
    fails to find a proof. Measured over 400 generated programs (275 derived
    facts): `explain` returned `None` **zero** times, and a same-round premise
    appeared only on *redundant* rediscoveries — 254 of 659 recorded derivations,
    which the all-derivations contract keeps and which the first-producing one is
    never among. E1 and E2 are the standing guards; `eval_stratum`'s apply loop
    now names batching as what they rest on. **Not implemented**, therefore: a
    sequence number would admit those redundant derivations and so *change which
    proof is printed*, for no correctness gain. What survives is a forward risk —
    interleaving collection with insertion (streaming, or the parallelism item)
    breaks the round bound silently, and E1 is the test that fires.
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

  *The existential reading of wildcards stands. The safety half — "must occur in
  a positive body atom" — was relaxed 2026-07-25 to "must be bound by the body";
  see "Negated atoms join the dependency schedule". Note that this entry's
  instinct was right and the 2026-07-20 entry below overturned it: the wildcard
  question does need a criterion of its own, and "is this slot bound anywhere"
  is the one that survives — it just lives in the scheduler rather than in a tag.*

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

  ***Falsified 2026-07-25 — see `bugs/001`.*** The load-bearing premise ("a fresh
  slot occurs at exactly one term position in the whole rule") stopped holding two
  days later, when **inline arithmetic in atom arguments** (2026-07-22, below)
  began hoisting compound arguments to `=`-assignments: the hoist mints a
  `None`-named slot occurring at *two* positions — the assignment target and the
  atom argument. Inside a negated atom the safety check therefore skips it and the
  anti-join treats it as an open wildcard, so `not q(X + 1)` silently degrades to
  `not q(_)` and returns wrong rows, while the hand-hoisted spelling of the same
  rule is correctly rejected. `var_names[slot].is_some()` no longer coincides with
  "named"; it coincides with "not generated", which is a different predicate. The
  entry is left standing rather than rewritten because the failure mode is the
  point: a correctness argument resting on a global invariant, recorded only at its
  own check site, with nothing to fail when a later feature invalidated it.
- **2026-07-20** — **`AbsentPattern` is `PredId` plus `Vec<Option<Value>>`**:
  `Some` for constants and bound variables (*positively* bound until 2026-07-25,
  when an assignment-bound argument became legal and started closing its slot
  too), `None` for wildcard-fresh slots (existential under the negation). `Premise` and
  `AbsentPattern` carry the full `Eq`/`Hash`/`Ord` derives — `Derivation`
  remains a dedup key. Proof trees terminate at `Absent` leaves; the
  first-round guard applies only to fact premises (absences carry no round
  and always qualify).
  - ***Amended 2026-08-16*** — **renamed.** `AbsentPattern` is now
    `NoMatchPattern`, `Premise::Absent`/`ProofTree::Absent` are now `NoMatch`,
    and "absence pattern" is "no-match pattern" in prose (the three-names entry
    above). Nothing structural moved. This entry and five others in §17 keep the
    old spelling, being an append-only record; everything outside §17 carries the
    new one, and this bullet is the pointer between them.
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

  ***Consequences 2026-07-25.*** The substance held: strict numerics, truncating
  `/`, the error trio and `Premise::Builtin` are all still in force, and B1 over
  comparison programs remains the property that carries them. Two mechanisms did
  not. "Comparisons are scheduled after positives and negations" and the
  assignment-safety exception "computed in source order" were both replaced by
  dependency scheduling (2026-07-25), which is what the entry's own
  evaluator-internal framing invited — it recorded an *order* as if it were a
  detail, and safety quietly came to depend on it. The larger unnoticed cost:
  admitting arithmetic made the Herbrand universe unbounded and falsified §6's
  "finite set of constants" and "reached in finitely many steps" — `bugs/004`,
  filed four days later. Nothing in this entry mentions §6, and nothing failed.
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

  ***Amended 2026-07-27*** (`bugs/006`). "arithmetic/ordered comparison ⇒
  numeric" was derived one step too far: §8 constrains *arithmetic* to numerics,
  and orders every primitive. Only the same-type half was ever §8's. No coercion
  still stands — that is the other half of the same sentence and is untouched.
  The type-blind `eval` decided here also held, and cost something: it is exactly
  why B1 could not catch the over-derivation, since the two evaluators agreed on
  string comparisons the whole time and only the checker refused them.
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

> Open **defects** are not listed here — they live in [`bugs/`](bugs/), one file
> each (`bugs/README.md` for the conventions). This section is for questions with
> no settled answer; a defect has a known-wrong answer. Where a defect falsifies a
> decision above, the decision carries the amendment inline.

- **What does a query print, and under what relation name?** Reopened 2026-08-16
  (user call), against the 2026-08-03 decision above. Not a move in the opposite
  direction: the user-facing language on both input and output is the thing this
  engine exists to get right, and it is not yet clear this shape is right. Nothing
  changes until this is answered, and **the 2026-08-03 widening is not built
  meanwhile** — building a widening of a shape under review buys the wrong thing
  twice. `notes/query-answer-shape.md` holds the long form; it already carries the
  measured boundary table, the Soufflé survey and two rejected alternatives, and
  gains the axes below.

  **Five axes, separable, and answering them as one is the trap**, stated with
  their evidence in the note: (1) does an unnameable query **error or synthesize** —
  the real disagreement, where tsdl errors so that nothing is ever printed under an
  invented relation name; (2) if it synthesizes, is **`answer/N`** the right name,
  colliding as it does with every other query's answer; (3) the **projection
  hazard**, which is unfixed in *both* engines and must not be read as their
  advantage; (4) **yes/no**, which has no real opposition and could be settled
  early; and (5) **naming at the invocation** — `-q 'conflict: p(X), q(X)'` — which
  keeps the language total, which neither engine has considered, and which is
  exactly where being a CLI and being an embedded library diverge.

  **No recommendation is recorded**, deliberately: one would turn a question the
  user asked to have thought through into a decision with a default.
  — §5/§14, `notes/query-answer-shape.md`, `notes/tsdl-cross-project-review.md`.
  - ***Answered 2026-08-17*** — by the two decisions of that date, and taking the
    axes one at a time was load-bearing rather than procedural: **4 settled 1**.
    Emitting `answer(true)` for the yes/no case collides with a 1-column bool
    projection, which is only visible once the two axes are held apart, and it is
    what forced `holds/1` and left `answer/N` in place (axis 2). Axis 1 answered
    *synthesize*, on the ground that erroring's measured support covers its
    prevention and never its recovery. Axis 5 was taken, and turned out to be the
    **fix for axis 3** rather than an ergonomic extra — which is also why the
    question's own framing of it as a CLI option was wrong: it belongs in the
    grammar, since a file program carries the same hazard.
    - Two narrower questions **survive**, both in §14's *Not covered*: the named
      form's grammar and printing, and **which query a synthesized answer answers**
      — found while reviewing this session's work, since two boolean queries in one
      run both print `holds(true).` A `%` comment is the direction, an extra
      argument having been rejected for the reason 2026-08-16 rejected
      provenance-as-facts.

- **Does a failed `as` conversion error, or yield `absent`?** The one piece of the
  cast left undecided (2026-07-25). `"abc" as int` can be a structured error,
  consistent with §8's treatment of division by zero and overflow; or it can be
  `absent`, consistent with §4/§9, where absent inputs are skipped *and reported*,
  so the loss would not be silent. The second answer only became possible when the
  absent value shipped (milestone 8) — it did not exist when §8's edge-case rule was
  written, which is why the rule does not already settle it. The trade-off is real:
  erroring means one unconvertible cell in a 170k-row import kills the whole query,
  which is hostile to the exploratory analysis §13 is built for; yielding `absent`
  makes dirty columns queryable but quietly reclassifies "malformed" as "missing". A
  strict/`try_` pair is the third option and doubles the vocabulary. Decide with the
  implementation. — §4/§8/§9.
  - ***Answered 2026-08-16*** — settled by the 2026-08-16 decision, and in
    **neither** of the two forms this question offered: the answer is `absent` for
    an *unrepresentable* conversion and an error for a *lossy* one, so the
    question's either/or was the wrong shape. Its own framing is what shows why —
    it names `"abc" as int` and the 170k-row import, both unrepresentable cases,
    and never asks what `2.5 as int` should do. The `try_` pair stays rejected.
    The narrower question that survives is **making the reclassification
    visible**: this trades "malformed" for "missing", and the skip-and-report
    machinery it would need rides on the aggregate literal, not on an
    `=`-assignment. That is a ROADMAP item.
    - ***Answered 2026-08-18*** — reported at the conversion (§4/§12), and the
      `=`-assignment turned out to be the *right* carrier rather than the missing
      one: lowering hoists every compound argument there, so one site covers every
      cast that can fail on data. What the ROADMAP item did not anticipate is that
      the report needs a **silencer** — §16.9's guarded idiom is a correct program
      the warning would otherwise fire on — and that a cast inside a *comparison*
      is not covered at all, the row being filtered out before a premise exists.
- **Should an accumulating recursion be able to *terminate* rather than merely be
  warned about?** Opened 2026-08-18 by the decision above, which chose to classify
  and not reject, and so left `path_cost` running exactly as long as its data lets
  it. **Limit predicates** are the candidate and the literature is settled
  (Kaminski, Cuenca Grau, Kostylev, Motik, Horrocks, IJCAI 2017; references.md
  group 1): restrict a numeric column to keep only the least or greatest value per
  group of key arguments, and cost-accumulating transitive closure becomes finite
  and decidable — it computes shortest paths instead of enumerating every path's
  cost. §4's `declare` is the obvious surface, since it already exists and already
  says extra things per column:

  ```datalog
  declare path_cost(from, to, min cost).
  ```

  What makes it a milestone rather than a rider: the fixpoint no longer accumulates
  a set but a per-group extremum, so §6's `T_P`, §9's aggregation and §11's
  provenance (which derivation survives when a better value replaces it?) all move.
  The rejected cheaper alternative is a bounded-counter recognizer, and why it does
  not reach this case is in `notes/termination.md`. — §4/§6/§9/§10.
- **What does a caller learn from a run that completed?** Opened 2026-08-18 by the
  feature-complete stock-take (`notes/taking-stock-2026-08-18.md`). Today: exit `0`,
  and silence on stdout, whether the program derived nothing or answered *no*. Two
  needs converge on one answer and neither is built. **Integrity constraints** —
  there is no `constraint`, no denial rule, and no way to say *this must never
  happen*, while the skill advertises consistency checking; a violation is
  discoverable only by parsing stdout. And the **truncation contract** (2026-08-16),
  whose one open piece is the CLI shape, already states that withholding "has to
  mean an exit code and a stdout discipline, not a return field". Designing them
  apart would give one exit code two vocabularies. What is genuinely open: whether a
  constraint is a language construct or a query convention, how many codes the
  vocabulary needs, and whether stdout stays a pure fact stream when a run has
  something to say and no rows to say it with. — §12/§14/§15.
  - ***Answered 2026-08-18*** — by the decision of that date, and all three
    sub-questions answered *smaller* than they were asked: a convention rather than
    a construct, three codes rather than a taxonomy, and stdout unchanged. The
    merge was worth making anyway, but not for the reason the question gives: the
    two needs never contended for the code, because **withholding turned out to
    have no trigger at all**. Measuring that was only prompted by designing them
    together, which is the argument for the merge that this entry could not make.
    - One question **survives and is new**: a conversion that fails inside a
      *comparison* narrows a filter with nothing to report it on (§12's *Not
      covered*), because the row is gone before a premise exists to carry the
      count. `ROADMAP.md`.
- **Builtin scalar functions with no relational spelling** — `abs`, `length`,
  `lower`, `substr`. *User-defined* scalar functions were declined 2026-07-25 (a
  rule already is one), and conversion is now the `as` cast, so what remains is the
  narrow case of builtins a rule cannot express. Deferred until a consumer needs
  them — the discipline used for statistical reducers, TSV, and database loading.
  If they land, the `ident (` ambiguity above must be solved (scan-ahead is the
  candidate), and note `min`/`max` would coexist harmlessly with the aggregates,
  since `min { X | goal }` and `min(A, B)` differ by delimiter. — §5/§8.
  - ***Answered 2026-08-19 — in the shape, not the deferral.*** The temporal
    session's `std`-module decision settles what a builtin *is*: a **relation**
    from a gated module, so `std/math` and `std/text` have a home and the
    `ident (` ambiguity never arises — it only ever existed for a *call* in
    expression position. The scan-ahead candidate is therefore **not needed** and
    is withdrawn rather than parked. What survives is exactly the original
    deferral: these land when a consumer needs them. Note what this entry got
    right and for the wrong reason — it called the ambiguity the blocker, which
    was true, and assumed the fix had to be lexical, which was not.
- **Is the three-way body-grammar split deliberate?** Rule bodies are DNF; queries
  and aggregate goals are conjunction-only. §5 states the query half as an aside
  ("Queries stay conjunctive") and §9 the goal half in passing, but nothing says
  *why* one body form admits `;` and two do not. Either extend `;` or state the
  asymmetry as a decision. Lands on the agent surface, where the rule form is the
  only workaround. — §5/§9/§14.
- **How does an existence check answer?** `?- p("a"), q("b").` prints nothing and
  exits 0 whether or not it holds, and §5's ban on 0-arity atoms removes the
  obvious workaround. §14 records this as a closure gap; the sharper reading is
  that a yes/no question has no answer. A single *ground atom* is distinguishable
  (output vs. none), so the hole is specifically the conjunction. — §5/§14.

  ***Amended 2026-08-16*** — this is **axis 4** of the answer-shape question at the
  top of this section, not a separate item, and it is the axis with no real
  opposition. Answer it there.
- **Should `declare` define a predicate?** A `declare`d but factless relation
  still warns "referenced but never defined" (§12, 2026-07-23), so there is no way
  to say "intentionally empty" — even though `declare` is precisely the user
  asserting the relation exists. — §10/§12.
- **`absent` × negation and repeated occurrences — two logical laws currently
  fail.** _Needs a design session; do not patch ahead of it._ Found in the
  2026-07-25 review. §4 splits "same" into a *semantic* notion (unification:
  absent matches nothing, not even another absent) and a *structural* one (set
  dedup and canonical `Ord`). Matching straddles the split: `try_match` binds a
  **fresh** slot to a stored `absent` — that is how a missing cell flows to a
  head, and it is wanted — but every **later** use of that slot goes through the
  semantic notion, which absent always fails. A variable bound to `absent` is
  therefore both matched and unmatchable, and two laws break:

  - **Non-contradiction.** `q(X) :- p(X), not p(X).` derives `q(absent)` when
    `p(absent)` is stored: the positive atom binds `X := absent`, and the
    anti-join's pattern (matching semantically) matches nothing, so the negation
    is satisfied too. P ∧ ¬P — in an engine that advertises consistency checking
    (§1). SQL's analogue, `NOT IN` over a `NULL`, returns *no* row, so this is not
    a two-valued simplification of three-valued logic; it is further from sound
    than either. Practically: absent rows leak through every "things with no …"
    query.
  - **Idempotence of conjunction.** `p(X), p(X)` selects strictly less than
    `p(X)` — the second occurrence, by then bound to `absent`, unifies with
    nothing. Repeating a literal is a no-op in any logic; here it changes the
    answer.

  Both are pinned as `#[ignore]`d tests asserting the *sound* behaviour
  (`a_fact_never_satisfies_its_own_negation`,
  `repeating_a_body_literal_does_not_change_the_answer`), so a resolution has an
  executable acceptance criterion. Note the B1 differential **cannot** find these:
  both evaluators implement the same semantics and agree: it is the semantics
  that is wrong, not one implementation of it.

  **Only the negation cell is anomalous.** Checked against SQLite (2026-07-25),
  which is three-valued where we are two-valued:

  | | `p(X)` | `p(X), p(X)` | `p(X), not p(X)` |
  |---|---|---|---|
  | SQL | `1, NULL` | `1` | `∅` |
  | datalog today | `1, absent` | `1` | **`absent`** |
  | with structural negation | `1, absent` | `1` | `∅` |

  So the failure of idempotence is *not* the outlier — SQL drops the `NULL` from
  a self-join too, and no one calls that a bug, because it is the direct
  consequence of `NULL ≠ NULL`, which is precisely the foreign-key-blowup
  protection `absent` exists to give (§4). What is anomalous is deriving the
  contradiction, and only we do it.

  Candidate directions, each moving a different part of the §4 split — the point
  of the session is to choose deliberately rather than let matching decide:
  1. **Anti-join tests structural membership** — *the leading candidate.* `not
     p(X)` with `X` bound to absent asks "is the tuple `p(absent)` in the
     relation?" — it is, so the negation fails. The argument is structural, not
     merely convenient: **a negated atom binds nothing, so it is a membership
     test rather than a join.** The blowup concern is about positive joins that
     bring in new bindings; membership is exactly what `Value`'s derived `Eq` is
     for (§4). It reaches SQL's answer set on both laws while staying two-valued,
     and costs nothing on the FK side because it never touches it. Costs: negation
     and joining then use different notions of "same", which must be *stated* in
     §4 rather than discovered; and "things with no …" changes behaviour on rows
     whose key is absent — `food(F), not measurement(F, _)` would stop reporting a
     food whose id is missing. That is SQL's answer and defensible, but it is a
     real change on the motivating dataset and should be decided, not absorbed.
  2. **A bound slot re-matches structurally.** Once a variable holds a value,
     later occurrences compare structurally; only *unbound*-to-stored matching
     applies the semantic rule. Restores both laws at once — but it buys
     idempotence by giving up `absent ≠ absent` in joins, i.e. the FK blowup
     comes back, and it diverges from SQL in the opposite direction. Restoring a
     law SQL also lacks looks like a poor trade for the property the value model
     was built around.
  3. **Absent never reaches a negated or repeated position** — a safety error
     instead of a semantics. Cheapest to specify, but the front end cannot know
     which columns carry absent (it is a *value*, not a type, §4), so this is
     likely undecidable in practice.

  Whatever is chosen must also say what `?- p(X), not p(X).` means for a
  *query*, and whether provenance's `AbsentPattern` follows the same rule (it
  currently delegates to `Value::unifies_with`). **Sequence this before the
  negation-scheduling item below**: that one changes *when* a negated atom runs,
  and there is no sense implementing it against semantics about to be replaced.

  ***Answered 2026-07-29*** — direction 1, as the decision entry above records.
  Two notes the entry does not carry. **The sequencing instruction was not
  followed and the cost was zero**: negation scheduling landed 2026-07-25, four
  days ahead of this, because `bugs/001` made it a live soundness fix. It
  changed *when* the anti-join runs, and this changed *what refutes it* — the
  two turned out orthogonal, so the stated worry ("no sense implementing against
  semantics about to be replaced") was wrong about its own risk. **And the two
  laws listed here were one item but not one phenomenon**: only
  non-contradiction was the defect.
  — §4/§7/§11.

  **Scope note (2026-07-25 review): aggregate group keys are a fourth site, and
  belong in this session.** A group key bound to `absent` matches nothing in its
  own goal, so the group is real but empty — verified: with `k(absent).` and
  `v(absent, 99).` stored, `g(K, N) :- k(K), N = count { C | v(K, C) }.` yields
  `g(absent, 0)`. That is *not* a contradiction and it agrees with SQL (a
  correlated subquery keyed on `NULL` counts nothing), so it is defensible — but it
  is a fourth place where the §4 structural/semantic split is decided by
  mechanism rather than by choice. Direction 1 above (negation goes structural,
  joins stay semantic) leaves the language with **four** sites and three answers:
  joins semantic, group keys semantic, negation structural, set dedup and
  canonical `Ord` structural. §17 already flags that direction's cost as "negation
  and joining then use different notions of 'same', which must be *stated* in §4
  rather than discovered" — the same applies here, so the session should settle all
  four in one table rather than three plus an omission.

  ***Consequences 2026-07-29 — taken as written.*** §4 now carries the four-site
  table, and the group-key row was re-verified unchanged by the fix (`g(absent,
  0)` still). The note earned its keep: the session's own plan had three sites
  and would have shipped the omission this predicted.

- **Negated atoms are outside the dependency schedule** — **resolved 2026-07-25**
  (Decisions above): negations join the schedule, reading the argument variables
  that something else in the body binds, with the ready ones kept in an early
  phase for pruning. The sketch recorded here was taken up essentially as
  written. What this entry got *wrong* is worth keeping: it argued the
  restriction was a uniform expressiveness limit with a clean workaround and
  "**not** a silent-wrongness bug". `bugs/001` disproved that — `not q(X + 1)`
  was neither refused nor correct — and the entry had already reasoned its way to
  the conclusion that the rule justified itself circularly, which should have
  been the louder signal. — §7/§8/§10.

- **Optional/absent value design** — **resolved 2026-07-24** (Decisions above): a
  single first-class, two-valued `absent` value; `absent ≠ absent` semantically but
  structurally identical for sets (sorts first); `is [not] absent` presence test;
  the literal producible but not body-matchable; aggregates skip-but-report; a
  uniform null→absent, type-neutral import policy. Written into §4/§8/§9/§13/§16.8.
  Remaining open: the full `X is Y` null-safe-equality generalization of `is`; the
  the all-absent import-column type (currently resolved by use-site flow, else
  unconstrained). — §4/§8/§9/§13.
- **Database loading** (SQLite/DuckDB files via the reserved `table "…"`
  grammar; Postgres via DuckDB attach) and **TSV**: deferred until a real
  consumer appears; the format table and dispatch errors already name them. — §13.
- **Filter pushdown for large sources:** v1 eagerly materializes every import;
  a program touching ten rows of a 10 GB parquet file pays for all of it. The
  ratified path/`table` syntax leaves room to push selections down into SQL if
  a consumer hits the wall. — §13.
- **Module namespacing:** v1 module imports share one global namespace;
  qualified names/visibility deferred until a real consumer needs them. — §13.
  - ***Amended 2026-08-19.*** It has a consumer now — `std/time`'s relations are
    scoped to the programs that import them — and the answer taken was **not**
    namespacing: `std/` **gates** a set of names rather than qualifying them, so a
    collision is an error instead of being resolved by a prefix. The question
    stands unchanged for user modules; what the consumer showed is that gating
    and qualifying are separable, and that the cheaper one covered this case.
- **Recursive/monotonic aggregation:** v1 rejects recursion through an aggregate
  via stratification (§9, resolved 2026-07-24); how far to take a fixpoint
  semantics for recursive aggregates (Zaniolo et al., `references.md`) is the
  remaining open question. — §9.
- **Statistical & collection reducers:** the five (`count`/`sum`/`min`/`max`/`avg`)
  shipped 2026-07-24; `median`/`stddev`/`variance`/`percentile` (the node reserves
  a param slot) and `collect`/`string_agg` (needs a first-class collection value,
  §4) are deferred until a consumer needs them. — §9.
  (Aggregate syntax `op { Expr | Goal }` and grouping resolved 2026-07-24, §9.)
- **Provenance query syntax:** `?why <fact>` is provisional across CLI and API; also
  decide proof-tree JSON encoding. (§16.6) — §11/§14.

  ***Answered 2026-08-16*** — the surface is decided (Decisions above): one union of
  `proof` / `underivable` / `unknown`, the sigil a cost hint, a near-miss a rule.
  What stays open is the **rendering**, the **JSON encoding**, and sequencing
  against the truncation contract, whose distinction `unknown` is.

  ***Answered 2026-08-21*** — and the **form that asks** is decided too (Decisions
  above): `?why` / `?whynot` as §5 statements, carried into a `-q` argument by the
  same parse-classifier that reads a query body, exit-code-neutral, with recording
  provisioned per sigil. The rendering shipped 2026-08-21. **What stays open is the
  JSON encoding alone** — the truncation-contract sequencing came with the exit-code
  ruling, `unknown` being the arm it distinguishes.
- **Semiring provenance under negation:** parked research thread with a worked
  sketch in `notes/semiring-provenance.md` — the derivation store is already a
  boolean provenance circuit, `Premise::Absent` a factored dual token; candidate
  work: `?whynot` with minimal repairs, tropical cheapest-proof selection for
  token economy. — §11.

  ***Amended 2026-08-16*** — `?whynot` with minimal repairs left this thread: it was
  designed **without** a semiring, a near-miss being a *rule* rather than a binding,
  which is what bounds it. What stays parked is the algebra and tropical
  cheapest-proof selection.
- **Provenance as facts:** the Datalog-in/Datalog-out closure property suggests
  `?why` output should also have a fact-shaped form (e.g. derivation edges as
  ground facts), so provenance can itself be piped back in and queried with
  Datalog — not just rendered as a tree or JSON. Design alongside §11; exercise
  with a §16 example. — §11/§14.

  ***Answered 2026-08-16 — in the negative*** (Decisions above). A proof tree is not
  a fact and joins a fact stream on no terms; emitting it as derivation edges invents
  relations the program never declared or flattens a tree into rows that no longer
  compose. It rides in `%` comments instead, which keeps the closure property
  byte-for-byte.
- **`--format json` scope:** the only surviving §14 CLI question — a documented
  future *edge* feature (structured errors §12, provenance §11), deferred as
  low-value 2026-07-23 with the data path staying Datalog-native. (`-q`
  semantics, multiple flags, stdin/`-`, optional source, and output ordering all
  resolved 2026-07-23; the agent skill definition is `docs/agent-skill.md`.) — §14.
- **Dependency choices:** `serde` for the API — decide as §14 stabilizes.
  (Lexer/parser + CLI: resolved 2026-07-22 / 2026-07-23 — hand-rolled, zero new
  deps. Source backends: resolved 2026-07-23 — DuckDB, default-on feature, §13.)
