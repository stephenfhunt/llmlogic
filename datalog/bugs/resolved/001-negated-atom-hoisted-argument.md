---
id: 001
title: A compound argument inside a negated atom is silently misread as a wildcard
severity: soundness
area: lower
spec: ["§5", "§7", "§9", "§10"]
found: 2026-07-25
resolution: fixed 2026-07-25 — negated atoms joined the dependency schedule (§17)
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

## Resolution

**fixed 2026-07-25** — `not q(X + 1)` now evaluates correctly (`r(1). r(3).`),
as do both hand-hoisted spellings.

**Candidate 2 (ask the schedule), taken further than sketched.** Fixing the
*check* alone was not enough: `src/schedule.rs` ran every negation in phase 2,
ahead of all builtins, so a corrected check would have flagged a slot the engine
still could not have bound. Negated atoms therefore joined the dependency
schedule outright (ROADMAP negation item 2, closed with this). A negation reads
the argument variables something *else* in the body binds; ready ones keep the
early phase so anti-joins still prune before aggregates. Safety relaxed to "bound
by the body" in §7/§10; full entry in §17.

**Candidate 1 (tag hoist-generated slots) dropped.** It would have kept the
restriction and merely reported it honestly. Two reasons against: the restriction
had no justification left — §17's own open question had already concluded it
justified itself by the phase order and the phase order by itself — and naming
the source expression in the error, which this file required, needed an
IR-expression renderer (`print.rs` handles AST only) built solely to explain a
rule we intended to delete. It would also have made `not q(1 + 1)` a semantic
error.

**The diagnosis held**, with one omission: the file located the fault in the
check and named the phase order only in passing, as part of candidate 2. The
phase order was the *second, independent* fault — either alone leaves the bug.
Two further sites the file did not name also encode the rule: the engine's
`validate_body` (hand-built-IR contract) and `src/engine/naive.rs`, whose
`matches` filtered every negation before running any builtin. The oracle mattered
most: left alone it would have agreed with a wrong engine.

**Cost elsewhere.** §7/§10 wording relaxed; §17 gains a decision entry and closes
the "negated atoms are outside the dependency schedule" open question (kept, with
what it got wrong); `schedule.rs`'s module docs argued the removed restriction and
were rewritten. Tests: testing.md **C7**
(`negation_over_a_computed_argument_is_spelling_independent`), a
`CompRule::NegShift` generator variant carrying the shape into B1's engine/oracle
differential, two `schedule.rs` unit tests, and five end-to-end tests in
`api::tests`. `schedules_bind_before_they_read` now covers negations too.

**Sequencing overridden.** ROADMAP put `absent` × negation (item 1) first, on the
grounds that it decides what a negated atom *means*. Checked before proceeding:
`m(K, X), not q(X)` with a stored `q(absent)` already returns every row, so that
hole is reachable through an ordinary positive binding — this change adds
spellings reaching an already-broken cell rather than creating one. Both
`#[ignore]`d tests still fail identically (`[[Absent]]`), so item 1 is untouched
and still owns the anti-join's matching rule.
