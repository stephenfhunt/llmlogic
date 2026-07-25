---
id: 004
title: §6 asserts a finite Herbrand universe and a finite-step fixpoint, which arithmetic falsified
severity: doc
area: spec
spec: ["§2", "§6", "§8", "§10", "§15"]
found: 2026-07-25
resolution:
---

§6 grounds the language's declarative semantics in a finiteness claim that stopped
being true when arithmetic landed (milestone 4). The engine behaves as the *code*
implies rather than as §6 says: a two-line program runs forever.

The spec-text half is recorded here. The behavioural fix — a static rule rejecting
value-creating recursion — is a design item in `ROADMAP.md` ("Termination &
value-creating recursion"), and **§6's text cannot be corrected until that lands**,
because what §6 should say depends on what the language decides to guarantee. This
is the first defect deliberately blocked on a roadmap item; both halves are one
phenomenon.

## The false assertions

§6 (`spec.md:337-350`) states:

> The **Herbrand universe** is the finite set of constants appearing in the program
> (facts, rule constants, and later imported values)

and

> For positive programs `T_P` is monotone over a finite lattice, so it has a least
> fixpoint, reached in finitely many steps — the **least Herbrand model**.

Both fail once §8 arithmetic exists. `N = M + 1` synthesises a value that appears
nowhere in the program, so the universe is not the set of constants appearing in
it, and the lattice it induces is not finite. The conclusion — a fixpoint reached
in finitely many steps — does not follow, and is false in general.

§15 inherits the problem more mildly: "The stratum stops when a round adds no new
facts" describes the loop accurately but never says a round *must* eventually add
none.

## Repro

```datalog
nat(0).
nat(N) :- nat(M), N = M + 1.
?- nat(X).
```

`timeout 10 datalog nat.dl` → exit 124, **no output at all** (not even partial
results, since answers print after the fixpoint). `N` is bound by the
`=`-assignment, which §10 accepts as a binder; `nat` depends on itself positively,
which stratification permits; so nothing in the front end objects. There is no
iteration cap, fact-count cap, or wall-clock budget anywhere in the engine
(grepped for all three).

## What is *not* wrong

Worth stating, because it narrows the fix and was checked:

- **§2's pillar is not a false promise.** "Predictable evaluation — termination and
  resource behavior an agent can rely on" sits in a section whose status is `TBD`
  and is explicitly labelled a *candidate* principle to ratify. It is unratified
  aspiration, not a broken guarantee — though it cannot be ratified as written.
- **§10 is honest.** It says "Still to fill in: termination guarantees" outright.
- **The engine is not wrong.** You cannot implement a false theorem. The defect is
  §6's claim; the behaviour change belongs to the roadmap item.

## Precision the fix must preserve

The tempting summary — "arithmetic makes the language Turing-complete" — is
**overclaiming**. Values are `i64` with overflow as a structured error, and floats
are finite, so the reachable state space is finite and the language is technically
decidable. The honest statement is narrower and still damning: the bound is
astronomical, so *"terminates after 2⁶³ iterations" is not a guarantee*, and the
finite-lattice argument §6 actually makes — which relies on the universe being the
program's constants — is unavailable regardless.

Datalog's PTIME data complexity result rests on the same finite-universe premise,
so any performance or governance claim derived from "it's Datalog, so it's PTIME"
inherits this defect.

## Acceptance criteria

- §6 states its finiteness premise in a form that is true of the language as it
  actually is — either scoped to the arithmetic-free fragment, or resting on
  whatever the termination rule guarantees.
- §6 or §10 says what happens to a program outside that fragment.
- §15's fixpoint description says what makes the loop terminate, not merely when it
  stops.
- §2's pillar is ratified in a form the implementation satisfies, or softened.
- The `nat` program above is rejected, or its non-termination is documented as
  in-scope behaviour. It must not be silently accepted while §6 claims it cannot
  happen.

## Fallout

- **Blocked on** `ROADMAP.md` "Termination & value-creating recursion" (*designing*).
- **Governance consequence, which is why this matters beyond tidiness.** The
  engine's pitch is running LLM-generated programs; a two-line program that hangs
  with no output and no cap is a denial-of-service vector on exactly that surface.
  Raised by the user 2026-07-25 while asking whether user-defined scalar functions
  would risk Turing completeness — the answer being that arithmetic got there
  first, and functions were never the exposure.
- **Not related to** the `as` cast (§17, 2026-07-25). Casts map a finite value set
  to a finite value set with no accumulation, so they create no new reachable
  values and are exempt from the termination rule. This was checked *before*
  choosing `as`.
