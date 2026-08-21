# Using `datalog` from an agent

`datalog` is a small logic engine you drive from the command line. You give it
facts and rules; it derives new facts and answers your queries. **Datalog is the
interchange format in both directions** — query answers come back as ground facts
in the same syntax you feed in, so one run's output is valid input to the next.

The point for an agent is **token economy**: put a large fact base in a file once,
then ask narrow questions with `-q`. You read back only the derived facts you
asked for, not the raw data. This is the piecemeal pattern you already use with
`jq` over JSON — here it is Datalog-native, and a follow-up `-q` *is* the jq.

## Invocation

```
datalog [<file> | -] [-q <query>]…
```

- The positional argument is a program **file**, `-` for **stdin**, or **omitted**
  (the base program is empty — useful with only `-q`). At most one source.
- Each `-q` appends a one-shot query (see below). Multiple `-q` apply in order.

Exit codes: **0** success · **1** program error (lex/parse/type/eval; structured
messages on stderr) · **2** usage error (bad arguments, unreadable file).

## `-q` query forms

A `-q` argument is one of three things, decided by parsing it:

1. **A query body** — a bare atom or a comma-separated conjunction. Answered
   directly.

   ```sh
   datalog family.dl -q 'ancestor("alice", X)'
   datalog people.dl -q 'person(name: N, age: A), A >= 18'
   ```

2. **A define-and-select rule** — a clause with a body (`head :- …`). The rule is
   added to the program and its head is queried for you.

   ```sh
   datalog family.dl -q 'grandparent(X, Z) :- parent(X, Y), parent(Y, Z)'
   ```

3. **An explanation goal** — `?why <fact>` or `?whynot <fact>`. Already a
   statement, so it is passed through as written. See *Explaining an answer*.

   ```sh
   datalog family.dl -q '?why ancestor("alice","dave")'
   ```

Notes:
- A trailing `.` is optional (`-q 'p(X)'` and `-q 'p(X).'` are the same).
- Later `-q` flags can use predicates defined by earlier ones. Each `-q` prints
  its own block of answers, in order.
- A file may contain its own `?-` queries; those answers print first, then the
  `-q` blocks. Explanations print after **all** answers, in program order.

## Output shape (so you can pipe it back)

Answers print as **canonical ground facts**, one per line, deduplicated and
sorted. Formatting is stable and round-trips as input:

- **Symbols** print bare (`red`); **strings** are double-quoted (`"red"`). These
  are distinct types — quote string data, leave enum-like symbols unquoted.
- **Floats** always keep a decimal point (`3.0`, not `3`) so they re-read as
  floats.
- **A query may name its answer**, and then that name is what prints:
  `?- adult: person(N, A), A >= 18.` gives `adult(...)` facts. The name is a real
  relation — a later query or rule can read it — and it is exact sugar for the rule
  whose head is the columns you asked about.
- An **unnamed** query whose positive atoms account for every variable you asked
  about re-emits **those atoms** with bindings substituted
  (`?- ancestor("alice", W).` → `ancestor("alice", "bob").` …). A filter beside the
  atom does not change that, so `?- person(N, A), A >= 18.` answers in `person`
  facts; and a fully **ground** conjunction prints all of its atoms.
- Other bodies emit synthesized `answer(...)` facts over the query's variables —
  which is what happens when something outside the atoms binds a column, an
  aggregate result being the usual case.
- A question with no variables answers **`holds(true).`** if it holds, and prints
  nothing if it does not — `name(true).` when it is named. Silence therefore means
  "no rows", never "something went wrong" — errors go to stderr and the exit code
  tells the two apart (below).

One trap worth knowing: an **unnamed** answer printed under a real relation's name
is **the rows your query matched, not the whole relation**. Piping
`?- ancestor("alice", W).` onward gives the next program an `ancestor` holding only
alice's rows, and nothing in the output says so. Naming the query is the fix — the
answer then wears a name no source relation has.

Because output is valid input, runs compose over pipes:

```sh
datalog people.dl -q 'adult(N) :- person(name: N, age: A), A >= 18.' \
  | datalog - -q 'adult(N), N != "bob"'
```

The first run materializes `adult/1` facts; the second reads them from stdin and
filters. Or write an intermediate result to a file and query it again later.

**Naming a query is not quite the same as writing that rule**, and the difference
is the columns: a named query publishes *every* variable its body binds, so
`-q 'adult: person(name: N, age: A), A >= 18'` gives `adult/2` — name and age —
where the rule above chose `adult/1`. Name the query when you want the whole row
under a name; write the rule when you want to pick the columns.

## Importing external data (§13)

Instead of inlining a large fact base, **import** it. A data import binds a
tabular file (or an `http(s)` URL) to a relation; a module import splices in
another Datalog file:

```datalog
import "data/parents.csv" as parent.     % CSV → parent/2 (base facts)
import "edges.jsonl" as calls.           % JSONL, Parquet, and http(s) URLs too
import "lib/rules.dl".                    % no `as` = splice another .dl file
```

- **Schema inference.** Field names come from the source (a CSV header row, JSON
  keys); column types come from the data, read by the *same* literal rules as
  in-program values (all-int → int, int/float → float, `true`/`false` → bool,
  else string; an empty CSV cell makes its column a string). So an import means
  exactly the facts you'd get by typing those rows as literals.
- **Explicit schema** overrides inference and is required for a headerless CSV:
  `import "data/parents.csv" as parent(parent: string, child: string).` — the
  types coerce each cell, and a cell that won't coerce is an error naming the
  file, row, and column.
- **Named access** works on a wide imported table with no `declare`:
  `manager(N) :- employee(name: N, title: "manager").`
- **Paths** resolve relative to the importing file's directory. A module import
  (`import "lib.dl".`) is included once even through diamonds or cycles; queries
  belong in the top-level program, not in imported modules.

Imports need the engine's default `duckdb` feature (present in the packaged
skill binary). A build without it reports a structured error for any data
import.

## The exit code, and checking a constraint

The code answers the question, on `grep`'s vocabulary:

| code | meaning |
|---|---|
| `0` | **rows found** — a query printed an answer (or the program had no queries) |
| `1` | **no rows** — every query ran, none answered |
| `2` | **did not answer** — a usage problem or a program error, on stderr |

`0` and `1` are answers; **`≥ 2` means the run did not answer**, so branch on that
boundary rather than on `2` exactly.

This is how you check a constraint — there is no `constraint` keyword, because a
query already is one. Write the check as a negation and let the code carry it:

```sh
# nobody double-booked? then deploy
datalog roster.dl -q 'not double_booked(_, _)' && deploy
```

**Phrase the check affirmatively, as here.** Errors are `≥ 2`, so `&&` cannot fire
on a program that failed to compile — while the inverted form (`-q
'double_booked(P, D)' || deploy`) deploys on a syntax error just as readily as on
a clean roster.

One thing to know: **any** query answering makes the run `0`. So impose your check
on a program whose queries *are* the checks — a `-q` added to a file that asks its
own questions is not a check, because the file's own answers set the code.

## Explaining an answer

`?-` enumerates rows; `?why` and `?whynot` interrogate **one** of them. Reach for
them when a row surprises you, or when one you expected is missing — an empty
answer and a join that quietly connects nothing print the same thing.

```sh
datalog family.dl -q '?why ancestor("alice","dave")'
datalog family.dl -q '?whynot ancestor("zoe","dave")'
```

A `?why` that finds the fact prints a **proof**: one line per node, depth as both
a leading integer and indentation, each derived node citing the rule that fired
and each leaf tagged `[fact]` or `[fact from "employees.csv"]`. Those leaves are
where a wrong answer usually comes from — check the rows before rereading the
rules.

A goal that does not hold prints a **failure trace**: one entry per rule whose
head could have produced it, the premises that rule satisfied, the first literal
that blocked, and a **repair**. A repair is a *step*, not a promise — it advances
that rule past the block, which need not be enough to derive the goal. Some name
no fact at all: a blocked derived predicate answers `repair: ask ?whynot …`, a
pattern with an unbound slot says so, a refuted negation names the row that
refuted it (there is no retraction in the language), and a slot bound to `absent`
cannot be matched by any row.

Three properties worth relying on:

- **A goal names one fact**, so it takes no variables. `?why ancestor("alice", W)`
  is an error that tells you to run the query first and pick a row.
- **Either sigil answers either way.** `?why` over a fact that does not hold gives
  the trace; `?whynot` over one that does gives the proof, and says that it re-ran
  to get it. Pick by what you saw — a wrong guess costs nothing but a line.
- **An explanation is `%` comments.** It never enters the fact stream, so
  `datalog p.dl -q '?why …' | datalog - -q '…'` composes exactly as without it,
  and it never changes the exit code, so it is safe to append to `&& deploy`.

## Errors

Errors are **structured prose** with source spans and, for common mistakes,
did-you-mean hints (e.g. `=<` → `<=`, an uppercase relation name, a chained
comparison). Read them directly — they are meant to be actionable without extra
tooling. There is no JSON error format; the data path stays Datalog.

## A minimal end-to-end example

```sh
echo 'parent("alice","bob"). parent("bob","carol").
ancestor(X,Y) :- parent(X,Y).
ancestor(X,Y) :- parent(X,Z), ancestor(Z,Y).' \
  | datalog - -q 'ancestor("alice", Who)'
# ancestor("alice", "bob").
# ancestor("alice", "carol").
```
