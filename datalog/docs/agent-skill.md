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
- A single positive-atom query re-emits that atom with bindings substituted
  (`?- ancestor("alice", W).` → `ancestor("alice", "bob").` …). Other bodies emit
  `answer(...)` facts over the query's variables.

Because output is valid input, runs compose over pipes:

```sh
datalog people.dl -q 'adult(N) :- person(name: N, age: A), A >= 18.' \
  | datalog - -q 'adult(N), N != "bob"'
```

The first run materializes `adult/1` facts; the second reads them from stdin and
filters. Or write an intermediate result to a file and query it again later.

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
