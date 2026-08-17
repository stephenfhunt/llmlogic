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

A `-q` argument is one of two things, decided by parsing it:

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

Notes:
- A trailing `.` is optional (`-q 'p(X)'` and `-q 'p(X).'` are the same).
- Later `-q` flags can use predicates defined by earlier ones. Each `-q` prints
  its own block of answers, in order.
- A file may contain its own `?-` queries; those answers print first, then the
  `-q` blocks.

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
  "no rows", never "something went wrong" — errors go to stderr and change the exit
  code.

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
