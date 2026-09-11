---
id: 011
title: a variable-form type clash names its classes' root slots, which can be a wildcard in another rule
severity: usability
area: typecheck
spec: ["§4", "§12"]
found: 2026-09-10
resolution: fixed 2026-09-10 — `union` labels the two slots it is unifying, not their roots
---

The variable form of `type-clash` is meant to name both participants, and §12
leans on it ("`variable Q in rule 0` still says *which* variable"). It names the
two **roots** of the union-find classes being merged instead, and a root is
whichever slot the class grew from. Once classes span rules, that is routinely a
slot in neither the rule at fault nor its head:

```datalog
cs(1, "x", "y").
v("b", C) :- cs(_, C, _).
v("c", I) :- cs(I, _, C), C = "y".
```

```
semantic error [type-clash]: variable `C` in rule 0 has type string but variable `_` in rule 0 has type int (at 3:14)
```

The fault is `I` in rule 1 — an int column bound into `v`'s string column. The
message names rule 0 twice, and one of its two participants is a wildcard. Found
building `skill/tools/ts-facts`'s `lib/checks.dl`, where a 50-rule program
reported `variable _ in rule 22`: finding the actual rule took a bisection.

## Root cause

`src/typecheck.rs` `union(a, b)` formatted `self.label[ra]` and `self.label[rb]`
(the roots of `find(a)` and `find(b)`) rather than `self.label[a]` and
`self.label[b]`. The types it prints are the classes' types, which *are* the two
slots' types — only the names were taken from the wrong nodes.

## Resolution

The message labels `a` and `b`. The repro now reads
`variable \`I\` in rule 1 has type string but \`cs\` column 0 has type int`, both
of which are the rule at fault. The corpus message pinned byte-exact by
`experiments/reference/malformed/type-clash.err` is unchanged (there the roots
and the slots coincide).

Acceptance: `tests/pipeline.rs` `a_variable_type_clash_names_the_slots_that_clash`.
Mutation: restoring the root labels reddens it.

Adjacent to, and not resolving, `bugs/009` — that one is the *column* form
locating only one side.
