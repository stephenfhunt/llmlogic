---
id: 013
title: `;` splits the whole clause, which no document says and the resulting error does not reveal
severity: usability
area: parser
spec: ["§3", "§12"]
found: 2026-09-12
resolution:
---

`;` in a body is disjunction, and it binds looser than `,` — so it splits the
*entire* clause, not the conjunct beside it. The skill's language guide says only
"`;` in a body is 'or'", gives no precedence and no example, and the error a
reader gets when they assume otherwise points at the symptom.

## Repro

```datalog
r(F, L, S, T) :- site(B, F, L), imports(file: F, line: L, specifier: S, target_file: T),
  B = "a" ; B = "b" ; B = "c" ; B = "d".
```

The author intended one rule with a four-way filter on `B`. What is parsed is
**four clauses**, three of which contain only their own equality. Output:

```
semantic error [unsafe-rule]: head variable F is not bound by the body — it must
occur in a positive body atom, or be bound by an `=`-assignment or an aggregate
result
```

…twelve times, once per unbound head variable per clause. Every one is true.
None of them says *your `;` split this rule*, which is the only fact the author
needs.

## Root cause

Precedence is correct and conventional; it is unstated. `bugs/resolved/002`
already covers a `-q` disjunctive rule, which is the same operator biting in a
different place — so this is the second report against `;` and the first one's
resolution did not reach the documentation.

## Acceptance criteria

Two things, and the second matters more than the first:

1. The language guide states the precedence and shows the parenthesised form
   (or says there is none, if the grammar has no grouping — in which case say
   *that*, since it changes how the filter must be written).
2. **The diagnostic names the cause.** When a clause containing `;` produces an
   `unsafe-rule`, the message should say the rule was split into N clauses at
   `;` and which one is unsafe. §12's standard is that a message names the
   participants and where to look; "head variable F is not bound" names the
   symptom of a transformation the author did not know had happened.

## Fallout

None recorded. Worth checking whether the same shape can produce a *silently
wrong answer* rather than an error — a split clause all of whose fragments happen
to be safe would derive extra rows with no diagnostic at all, which would make
this `wrong-answer` rather than `usability`.
