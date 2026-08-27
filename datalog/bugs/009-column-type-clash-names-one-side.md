---
id: 009
title: a column-level type clash names one occurrence, so neither side of the conflict is locatable
severity: usability
area: typecheck
spec: ["§4", "§12"]
found: 2026-08-26
resolution:
---

Two diagnostics come out of the same `type-clash` code, and only one of them says
what §12 requires. The **variable** form names both participants and both types:

```
semantic error [type-clash]: variable `Q` in rule 0 has type int but variable `W` in rule 0 has type float (at 6:38)
```

The **column** form names one label, two type words, and a span that belongs to
whichever occurrence happened to arrive second:

```
semantic error [type-clash]: `p` column 0 is used as both symbol and string (at 2:4)
```

It does not say *which* occurrence made the column a symbol, where that occurrence
is, or — the part that costs the most — **how each term was read**. §12 is explicit
that this is the standard: "Naming has not gone away — `variable Q in rule 0` still
says *which* variable — the position says where to look for it, which is what stops
the message degrading with program length." The column form degrades exactly that
way.

## Repro

```datalog
p(alice).
?- p("alice").
```

Prints (exit 2):

```
semantic error [type-clash]: `p` column 0 is used as both symbol and string (at 2:4)
```

Should print both sides, each with its own span and the term as written — e.g.

```
semantic error [type-clash]: `p` column 0 is used as both symbol and string:
  symbol `alice` (at 1:3) and string `"alice"` (at 2:4)
```

### A second repro, worse

Two facts clashing carry **no position at all**:

```datalog
p(1).
p("x").
?- p(X).
```

```
semantic error [type-clash]: `p` column 0 is used as both int and string
```

No `(at line:column)` is rendered, because neither side of this clash is the
"incoming" occurrence of a `gather` step that set `self.at` — both are facts, and
the span is only carried for whatever the checker was looking at. §12 requires an
error to carry "the **span** it is about"; here it carries none.

## Root cause

`src/typecheck.rs:310` — `set_type` reports a clash from `self.label[root]`, the
*existing* type and the *incoming* type. It cannot name the other side because
`self.ty: Vec<Option<TypeName>>` records **what** type a class resolved to and not
**where that came from**: the span and term of the occurrence that fixed it are
dropped at the moment the type is stored.

`union` (`src/typecheck.rs:335`) reads well for the opposite reason — it has two
labelled slots in hand, so it can name both. The asymmetry is not in the message
strings; it is in what the union-find carries.

The incoming side's span is already available (`self.at`, set during `gather`), so
the fix is one field: store `Option<(TypeName, Span)>` — with the term as written,
or a label built at the site — and have `set_type` render both.

## Acceptance criteria

- `#[ignore]`d until fixed, in the style of `bugs/002`'s criterion
  (`src/api.rs:1182`): for the repro above, the rendered diagnostic contains
  **two** distinct `line:column` positions, and the earlier one resolves to the
  fact on line 1.
- **The general property this is one instance of:** *every* `type-clash`
  diagnostic names both participating occurrences. File it as a test over the
  corpus of clash-producing programs — both forms, not just the column one —
  because the variable form satisfies it today only by accident of having two
  slots, and nothing pins that.

## On the suggested fix, and what was deliberately not proposed

§12 provides a **suggested-fix** field, and this is a candidate site. One
suggestion is always true and is recommended: when the two clashing terms have the
same text and differ only by quoting, name the two spellings that would agree
(`alice` or `"alice"`).

A second, tempting suggestion is **rejected**: *"did you mean a variable? variables
begin with an uppercase letter."* It would misfire on the repro above, where
`p(alice).` is a perfectly good fact and the query is simply the wrong spelling —
the engine would be guessing intent from a shape it cannot see. Naming both
occurrences and how each was read makes the variable mistake self-evident without
the guess, which is why that is the primary fix here. Recorded so it is not
re-proposed as an improvement.

## Fallout

Nothing recorded is falsified. The pinned diagnostic in
`experiments/reference/malformed/type-clash.err` covers the *variable* form and is
unaffected; a fix here changes the column form, which that corpus does not pin —
which is itself worth noting, since the corpus was built to catch diagnostic drift
and this message sits outside it.

## How it was found

`experiments/results/run-20260827T015701Z`, the `qwen3:14b` gate run — a local
subject on the `engine-forced` arm wrote

```datalog
employee(name, department, salary) :- read('employee.csv', name, department, salary).
?- employee('Carol', department, _).
```

using lowercase identifiers where variables were meant, then rewrote and re-ran
`datalog query.dl` **fifteen times** against this message without recovering, and
spent the whole cell to `no-answer`. The mistake is a one-token fix the diagnostic
never points at. This is the first defect the harness has produced about the
engine rather than about itself, which is what it was built for.
