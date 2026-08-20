---
id: 007
title: sum/avg fold witnesses in enumeration order, so two spellings of one goal give two answers
severity: wrong-answer
area: engine
spec: ["§9", "§14", "§17"]
found: 2026-08-20
resolution: fixed 2026-08-20 (88ce7e1, 7f62d59, 041889b)
---

`fold_aggregate` folds its witnesses in the order `enumerate_from` pushed them
into a `Vec` (`src/engine/mod.rs:807`). That order is a function of the goal's
**literal order**, so permuting a goal's atoms — which §17 (2026-07-25) ratified
as meaning-preserving — changes the answer whenever the fold is not associative
over the value type.

It is not, in two places: `float` addition, and `int` addition's *overflow*
check.

## Repro

Both files differ only in the order of the two atoms inside the goal.

```datalog
p(1.0e16, a).  p(-1.0e16, b).  p(0.1, c).
q(a).  q(b).  q(c).
r(S) :- S = sum { V | p(V, K), q(K) }.
?- r(S).
```
prints `r(0.0)`. Swapping the goal to `q(K), p(V, K)` prints `r(0.1)`.

The integer case is worse — it is answer-versus-failure, not a rounding
difference:

```datalog
p(9223372036854775807, a).  p(1, b).  p(-1, c).
q(a).  q(b).  q(c).
r(S) :- S = sum { V | p(V, K), q(K) }.
?- r(S).
```
prints `r(9223372036854775807)` and exits 0. Swapping the goal exits **2** with
`arithmetic error: integer overflow in `9223372036854775807 + 1``. By the
README's severity table that half is `crash` — a non-zero exit on a valid
program — reached purely by reordering a conjunction.

Both need the join key to sort differently from the collected value, which is
why a single-relation goal never shows it: with `p(V, K)` outer the witnesses
arrive in `V` order, with `q(K)` outer they arrive in `K` order.

## Root cause

`src/engine/mod.rs:807` — `let mut values: Vec<Value> = Vec::new();` filled by
the `enumerate_from` callback, then handed to `fold_aggregate`
(`engine/mod.rs:866`), which folds left-to-right through `apply_arith`
(`sum_values`, line 887). Relations are `BTreeSet<Tuple>` so enumeration is
deterministic *for a fixed program*; what is not fixed is which literal is
outer, and that follows the schedule, which follows source order among ready
literals (§17's tie-break, deliberately).

So the defect is not non-determinism — one program always gives one answer. It
is that **the answer is a function of a spelling §17 promises is irrelevant**:
"Two orderings of one conjunction, two answers, which no declarative reading
permits" (spec.md §17, 2026-07-25) — written about the scheduler, and true again
here by a different route.

## Acceptance criteria

`fold_is_permutation_invariant` (`engine/mod.rs`, in the fold proptest block) —
permuting the witness multiset leaves all five ops unchanged. Written
**`#[ignore]`d and failing** in the sitting the defect was found, per the
precedent of `002`'s `dash_q_rule_equals_the_same_rule_in_a_file` and `006`;
deleting the `#[ignore]` is what closing this looks like. Its shrunk
counterexample is recorded in `proptest-regressions/engine/mod.txt`.

**The general property this is one instance of**, filed per the README's rule:
an aggregate is a fold over a **multiset**, so every law that mentions its
witnesses must be permutation-invariant. That is wider than this fix — it also
covers `b1_aggregate_goal_shapes_agree` and
`b5_aggregate_body_order_does_not_change_the_model`, both of which state the
claim and neither of which can see it, being int-pooled with values in
`-1000..1000` where both hazards are unreachable. The float-pooled versions are
the second half of the acceptance criteria.

## Candidate fixes

1. **Sort the witness values before folding.** One answer per multiset, by
   construction, and it reuses the `Ord` §14 already requires to be total. Costs
   an O(n log n) sort per aggregate evaluation, and changes float sums to a
   canonical association rather than a *correct* one — there is no
   order-independent exact float sum without compensated summation.
2. **Sort, and use compensated (Kahan/Neumaier) summation for floats.** Fix 1's
   determinism plus much better accuracy; more code, and the accuracy is not
   what the defect is about.
3. **Fold in a canonical order derived from the group, not the enumeration** —
   equivalent to 1 with the sort moved.
4. **Document the restriction in §9** (`sum`/`avg` over floats are
   association-sensitive; write your own fold if it matters). Cheapest, and the
   one to reject: it keeps a case where reordering a conjunction changes an
   exit code, which is exactly what §17 ruled out.

Fix 1 is the recommendation. The int overflow half is fixed by it only
incidentally — sorting makes overflow reachable or not deterministically, but a
sum whose true value fits in `i64` can still overflow a sorted partial fold
(`MAX, MAX, -MAX`). Whether §9 should say so, or `sum_values` should widen its
accumulator, is the open half.

## Fallout

- **§17's 2026-07-25 scheduling entry** claims body reordering cannot change an
  answer. That claim is now false for a second reason unrelated to scheduling,
  and the entry wants an amendment saying the guarantee is conditional on the
  fold being associative over the value type.
- **`testing.md`'s C8 table** lists "body order ≡ any order | property (B5) |
  held". It held because the generator could not reach the case; the row wants
  the qualification.
- No ROADMAP item's rationale moves.

## Resolution

**Fixed 2026-08-20.** `fold_aggregate` sorts its present values into §14 order
before folding, so an aggregate is a function of its witness multiset and the
enumeration order cannot reach the answer. `fold_is_permutation_invariant` lost
its `#[ignore]` in the same commit (`88ce7e1`).

**The diagnosis held exactly**, including the part that predicted which shapes
could see it. The float-widened `b1_aggregate_goal_shapes_agree` reddens without
the sort and `b5_aggregate_body_order_does_not_change_the_model` does not: B5's
goal is the single atom `edge(K, V)`, so its witnesses arrive in the relation's
own order, which *is* the sorted order. Widening it was still worth doing — it
carries floats through typing, grouping and printing — but it is an equivalent
mutant for this defect, and its doc comment now says so rather than looking like
coverage.

**The fix taken was none of the four candidates.** Candidate 1 (sort) is the
mechanism, but sorting *alone* was rejected: §14's order is by value rather than
by magnitude, so it would have canonised `0.0` for `{ 1e16, -1e16, 0.1 }` — the
worse of the two answers this file reports, fixed forever. So the sort carries
determinism and two further rules carry accuracy (§17, 2026-08-20):

- **floats sum with Neumaier compensation** (candidate 2's second half, adopted
  for a reason candidate 2 did not give — not accuracy for its own sake, but so
  that the one canonical answer is the right one), with a finiteness guard,
  because `F64::new` permits infinities and `inf + -inf` would have turned an
  existing answer into a new error;
- **ints and durations accumulate in `i128`**, which no candidate proposed. This
  is the honest fix for the second repro: sorting makes the exit code
  deterministic, but every fixed association still errors on a multiset whose
  total is representable (`{ -MAX, -MAX, MAX, MAX }` sums to `0`). A multiset
  fold has no observable partial sums, so only the total has to fit.

Candidate 3 is candidate 1 with the sort moved; candidate 4 (document the
restriction) was rejected for keeping a case where reordering a conjunction
changes an exit code, which §17 2026-07-25 rules out in terms.

**The sort's cost, which candidate 1 named, is not measurable here.** 30k facts
over 200 groups with three aggregates each runs in 0.19–0.20 s at `--release`
both before and after: enumeration and provenance dominate, and the sort sits
inside the noise. That is one shape, not a profile — the real one is the next
session's.

**Cost elsewhere.** §9 gained the fold-order rule (a new normative home; §8's
binary `+` keeps its operand-pair overflow). §17's 2026-07-25 body-order entry,
already ***Falsified*** by this file, is now ***Amended***: the generalisation
holds again, but because two things are order-independent rather than one. The
`i128` change is a §9 semantic widening, not a bug fix — spec and code had agreed
before it — so it took its own commit and its own decision entry. Four unit tests
landed, each with a unique-kill mutation recorded in its doc comment.
