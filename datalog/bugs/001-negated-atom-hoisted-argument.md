---
id: 001
title: A compound argument inside a negated atom is silently misread as a wildcard
severity: soundness
area: lower
spec: ["§5", "§7", "§9", "§10"]
found: 2026-07-25
resolution:
---

`not q(X + 1)` is accepted and returns wrong answers. The equivalent hand-hoisted
spelling of the same rule is a structured error, so one conjunction has two
meanings depending on how it is written — the same class of defect as the
aggregate group-key bug fixed 2026-07-25, and again on the silent side.

## Repro

```datalog
p(1). p(2). p(3). q(3).
r(X) :- p(X), not q(X + 1).
?- r(X).
```

Prints **nothing**, exit 0. Correct answer: `r(1).` and `r(3).` — for `X = 1`,
`not q(2)` holds; for `X = 2`, `not q(3)` fails; for `X = 3`, `not q(4)` holds.

The hand-hoisted form — which §5 says is the *same IR* — is rejected instead:

```datalog
r(X) :- p(X), Y = X + 1, not q(Y).
% semantic error: unsafe negated atom in `r`: variable `Y` does not occur in a
% positive body atom
```

Same defect via an aggregate argument, and in a query:

```datalog
r(X) :- p(X), not q(count { Y | p(Y) }).   % always empty, whatever q holds
```
```sh
datalog f.dl -q 'p(X), not q(X + 1)'       # empty, no error
```

A positive atom with a compound argument is correct (`r(X) :- p(X), q(X + 1).`
works), so the defect is specific to negation.

## Root cause

`src/lower.rs:970-994`. The negation safety check tests only *named* variables,
justified in place by:

> A `None`-named slot here is necessarily wildcard-fresh inside this very literal
> — `VarScope::fresh` never enters the name map, so a fresh slot occurs at exactly
> one term position in the whole clause.

That invariant held when it was written (§17, 2026-07-20, "No wildcard tagging
needed for negation safety"). **Inline arithmetic in atom arguments (§17,
2026-07-22) broke it.** The hoist at `src/lower.rs:556-562` mints a `None`-named
fresh slot that occurs at *two* term positions: the `=`-assignment target and the
negated atom's argument. So `var_names[var.0].is_some()` is false, the check skips
the slot, and the engine's anti-join treats it as an open wildcard — the rule
silently degrades to `not q(_)`, "no `q` fact at all", which is why a single
unrelated `q(3)` suppresses every row.

Lowering does hoist correctly; the IR is the same shape as the hand-written form.
Only the *check* distinguishes them, on a criterion (`is_some()`) that stopped
tracking what it was standing in for.

## Fix sketch

The check needs "is this slot bound elsewhere in the body" rather than "does this
slot have a name". Two candidates:

1. **Tag hoist-generated slots.** Restores the 2026-07-20 invariant explicitly
   instead of relying on it: a slot introduced by hoisting is not wildcard-fresh
   and must be positively bound like a named one. Smallest change, and it keeps
   negation safety exactly as §10 states it.
2. **Ask the schedule.** A wildcard-fresh slot is bound *nowhere*; a hoisted slot
   is bound by its assignment. `safe_bound_vars`/`schedule` already distinguishes
   these, and `src/lower.rs:1295-1310` shows the pattern. This is also the shape
   the design sketch in §17's "negated atoms are outside the dependency schedule"
   open question proposes, so it may fall out of that work — but it should not
   *wait* for it (below).

Either way the error must name the *source* expression, not the invisible
generated slot: `push_unsafe` renders a `None`-named slot as `` `_` ``, which
would produce "variable `_` does not occur in a positive body atom" for a rule
containing no `_`.

## Acceptance criteria

- `r(X) :- p(X), not q(X + 1).` either evaluates correctly (`r(1). r(3).`) or is
  a structured error naming `X + 1`. It must not silently return the wrong rows.
- The inline and hand-hoisted spellings agree: same answers, or the same error.
  Worth a property — the inline/hoisted equivalence claimed by §5 and checked in
  `api::tests` for positive atoms extends to negated ones.
- Same for an aggregate argument and for the query form.

## Fallout

- **§17's 2026-07-20 entry is falsified** and needs an amendment: "no wildcard
  tagging needed" rested on fresh slots occurring at exactly one term position,
  which inline hoisting ended two days later. The entry currently reads as settled.
- **ROADMAP's negation item 2 loses its deferral rationale.** It defends waiting
  on the grounds that the restriction is "uniform, both orderings are refused
  identically ... an expressiveness limit, not silent wrongness". The inline
  spelling is neither refused nor correct, so it is not uniform and it *is* silent
  wrongness.
- **Independent of the absent × negation session** (negation item 1). This is
  about safety-checking generated slots, not about what `absent` means under
  negation, so it can be fixed first and should be.
