# Datalog Language Specification

**Status:** `v0.1-draft` — living document. Sections are drafted, refined, and
prototype-validated over time. Nothing here is final until its section is marked
*Stable* and the corresponding decision is recorded in §17.

This is both the specification and the design workspace for the `datalog` engine. It
carries the current design **and** an explicit decisions/open-questions log so the
reasoning behind each choice stays visible.

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
3. **Programmatic / agent API** — a JSON-in/JSON-out interface for agents.

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

*Status: TBD*

*To fill in: tokens, identifier rules, literal forms (symbols, strings, integers,
floats, booleans), comments, whitespace, reserved words.*

## 4. Data model & types

*Status: TBD*

*To fill in: constant kinds (atoms/symbols, strings, integers, floats, bools),
variables, term shape (flat vs compound terms), and the typing discipline
(untyped/dynamically-checked vs declared schemas).*

## 5. Syntax

*Status: TBD*

*To fill in: facts, rules, queries, declarations — with a concrete EBNF grammar.*

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

*Status: TBD*

*To fill in: range-restriction/safety rules that guarantee finite, well-defined
results; termination guarantees; treatment of recursion through negation/aggregation.*

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

*Status: TBD*

*To fill in: how external fact tables are declared and mapped onto predicates;
schema declarations; supported backends. Concrete source list is an open decision
(see §17). Imported facts are base facts and form the leaves of provenance trees.*

## 14. Programmatic / agent API

*Status: TBD*

*To fill in: the JSON-in/JSON-out surface — load facts, add rules, run queries,
fetch provenance, retrieve structured errors — co-designed with the CLI.*

## 15. Evaluation strategy (non-normative)

*Status: TBD*

*To fill in: stratified, semi-naive bottom-up evaluation; how provenance is captured
during the fixpoint; magic-sets as a future optimization.*

## 16. Worked examples

*Status: Draft*

> **Provisional syntax.** The programs below are written in a conventional,
> Prolog-/Datalog-style syntax chosen to be legible and to *drive* the design of
> §3–5. Concrete lexical/grammar decisions are not yet ratified — anything here may
> change. Each example calls out the design questions it raises; those are mirrored
> in §17.

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
*Raises:* fact vs rule vs query syntax; variable vs constant lexing (capitalized `X`
vs quoted `"alice"`?); comment syntax; how query results are shaped.

### 16.2 Negation — stratified negation-as-failure

```datalog
person("alice"). person("bob"). person("carol").
parent("alice", "bob").
parent("bob", "carol").

% someone with no recorded parent
root(X) :- person(X), not parent(_, X).
% expected: root("alice")
```
*Raises:* the `not` keyword and wildcard `_`; the safety rule that every variable in
a negated literal (and the head) must be bound by a positive body literal; how
stratification is computed and reported when violated.

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
*Raises:* aggregate expression syntax (`count { Var : Goal }` is provisional); how
grouping keys are determined (the head vars outside the aggregate); supported
aggregates and their result types; interaction with recursion/stratification (§9).

### 16.5 External fact source — import

```datalog
% declare an extensional predicate backed by an external table
% (declaration syntax is provisional)
.import parent(parent: string, child: string)
    from csv("data/parents.csv").

% rules then use `parent/2` exactly like in-program facts
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
```
*Raises:* declaration syntax and where declarations may appear; schema/column→arg
mapping; type declarations for imported columns; which backends ship first
(CSV/JSON, SQLite, Parquet, Postgres) (§13/§17); imported tuples are base facts and
anchor provenance leaves.

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

### Open questions

- **Surface syntax:** stay fully conventional (Prolog-/Datalog-style) or add
  ergonomic extensions? Leaning conventional for LLM reliability — to confirm in §2/§5.
- **Type discipline:** untyped/dynamically-checked terms vs declared predicate
  schemas (the latter helps imports and error quality). — §4/§13.
- **Term shape:** flat terms only, or allow compound/function terms? — §4.
- **Data sources priority order** (CSV/TSV/JSON, SQLite, Parquet/Arrow, Postgres):
  which land first? — §13.
- **Aggregation vs recursion:** how far to go on recursive aggregation semantics. — §9.
- **`=` semantics:** unification, assignment, or an equality builtin? (raised by §16.3) — §8.
- **Aggregate expression syntax:** `count { Var : Goal }` is provisional; ratify a
  form and grouping-key rule. (raised by §16.4) — §9.
- **Import declaration syntax:** the `.import pred(col: type) from src(...)` form is
  provisional; decide placement, schema mapping, and column typing. (§16.5) — §13.
- **Provenance query syntax:** `?why <fact>` is provisional across CLI and API; also
  decide proof-tree JSON encoding. (§16.6) — §11/§14.
- **Dependency choices:** lexer/parser approach, `serde` for the API, source
  backends — decide as the relevant sections stabilize.
