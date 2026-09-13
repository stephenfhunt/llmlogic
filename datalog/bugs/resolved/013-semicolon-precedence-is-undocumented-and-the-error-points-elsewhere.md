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

## Resolution

**Fixed 2026-09-13.** Both criteria landed.

1. **The language guide** (`skill/SKILL.md`) now says `;` binds looser than `,`,
   shows the expansion, and says there are no parentheses. It gives the idiom for
   a filter on alternatives inside a longer body: a rule of their own
   (`pick(B) :- B = "a" ; B = "b".`), joined. `spec.md` §5 already stated the
   precedence, so the gap was the guide alone.
2. **The diagnostic names the cause.** `ast::Clause` carries
   `disjunct: Option<Disjunct { index, of, span }>`, set by the parser only for a
   rule `;` split. `lower::check_rule_safety` gives an unsafe alternative **one**
   error:
   - it names every head variable that alternative leaves unbound;
   - it says `alternative i of n` and that `;` split the whole body;
   - it sits at the alternative's own span;
   - it suggests the idiom above.

   The repro's twelve errors are now three. A rule written without `;` renders
   exactly as before: `experiments/reference/malformed/unbound-head.err` is
   byte-identical.

**The open question in Fallout is settled: no wrong answer.** A split whose
alternatives are all safe is the stated semantics (§5 decision 7). It derives what
the separate rules would, and the engine cannot tell an intended disjunction from a
misread one. So no lint was added: it would fire on correct programs. The guide is
the defence there.

**Property**: testing.md **A16** runs over 2–4-alternative rules. Each rule has one
alternative forced safe and one forced unsafe, so it cannot pass vacuously. It
asserts one error per unsafe alternative, naming its position and the split, at its
span. *Mutations (both killed, A16 and the pipeline test)*: drop the split note; use
the rule's span.
