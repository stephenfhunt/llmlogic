---
id: 008
title: a rule-level type clash manufactures a second, false error about the fact table
severity: usability
area: typecheck
spec: ["§4", "§12"]
found: 2026-08-23
resolution: fixed 2026-08-24 — the sweep is skipped once inference is poisoned, and its message re-reads the facts before claiming them (§17)
---

A type clash between two **declared** columns inside a rule is reported twice:
once correctly, against the rule, and once falsely, against the data. The second
message asserts that the fact table holds values of a type it does not hold. It
names the *opposite* column from the one at fault, so an agent that follows it
edits a correct declaration or correct data and leaves the actual error standing.

Found 2026-08-23 while probing diagnostics for `experiments/reference/malformed`.
It is the same class as `003` (the spec asserting something untrue), moved into
the diagnostics: **the engine says something about the data that is not so.**

## Repro

```datalog
declare item(name: string, qty: int, weight: float).
item("bolt", 4, 1.5).
total(N) :- item(qty: Q, weight: W), N = Q + W.
```

Prints (exit 2):

```
semantic error: type error: variable `Q` in rule 0 has type int but variable `W` in rule 0 has type float
semantic error: type error: `item.weight` is declared as float but its values are int
```

The second line is false. `item.weight` is declared `float`, its one value is
`1.5`, and the declaration and the data agree. Only the rule is wrong. The same
fact base with the same declaration typechecks clean the moment the rule stops
clashing:

```datalog
declare item(name: string, qty: int, weight: float).
item("bolt", 4, 1.5).
ok(N) :- item(name: N).            % exit 0, no diagnostics
```

Should print: the first line only.

**The accusation follows operand order, which is the tell.** Swapping the
addition to `N = W + Q` keeps the true error and flips the false one to
`` `item.qty` is declared as int but its values are float ``. Neither column's
values changed.

**It needs a `declare`.** Dropping the declaration leaves only the true error —
the false claim is manufactured by the declared-vs-inferred check, not by
inference. A clash against a *literal* (`W > 1`) also reports only the true
error: the second class has no column behind it to accuse.

## Root cause

`src/typecheck.rs:298` — `union()` reports a conflict and then **merges anyway**,
keeping `Some(x)`, the first operand's type:

```rust
(Some(x), Some(y)) if x != y => {
    self.errors.push(/* the true error */);
    Some(x)
}
```

So after the failing unification `item.qty` and `item.weight` are one union-find
class carrying a single type. `finish()` then runs the declared-vs-inferred sweep
at `src/typecheck.rs:733` unconditionally, over that poisoned class: for each
declared column it compares `self.ty[root]` against the declaration. One of the
two merged columns must now contradict its own declaration, and it is always the
second operand's — which is why the direction flips with the operand order.

`inferred` there is a unification result. The message renders it as *"its values
are"*, which is a claim about the fact table that nothing ever scanned.

## Acceptance criteria

- The repro prints exactly one diagnostic, the rule-level one, and still exits 2.
- `` `item.weight` `` and `` `item.qty` `` appear in no message for that program.
- A *genuine* declared-vs-actual mismatch with no rule error still reports —
  `declare item(name: string, weight: float). item("bolt", 2).` must keep saying
  `` `item.weight` is declared as float but its values are int ``.
- Operand order changes nothing about which diagnostics appear.

**Candidate fix**: skip the declared-vs-inferred sweep in `finish()` when
`self.errors` is already non-empty. Inference is known-poisoned at that point, so
every column conclusion drawn from it is unreliable — not just the ones that
happen to look wrong. This suppresses a true column error until the rule error is
fixed, which is the right trade: one re-run against a false accusation.

The narrower alternative — mark classes merged by a *failing* union and exclude
only those — reports more per run, at the cost of carrying a poison bit through
the union-find. Worth it only if a program with both kinds of error is common,
which nothing suggests.

**The general property this is one instance of** (`bugs/README.md` asks for it,
and `001` is what happens when it is skipped): *no diagnostic asserts a fact
about the program's data that was not read from the data.* Testable as a property
over generated clashing programs — every message naming a `relation.column` and
the words "its values are" must survive re-deriving that column's type from the
facts alone. File it with the fix.

## Fallout

- **§12's error taxonomy** (ROADMAP, *Errors & API edges*) gains a case: a
  diagnostic's *truth*, not just its shape and its code. A stable code vocabulary
  does not help if the message under the code is false.
- **`experiments/`**: the engine arm hitting a type clash is handed a message
  pointing at the wrong file. It is not a grid confound — no fixture carries a
  declared column a task's arithmetic clashes on — but it is a plausible
  round-waster, and `signals.py`'s rounds-to-correct is where it would show up.
- No §17 decision is falsified. §4's declared-type check is right to exist; it is
  running at a point where its input is already known bad.

## Resolution — 2026-08-24

Two changes in `src/typecheck.rs`, and **neither alone is the fix**.

**The suppression.** `finish()`'s declared-vs-inferred sweep now runs only when
`self.errors` is empty. The recommended blanket form was taken over the poison
bit: `set_type` leaves a class carrying the incumbent type without merging, so a
bit would have to be maintained at two sites, for a program shape — a rule error
*and* a genuine column error at once — that nothing suggests is common.

**The wording, which this file's own acceptance criteria did not cover.** The
sweep's message is emitted wherever inference contradicts a declaration, and
inference reaches a column from rules as well as from facts. A second instance
was found while fixing the first, with no other error to suppress:

```datalog
declare p(x: int).
s("a").
r(S) :- p(x: S), s(S).
```

`p` holds **no facts at all**, and the engine said `` `p.x` is declared as int
but its values are string ``. So the message now re-derives the column's type
from `program.facts` alone and speaks about *values* only when the values say so
— otherwise `is used as`. The genuine data mismatch is unchanged, which is
criterion 3.

This is the half that carries the general property. It shipped as **C15** in
`testing.md`, over `arb_well_typed_program` with a contradicting signature
asserted on every column, against an oracle that re-reads the facts in the test
module rather than calling the engine's own derivation. Both mutations are
recorded on its catalog line.

**What the pin caught.** `experiments/reference/malformed/type-clash.err` pinned
both diagnostics deliberately, the second annotated as false — so the corpus
moving is what says the fix landed, exactly as `bugs/README.md` intends a
tripwire to work.

**Not falsified:** §4's declared-type check was right to exist. It was running
at a point where its input was already known bad, and saying *"its values are"*
of a conclusion it had not read from any value.
