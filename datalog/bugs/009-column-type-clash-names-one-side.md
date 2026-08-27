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

## 2026-08-27 — the half that needed no new machinery, landed

**Still open.** The message now names both terms; neither side is *located* yet,
and both `#[ignore]`d criteria stay red on purpose.

What the transcript's program prints today:

```
semantic error [type-clash]: `employee` column 0 is used as both symbol `name` and string `"Carol"` (at 3:4)
```

`symbol `name`` is the fix for what actually cost the fifteen rewrites: the
message never said the token had been *read* as a constant. `set_type_from`
carries the literal that pinned a class (`Fixed { ty, witness }`), so a clash
renders the term as written — `alice` against `"alice"` — using `print_value`,
which already spells a symbol bare and a string quoted.

**The `union` form deliberately does not do this.** Its two sides are two slots,
not two terms: a variable's class inherited its type from whatever pinned the
column, so "variable `Q` has type int `4`" would read as Q's own value. Pinned by
`a_variable_type_clash_names_types_without_borrowing_a_literal`, and the
reference corpus stays byte-identical.

**Correcting the root cause above, which was written from `set_type` alone.**
"The fix is one field" is wrong twice:

1. **`Error` carries exactly one span** (`error.rs:399`) and `Display` renders
   exactly one `(at …)`. There is no secondary-span concept, and the typechecker
   never sees the source — positions are resolved later by `locate_all` at the
   API boundary — so it cannot put a resolved `1:3` in the message text. Locating
   both sides needs the diagnostic model extended, and §12 specifies that
   rendering normatively under a stability contract: a spec change and a §17
   entry, not a typecheck patch.
2. **Facts carry no span at all.** `ir::Fact { pred, tuple }` derives `Eq`/`Hash`
   as a set member (§17: a fact derived multiple ways is one fact), so a span
   cannot go on `Fact` without changing its identity — and 108 uses of `.facts`
   construct it directly. A parallel `fact_spans` table on `ir::Program` is the
   shape that keeps both. Note that an *imported* row's honest location is not a
   program span at all but the source name plus row and column (§12), which is
   its own design question.

So what remains is one design pass over the diagnostic model, not a fix: related
spans on `Error`, how `locate_all` resolves them, how they render (and serialize
for a future `--format json`), and what locates an imported row. Both criteria
above become green from that work.
