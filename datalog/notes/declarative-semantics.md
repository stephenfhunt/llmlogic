# Declarative semantics — the operator, the match relation, and the two finiteness claims

*Recorded 2026-08-18, from the §6 extension session. Status: **descriptive** —
`spec.md` §6 is the normative statement and this note is its formal backing, in
the pattern `notes/termination.md` set for §10. Nothing here decides anything that
§4, §7, §8, §9 or §10 had not already ratified, with one exception marked as such:
the meaning of a run that raises an error.*

## Why the textbook operator does not fit unchanged

The classical `T_P` maps a fact set `I` to the program's facts plus every ground
rule head whose body atoms hold in `I`, where "hold" is ground substitution. Three
of this language's features break that reading, and §6 was written before any of
them existed:

1. **`absent` is a value that unification rejects**, so membership in `I` and
   satisfaction of a body atom come apart (§4).
2. **Comparison and arithmetic literals have infinite extensions**, so the set of
   satisfying substitutions is not finite for syntactic reasons alone (§8).
3. **An aggregate literal is not a relation at all**, but a fold over one (§9).

Each is handled below by refining the operator rather than by adding a case to the
semantics: the result is still a least fixpoint of a monotone operator, per stratum.

## The match relation

Let `θ` be a substitution from variables to values. Satisfaction of a **positive
body atom** `q(t₁ … tₙ)` in `I` is not `θ(q(t₁…tₙ)) ∈ I`; it is the existence of a
tuple `(v₁ … vₙ) ∈ I[q]` matching argument-wise, where each position is one of
three cases:

| position | rule | on a stored `absent` |
|---|---|---|
| a **wildcard** `_` | matches anything | matches |
| a variable **not yet bound** by `θ` | binds to `vᵢ`, whatever it is | **binds** — a missing cell flows into the variable |
| a value (a constant, or a variable `θ` already bound) | **semantic** match: equal, and neither side `absent` | never matches |

So **binding is total and matching is semantic**, and the whole `absent` account
follows from holding those two apart. §4's table is the normative statement of
which notion each of the four sites uses; the unit test
`try_match_binds_a_var_to_a_stored_absent_but_never_rematches_it` is where the
split is pinned in the engine, and `values_unify_matches_eq_off_absent` is the
property that `unifies_with` is equality minus `absent`.

Two facts §4 states and §6 can now *derive* rather than assert:

- **`p(X), p(X)` selects strictly less than `p(X)`.** The first occurrence binds,
  the second matches — and if `X` bound to `absent`, the match fails. Conjunction
  is not idempotent over `absent`. SQL pays the same price on a self-join, for the
  same reason (`NULL ≠ NULL`).
- **`p(X), not p(X)` derives nothing**, for every `X` including `absent`. The
  negated literal is a **structural** membership test — it binds nothing, so the
  blowup the semantic notion prevents (missing keys joining each other) cannot
  arise, and a stored `p(absent)` refutes it. Under one uniform notion the pair
  derived `p(absent)`'s row, which is `P ∧ ¬P` (§17 2026-07-29; property **C9**).

**In the Herbrand base, not reachable by unification.** `absent` belongs to the
universe — a program produces it in a fact, in a rule head, as an arithmetic
result, or through an import (§13) — and ground atoms containing it are ordinary
members of `I`, deduplicated structurally like any other. What it is not is
reachable by *matching*: no body atom mentioning a bound value ever selects it.

## Builtins as interpreted predicates, and the two finiteness claims

A comparison or arithmetic literal is an **interpreted predicate**: its extension
is fixed by §8 rather than derived, and it is infinite (`<` holds over infinitely
many `int` pairs). Its two-valued treatment of `absent` — annihilation in value
space, false in truth space — is a property of that fixed extension, not a third
truth value; the operator stays two-valued throughout.

Range restriction (§10) is what keeps this well-defined. Every variable occurring
only in comparisons must be bound by the body, so every builtin literal is
evaluated at arguments some positive atom or `=`-chain already ground. Hence:

> **Claim 1 — each application is finite.** For finite `I`, a rule has finitely
> many satisfying substitutions, so `T_P(I)` is finite. It is a claim about the
> *size* of one application, not about how many are needed and not about whether
> the application succeeds — builtin evaluation can still fail, which is the last
> section here.

This is *not* the claim that the fixpoint is reached, and conflating the two is
what `bugs/004` caught §6 doing:

> **Claim 2 — the fixpoint is reached in finitely many rounds** — holds exactly
> when the program is **certified terminating** (§10), because certification is
> what bounds the *universe* rather than any one application. An `=`-assignment
> is the only construct that synthesises a value absent from the program and its
> inputs; when no such value reaches the head of a positively recursive predicate,
> each SCC's active domain is fixed before the SCC runs. The proof is in
> `notes/termination.md`.

Claim 1 holds for every program. Claim 2 holds for the certified fragment, and
outside it the least model may be infinite and the fixpoint is a limit evaluation
approaches without reaching. §6 states both; **the previous §6 stated only the
second, and stated it unconditionally.**

**PTIME data complexity** returns with Claim 2 and is scoped to the same fragment:
for a fixed certified program the number of SCC levels is fixed, and each level's
active domain is bounded by a polynomial in `|D|` whose degree depends only on the
program's arities and variable counts. Combined complexity is higher, as usual.

## Stratification: why an aggregate leaves the operator monotone

Negated and aggregated dependencies point strictly down (§7, §9); a program where
they do not is rejected, so the condensed dependency graph is a DAG and evaluation
proceeds stratum by stratum, each to its least fixpoint with lower strata frozen.
The result is the **perfect model** (Apt/Blair/Walker; `references.md` group 3),
and by the independence theorem every valid stratification yields the same one.

For an aggregate literal `Y = op { E | G }` in a rule of stratum `i`:

- the **group keys** are the variables occurring both inside the aggregate and
  outside it in the enclosing body; each must be bound by that body (§9/§10);
- for a substitution `θ` of the group keys, the **witness set** `W(θ)` is the set
  of substitutions extending `θ` to *all* of `G`'s variables that satisfy `G` in
  the model of the strata below. It is a **set**, because set semantics dedups
  facts — which is why a wildcard inside a goal is a witness *dimension* and
  `count { P | parent(P, _) }` counts edges;
- the value is the fold of the **multiset** `{| E(ω) : ω ∈ W(θ) |}`. Two witnesses
  agreeing on `E` and differing elsewhere are two elements, so `sum` counts two
  equal salaries twice;
- `sum`/`avg`/`min`/`max` drop `absent` elements and `count` keeps them; over an
  empty present-set `count` is `0` and the other four are `absent`, which then
  flows on under §8's rules.

**The monotonicity argument is one sentence.** `G`'s predicates lie in a strictly
lower stratum, so `W(θ)` is *already fixed* when stratum `i` begins: the aggregate
is a total function from group-key bindings to values, computed before the round
loop and constant throughout it. `T_P` restricted to stratum `i` therefore stays
monotone even though aggregation is not monotone in general, and the least fixpoint
exists for the ordinary reason. Stratification is not a restriction bolted onto the
aggregate — it is the entire reason the aggregate has a declarative reading at all.

This is also precisely what recursive or monotonic aggregation would give up
(Ross & Sagiv; Zaniolo et al., `references.md` group 4), and why that extension is
a milestone rather than a relaxation. **Limit predicates** (group 1) change the
same joint: the fixpoint would accumulate a per-group extremum rather than a set.

## The operator, assembled

For each stratum `i`, over the model `M₍<ᵢ₎` of the strata below:

```
Sat(r, I) = { θ ground on vars(r) | every literal of r's body is satisfied under θ }

  positive atom      the match relation above, against I
  negated atom       no tuple of M₍<ᵢ₎ structurally equals the instantiated pattern
  builtin literal    §8's fixed extension, at arguments θ has ground
  aggregate literal  θ(Y) is the fold of the multiset over W(θ|keys) in M₍<ᵢ₎

T_P(I) = facts(P) ∪ { θ(head(r)) | r in stratum i, θ ∈ Sat(r, I) }
```

Range restriction guarantees `θ(head(r))` is ground, which is why the operator
lands in the Herbrand base at all. `M₍ᵢ₎ = lfp(T_P)` over `M₍<ᵢ₎`, and the model of
the program is `M₍ₙ₎`.

## The one decision: a run that raises an error has no model

**Decided 2026-08-18.** `T_P` as written above is a *partial* function: §8's builtin
evaluation can fail — integer overflow, division by zero, a NaN-producing float
operation, a lossy `as` conversion. §6 had no account of what such a program means.

The rule: **a run that raises a structured error yields no model at all.** Not a
smaller model, not a model with a hole. The error is neither a truth value nor a
missing fact; it is the absence of an answer.

Three supports:

1. **The language already separates the two cases, deliberately.** §8 sends an
   *unrepresentable* conversion to `absent` and a *lossy* one to an error, on the
   ground that the first is dirty data and the second is a wrong program. Letting
   an error degrade to `absent` or to a dropped row would erase that line at the
   exact point it was drawn.
2. **It is the limiting case of the truncation contract** (§17 2026-08-16). An
   incomplete model holds missing facts and never false ones, so a whole-model
   dump survives it while answers do not. An erroring run is the same rule taken
   to its end: there is nothing to project, so nothing is owed an answer.
3. **It describes what the engine does, and the behaviour was already
   load-bearing.** `engine::eval` is
   `pub fn eval(program: &Program) -> Result<Model>` and the arithmetic helpers
   return `Err` rather than a value, so no partial model escapes. §17 2026-08-16
   *measured* this — "`eval_expr` is `?`-propagated, so an error aborts the run and
   emits *nothing*, not even facts already derived" — and used it to decide the
   cast's failure mode. What was missing was never the behaviour; it was the
   statement of it as a semantics.

**What is schedule-dependent is the message, not the outcome.** A program with two
erroring rule instances reports whichever the schedule reaches first, but a
complete application enumerates every instance, so *whether* a run errors is a
property of the program and its data alone. **B1** is the guard: its comparison
extension asserts that the naive and semi-naive evaluators agree on the error path
as well as on the model (`b1_comparison_programs_agree`, where a generated `/ 0`
makes both reject). It does not check that they choose the same message, and they
are not required to.

**What this does not settle** is the *reporting* surface — an exit code and a
stdout discipline for a run that has nothing to print. That is the open half of the
truncation contract (`ROADMAP.md`), and it is a §14/§15 question rather than a §6 one.
