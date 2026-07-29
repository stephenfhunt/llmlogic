---
name: datalog
description: >-
  Solve reasoning problems by encoding them as Datalog and running a real logic
  engine instead of reasoning step-by-step in prose. Reach for this whenever a
  task involves transitive or recursive relationships (ancestry, reachability,
  dependency/transitive closure), multi-hop deduction over a set of facts and
  rules, stratified negation ("things with no ..."), arithmetic thresholds and
  filtering, or constraint/consistency checking (scheduling, assignment, logic-
  grid puzzles). The engine derives the complete, correct set of consequences and
  returns them as facts — reliable where an LLM's own chain-of-thought is
  error-prone.
---

# datalog — a logic engine you run

`datalog` takes facts and rules and computes **all** their logical consequences,
then answers queries. Both directions speak Datalog: answers come back as ground
facts in the same syntax you feed in, so one run's output is valid input to the
next.

## When to use this (and when not)

**Use it** when correctness depends on exhaustively and consistently applying
rules over facts:
- transitive / recursive relationships — ancestry, reachability, "can A reach B",
  transitive closure, dependency graphs;
- multi-hop deduction — chaining several rules to reach a conclusion;
- negation over a closed set — "which X have no Y", orphans, unmatched items;
- arithmetic thresholds / filtering over structured records;
- constraint & consistency checks — scheduling, assignment, logic-grid puzzles,
  "is this configuration possible / who must be where".

**Skip it** for one-off arithmetic, free-form text, or anything with no
facts-and-rules structure — the encoding overhead isn't worth it there.

The payoff is twofold: **correctness** (the engine can't skip a case or lose
track mid-chain) and **token economy** (state facts once, then ask narrow
queries and read back only the derived answers).

## How to run it

Call the co-located executable `./datalog` (this directory). Give it a program —
facts, rules, and optionally `?- …` queries — as a file, or `-` for stdin, plus
zero or more one-shot `-q` queries:

```
./datalog [<file> | -] [-q <query>]…
```

Write the program to a temp `.dl` file (or pipe via stdin). Exit codes: **0** ok
· **1** program error (structured messages on stderr — read them, they include
spans and did-you-mean hints and are meant to guide a fix) · **2** usage error.

### `-q` one-shot queries — the two forms
- **A query body** — a bare atom or a comma-separated conjunction, answered
  directly:
  ```sh
  ./datalog family.dl -q 'ancestor("alice", X)'
  ./datalog people.dl -q 'person(name: N, age: A), A >= 18'
  ```
- **A define-and-select rule** — a `head :- body` clause; the rule is added and
  its head is queried for you:
  ```sh
  ./datalog family.dl -q 'grandparent(X, Z) :- parent(X, Y), parent(Y, Z)'
  ```
A trailing `.` is optional. Multiple `-q` apply in order (a later one may use a
predicate an earlier one defined); each prints its own block of answers.

## Reading the output

Answers print as **canonical ground facts**, one per line, deduplicated and
sorted — and they round-trip as input, so runs compose over pipes:
- **symbols** print bare (`red`); **strings** are double-quoted (`"red"`) — these
  are distinct types, so quote string data and leave enum-like symbols unquoted;
- **floats** always keep a decimal point (`3.0`, not `3`);
- a single positive-atom query re-emits that atom with bindings substituted;
  other bodies emit `answer(...)` facts over the query's variables.

## Datalog in 30 seconds

- **Facts**: `parent("alice", "bob").` — relations lowercase, string data quoted.
- **Rules**: `head :- body1, body2, … .` — `,` is "and"; `;` in a body is "or".
- **Variables** are Capitalized (`X`, `Who`); `_` is a wildcard.
- **Recursion** is allowed and is the whole point:
  ```
  ancestor(X, Y) :- parent(X, Y).
  ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
  ```
- **Negation**: `not covered(X)` (stratified — no recursion through negation).
- **Comparisons/arithmetic**: `A >= 18`, `M = N + 1`; strict numeric types.
- **Aggregation**: `N = count { C | parent(P, C) }` — set-builder `op { Expr | Goal }`,
  `op` one of `count`/`sum`/`min`/`max`/`avg`. Grouping is implicit: one result per
  binding of the rule's *other* variables (here `P`, so it's children-per-parent).
  `sum`/`avg`/`min`/`max` skip `absent` inputs (an all-absent or empty group →
  `absent`) and say so on stderr when they do; `count` counts bindings, so count
  only present values explicitly with `count { A | m(A), A is not absent }`.
  `min`/`max` also work on strings/symbols. A `_` inside the `{ … }` is a witness
  dimension, not a "don't care": `count { P | parent(P, _) }` counts *edges*, not
  distinct parents. In a `-q` query, bind the group key outside the aggregate —
  `-q 'parent(P,_), N = count { C | parent(P,C) }'` groups by `P`, while
  `-q 'N = count { C | parent(P,C) }'` is one global count.
- **Missing data** is the value `absent` (an empty CSV cell, a JSON/DB null).
  Any comparison with it is *false* and any arithmetic yields `absent`, so test
  it explicitly with `A is absent` / `A is not absent`. Writing `A = absent` or
  `A != absent` is an error that says so — both would be always-false. A numeric
  column with gaps still counts as numeric.
  **Negation is the one place `absent` does match**: in `p(X), not q(X)`, an `X`
  bound to `absent` is refuted by a stored `q(absent)`. So a "things with no …"
  query does not report rows whose key is missing — if you want those, ask for
  them with `X is absent` rather than expecting the negation to surface them.
- **Queries**: `?- ancestor("alice", Who).`
- **Imports** load external data or split a program across files:
  ```
  import "data/parents.csv" as parent.   % CSV/JSONL/Parquet/http(s) → a relation
  import "lib/rules.dl".                  % splice another Datalog file (no `as`)
  ```
  Field names and types come from the source; add an explicit schema
  (`as parent(parent: string, child: string)`) for headerless files. Paths are
  relative to the importing file. Bulk facts belong in a CSV/JSONL import rather
  than thousands of inline `fact(...).` lines.

## Worked example

```sh
cat > /tmp/family.dl <<'DL'
parent("alice", "bob").
parent("bob", "carol").
parent("carol", "dave").
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
DL
./datalog /tmp/family.dl -q 'ancestor("alice", Who)'
# ancestor("alice", "bob").
# ancestor("alice", "carol").
# ancestor("alice", "dave").
```

## More

- **Recipes** for whole use cases — `recipes/source-analysis.md` (analysing a
  codebase: extract facts with a real parser, import, ask; the traps).
- Full usage guide: [`../docs/agent-skill.md`](../docs/agent-skill.md).
- Canonical example programs (recursion, negation, arithmetic, named args):
  `../spec.md` §16 and `../tests/programs/*.dl`.
- Try-it tasks with expected answers: [`../EXPERIMENTS.md`](../EXPERIMENTS.md).
