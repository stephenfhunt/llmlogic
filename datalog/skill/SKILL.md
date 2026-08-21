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

Write the program to a temp `.dl` file (or pipe via stdin).

**Exit codes answer the question**, on `grep`'s vocabulary: **0** rows found (or
no queries asked) · **1** no rows — every query ran and none answered · **2** the
run did not answer, which covers a usage problem and a program error alike
(structured messages on stderr — read them, they include spans and did-you-mean
hints and are meant to guide a fix). `0` and `1` are answers; **`≥ 2` means it did
not answer**, so branch on that boundary.

That is also how you **check a constraint** — there is no `constraint` keyword,
because a query already is one:

```sh
# nobody double-booked? then deploy
./datalog roster.dl -q 'not double_booked(_, _)' && deploy
```

Phrase the check affirmatively, as here: errors are `≥ 2`, so `&&` cannot fire on
a program that failed to compile. And impose it on a program whose queries *are*
the checks — **any** query answering makes the run `0`.

### `-q` one-shot queries — the forms
- **A query body** — a bare atom or a comma-separated conjunction, answered
  directly:
  ```sh
  ./datalog family.dl -q 'ancestor("alice", X)'
  ./datalog people.dl -q 'person(name: N, age: A), A >= 18'
  ```
- **A named query** — the same body with `name:` in front, which is what the
  answer is published as:
  ```sh
  ./datalog people.dl -q 'adult: person(name: N, age: A), A >= 18'
  ```
- **A define-and-select rule** — a `head :- body` clause; the rule is added and
  its head is queried for you:
  ```sh
  ./datalog family.dl -q 'grandparent(X, Z) :- parent(X, Y), parent(Y, Z)'
  ```
- **An explanation goal** — `?why <fact>` or `?whynot <fact>`; see *When the
  answer looks wrong* below.
A trailing `.` is optional. Multiple `-q` apply in order (a later one may use a
predicate an earlier one defined); each prints its own block of answers.

## Reading the output

Answers print as **canonical ground facts**, one per line, deduplicated and
sorted — and they round-trip as input, so runs compose over pipes:
- **symbols** print bare (`red`); **strings** are double-quoted (`"red"`) — these
  are distinct types, so quote string data and leave enum-like symbols unquoted;
- **floats** always keep a decimal point (`3.0`, not `3`);
- **dates, timestamps and durations** print with their `@` sigil
  (`@2026-08-19`, `@2026-08-19T10:30:00`, `@1d12h`) — that is also how you write
  them, so answers carrying them compose back as input;
- **name the query and the answer wears that name**: `?- adult: person(N, A), A >=
  18.` prints `adult(...)` facts. Optional, but it is the right default when the
  output will be read by anything other than you — see the warning below;
- an unnamed query whose atoms account for every variable you asked about re-emits
  **those atoms** with bindings substituted — so `?- person(N, A), A >= 18.`
  answers in `person` facts, and a filter alongside the atom does not change that;
- otherwise you get synthesized `answer(...)` facts over the query's variables —
  which happens when something outside the atoms binds a column, such as an
  aggregate result;
- a question with no variables answers **`holds(true).`** if it holds and prints
  nothing if it does not — or `name(true).` when you named it.

**Reading `person` facts back does not mean you have all of them** — an unnamed
answer is the rows your query matched, under the real relation's name, and nothing
in the output says so. **Name the query** and that ambiguity is gone:
`-q 'adult: person(N, A), A >= 18'` answers in `adult` facts, which no source
relation wears. A name is also a real relation, so a later `-q` can read it. Note
it publishes **every** variable the body binds — that one is `adult/2`; to choose
the columns, write the rule (`-q 'adult(N) :- person(N, A), A >= 18'`).

## When the answer looks wrong

An empty answer and a query whose join quietly connects nothing print exactly the
same thing. Two goals tell them apart. **`?-` enumerates; `?why` / `?whynot`
interrogate one of the rows it returned.**

```sh
# a row you did not expect — what derived it?
./datalog /tmp/family.dl -q '?why ancestor("alice","dave")'
# % why ancestor("alice", "dave")
# % 0  ancestor("alice", "dave")  by ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)
# % 1    parent("alice", "bob")  [fact]
# % 1    ancestor("bob", "dave")  by ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)
# % 2      parent("bob", "carol")  [fact]
# % 2      ancestor("carol", "dave")  by ancestor(X, Y) :- parent(X, Y)
# % 3        parent("carol", "dave")  [fact]

# a row you expected and did not get — how far did each rule get?
./datalog /tmp/family.dl -q '?whynot ancestor("zoe","dave")'
# % whynot ancestor("zoe", "dave")
# % not derivable
# % 0  ancestor(X, Y) :- parent(X, Y)
# % 1    blocked at parent(X, Y)
# % 1    repair: add parent("zoe", "dave")
# % 0  ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y)
# % 1    blocked at parent(X, Z)
# % 1    repair: none names one fact — parent("zoe", _) leaves a slot open
```

- **A goal names one fact**, so no variables: `?why ancestor("alice", W)` is an
  error telling you to run `?- ancestor("alice", W)` first and ask about one row.
- **Pick the sigil by what you saw, and a wrong guess still answers** — `?why`
  over a fact that does not hold gives the trace, `?whynot` over one that does
  gives the proof.
- **A proof bottoms out in the facts it rests on**, tagged `[fact]` or `[fact from
  "employees.csv"]`. When an answer is wrong, that is usually where the problem
  is — check those rows before rereading the rules.
- **A trace names the first literal that blocked**, under the bindings that
  reached it, and one **repair**: a step that advances *that rule*, not a promise
  that the goal then holds. When the blocked predicate is itself derived, the
  repair is the next question — `repair: ask ?whynot …`.
- **The answer is `%` comments**: it never enters the fact stream and never
  changes the exit code, so it is safe to append to a `&& deploy` pipeline.

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
- **Dates and times**: `@2026-08-19`, `@2026-08-19T10:30:00`, and durations
  `@1d12h` / `@90m` / `@500ms`. A CSV or Parquet date column is typed for you —
  no cast needed. The arithmetic is *points and vectors*: subtracting two dates
  gives a duration, adding a duration to one moves it, and **dividing two
  durations is the only way to get a number** — which is where you name the
  unit: `N = (Closed - Opened) / @1d` is a number of days. There is deliberately
  no `duration as int` (a number of *what*?), and no month or year duration.
- **Arithmetic inside a recursion can run forever**, and the engine says so on
  stderr before it starts rather than refusing to run:
  ```
  cost(X, Z, C) :- cost(X, Y, C1), edge(Y, Z, C2), C = C1 + C2.
  ```
  This is correct on an acyclic graph and never finishes on a cyclic one, so if
  you see `warning: value-creating recursion`, either be sure the relation it
  names is acyclic or move the arithmetic out of the recursive rule. A recursion
  that only passes stored values along (`ancestor` above) always terminates.
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
  ```
  import "std/time".                      % year/month/day/hour/minute/second
  ```
  `std/time` is how you reach a component or a period: `year(D, Y)` binds the
  year (and `day(D, 15)` filters), `truncate(D, month, M)` gives the first of
  the month — one sortable key to group by. They are only in scope where that
  import is written, which is what lets them take names your own data might
  also use.

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
