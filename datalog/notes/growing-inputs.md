# Property tests whose inputs grow — plan, 2026-09-12

**The ruling (the user's, §17 2026-09-12):** proper and complete testing is
paramount, and property-based testing's usual discipline, inputs that grow, is
missing. Sort it out **before more refactoring or performance work**: answer
streaming, delta-first ordering, column projection and interning all wait.

## What forced it

In one session of engine performance work, two defects in code the work touched
**passed all 615 tests**, each shown by mutation
(`testing.md` B1's shape note, E9's mutations):

- a semi-naive round reading the first atom of a 3+-atom recursive body as `Old`
  before a later delta — wrong answers on `code-analysis`'s `pointsto.dl` shape;
- a reporting run dropping a kept derivation whose fact the same round also
  supplied unkept — a silently smaller §9/§12 warning.

Both hid behind **fixed, small generator ranges**. `eval_bounds` is 2–3
predicates, arity ≤ 2, ≤ 6 facts, ≤ 4 rules, bodies of 1–2 atoms; allowing 3-atom
bodies alone still missed the first defect, because it needs data deep enough for
two premises to be new in one round. `arb_shaped_program` caught both — **one
generator, at one size** (`ShapeSize`, only `SHAPED_LARGE` used). A third finding
was the same kind: an unconverging fixpoint hung the suite instead of failing it
(now a round cap).

Separately, the scaling problems profiled this session (a fact pending once per
path, the answer copied four times) are invisible to any answer-comparing property
at any size; that remains the bench's job (`code-analysis`), not this plan's.

## What "grow" has to mean here

Proptest, unlike Haskell's QuickCheck, has **no size parameter that rises across
a run** — a strategy's ranges are fixed, and shrinking moves toward the small end. So growth has to be designed in, on three axes:

- **size** — facts, distinct values, relation cardinality;
- **depth** — fixpoint rounds, recursion chain length, strata;
- **breadth** — rules, body length, arity, predicates, mutually recursive groups.

**Growth is proven on the run, not the text** (testing.md rule 2): a tier's guard
reads the recorded model — rounds reached, derived facts, a derivation whose
premises span rounds — the way `shaped_generator_reaches_deep_rounds_and_a_split_instance`
does.

## Inventory to start from

- `arb_program_with_edb` / `eval_bounds`, `arb_safe_program` / `lowering_bounds`:
  fixed `SpecBounds`, no size knob exposed to properties.
- ~40 strategies in `testgen.rs`, plus test-local IR builders in
  `engine/mod.rs` (`absent_ir`, `aggregate_ir`, `grouped_ir`, `aggregate_goal_ir`,
  `arb_parent_edges`…), each with its own fixed ranges.
- Case counts fixed per block (32–96); no environment scaling.
- `arb_shaped_program`: sized, one tier, used by B1 and E9 only.
- Oracle cost: the naive evaluator re-joins every relation each iteration. B1 over
  `SHAPED_LARGE` at 48 cases took the lib suite from 5.5 s to 14.2 s.

## Design questions

Questions 1, 3 and 4 were answered 2026-09-12 (§17, *sized generators*;
`testing.md` § Generator sizes); 2 is half answered — the naive oracle measured
affordable through `Large`, and the scaling oracles are still to build.

1. **Tiers, or a drawn size?** A size drawn *inside* the strategy, skewed small
   with a long tail, shrinks toward small counterexamples for free. Fixed tiers
   (small × many cases, medium, large × few) guarantee each run reaches large.
   Possibly both: a drawn size within each tier.
2. **Oracles that scale.** B1's naive differential bounds its own size. The
   metamorphic properties need no second evaluator — B2 idempotence, B3
   duplication, B4 monotonicity, B5 body order, B6 rule order, B13 pruning, C11
   antitonicity, C13 statement order, E9 provisioning — and B7's closure oracle is
   a graph algorithm. Large tiers should lean on those; naive stays at small and
   medium.
3. **Budgets.** A per-commit budget for the lib suite, stated and measured, and a
   deep tier outside it (an environment scale, or `#[ignore]` run on demand) with
   its own recorded time.
4. **Shrinking at size.** A large failing case must still shrink to something
   readable: text generators shrink through their spec vectors; measure it.
5. **Scope of the audit.** Every generator gets a size knob or a written reason it
   has none: parser (long programs, deep nesting — D2–D4), imports (large tables —
   F1–F4, F7), temporal, diagnostics (C16), explanations (E1–E10).
6. **More shape.** `SHAPE_RULES` lacks named arguments, arithmetic (not in a
   recursion — that is C10's), temporal values, negation within the recursion's
   strata, and imported relations.
7. **`code-analysis`'s own properties** (fast-check) have the same question; a
   separate project, flagged here only.

## Sequence

The working plan widened step 3 into the scaling oracles — an all-paths
differential over every way to evaluate, staged evaluation, renaming and
disjoint union, a first-round oracle, and an independent Andersen points-to
solver — and added the non-engine generators (imports, seek, parser). Done so
far: step 2 for `arb_program_with_edb`, and step 3's B1, B5 and B13.

1. **Audit** — a table of every generator: its ranges, the properties reading it,
   what growing it would take. Into `testing.md`'s coverage map.
2. **Size as a parameter** — `arb_program_with_edb` and `arb_shaped_program` take
   a size; add small and deep tiers of the shaped generator; per-tier growth guards.
3. **Metamorphic properties onto sized generators** — B5 and B13 first (body
   order is where semi-naive broke), then B2–B4, B6, C11, C13, E1–E8. Each
   re-verified by a recorded mutation.
4. **The deep tier** — outside the per-commit suite, with its time recorded.
5. **A testing.md rule**, if the pattern holds: a generator states its size
   knob and the guard that proves growth, the way it states its non-vacuity guard.
