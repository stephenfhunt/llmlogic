---
id: 014
title: a declared temporal column fixes no type, so an empty relation makes `A + @1d` a false type error
severity: wrong-answer
area: typecheck
spec: ["§4", "§8", "§13"]
found: 2026-09-12
resolution:
---

A column's declared type — `declare ev(at: timestamp).` or an import schema
`as ev(at: timestamp)` — is **checked after** inference, never used as a
constraint by it. Only a fact fixes a column's type. So a program that is
well-typed by its declarations is rejected when the relation has no rows, and
accepted as soon as it has one.

## Repro

```sh
: > empty.jsonl
echo '{"at": "2026-01-01T00:00:00"}' > one.jsonl
```

```datalog
import "empty.jsonl" as ev(at: timestamp).
later(T) :- ev(at: A), T = A + @1d.
?- later(T).
```

```
semantic error [type-mismatch]: cannot tell what `+` means here — duration is
temporal, and the other operand's type is never fixed, so the result could be a
point or a duration (§8) (give the other operand a type — a temporal literal, a
column, or an `as` cast)
exit 2
```

The same program over `one.jsonl` prints `later(@2026-01-02T00:00:00).` and exits
0. The suggestion is itself wrong: the other operand *is* a column with a type.
Same result with an in-program `declare` and no facts. Measured on the binary at
`3a9487e`.

Severity is `wrong-answer` rather than `usability`: the program is valid and does
not answer. An extraction whose layer came out empty (a repository with no
matching files) hits exactly this.

## Root cause

`src/typecheck.rs` `gather` pins column types from `program.facts` only
(`:415`). Declared `field_types` enter at two places, neither of them inference:
the post-hoc declared-vs-inferred sweep (`:841`, "a declared column inference
never constrained is simply unrefuted") and the `TypeEnv` reporting fallback
(`:894`). `fall_back_binary` (`:771`) runs before either, sees one temporal
operand and an unfixed one, and errors.

## Acceptance criteria

The repro answers on the empty file. **The general property**: typecheck's
verdict on a program whose every column is declared with a type does not depend
on the facts — a biconditional over generated fully-typed programs with and
without their fact tables (`testing.md`'s rejection-claim corollary).

## Fallout

- **It is the soundness condition for checking a program before its facts load.**
  With declared types as constraints, a fact-free typecheck is authoritative for
  fully-typed schemas; without, it falsely rejects this repro. That is why
  `bugs/012`'s fix stops at lowering (§17, 2026-09-12). Carried by the ROADMAP
  item *a column's type is known before its rows*.
- Making declarations constrain changes what `bugs/resolved/008`'s wording rests
  on — its message re-derives a column's type from the facts before claiming it —
  so the fix is a design decision with a §17 entry, not a patch.
