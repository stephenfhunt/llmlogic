---
id: 007
title: sum/avg fold witnesses in enumeration order, so two spellings of one goal give two answers
severity: wrong-answer
area: engine
spec: ["§9", "§14", "§17"]
found: 2026-08-20
resolution:
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
