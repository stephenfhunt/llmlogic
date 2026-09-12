---
id: 012
title: a syntax or semantic error costs a full fact load before it is reported
severity: usability
area: import
spec: ["§12"]
found: 2026-09-12
resolution: fixed 2026-09-12, with relation pruning (ROADMAP § Performance)
---

The program is parsed and checked *after* its imports are materialised, so a
typo costs whatever the fact base costs to load. On a small program that is
invisible. On a large one it is most of an exploratory session.

## Repro

Grafana's frontend facts (4.0M facts; `symbol.jsonl` alone is 489 MB / 831,625
rows). Any program importing `schema/structure.dl`:

```datalog
import "schema/structure.dl".
oops(P) :- file(path: P), contains(P, ".story.")   % missing final period
```

```
$ /usr/bin/time -f "%e s %M KB" datalog oops.dl
syntax error [unexpected-token]: …
10.9 s  2793288 KB
```

**11 seconds and 2.7 GiB to be told about a missing period.** Measured across an
exploratory session on that fact base: every failed program cost the same 11 s,
and the floor for a *correct* program importing the same schema is the same 11 s
— so the check is free to run first and there is nothing to trade off.

Both agents dogfooding the skill hit this, independently, and one of them changed
how it worked because of it (batching four or five `-q` per run to amortise the
load, which is a workaround for the load, not for the check).

## Root cause

Order of operations in the driver: imports are resolved and their JSONL
materialised, then the program is lowered and typechecked. Nothing in the
parse or the semantic check needs a single fact — arity, safety, stratification,
field names and types all come from `declare`s in the schema files, which are
source, not data.

The one exception to name in the fix: a `declare` may itself come from an
imported *schema* file, so the import graph still has to be read. Reading
`schema/*.dl` is not the expensive part; reading `facts/*.jsonl` is.

## Acceptance criteria

A test that a program with a syntax error, and one with a semantic error, each
exit 2 without opening any `facts/*.jsonl`. The strongest form asserts it by
pointing the program at a fact directory whose JSONL files are unreadable
(mode 000) and checking the diagnostic still arrives.

## Fallout

Nothing recorded depends on the current order. It raises the value of the
`--check`-only mode nobody has needed yet, and it interacts with nothing in §17.

**Carried by a ROADMAP item.** *Load only the relations the program names*
(`../ROADMAP.md` § Performance, one of the two ruled next on 2026-09-12) has to
decide which relations to open from the rule graph, which means the program is
already parsed and checked when the first file is read. This closes as a side
effect of that item; it does not need its own fix.

## Resolution

**Fixed 2026-09-12**, by *load only the relations the program names* (§17 that
date). With every data import under an explicit schema, the program is parsed,
resolved and **lowered** before any import is read, and only the imports a goal
reaches are read after that.

**The diagnosis held for semantic errors and not for syntax errors.** Parsing
has always run before loading: on `vs/base` the unchanged binary reports a
missing period in 0.00 s, and the 10.9 s repro above could not be reproduced. A
lowering error did pay — an unknown field, 5.5 s / 1.10 GB — and now costs
0.00 s / 8 MB.

**The acceptance criterion was narrowed, deliberately** (the user's call, same
day): the early check is lowering, not typecheck. "Nothing in the semantic check
needs a single fact" is false for types. A declared column type is not an
inference constraint, so a fact-free typecheck rejects valid programs
(`bugs/014`). A type error still waits for the load, which is now only the
reached relations, and a rejection over that partial load is re-checked over a
full one. The test is `tests/system.rs`
`a_program_error_arrives_before_any_fact_is_read`, over a mode-000 fact file, for
a syntax error and a lowering error.
