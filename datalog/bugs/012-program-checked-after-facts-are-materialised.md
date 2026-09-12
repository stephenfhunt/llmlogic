---
id: 012
title: a syntax or semantic error costs a full fact load before it is reported
severity: usability
area: import
spec: ["§12"]
found: 2026-09-12
resolution:
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
