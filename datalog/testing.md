# Testing strategy

*Living document. This is the single source of truth for how the `datalog`
crate is tested; this project's `AGENTS.md` and `spec.md` §17 point here. Update
the property catalog's checkboxes as layers land.*

## The pyramid

The test pyramid grows outward with the pipeline:

1. **Engine unit tests** over hand-constructed IR (`ir::Program`); lowering
   tests over hand-constructed ASTs. Verbose literal-struct construction, no
   builders.
2. **Property-based tests** (this document's main subject): generated programs
   checked against algebraic laws, metamorphic relations, and reference
   oracles.
3. **Parser golden tests**: source text → expected AST; structured errors.
4. **Integration tests**: source text → query results through the pipeline.
5. **System tests**: run the binary on program files, assert on output.

The spec **§16 worked examples are the canonical example corpus at every
level** — encoded as AST/IR fixtures now (`ast::fixtures`, `ir::fixtures`),
reused as source-text fixtures once the parser exists.

## Why property-based testing fits this engine

The language's ratified design decisions (spec §17) *are* executable
properties: set semantics ("duplicating inputs changes nothing"),
deterministic canonical output ordering, Datalog-out-is-Datalog-in closure
(§14), stratified monotonicity, type-errors-before-evaluation, and
provenance-for-every-fact are all mechanically checkable over generated
programs — no hand-written expected outputs needed.

This is proven bug-finding ground: queryFuzz ("Metamorphic Testing of Datalog
Engines", Mansur, Christakis & Wüstholz, ESEC/FSE 2021 — references.md group 9)
found real bugs in Soufflé, μZ, and DDlog using exactly these metamorphic
relations. And differential testing is nearly free here: naive and semi-naive
evaluation compute the same least fixpoint, so a deliberately dumb naive
evaluator (~50–100 obviously-correct lines) is a permanent oracle for the
optimized engine.

**Where PBT does *not* apply** (stays example-based/golden): parser
error-message quality and span exactness, type-inference *precision* (which
programs should be rejected is spec judgment; PBT checks only the soundness
direction), proof-tree rendering and JSON encodings, CLI ergonomics.

## Four rules

Their normative home; `AGENTS.md` points here. Each was paid for by a defect in
`bugs/`, and each is cheap to state and was expensive to learn.

1. **An equivalence claim ships as a property, not a unit test.** Any "these two
   spellings mean the same thing" claim — surface sugar, a desugaring, an
   IR-identity claim — gets a generated-input property *in the same sitting*. The
   record is exact: every such claim carrying a property has held; both that
   shipped with only a unit test became defects (`bugs/resolved/001`, `002`).
2. **Every generator carries a non-vacuity guard**, and the guard is checked
   against **the property's sentence** — not the generator's breadth, and not the
   assertion's reach. Those are the two ways a guard passes while certifying
   nothing: one that certifies the *data* where the claim is about what the **run**
   carried, and one that certifies breadth *no property reads* — which is exactly
   what `arb_comparison_program`'s six drawn operators did until `bugs/006` made
   something read an answer. Auditing a guard means reading it against its
   property's sentence; nothing mechanical substitutes, and a guard may live inside
   the property it guards. `testgen::tests` is where most of ours live.
3. **Mutation-verify a property before keeping it, and write the mutation on its
   catalog line in the same sitting.** Revert the fix, watch it go red, put the fix
   back. A property never observed failing may be asserting nothing — and *a
   mutation nobody recorded is a check that happened once*, since the run is
   momentary and the next session has no way to tell a verified property from an
   unverified one. This stays a manual habit and does not become a mutation-testing
   tool: the informative outcomes are "the mutation was aimed wrong, a sibling
   property reddened" and "this one hangs", neither of which a runner can assign.
4. **Widening a generator needs a matching acceptance property.** A wider
   differential is not a wider check — two evaluators sharing one semantics agree
   whether or not that semantics is sound, so a differential can never catch a
   type-checker defect (`bugs/resolved/006`, measured: the extended
   `b1_comparison_programs_agree` stays green with the fix reverted). If a generator
   starts emitting a new shape, something must assert the **checker** accepts or
   rejects it correctly.

Two corollaries about the shape of a check, both learned the same way:

- **An oracle that calls the engine's own function agrees with a wrong engine
  forever.** A *differential* oracle compares two implementations of one semantics
  and catches a one-sided change and nothing else; an **independent** oracle
  re-derives the rule *from the spec's words*, and is the only kind that can catch
  a soundness bug. Ours that qualify are named as such in the catalog —
  `aggregation_matches_an_independent_group_by` restates §9's fold table, and C2 is
  a hand-rolled set difference touching neither evaluator. When writing one,
  restate the spec section; do not
  call the function under test, and do not consult the *generator's* intent either,
  since a generator and an oracle sharing one misconception agree forever too.
- **A rejection claim wants a biconditional property.** "The checker rejects X" is
  half a claim: a checker that rejects *everything* satisfies it. State it as
  *rejected exactly when* — `bugs/resolved/006` is this failure in the wild, where
  the checker refused comparisons §8 orders and no property was looking at the
  accepting direction.

## Tooling

- **proptest** is the crate's first dev-dependency (§17, 2026-07-19). Decisive
  feature: integrated shrinking — counterexamples shrink *through the
  strategy* that produced them, which matters enormously for recursive program
  structures. Dev-dependencies do not affect the shipped library's
  zero-dependency posture.
- **Commit `proptest-regressions/` directories** when a seed records a
  *genuine* failure — persisted seeds are free regression tests. Seeds
  produced by deliberate mutation checks record a mutant, not a bug: delete
  those before committing.
- Keep per-property case counts modest (the default 256 is fine for cheap
  properties; cap expensive evaluator differentials with
  `ProptestConfig { cases: 64, .. }` if `cargo test` slows).

## Generators (`src/testgen.rs`)

Registered as `#[cfg(test)] pub(crate) mod testgen;` — visible to every unit
test, zero shipped code. **Promotion trigger**: switch to
`cfg(any(test, feature = "testgen"))` plus a self-dev-dependency only when an
integration test or fuzz target actually needs the generators; don't
re-litigate before then.

Design rules (Csmith lessons):

- **Valid by construction, no rejection sampling.** Rules are generated
  body-first; head variables are drawn from the body's variables by index, so
  every generated program is safe and arity-consistent by construction.
- **Small-biased, collision-rich pools** (~3 symbols, ints in −3..=3) so
  generated joins actually join — sparse random values make every relation
  empty and test nothing.
- **Index-vector selection + `prop_map`**, not `prop_flat_map` over
  materialized sets — keeps proptest's integrated shrinking effective.
- **Defect injection is single and self-contained**: `inject_defect` appends
  exactly one defective statement over fresh `defect_*` predicates, so the
  expected error is unambiguous regardless of the surrounding program.
- **Both argument forms are generated.** Roughly half the predicates get a
  `declare` naming their fields (`f0`, `f1`, …), and atoms over those may be
  written with named arguments — partially selected in bodies, fully supplied
  in heads (§4). Safety by construction is preserved by collecting a rule's
  body variables *after* partial selection: an omitted field binds nothing, so
  head variables are still drawn only from variables the body actually binds.
- **Negation is generated stratifiable-by-construction.** Each predicate
  carries a coarse level (0..=2); positive body selectors remap into
  predicates at ≤ the head's level, negated ones strictly below (empty pool ⇒
  positive fallback). The level function is a stratification witness, so no
  rejection sampling. Negated atoms come after the positives with named
  variables drawn only from positively-bound body variables; wildcards under
  negation are emitted both explicitly and via named partial selection. This
  generator keeps every column a symbol (`monotype`), so it cannot carry a
  *computed* negated argument — `arb_comparison_program`'s `CompRule::NegShift`
  covers that path over the int-valued EDB instead (C7).
- **Queries are generated** from the same body machinery (`build_body`),
  positives-then-negations, over unrestricted predicate pools (queries read
  the finished model, §7). Queries binding no named variable are skipped —
  B8's query≡rule projection needs at least one.
- **Source text is a legitimate generator output.** `arb_recursive_arithmetic_program`
  and `arb_taint_spellings` (C10, C8) emit `.dl` **text** and parse-then-lower it,
  rather than building `ir::` values directly. They are about how a program is
  *written* — where the arithmetic sits relative to the recursion, and which of
  four spellings expresses it — so going through the front end is the point, not a
  shortcut, and a shrunk counterexample is readable. They return the source
  alongside the program for exactly that reason.
- **Generator coverage is itself guarded.** `testgen::tests` asserts that
  sampling produces both argument forms, partial selection, negated atoms,
  wildcards under negation, multi-stratum programs, queries, and negated
  queries — without these, A13/C1/C3/B8 could pass vacuously if generation
  silently regressed.

**Policy — generators vs. the no-DSL rule.** The no-macro-DSL/no-builder rule
(the pyramid, item 1) is about ergonomic sugar for hand-written tests;
generators are *coverage machinery* that construct plain `ast::`/`ir::` values
and return them. They are `pub(crate)`, test-only, and must never be used to
shorten hand-written example tests — §16-derived example tests construct
literal structs verbatim. If a generator helper starts looking like a
convenience API for humans, it has crossed the line.

## First-order algebra coverage map

The audit frame (2026-07-20 audit session): every operation of the safe
first-order / relational algebra the language embodies, mapped to what covers
it. A future audit starts here.

| Operation | Generated by | Pinned by |
|---|---|---|
| Selection (constants in atoms) | `ArgSpec.constant` | B1/B7 differentials |
| Projection (heads, query vars) | head specs; query `var_names` | B1; **B8** |
| Join (shared variables) | collision-rich pools | B1, B7 |
| Cartesian product (disjoint vars) | unconstrained per-atom vars | B1 |
| Union (multi-rule heads) | independent head selectors | B1, B6 |
| Difference (stratified negation) | level-witnessed negated atoms | C1–C3, C2 oracle, §16.2 |
| Recursion (beyond FO) | positive recursion within a level | B7, §16.1 |
| Existential (wildcards, partial selection) | wildcard/named specs | A8/A13; §16.2/§16.7 |
| Ad-hoc queries (incl. negated) | `QuerySpec` bodies | **B8**; §16.1/§16.2 hand tests |
| Set semantics | duplication mutators | A5, B3 |
| Comparisons / arithmetic (§8) | `arb_comparison_program` (filter/assign/join on the int column **and the string key**) | B1 extended (incl. error path); `comparison_generator_is_well_typed` — the acceptance half, which is what B1 cannot be (`bugs/006`); §16.3 hand test |
| Value order across constructs (§4/§8/§9) | `arb_order_agreement_spellings` (two distinct constants of one type, over all five primitives, both ends of the order) | **C8** `ordered_comparison_and_minmax_agree_on_every_type`; `order_agreement_spellings_reach_every_type_and_both_ends` |
| Aggregation (§9) | `aggregate_ir` (grouped, one relation); `grouped_ir` (group keys from a *second* relation, empty groups, absent witnesses); `aggregate_goal_ir` (multi-atom + negated goal, all 6 goal orderings) — it and B5's `aggregate_ir_permuted` draw values from an ill-conditioned float pool (`bugs/007`) | **independent group-by oracle** (`aggregation_matches_an_independent_group_by`); B1 differentials (`b1_aggregate_programs_agree`, `b1_aggregate_goal_shapes_agree`); body-order invariance (`b5_aggregate_body_order_does_not_change_the_model`); fold laws (absent-skip, empty→absent, count=witnesses, sum oracle, min/max bounds); §16.4 hand test; query-position and nested/assignment-bound goals in `tests/pipeline.rs` |
| Termination (§10) | `arb_recursive_arithmetic_program` (ten placements of arithmetic relative to a positive cycle, over a freely cyclic graph); `arb_taint_spellings` (four spellings of one value-creating recursion) | **C10** `c10_a_certified_program_reaches_its_fixpoint` on a test-only round cap, with the B1 differential; `c10_generator_certifies_programs_that_do_arithmetic_in_a_cycle` — the non-vacuity half, without which C10 is a claim about arithmetic-free Datalog; **C8** `c8_the_taint_spellings_classify_alike`; §16.12 hand test (the diagnostic, on stderr) |
| Absent value (§4/§8) | `arb_fact_constant` (absent in *data*, ~1 in 10) + `absent_ir` (join / self-join / anti-join / comparison / arithmetic / presence over a `{0, 1, absent}` pool) | B1 absent differential (`b1_absent_programs_agree`); value laws (annihilation, comparison-false, unify-vs-eq, sorts-first); `generator_emits_absent_in_facts_only` |
| Declarative semantics (§6) | nothing of its own — §6 describes the semantics every generator above already exercises | the match relation: `try_match_binds_a_var_to_a_stored_absent_but_never_rematches_it`, `values_unify_matches_eq_off_absent`; its two derived consequences: `repeating_a_body_literal_drops_absent_rows` and `a_fact_never_satisfies_its_own_negation` (**C9**); the perfect model: **C1**–**C3**; the aggregate fold: the group-by oracle above; the fixpoint: **C10**; the error rule: B1's error path |

| Idempotence (join, union) | `arb_structural_law_spellings` | **C12**; its absent-valued *exception* is `repeating_a_body_literal_drops_absent_rows` |
| Distribution (∧ over ∨) | **unspellable** — §5 is top-level DNF, no parentheses in v1 | nothing, and nothing is correct: the law has one spelling, so no two sides to compare |
| Antitonicity (negation) | `arb_program_with_edb`, parity walk recomputed in-test | **C11**, with `c11_generator_reaches_a_non_empty_odd_dependent` |
| Aggregate monoid laws (§9) | int pools + partition split; an ill-conditioned float pool (`ILL_CONDITIONED_FLOATS`) for the order laws | **B11** — `avg_is_sum_over_count`, `folding_a_partitioned_group_combines`, `fold_is_permutation_invariant` (`bugs/007`); the fold's three arithmetic rules, one test each: compensated float sum, its finiteness guard, wide int/duration accumulation |

| Confluence (inference, interning, strata) | `arb_statement_permutation` | **C13** — the type half has nothing else; C4/C5 both fix statement order |
| Order-invariance of *explanations* | B5/B6's mutators, compared at the derivation level | **C14**; E1–E4 check one evaluation each and are blind to it |
| Closure at the program level (§14) | `arb_closure_program` | **D5**; D1 is the fact-set half |
| The rendered proof's structure (§11) | `arb_program_with_edb`, every derived fact explained and printed | **E7** (every line is a comment — E5's lexical precondition) and **E8** (the declared depth is the node's); a guard pins the generator reaches a proof deeper than one node |
| Explanations against the fact stream (§11/§14) | `arb_closure_program` plus both sigils over a fact the run answered and one it did not | **E5** — stripping the comments leaves the run without its goals, byte for byte; also the printed half of E9 |
| Demand-provisioned provenance (§11/§15) | `arb_program_with_edb`, evaluated all three ways | **E9** — identical model, answers and §9/§12 warnings, and a model that did not record says `Unrecorded` rather than `DoesNotHold` |
| The failure trace (§11) | `arb_program_with_edb`, goals built from the model's own value pool that do **not** hold | **E10** — five claims about a near-miss, including *a repair must repair*, which found `Repair::AbsentKey` |
| §10's std-builtin exemption | `ArithShape::StdBuiltin` | **C10**'s guard — the mutation lands on the classification, not the fixpoint |
| Diagnostics as a branchable surface (§12) | `arb_corrupted_program_text` — the first generator that makes programs **fail** | **C16**, with `c16_generator_rejects_and_reaches_several_families`; the pinned set itself is `every_code_is_pinned_and_belongs_to_its_category` |
| The physical access path (§15, evaluator-internal) | small collision-rich tuple pools with `absent`; prefixes drawn from the generated relation | **B12a/b/c**, with their three guards — the differential is blind to an over-yield, so these are what pin the seek |

**The 2026-08-20 audit's lesson, for whoever reads this map next.** Every row
above answers "is this operation exercised". The gaps that audit found were a
different question — **is each property's generator still as wide as its
sentence** — and three of the four it found were invisible from this table
because the operation *was* covered, over a value space that had stopped
matching the language. The first check on a future audit is therefore not this
map but `arb_constant`: read what it draws, against §4's list of types. The
second is to look for laws stated only as their exceptions (join idempotence was
one) and for claims relating two constructs rather than describing one (the
aggregate monoid laws were three).

| The caller's contract (§14) | nothing generated — the exit code is a property of the *process*, not of a program a generator can emit | `tests/system.rs`, listed under Phase D below: one test per code, §16.13's check in both halves, a warning shown not to move the code, and the any-query boundary |

**§14's contract is pinned only at the system layer, and that is not a thin spot**
(2026-08-18). Every inner layer returns a `RunResult`; the code is computed from it
in `main.rs` and observable only by running the binary. A property would have to
generate programs *and* predict their row counts, which is the evaluator's job
restated — so the guard is the exhaustive system-test set instead, exhaustive being
affordable because the vocabulary has three values.

**§6 adds no property, and two of its claims cannot have one** (2026-08-18). The
section is descriptive, so every rule it states was already guarded — the row above
is an index, not new work. The exceptions are the two *metatheoretic* claims: that
`T_P` is total on finite inputs for every program, and PTIME data complexity for
the certified fragment. Neither is a statement about a program a generator can
emit, so both are carried by the proof in `notes/declarative-semantics.md` and
`notes/termination.md` instead. C10's round cap is the closest a property gets to
the first, and it witnesses the *fixpoint* rather than the application. Recorded
rather than softened: a spec sentence with no guard should be visible.

## Property catalog

Status: `[x]` implemented and green; `[ ]` specified, waiting on its layer.
Each phase names the roadmap step that unblocks it and the §16 examples it
generalizes.

**Every entry added from 2026-08-16 states its mutation** — the change that was
made to watch it go red, per rule 3, in the sitting the property was written.
Entries predating that rule are **not** retrofitted: a mutation reconstructed
afterwards records what someone believes would have failed, which is the thing
rule 3 exists to stop being taken for evidence. The catalog will therefore be
mixed for a while, and an entry with no mutation line means "written before the
rule", not "unverified".

### Phase A — types + lowering (roadmap step 1) — generalizes §16.1

Value layer (`src/ir.rs`):

- [x] **A1** F64 order is total; `Ord` agrees with `PartialOrd`; sorting is
  deterministic.
- [x] **A2** `a == b ⇒ hash(a) == hash(b)` (bit-hash + `-0.0` normalization).
- [x] **A3** Every NaN bit pattern is rejected by `F64::new`; every non-NaN
  value round-trips (modulo `-0.0 → +0.0`).
- [x] **A4** `Value`'s derived `Ord` is lawful and respects the canonical
  cross-type order symbol < string < int < float < bool < date < timestamp <
  duration (§14).
- [x] **A5** `Fact` set semantics: `HashSet` size equals Ord-dedup size under
  arbitrary duplication.

**The value pools reached all eight types on 2026-08-20, not before.**
`arb_constant` drew symbol/string/int/float/bool and `arb_value` carried an
`unreachable!("arb_constant generates no temporal")`, so A4's sentence — the
derived `Ord` respects §14's cross-type order — was certifying five-eighths of
the order it names, and the three variants whose **enum position is that order**
(`ir.rs`, `Date`/`Timestamp`/`Duration` appended after `Bool`) were exactly the
ones it could not see. Everything downstream of `arb_constant` inherited it:
A5–A15, D1/D2/D3, and the whole B/C/E series.

*Measured, not assumed:* with `Date` and `Bool` swapped in `ir::Value`'s variant
order, **A4 passes on the pre-widening generator and fails on the widened one**.
D1 does *not* fail on that mutation and was never going to — it is a set-equality
closure, blind to ordering by construction; the order claim belongs to A4 and to
the printer's sort. Non-vacuity is `generator_emits_every_temporal_type`, which
asserts against `arb_value` and `arb_safe_program` rather than against
`arb_temporal`, since certifying the pool would certify nothing downstream of it.

This is `testing.md` rule 4 read in reverse — the *language* widened and the
generator did not follow — and it is the shape to look for first in any future
coverage audit.

Lowering (`src/lower.rs`), over `arb_safe_program()` and defect-injected
variants:

- [x] **A6** `lower()` never panics (both generators).
- [x] **A7** Lowering is deterministic: same AST → structurally equal IR.
- [x] **A8** Dense per-rule variable numbering: referenced slots are exactly
  `0..var_names.len()`; named source variables appear once; fresh slots are
  `None`.
- [x] **A9** Body literal order is preserved 1:1 (provenance contract:
  `BodyIdx` stability).
- [x] **A10** Interning is closed: every `PredId` is in-table; every atom and
  fact is at its predicate's full arity.
- [x] **A11** Safety both directions: safe-by-construction programs always
  lower `Ok`; one injected defect (unsafe head var / non-ground fact / arity
  clash) always yields the matching `Err`.
- [x] **A12** Facts and rules split correctly; strata is a single stratum in
  source order (until negation lands).
- [x] **A13** Named arguments are invisible to the IR: rewriting every named
  literal into the positional literal it denotes — each field at its schema
  position, omitted fields as `_` — lowers to structurally equal IR
  (`testgen::positionalize`). This is the defining invariant of named-argument
  lowering; A6–A12 additionally cover the named path because the generator
  emits both forms.

- [x] **A14** Field names attach to exactly the predicates the program gives a
  schema (`declare` or explicit import schema), and match it in order — hence
  `Some(f)` implies `f.len() == arity` on every `ir::PredicateInfo` of every
  safe program.

Named-argument defects injected by A11: unknown field, partial selection in a
head, and named arguments on a predicate with no schema.

### Phase B — core evaluator (roadmap step 2) — generalizes §16.1, §16.3

Requires: the semi-naive evaluator, the **naive reference evaluator**
(ratified §17 2026-07-19 as a *permanent* test-cfg oracle in `src/engine/`),
and generators `arb_parent_edges()` (the `arb_edb` shape) /
`arb_program_with_edb()`. Evaluator properties run on tighter generator
bounds than lowering properties (`testgen::eval_bounds`) — fixpoint cost
grows much faster than lowering cost — and B3–B6 mutate the lowered
`ir::Program` directly via the `testgen::with_*` mutators. Where a mutation
can change predicate interning order (B4's added statements), outputs are
compared keyed by predicate *name*, not `PredId`.

- [x] **B1** Differential oracle: `naive(p) == seminaive(p)` as `Fact` sets.
  The anchor property — catches delta-bookkeeping bugs directly. Extended over
  §8 comparison/arithmetic programs (`arb_comparison_program`,
  `b1_comparison_programs_agree`): both evaluators agree as fact sets and agree
  on the error path (a generated `/ 0` makes both reject). Extended again over
  the **absent value** (`absent_ir`, `b1_absent_programs_agree`, 2026-07-25) —
  see the note below; and over aggregation (`aggregate_ir`).

  **The absent gap, and why a shape-targeted generator was needed.** Until
  2026-07-25 `naive.rs` had *no* absent semantics — plain `==` in `match_atom` /
  `refutes`, no annihilation in `arith`, no absent arm in `compare`, and the
  presence filter ignored outright — so it silently disagreed with the engine on
  every absent value. B1 stayed green only because no generator emitted one. The
  fix is in two halves, and **both** are load-bearing: the oracle now implements
  §4/§8 independently (written from the truth tables, not by calling
  `engine::values_unify`), and `arb_fact_constant` puts absent into generated
  *facts*. The general generator alone was not enough — mutation-testing the
  restored oracle showed the odds of two absent values meeting in a joined
  position of one small program are too low to hit — so `absent_ir` builds the
  discriminating shapes deliberately (join on an absent key, repeated variable
  within an atom, anti-join, comparison, arithmetic, presence). Each of the five
  absent rules in the oracle was verified by mutation: reverting any one fails
  `b1_absent_programs_agree`. Absent stays out of `arb_constant` (rule/query
  bodies): a literal `absent` in a body atom argument or as a comparison operand
  does not lower (§4/§8), so it would only generate rejected programs.

  **The two logical laws absent is measured against** are unit tests beside the
  differential, because B1 cannot see either: both evaluators implement the same
  semantics, so they agree whether or not it is sound.
  `a_fact_never_satisfies_its_own_negation` holds since 2026-07-29 (§4/§7 —
  the anti-join is structural); `repeating_a_body_literal_drops_absent_rows`
  pins the law that stays broken **by design**, since idempotence over `absent`
  is a *join* property and restoring it means giving up `NULL ≠ NULL`. The pair
  is the reason C9 below is asserted against the model rather than across the
  two evaluators.
- [x] **B2** Fixpoint idempotence: re-running with `facts ∪ output` derives
  nothing new.
- [x] **B3** Set semantics: duplicating any subset of input facts leaves
  output unchanged.
- [x] **B4** Monotonicity for positive programs (the queryFuzz relation):
  `output(p) ⊆ output(p + fact)` and `⊆ output(p + fact + rule)`.
- [x] **B5** Body-reorder invariance: permuting a rule's body leaves output
  unchanged. **Now unconditional** (2026-07-25): builtins are dependency-scheduled
  (`crate::schedule`), so `N = A+1, M = N+1` and its reverse are the same clause,
  and so are the two placements of a computed aggregate group key. Previously
  scoped to atoms and negations, because §8 assignment chains bound in source
  order. Checked at three levels:
  `b5_body_order_is_irrelevant` (atoms/negations, the original),
  `b5_aggregate_body_order_does_not_change_the_model` (all six permutations of an
  aggregate rule must now *lower* as well as agree — before scheduling, some were
  rejected; before that, they silently mis-grouped), and
  `b5_a_computed_group_key_is_order_independent` (the group key bound by an
  `=`-assignment written on either side of its aggregate).

  The scheduler needs its **own** contract property,
  `schedules_bind_before_they_read`: lowering and both evaluators consume the one
  `schedule_body`, so a scheduling bug moves them together and B1 sees two
  evaluators agreeing on a wrong answer. `source_order_breaks_ties_among_ready_literals`
  pins the tie-break that makes scheduling a widening — without it, a body that
  worked before could silently change pruning order (observable on the error
  path). Both were verified by mutation: reversing the tie-break, or making an
  aggregate ignore its group keys, fails the suite.
- [x] **B6** Rule-order invariance within a stratum.
- [x] **B7** Independent oracle for a fixed shape: random `parent` edge sets
  into the §16.1 ancestor program vs. a hand-rolled DFS transitive closure
  (independent of *both* evaluators). Extended to §9 aggregation
  (`aggregation_matches_an_independent_group_by`, 2026-07-25): a plain group-by
  fold over random `node`/`edge` data, calling neither evaluator nor
  `fold_aggregate`. This is the only check that the **grouping** is right —
  which keys exist and which witnesses land in which group. The fold proptests
  pin the reducer in isolation and B1 pins the evaluators against each other;
  neither would notice a group-assignment bug, and until this landed §9 had a
  single generated program shape (one relation, no empty groups, single-atom
  goal), so the query-position panic and the source-order grouping hole both sat
  inside the untested region.
- [x] **B11** **The aggregate laws** (§9, 2026-08-20) — three claims about the
  fold that were each pinned in isolation, or not at all:
  - **`avg = sum / count`** over the present values
    (`avg_is_sum_over_count`). `avg_values` is a separate code path from
    `sum_values` and nothing tied them together. *Mutation*: perturbing
    `avg_values`' own accumulator reddens **this and nothing else** —
    `avg_is_the_float_mean_of_present_values` and
    `absent_inputs_are_skipped_not_folded` both stay green, because each compares
    avg against avg.
  - **The fold is a monoid homomorphism over group partition**
    (`folding_a_partitioned_group_combines`): splitting a group's witnesses and
    combining equals folding the whole, for `count`/`sum`/`min`/`max`. This pins
    **grouping** as a law where B7's oracle pins it for one shape. `avg` is
    deliberately excluded — the mean is not a homomorphism, and asserting it
    would be stating a false law. *Mutation*: `extreme_value` keeping the first
    present value reddens the min/max halves (broadly, alongside five sibling
    min/max tests — a broad aim rather than a wrong one).
  - **Permutation invariance** (`fold_is_permutation_invariant`) — permuting the
    witness multiset leaves all five ops unchanged. This was `bugs/007`, red when
    written and green since 2026-08-20: the fold sorts into §14 order, so the
    answer is a function of the multiset. *Mutation*: deleting the sort reddens
    this and `b1_aggregate_goal_shapes_agree`, and nothing else.

  **Why these were invisible until 2026-08-20.** Every aggregate generator was
  int-pooled with values in `-1000..1000`, where `sum` is associative and cannot
  overflow — so `b1_aggregate_goal_shapes_agree` (six goal orderings) and
  `b5_aggregate_body_order_does_not_change_the_model` both *state* the
  order-invariance claim and neither could reach the case that breaks it. Both
  draw floats now, which was `007`'s second acceptance half; **measured, only B1
  can see witness order**, since B5's single-atom goal enumerates in the
  relation's own order, which is the sorted one. That makes B5 an equivalent
  mutant here rather than a hole, and its doc comment says so — a green test that
  cannot fail is worth less than the sentence explaining why.

- [x] **B8** Query/rule equivalence: `Model::answer(q)` equals the relation of
  a synthesized rule whose head projects `q`'s named variables over `q`'s
  body, evaluated in a fresh final stratum. A query is a rule plus projection,
  mechanically checked; under the Phase-C generator this extends to negated
  query bodies. (Added by the 2026-07-20 coverage audit — queries were the
  one near-untested half of the algebra.)
- [x] **B9** **§8's conversion table, as an independent oracle**
  (`the_conversion_table_matches_section_8`, 2026-08-16; **widened to 8×8
  2026-08-20**). Every cell of the grid plus `absent` and each column's edge
  values, with the expected outcome —
  a value, `absent`, *lossy*, or *undefined* — transcribed from §8 rather than
  derived from the code. It has to be independent: `naive.rs` deliberately
  **calls** `apply_cast` rather than reimplementing it (what that oracle varies
  is the fixpoint strategy, and a second copy of the table would be one more
  thing to drift), so a differential over casts would agree with a wrong table
  forever. *Mutation*: dropping `ir::f64_as_exact_i64`'s fractional-part guard
  turns `2.5 as int` from *lossy* into `2`; moving `apply_cast`'s absent
  short-circuit below the undefined-pair check reddens the `absent` row.
  **The temporal rows and columns (2026-08-20)** are transcribed from §8's second
  table: a temporal renders to `string` and reads back from one, `date as
  timestamp` widens exactly, `timestamp as date` is a *lossy* structured error,
  and **every other pair is a `—`** — in particular `duration` against `int` or
  `float` in both directions, which §8 excludes deliberately because a duration
  rendered as a bare number is a number of nothing. That last row is the one most
  worth pinning, being a hazard decision rather than a mechanical consequence.
  The table passed on its first run, so the widening found no defect; what it
  bought is that the implementation and §8 are now checked against each other on
  24 cells that previously had none. *Mutation*: making `timestamp as date`
  truncate instead of erroring reddens it.
  The `string` cell is the **unsigilled** canonical spelling — the `@` is a
  delimiter the printer supplies, as a string's quotes are — which is what makes
  render and read inverse against the text a CSV cell actually holds.

  - Three algebraic laws beside it, none of which asks a second evaluator
    anything: **`V as string as T == V`** (`casting_through_string_is_the_identity`)
    — D1's closure property at the value level, holding because the renderer is
    §14's canonical spelling and the reader is §3's literal grammar; **`absent as
    T == absent` for all five** (`absent_survives_every_cast`); and **casting to
    a value's own type is the identity** (`casting_to_a_values_own_type_is_the_identity`).
    *Mutations*: `print_f64` emitting `{}` rather than `{:?}` reddens the first
    (`2.0` prints as `2` and reads back an int); dropping the `String` arm's
    early return reddens the third (the string is re-rendered *with* its quotes).
- [x] **B10** **The cast's typing claim, as a biconditional** (testing rule 4's
  corollary, 2026-08-16). Accepting: `a_cast_types_as_its_target_over_every_operand_type`
  — the same cast typechecks over an operand of every primitive type and its
  column comes out as the target, which is §4's "inference never flows `T` back
  into the operand". Rejecting: `a_casts_result_type_conflicts_like_any_other` —
  the result type is a real type, not a free variable, so joining it against a
  different column still conflicts. *Mutation*: unioning the operand into the
  result reddens **only** the accepting half, which is what shows both directions
  are load-bearing; dropping `set_type` reddens both and so discriminates
  nothing.
- [x] **B12** **The seek is the scan** (§15/engine, 2026-08-21) — the equivalence
  the prefix seek rests on, in three parts, all in `src/engine/seek.rs`.
  - **B12a** `b12a_the_seek_is_the_scan` — `tuples_with_prefix` yields exactly
    what `set.iter().filter(starts_with)` yields, in the same order. The oracle
    is the filter: an independent restatement, not a call to the code under
    test. Its prefix is usually drawn *from* the generated relation, so the range
    is non-empty by construction. *Mutation*: `.range(prefix..)` →
    `.range(prefix..).skip(1)`.
  - **B12b** `b12b_the_bound_prefix_loses_no_match` — every tuple `try_match`
    accepts starts with `bound_prefix`'s key, and a `None` key means no tuple is
    accepted. This is the half that licenses replacing the scan; it never calls
    the seek. *Mutation*: let the prefix keep extending past an unbound variable,
    so a bound column at a non-leading position joins the key.
  - **B12c** `b12c_the_closed_prefix_loses_no_refutation` — B12b for the
    anti-join, where the comparison is structural and `absent` is therefore a
    legal key. *Mutation*: `map_while` → `filter_map` in `closed_prefix`.

  **Why B1 is not the guard here, and why these had to be written.** Both
  callers re-check every candidate, so a seek that returns *too many* tuples is
  silently corrected and only costs time. The differential (`naive.rs` scans,
  the engine seeks) therefore cannot see an over-yield at all, and every
  mutation above is deliberately an **under**-yielding one. Guards: B12a's
  `b12a_generator_reaches_a_proper_non_empty_sub_range` (some tuples kept *and*
  some rejected, or contiguity is untested); B12b's
  `b12b_generator_reaches_matches_with_a_prefix_and_both_impossibilities` (a
  match on a non-empty prefix, plus `None` reached from a constant `absent` and
  from a slot bound to one); B12c's
  `b12c_generator_reaches_refutations_on_a_prefix_and_on_an_absent_key`.

### Phase C — negation + type inference (roadmap step 4) — generalizes §16.2, §16.3

- [x] **C1** Stratification correctness: every negated dependency sits in a
  strictly lower stratum (checked against a dependency graph the test
  recomputes from the lowered rules); generated negative-cycle programs are
  rejected (A11's `NegativeCycle` defect).
- [x] **C2** Independent oracle on the §16.2 shape: random person/parent EDBs
  through `root(X) :- person(X), not parent(_, X).` vs. a hand-rolled set
  difference touching neither evaluator (the B7 pattern). The per-stratum
  iterated fixpoint itself lives in the naive oracle, which B1 exercises.
- [x] **C3** B1–B6 re-run over stratified programs: the generator gives each
  predicate a level and negated selectors draw strictly below the head's, so
  programs are stratifiable by construction, and B1 becomes the perfect-model
  differential. B4's fact half restricted to fact additions in predicates no
  negation transitively depends on (`negation_independent_preds`); its rule
  half extends on fresh `ext_*` predicates, monotone by construction.
- [x] **C4** Type-inference soundness: any program `typecheck` accepts
  evaluates with no type-based runtime error; every fact's values match the
  inferred column types (`arb_well_typed_program`, division-free so evaluation
  cannot error).
- [x] **C5** Typed-generator completeness: every well-typed-by-construction
  program is accepted by `typecheck` (no false rejections). Also
  `injected_type_conflict_is_rejected` (a fresh predicate with two
  differently-typed facts is always rejected — the A11 analogue for types) and
  `evaluation_generator_is_well_typed` (the migrated B/E generator only produces
  type-checkable programs).
- [x] **C6** `declare`-signature verification: a `declare`/import-schema type
  that contradicts inference is rejected, a matching one is accepted, and neither
  changes evaluation. Two proptests over `arb_well_typed_program`
  (`c6_correct_declared_signature_is_accepted` — asserting the inferred types as a
  signature never changes acceptance; `c6_wrong_declared_type_is_rejected` — a
  self-contained probe column declared against its inferred type is rejected,
  naming the column). Hand units cover a matching signature, a fact/declare
  conflict naming `person.age`, an untyped field (no constraint), a declared-only
  column (unrefuted, seeds the type), and a two-schema type conflict
  (`schemas_conflicting_only_on_types_are_reported`); `declared_types_reach_the_ir`
  pins the AST→IR threading and the "`field_types` is `Some` iff `fields` is
  `Some`, same length" invariant (A14's type-side companion). Deferred: imported
  *inferred* column types (need §13).
- [x] **C7** Negation over a **computed argument** is spelling-independent
  (`bugs/001`, §7/§10 2026-07-25): `not q(V + c)`, `W = V + c, not q(W)` and
  `not q(W), W = V + c` are the same conjunction and must give the same model
  (`negation_over_a_computed_argument_is_spelling_independent` over
  `arb_neg_shift_spellings`). All three used to differ — the inline form
  silently degraded to `not q(_)`, and both hoisted forms were rejected. The
  `CompRule::NegShift` variant also carries the shape into **B1's** comparison
  differential, which is what covers the *oracle* on the deferred-negation path:
  the naive evaluator used to filter every negation before running any builtin,
  so it would have agreed with a wrong engine. Its structural counterpart is
  `schedules_bind_before_they_read`, now extended over negations.
  **B5 over comparison programs** (`b5_comparison_body_order_is_irrelevant`)
  carries the arbitrary-permutation half — `arb_program_with_edb` cannot, being
  all-symbol — compared only when both orders evaluate, since pruning ahead of a
  `/ 0` is the one way order is legitimately observable. And
  `a_computed_negated_argument_is_recorded_as_a_closed_no_match` pins the
  **provenance** half: the no-match pattern must record `Some(2)`, not an open
  slot, or "why?" answers the far stronger "because no `q` fact exists".

  *Known gap:* derivation **replay** does not reach a deferred negation, whose
  no-match pattern depends on an assignment-bound value — see **E6** below.
- [x] **C8** **Surface-spelling equivalence** — the language's "form A means the
  same as form B" claims, each as a property rather than a unit test. This group
  exists because the record was unambiguous: every such claim carrying a property
  held, and both carrying only a unit test became defects.

  | claim | coverage before | outcome |
  |---|---|---|
  | named ≡ positional | property (A13) | never drifted |
  | body order ≡ any order | property (B5) | held |
  | inline arith ≡ hand-hoisted | unit test only | `bugs/001` |
  | disjunction ≡ separate rules | unit test only | adjacent to `bugs/002` |
  | `-q` ≡ equivalent file program | nothing | `bugs/002` (closed 2026-07-26) |
  | a computed argument ≡ its value | unit test only (facts) | `bugs/005` (closed 2026-07-27) |
  | `<` ≡ `min`/`max` on one value order | nothing (unwritable — `<` was rejected) | `bugs/006` (closed 2026-07-27) |
  | a substituted answer ≡ the synthesized one | property, written with the change (2026-08-17) | — |
  | a named query ≡ a rule over the projection | property, written with the change (2026-08-17) | — |
  | four spellings of one value-creating recursion ≡ each other | property, written with the change (2026-08-18) | — |

  - **`c8_the_taint_spellings_classify_alike`** over
    `testgen::arb_taint_spellings` — §10's termination classification does not
    depend on how the computation is written. `N = M + 1`, `K = M + 1, N = K`,
    `K = M + 1, N = K as int` and `N = (M + 1) as int` are one computation in four
    spellings, and the first binds the head variable by arithmetic while the rest
    bind it by a bare variable or hide the arithmetic under a cast. Non-vacuity is
    inside the property: all four must actually warn, or a lint that never fires
    would satisfy it. **Mutations killed:** dropping the `=`-chain propagation from
    `schedule::computed_vars` (spellings 2 and 3 stop warning) and making
    `has_arithmetic` stop at a `Cast` node (spelling 4 does). Both also kill C10.

  - **`c8_an_answer_shape_neither_drops_nor_collapses_a_row`** over
    `testgen::arb_answer_shape_case` — the §14 shape rule: an answer printed under a
    real relation's name carries the same rows the synthesized form would have. The
    oracle **filters the generator's own fact list**, never calling `answer_lines`,
    per the corollary below.

    Two things this property records about *building* one. Its generator is built
    **backwards from facts the EDB contains**, so the query is guaranteed to answer
    — the cautionary precedent being
    `a_computed_query_argument_answers_like_its_value`, whose rewrite-based first
    version passed unfixed. And its fourth shape — a body binding a variable **no
    atom mentions** — is what makes it guard the rule rather than restate it:
    without that case, every generated case passes under the one-directional check
    the change replaced, which is mutation **M1**. Also killed: emitting only the
    first atom of a ground conjunction, dropping the canonical sort, and refusing
    the ground-conjunction branch.

  - **`c8_a_named_query_matches_its_desugared_rule`** over the same generator —
    `?- name: body.` answers exactly as `name(<projection>) :- body.` plus
    `?- name(<projection>).` (§14, §17 2026-08-17). **The oracle is the
    desugaring**, written out as program text, so the property compares two
    spellings rather than a spelling against a restatement of the rule.

    That is only sound because the generator **writes down the projection it
    emitted** rather than asking lowering for it — recovering the variable order
    from the code under test would have made the oracle circular, which is the
    corollary below reached from a new direction. The second assertion is
    independent of both spellings: the rows come from the generator's own fact
    list. Four mutations killed — a head one column short of the projection
    (caught by the existing IR well-formedness check, not by this property's
    comparison), querying the original body instead of the synthesized head,
    dropping the empty-projection `true` so the answer prints the unparseable
    `ans().`, and dropping the collision guard, whose kill is the unit test
    `naming_a_query_after_a_defined_relation_is_rejected` and whose failure output
    is the hazard itself: `age(N, A) :- age(N, A), A >= 18.`

  - **A15** `a15_inline_and_hoisted_arguments_agree` — an inline compound atom
    argument lowers to the same program as the hand-written `=`-assignment
    (`testgen::hoist_atom_args`), in **rule bodies**. Queries are excluded and
    the exclusion is load-bearing: a query's answer variables are its *named*
    slots (§14), so a hand-written variable becomes an answer column where
    lowering's anonymous slot does not — a real difference in what was asked,
    not an artifact. On its first run the property found `bugs/005`, a separate
    consequence of that same §14 rule, closed 2026-07-27 by the fold below — the
    exclusion stays, because the projection difference it names is real.
    Compared with `testgen::alpha_eq`, not `==`:
    the hand-written variable is *named* where lowering mints an anonymous slot
    and the two number slots differently, neither observable to the evaluator —
    which is exactly what `lower_arg_expr`'s "engine-identical" asserts, so
    alpha-equivalence formalizes the claim rather than weakening it.
  - **A computed argument ≡ its value**
    `a_computed_query_argument_answers_like_its_value`
    (`testgen::arb_ground_query_spellings`) — `?- n("a", 1 + 1).` answers as
    `?- n("a", 2).`; `bugs/005`'s acceptance criterion, green since 2026-07-27
    with the failing seed recorded. Unlike A15 this is an **output** claim, since
    the two spellings now lower differently by design.

    Its generator is targeted, and that is the lesson worth keeping: the first
    version of this property rewrote `arb_ast_program` and **passed unfixed**.
    Three things must coincide before the spellings can differ — a single-atom
    query, a ground computed argument, and *a matching fact* — and over arbitrary
    programs the third almost never holds, so both spellings printed nothing and
    agreed. Building the expression backwards from a value the EDB contains is
    what makes it bite. The syntactic non-vacuity guard was green throughout;
    `ground_query_spellings_generate_queries_that_hold` is the one that would
    have caught it, and it asserts the queries *hold*, not merely that they
    compute. `folding_a_ground_argument_anywhere_does_not_change_the_answer`
    keeps the rewrite-based version for the fact/rule positions, documented as
    the weaker claim it is.
  - **Disjunction** `disjunction_equals_separate_rules` — green.
  - **`-q`** `dash_q_rule_equals_the_same_rule_in_a_file` — green since
    2026-07-26. It was written `#[ignore]`d and failing as `bugs/002`'s
    executable acceptance criterion, shrinking to
    `d(K) :- n(K, V), V = 0 ; n(K, V), V = 0`, and closing that defect was
    deleting the `#[ignore]` — the first time in this project a property was
    written before the fix it specified.

  - **One value order, three constructs**
    `ordered_comparison_and_minmax_agree_on_every_type`
    (`testgen::arb_order_agreement_spellings`) — for two distinct constants of
    one type, the value `<` puts first is the value `min` returns, over all five
    primitives; `max` reads the other end of the same `A < B`. `bugs/006`'s
    acceptance criterion. Unlike the other rows this is not two spellings of one
    construct but **two constructs reading one order** — §4 fixes it, §8's `<`
    and §9's `min`/`max` both consume it, and the printer sorts by it, all from
    one `Ord` on `Value` with nothing to notice when one drifts out of scope.
    The pair is distinct by construction: at `a == b` the comparison answers
    nothing where `min` still answers `a`, which is a difference between "the
    smallest" and "strictly smaller than something", not about order.

    **The generator lesson here is the inverse of `bugs/005`'s.** There the
    danger was a generator too broad to hit the case; here the extended
    `arb_comparison_program` hit the case and B1 *still* could not see it,
    because `eval` is type-blind by design (spec §17, 2026-07-21) and a
    differential over two evaluators never asks what the type checker accepts.
    Reverting the fix leaves `b1_comparison_programs_agree` green and fails
    `comparison_generator_is_well_typed`. **Widening a generator therefore needs
    a matching acceptance property, not just a wider differential.**

  - **One answer set, two output shapes** — `filtered_atom_query_answers_like_its_unfiltered_shape`
    was specified here as written-`#[ignore]`d-and-failing, the acceptance
    criterion for §14's widening. **It was never written under that name.** The
    widening shipped 2026-08-17 with
    `c8_an_answer_shape_neither_drops_nor_collapses_a_row` above, whose generator
    is built backwards from facts the EDB contains — the same "backwards from a
    matching fact" discipline this entry specified — and the entry was not swept
    at the time. Recorded as a substitution rather than deleted, because a
    catalog line describing a test that does not exist is the failure mode this
    document exists to prevent, and the 2026-08-20 audit found it by grepping for
    the name (**swept 2026-08-20**).

  Non-vacuity is guarded in `testgen::tests`, per the generator-coverage
  convention above:
  `generator_emits_compound_atom_arguments_that_hoisting_rewrites` asserts the
  generator emits compound arguments, that the rewrite fires, and — the pointed
  one — that a compound argument appears **under `not`**. That is the `bugs/001`
  shape and the reason A15 would have caught it: pre-fix the inline form lowered
  while the hoisted form was a semantic error, so the acceptance arm fails
  without evaluating anything. The prior test of that same claim was a unit test
  over one hand-written positive atom, and the spelling that broke it was simply
  never written down.
  `order_agreement_spellings_reach_every_type_and_both_ends` asserts all five
  primitive pools are drawn from, both `min` and `max` are generated, and every
  generated pair *answers* — the same "holds, not merely computes" bar as the
  query guard above.

  **The rule this group encodes** (`datalog/AGENTS.md`, "Working style"): a new surface
  form, desugaring, or IR-identity claim ships with a property here, in the same
  sitting.
- [x] **C9** **Non-contradiction** — a body asserting both `p(X, _)` and
  `not p(X, _)` derives nothing, over `absent_ir`'s row pool including the rows
  that bind `X` to `absent` (`c9_a_body_and_its_negation_derive_nothing`, the
  `contra` rule). Acceptance criterion for the 2026-07-29 absent × negation
  decision (§4/§7), the second of milestone 4's two negation follow-ons.

  **Asserted against the model, not across the evaluators, and that placement is
  the point.** B1 stays green with the fix reverted: `naive` and `seminaive`
  implement one semantics, so a differential cannot ask whether the semantics is
  *sound* — the same blind spot C8 records from `bugs/006`, reached by a
  different route (there it was `typecheck` the differential could not see; here
  it is a logical law neither evaluator claims). What B1 **does** catch is a
  one-sided change: `b1_absent_programs_agree` goes red when the engine is
  reverted and the oracle is not, which is what an independent re-expression of
  the rule buys.

  Non-vacuity is `absent_ir_binds_an_absent_key_under_negation`: C9 proves
  nothing over row sets whose key column never holds `absent`, so the guard pins
  that `absent_ir` reaches the discriminating case (`unmatched` derives
  `(absent)` from the same positive prefix). The three plus B1 were all confirmed
  red with `NoMatchPattern::matches` reverted before being kept — `bugs/005`'s
  discipline.

  Its counterpart in the other direction is
  `repeating_a_body_literal_drops_absent_rows` (Phase B above): the law C9 does
  *not* generalize to, kept so the asymmetry is a decision on the record rather
  than a gap.
- [x] **C10** **Termination** (§10, 2026-08-18) — *the guarantee itself, and the
  one property whose failure mode is an infinite loop.*
  `c10_a_certified_program_reaches_its_fixpoint` over
  `testgen::arb_recursive_arithmetic_program`: a program the value-creating-recursion
  lint does not warn about reaches its fixpoint, with the B1 naive differential
  riding along so certification cannot buy termination by computing the wrong
  model. The generated `step` graph is **freely cyclic** — a certified program has
  to terminate on the graphs an uncertified one runs forever on.

  **The oracle is a round cap** (`engine::eval_capped`, test-only; no budget ships
  — §17 2026-08-16), so a wrong certification *fails* rather than hanging the
  suite, and it is deterministic where a wall-clock timeout would flake under
  load. This is the general shape for any property whose negation does not
  terminate: bound the computation in the test, never in the engine.

  **Non-vacuity** is `c10_generator_certifies_programs_that_do_arithmetic_in_a_cycle`,
  and it is not optional here — without it C10 holds over arithmetic-free Datalog,
  which was never in doubt. Five of the generator's ten shapes are certified *and*
  contain both arithmetic and a positive cycle (a cast alone, a ground expression,
  an aggregate result, a computed value that never reaches a head, and value
  creation hoisted outside the cycle); the guard pins each shape to its side.

  **Mutations killed:** both of C8's, above — dropping transitive taint through an
  `=`-chain, and a check that does not look under a `Cast` node. Each certifies a
  program that then runs to the cap, which is exactly the failure the property
  exists to catch.

  **The `StdBuiltin` shape (2026-08-20)** is the eleventh, and it exercises the
  one §10 exemption that had no test: a `std` relation is exempt from
  value-creating recursion as a *finite-domain map* (`stdlib.rs`,
  `schedule::has_arithmetic`'s `Expr::Builtin` arm), which is a
  termination-soundness claim whose failure mode is a program that never
  finishes. The shape puts `truncate` in a genuine positive cycle; it terminates
  because truncation is idempotent (**T5**). *Mutation*: making `has_arithmetic`
  return `true` for `Expr::Builtin` reddens the **non-vacuity guard and only it**
  ("shape StdBuiltin changed sides") — C10 itself stays green, since a warned
  program is skipped by its `prop_assume!(!warned)`. That is the argument for
  guarding the *classification* separately from the fixpoint.

  The generator now runs `parse → resolve_modules → lower`, since a `std` import
  is spliced by the resolver and lowering an unresolved one is a structured error
  by design.

- [x] **C11** **Negation is antitone, by parity** (2026-08-20). Adding a fact to
  an EDB relation grows every predicate depending on it through an **even**
  number of negations and shrinks every predicate depending on it through an
  **odd** one; predicates reachable both ways carry no claim
  (`c11_adding_a_fact_moves_dependents_by_negation_parity`).

  **The half B4 excludes rather than covers.** `with_extra_fact` adds only to
  `negation_independent_preds`, because over a negated predicate the monotone
  claim is false — and excluding it left the *true* law for that half unstated.
  B4 is C11's even case restricted to parity-0-only predicates; C11 derives which
  direction applies from a dependency walk it recomputes from the lowered rules,
  in the C1 style. The simple form ("negation shrinks") is wrong two strata up:
  if `h` negates `q` then `h` shrinks, but `s :- not h.` grows again.

  *Mutation, and what aiming it taught:* dropping the strictness of a negated
  dependency in Ullman relaxation reddens C11 **while C2 stays green** — C2's
  program is a hand-built fixture, so the exact oracle covering that shape cannot
  see a stratification bug at all. That contrast is C11's justification: not a
  stronger claim than C2's, a wider one, over generated multi-stratum programs.
  Two earlier aims failed and are recorded at the property: mutating the
  anti-join itself reddens fourteen tests including B1, because a change at one
  call site is *one-sided* and B1 catches those by construction. Isolating C11
  would need the same change made in **both** evaluators — the same reason C9 is
  asserted against the model rather than across them.
  Non-vacuity: `c11_generator_reaches_a_non_empty_odd_dependent`, a deterministic
  sampler asserting an odd-parity dependent with a **non-empty** relation is
  reached, since `∅ ⊆ ∅` proves nothing.

- [x] **C12** **The structural laws** (2026-08-20) — join idempotence (repeating
  a body literal changes nothing) and union idempotence (writing a rule twice
  changes nothing), over absent-free data (`the_structural_laws_hold`).

  B5 covers the *commutativity* of a conjunction and B6 that of a union, which
  left the **idempotent** laws with nothing looking at them. Join idempotence is
  the pointed one: its only written form in this crate was
  `repeating_a_body_literal_drops_absent_rows`, which pins the case where the law
  **fails** — deliberately, since idempotence over `absent` is a join property
  and restoring it means giving up `NULL ≠ NULL` (§4). A law recorded only as its
  own exception reads as an accident; stating the positive is what makes the
  asymmetry legible as a decision.

  **Distribution is absent, and that is a finding.** `(q ; r), s ≡ q, s ; r, s`
  needs two spellings, and §5 gives the language one: disjunction is top-level
  DNF, **no parentheses in v1**, so `(q ; r), s` is a syntax error and a body is
  already distributed. The first version of the generator emitted it and the
  property failed on *acceptance*, which is how the law was found to have no
  content in this surface.

  *Mutation — the honest result:* **no single-site mutation isolates these two.**
  Rebinding rather than re-checking an already-bound variable in `try_match`
  reddens eight other tests including B1, because the engine has **no code path
  for a repeated literal or a duplicated rule** — both laws are consequences of
  set semantics and the join loop, not behaviours implemented anywhere. That
  makes C12 a regression guard against a *future* optimisation (a body-literal
  deduplicator, rule-level CSE) rather than a guard on current code, which is a
  legitimate thing for a property to be as long as the record says so.

- [x] **C13** **Statement order carries no meaning** (2026-08-20), in three
  senses at once: permuting a program's statements changes neither the
  accept/reject verdict, nor the inferred column types, nor the model
  (`c13_statement_order_changes_nothing` over `arb_statement_permutation`).

  B6 swaps two rules *within a stratum*; this reorders facts, `declare`s, rules
  and queries against each other, varying three things B6 holds fixed — predicate
  **interning** order (so every comparison goes by name), the numbers **Ullman
  relaxation** assigns, and the order **inference** unifies columns.

  **The type half is the one with nothing else looking at it**: C4 checks
  soundness and C5 completeness, both at one fixed statement order, so a unifier
  seeding a column from whichever constraint it met first satisfies both.

  *Two mutations, and the pair is the record.* Making `set_type` take the last
  constraint rather than unifying reddens C13 while **C4, C5 and C6 all stay
  green** — that contrast is the confluence claim, measured. Gathering fact
  constraints in reverse order reddens **nothing at all**: an equivalent mutant,
  which is not a gap but what confluence *means*, and the positive evidence that
  C13 asserts something true rather than something merely unbroken.

- [x] **C14** **The explanations are order-invariant too** (2026-08-20) —
  permuting a stratum's rules, or a rule's body, leaves the recorded derivation
  set unchanged (`c14_reordering_does_not_change_the_derivations`).

  B5 and B6 compare `model_facts` and nothing else, so the model was
  order-invariant by property while the **derivations** were order-invariant only
  by hope — and provenance is pillar 1. The comparison is exact rather than a
  premise multiset, because `premises[i]` is `BodyIdx`-aligned: the property
  un-swaps the two moved positions and asserts equality, where a multiset would
  not notice a premise landing at the wrong index.

  *Mutation — the cleanest kill in the suite:* recording premises in **schedule**
  order rather than at their `BodyIdx`, with evaluation untouched, reddens **C14
  and nothing else**.

  **E3 staying green is the finding.** Replay zips body literals against premises
  and `continue`s on a mismatched pair rather than failing, so a permutation that
  moves a positive premise opposite a negated literal slips through the strongest
  provenance property in the suite. E1/E2/E4 are equally blind, each checking
  derivations against a *single* evaluation.

- [x] **C15** **A diagnostic never asserts something about the data that was not
  read from the data** (2026-08-24, `bugs/resolved/008`). Inference reaches a
  column from two directions — the fact table, and the way rules use it — and
  only the first is a claim about values, so every rendered *"its values are T"*
  must survive re-deriving that column's type from `program.facts` alone
  (`c15_a_values_claim_is_backed_by_the_facts`, over `arb_well_typed_program`
  with a contradicting `declare` signature asserted on every column).

  The oracle is **independent** in the sense the corollary above requires: it
  re-reads the facts in the test module and never calls the typechecker's own
  derivation. Truth is not precision — *which* programs are rejected stays
  example-based per the exclusion list, and this property reads only what a
  message claims once one is emitted.

  *Guard* — `c15_generator_produces_both_kinds_of_contradiction` counts the
  *"its values are"* messages a run actually produced, which is the property's
  own sentence, and separately counts rule-derived (*"is used as"*) ones: the
  second is the half `bugs/008`'s acceptance criteria missed, and without it the
  generator could stop producing the shape unnoticed.

  *Mutations, both recorded:* reverting the **wording split** (message always
  says "its values are") reddens C15 with `p.f1 was said to hold int values,
  which the facts do not say`, and reddens the guard too, since no "is used as"
  message survives. Reverting the **suppression guard** in `finish()` instead
  reddens the two hand units — and the surviving second diagnostic is then
  `` `item.weight` is declared as float but is used as int ``, which is why both
  halves are the fix and neither alone is.

### Phase D — lexer + parser (roadmap step 5) — generalizes all §16 source texts

Implemented 2026-07-22 (`src/lexer.rs`, `src/parser.rs`, `src/print.rs`). The
§16 corpus is now **source-text-first**: the ratified examples are golden AST
fixtures (`parser::tests::golden_16_*` assert `parse(src) == ast::fixtures::…`
modulo spans, via a span-zeroing helper) — 16.4/16.6 excluded (not in the
grammar). Structured-error quality stays example/golden-based (the near-miss
did-you-mean set, one test each).

**§3's keyword lists are asserted, not just written** (2026-07-27, `bugs/003`).
`every_reserved_word_is_rejected_as_a_relation_name` and
`contextual_keywords_are_ordinary_relation_names` are table-driven over the two
lists §3 states — the eight reserved words, and the fourteen contextual ones
(`table`, the eight type names, the five aggregate operators) — plus a field-name
case for the other half of "relation *or field* names". They exist because §3 had
silently fallen two words behind the lexer: `absent` and `is` were reserved with
no §3 edit, and nothing failed. A doc list a test walks is the only version of
that list that cannot drift.

- [x] **D1** **The §14 closure property**: print any IR fact set in canonical
  output form → parse → lower → identical `Fact` set
  (`d1_fact_set_closure` over `arb_printable_fact_set`). Datalog-out is
  Datalog-in, mechanically checked.
- [x] **D5** **§14's closure at the *program* level** (2026-08-20) — a run's own
  output, appended to the program that produced it, re-runs to the same answers
  (`a_runs_output_appended_to_it_changes_nothing` over `arb_closure_program`).
  D1 closes *fact sets*; this is the claim pillar 3 actually makes, and it
  existed only as one hand-written example in `tests/pipeline.rs`. It is B2's
  fixpoint idempotence carried through the **text** layer, and not implied by
  B2: B2 saturates the IR, where this appends what the *printer* emitted and the
  *parser* read back.

  The generator covers all three §14 rendering paths — substituted atom,
  `answer/N`, and the ground yes `holds(true).` — and the appended output is
  deliberately not inert: a single-relation query answers under that relation's
  name, so the facts land in a relation the program already derives, and in one
  shape in one another rule negates.

  *Mutation — the honest result after five aims.* It has **no kill of its own**,
  because it is a composition claim over stages that each already carry a guard:
  a wrong value rendering reddens D1 too, a wrong answer shape reddens C8 and not
  this, an unparseable ground yes reddens `tests/system.rs` and not this.
  Recorded rather than smoothed over — "no unique kill" is the expected result
  for an end-to-end claim over well-guarded stages, and it is still the only
  check that the stages *compose*.

  **A lesson for the whole catalog came out of that fifth aim**: the `holds.`
  mutation is invisible to `cargo test --lib`, so a mutation check run under that
  filter records a kill that never happened. **Mutation-verify against the full
  suite**, not a filtered one.
  Non-vacuity is `closure_generator_produces_runs_that_answer`, asserting both
  that runs answer at all and that some answer under a real relation's name.

- [x] **D2** `parse(print(ast)) == ast` modulo spans, over generated
  parse-reachable ASTs incl. named-argument and arithmetic forms
  (`d2_ast_round_trip` over `arb_ast_program`; expressions are **arbitrarily
  shaped** binary trees, so the printer's parenthesization is what it holds).
  **D2 is the one that bites, not D3**: measured by mutation, a flat printer
  leaves D3 green — dropping parentheses is still a *fixpoint*, it just
  re-parses to a different tree. Non-vacuity is
  `generator_emits_expression_shapes_that_need_parentheses`, which asserts the
  generator emits right-nesting and mixed precedence; the pre-grouping generator
  produced neither by construction.

  Widened again over the **`as` cast** (2026-08-16), which is a parenthesization
  case arithmetic alone cannot reach: it is postfix and binds tightest, so the
  parentheses that must survive sit on its *operand*
  (`(A + B) as int`). Second guard,
  `generator_emits_casts_over_shapes_that_need_parentheses`, counting three
  shapes against the sentence it certifies — a compound operand, a chain, and a
  cast *inside* arithmetic (the arm asserting a cast goes **un**parenthesized as
  an operand). *Mutation*: flattening `print_cast_operand` reddens D2 as well as
  the acceptance test, which is the evidence the widening reached D2 at all.
- [x] **D3** Canonical fixpoint: `print(parse(print(ast)))` equals
  `print(ast)` (`d3_canonical_print_is_a_fixpoint`) — the printed form is a
  stable canonical representative. Also `print::tests::corpus_round_trips`
  over §16.
  - Concrete acceptance partners, both testing rule 4 for the generator
    widenings they follow: `grouping_survives_the_print_round_trip` pins seven
    shapes by hand — five that must keep their parentheses and two that must
    not — and `casts_survive_the_print_round_trip` pins eight the same way, four
    each side, including the case where dropping the parentheses re-parses
    `(A + B) as int` into `A + (B as int)` and so silently changes the type.
- [x] **D4** Lexer/parser never panic on arbitrary text or bytes
  (`d4_parse_never_panics_on_{text,bytes}`). *Found a genuine bug*: a
  multi-byte escape (`"\¡`) advanced the string scanner off a char boundary;
  the fix is char-aware advancement (regression seed kept in
  `proptest-regressions/parser.txt`).

### Phase D — integration & system tests (roadmap step 5)

The outer layers stay **thin by design** (breadth lives in the unit/property
suites); these prove the *wiring* — that features survive end-to-end through
`run`/the binary — not the feature matrix. Coverage targets the genuinely
integration-level behaviors: the process contract, output shaping, and error
*rendering*.

- **Integration** (`tests/pipeline.rs`): source text → query answers through
  the public `datalog::run`, over corpus files in `tests/programs/*.dl` —
  16.1/2/3/7 answers; 16.5's structured imports-not-yet-supported eval error;
  multi-error recovery; a closure test (materialize output, re-query it);
  `features.dl` (inline arithmetic, float + symbol value formatting, the
  `answer/N` fallback, two queries in one program); `disjunction.dl`; and the
  two error-path shapes lacking earlier — a **type error** (`broken_types.dl`)
  and a **runtime arithmetic error** (`broken_arith.dl`, div-by-zero).
- **System** (`tests/system.rs`): run the compiled binary via
  `env!("CARGO_BIN_EXE_datalog")` (no new deps), asserting stdout bytes, stderr
  content, and exit codes (0/1/2). Includes the §14 **composition-over-a-pipe**
  test — run 16.1, feed its stdout back on stdin with an appended query —
  stdin (`-`) / usage-error paths, exact float/symbol stdout via `features.dl`,
  and stderr rendering for type, runtime, and lexical near-miss errors
  (the `=<`→`<=` hint).
- **The caller's contract (§14) is pinned here and only here**, because the exit
  code is not visible from any inner layer: every code in the vocabulary has a
  test (`rows_found_is_zero_and_no_rows_is_one`,
  `a_program_with_no_queries_exits_zero`,
  `usage_and_program_errors_share_the_did_not_answer_code`), a warning is shown
  not to move it, and §16.13's consistency check is run in both its clean and
  violated halves. One test pins a **boundary** rather than a guarantee —
  `any_query_decides_so_a_files_own_answers_mask_a_dash_q` — so that a later
  session changes that rule deliberately rather than by accident.
- Corpus lives in `datalog/tests/programs/`: the §16 examples as real `.dl`
  files, `features.dl` / `disjunction.dl` for feature wiring, and
  `broken_{multi,unsafe,types,arith}.dl` for the error paths.

Known remaining thin spots (deliberate, per the pyramid): the full did-you-mean
near-miss set is unit-tested, not each re-checked through the binary; provenance
output (§11) has a renderer and its properties (E7/E8) but no end-to-end path,
since no §5 form asks for one.

### Phase E — provenance (§11) — generalizes §16.6

E1–E4 were pulled forward to roadmap step 2 (decided 2026-07-19, spec §17):
provenance recording lands inside the evaluator's fixpoint, so its properties
are tested the session it is written. E3's replay deliberately reuses the
naive oracle's matcher, keeping the check independent of the semi-naive join
loop that recorded the derivation. E6 is E3's claim over a wider generator rather
than a new claim. E7/E8 arrived 2026-08-21 with
the rendering (§11); E9 and E10 the same day with demand-provisioned recording
and the asking form, which is also what unblocked E5.

- [x] **E1** Every derived fact has at least one derivation, and at least one
  is *well-founded* — every fact premise first appeared strictly earlier than
  the fact itself (absences exempt). Pins first-round stamping's cross-strata
  monotonicity directly, not just transitively through E2.
  - **It also pins batched application**, which is what makes the round bound
    strict enough to always *find* a proof rather than merely reject cycles
    (§17, ***Falsified 2026-08-16***). Interleaving collection with insertion —
    streaming, or parallelism — is the change that breaks it, and E1 is the test
    that fires. Measured: a same-round premise occurs on 254 of 659 recorded
    derivations, always a redundant rediscovery and never the first-producing one.
- [x] **E2** Every fact has a proof tree, and every proof-tree leaf is a base
  (EDB/imported) fact.
- [x] **E3** Replay: each derivation node's rule instance applied to its child
  facts rederives exactly the fact (and every premise holds in the model).
- [x] **E4** A base fact's provenance is a leaf.
- [x] **E5** **Comment-stripping is the closure guard.** Proof trees are *not*
  facts (§17, 2026-08-16 — decided in the negative), so there is no fact-shaped
  provenance output to run D1 over. What replaces it: a proof rides in `%`
  comments, so stripping every comment from a program's output must leave
  **byte-for-byte** what the same program prints without its goals. That is what
  keeps Datalog-out-is-Datalog-in true in the presence of provenance, and it is
  cheaper than the closure test it replaces. Over `arb_closure_program` with both
  sigils appended over a fact the run answered, plus one over a fact it did not,
  so the trace arm is exercised beside the proof arm.
  - **It guards a second thing for free**: `?why` provisions the recorder and the
    plain run does not, so this is also *provisioning does not change what a run
    prints* — the printed half of **E9**'s engine-level claim.
  - *Mutations (both killed):* drop the `%` from the trace's `not derivable`
    line; drop it from the explanation header. Non-vacuity: the property asserts
    at least one `%` line was printed, since stripping nothing is trivially
    byte-identical. Its lexical precondition is **E7**.
- [x] **E6** **E3 over §8 builtins.** E3's own generator makes every column a
  symbol (`monotype`), so arithmetic cannot appear and the strongest provenance
  property had never seen an `=`-assignment, a presence test, an aggregate, or a
  **deferred negation** — one whose no-match pattern is closed by an
  assignment-bound value (§7/§10, 2026-07-25). Same claim, wider generator:
  `arb_comparison_program`, which is int-typed and emits all four.
  - **`replay` folds builtin premises in *schedule order*, and that is the whole
    fix.** An `=`-assignment binds a variable a later builtin reads, so body
    order evaluates an expression whose operand is not bound yet and reports a
    sound derivation as broken. A self-justifying premise records the **values**,
    not the expression, so replaying it is *re-evaluation*: the expression is
    evaluated against the replayed environment and compared with the record. The
    bound value comes from the **record**, not the re-evaluation — replaying the
    derivation is the claim, so a wrong recorded value must reach the head and
    fail there.
  - **A §9 aggregate is bound, not recomputed**, its fold being over a model this
    helper does not have. Stated as a limit rather than left to be rediscovered;
    the fold has B11.
  - *Mutations:* replay in body order rather than schedule order — killed;
    **the recorder writes the wrong operand for an assignment** — killed.
    Deleting replay's own agreement check is an **equivalent mutant** and was
    measured as one: it can only fire on an engine already broken, which is why
    the mutation that kills it has to be made in the *recorder*. Non-vacuity: a
    guard pins that the generator reaches both a `Premise::Builtin` and a
    `Premise::NoMatch`, i.e. that E6 is genuinely wider than E3.
- [x] **E7** **Every proof line is a comment** — it starts with `%` and holds no
  newline. What E5 rests on: stripping comments can leave the fact stream
  untouched only if every rendered line *is* one, and a value carrying a newline
  would end the comment and spill arbitrary text into the stream as syntax (§3's
  escapes are what stop it, asserted here rather than assumed). *Mutation:* drop
  the `%` from the node line's format string — E7 reddens, E8 stays green.
- [x] **E8** **The declared depth is the node's depth.** A proof line carries its
  depth as a leading integer *and* as indentation (§17, 2026-08-21); read back off
  each line, the integer must equal that node's depth under an independent walk of
  the tree. This is what makes the number load-bearing rather than a remark about
  the spaces beside it. *Mutation:* freeze the emitted number at `0` — every line
  still a well-formed, correctly-indented comment — and E8 reddens where E7 cannot
  see it. Non-vacuity: a guard pins that `arb_program_with_edb` reaches a proof
  deeper than one node, both properties holding trivially over a program with
  nothing to explain.
- [x] **E9** **Provisioning provenance changes nothing an answer can see.** The
  recorder is provisioned by the run (§17, 2026-08-21), so the same program under
  `Provenance::Recorded`, `Provenance::Reports` and `Provenance::Unrecorded` must
  produce the identical model and the identical answer to every query — which is
  the claim that the three provenance-only maps (`derivations`, `base`,
  `first_round`) never feed the fixpoint. This is the profile's own answer guard
  promoted to a property: it was a stdout digest over 26 programs run from a
  scratch build (`notes/profile-2026-08-20.md`) that was then thrown away, and as
  a property it outlives the build and the corpus both.
  - **`Reports` needs a third claim, and it must not be stated with
    `Derivation::reports`.** That mode keeps *some* derivations (§17,
    2026-09-11), so identical answers no longer imply identical warnings — and an
    oracle that asks `reports()` which derivations should have been kept agrees
    with `reports()` forever. The claim is therefore over the **warnings**,
    through `api::absent_skip_warnings`, plus a subset claim that needs no
    predicate of its own. Most generated programs skip nothing, so that half is
    usually vacuous here;
    `api::tests::the_reporting_store_carries_every_warning_the_full_one_does` is
    the non-vacuous case, and asserts the counts.
  - **Its second half is the one that had to exist.** An unrecorded model must
    answer `Explained::Unrecorded` about a fact that *holds* — never
    `DoesNotHold`, which would be a wrong answer rather than a missing one, and
    is exactly what a two-valued `Option` return would have made unavoidable.
  - *Mutations (all killed):* collapse `Unrecorded` into `DoesNotHold` in
    `ProofTree::explain` — the second half reddens; invert the gate in
    `insert_derived` so an unrecorded run records — the emptiness assertion
    reddens; make `Derivation::reports` return `false` so the reporting store
    keeps nothing — the api-level warning test reddens (E9 itself does **not**,
    measured 2026-09-11, which is why the third claim was rewritten off that
    predicate). Non-vacuity: a guard pins that the generator reaches a program
    with a derived fact *and* a query, both halves holding trivially over an EDB.
- [x] **E10** **A near-miss holds against the model.** `?whynot`'s guard: for a
  goal that does not hold, `trace_failure` re-solves each candidate rule through
  the scheduler the fixpoint uses, so everything it reports must be true of the
  finished model — checked here with this property's own scan, independent of the
  engine's matcher, exactly as E3 checks a no-match premise. Five claims: the
  rule's head really could have produced the goal (stated as the spec states it,
  not asked of the engine); a premise the body *satisfied* holds; a fact a repair
  says to **add** or **ask** about does not already hold; a negation reported
  **refuted** is refuted by a row that is really there; and **a repair must
  repair**.
  - **That last claim is not decoration — it found a defect on the property's
    first run.** A blocked pattern with a slot bound to `absent` was rendered as
    `repair: add p(…, absent)`, and asserting that fact would not advance the
    rule at all: `absent` unifies with nothing (§4), so the join cannot use the
    row even once it exists. A repair that does not repair is the one thing a
    repair must not be, and `Repair::AbsentKey` is what the arm became.
  - **`Repair::Unbound` is checked for shape only, deliberately.** A
    `NoMatchPattern` cannot express a *repeated variable*, so `p(X, X)` blocked
    with `X` unbound renders as the all-open `p(_, _)`, which any row matches
    while the atom is genuinely blocked. The pattern is a rendering of the block,
    not a decidable statement of it — learned by asserting the stronger claim and
    watching it fail on a correct trace.
  - *Mutations (all three killed):* report the blocked literal one past the
    deepest prefix reached; drop the head-unification guard so every rule of the
    predicate near-misses; drop the absent-key guard. Non-vacuity: a guard pins
    that the generator reaches a near-miss carrying both a satisfied premise and
    a fact-naming repair.

### Phase F — §13 imports (roadmap step 7) — generalizes §16.5, §16.7

Ratified 2026-07-23 with the §13 deep-dive. The reader is DuckDB (default-on
feature), so the default `cargo test` exercises it; a `--no-default-features`
lane pins the structured feature-error path. Temp files via a small hand-rolled
scratch-dir helper in tests (`std::env::temp_dir()` + pid + counter — no
`tempfile` dep). The URL happy path is `#[ignore]` (network).

- [x] **F1** CSV round-trip: arbitrary cell strings (incl. quotes, commas,
  newlines, CRLF) → test-side RFC 4180 writer → import ≡ the original table.
  Doubles as a conformance check on the DuckDB read options (`all_varchar`,
  `header=false`).
- [x] **F2** Inference oracle: generated text tables import with column types
  matching an independent in-test implementation of the §13 literal-grammar
  rules (lexer-classified cells, column unification).
- [x] **F3** **Import ≡ inline facts** (the anchor property): a generated typed
  table, imported, evaluates to the same model and answers as the same facts
  written as in-program literals — the full pipeline run twice.
- [x] **F4** JSONL round-trip: generated typed records → test-side writer →
  import ≡ the original values (JSON strings never re-inferred). At least one
  record: a record-less file names no fields, and both of its outcomes are pinned
  by `a_jsonl_file_with_no_records_is_empty_under_a_schema_and_an_error_without`
  (`bugs/resolved/010`).
- [x] **F5** Module diamond: root→{a,b}, a→c, b→c import graphs evaluate
  identically to the flat concatenation of the four files (once-only splice).
- [x] **F6** Module cycles: mutually-importing files terminate and equal the
  union of their statements.
- [x] **F7** Parquet round-trip: fixture written at test time via DuckDB `COPY`
  (no binary files in the repo), imported, ≡ the original typed table.

### Phase T — temporal values (§4/§8/§13) — generalizes §16.14

Ratified 2026-08-19 (`notes/temporal-values.md`). The value layer is
`src/temporal.rs`, which is dependency-free: civil↔day conversion is Hinnant's
algorithm, so **the oracle is the calendar's own algebra** rather than a second
implementation.

- [x] **T1** Canonical text round-trips: `parse(print(v)) == v` for all three
  types, with the duration generator's non-vacuity guard (it reaches every unit
  of the decomposition and both signs). This is §14's closure property at the
  value layer. `t1_date_text_round_trips`,
  `t1_timestamp_text_round_trips`, `t1_duration_text_round_trips`,
  `t1_duration_generator_reaches_every_unit`; plus the calendar bijection
  (`civil_conversion_round_trips`, `consecutive_days_are_one_apart`).
  *Mutation:* dropped the `us` component from `Duration`'s decomposition —
  T1 and its non-vacuity guard both went red (the guard first, which is the
  point of having it).
- [x] **T2** Affine laws through the whole pipeline (`tests/pipeline.rs`):
  `(D + K) - D == K` and `D + (E - D) == E` — §8's algebra as an equation
  rather than a table. `t2_affine_laws_hold`.
- [x] **T3** **The unit is the divisor**: `(D2 - D1)` over both `@1d` and
  `@36h` equals a difference computed from the day counts themselves, not by a
  second pass over the engine's arithmetic. The `@36h` divisor is what makes
  the expected value fractional, and so what makes the property sensitive to
  the result type. This is the property that pins the sibling engine's
  `172800000` finding. `t3_the_divisor_names_the_unit`, with
  `t3_generator_reaches_both_signs` as the non-vacuity half.
  *Mutation:* made `duration / duration` return a truncating `int` — T3 and
  `a_duration_divided_by_a_duration_keeps_the_half_day` both went red.
- [x] **T4** Ordering agreement: comparing two dates gives what comparing their
  ISO text gives. The old behaviour is a free reference implementation —
  ISO-8601 sorts lexicographically, which is why date *filtering* already worked
  before this feature (`bugs/006`) — so this is the property that says the
  feature took nothing away. `t4_date_order_agrees_with_the_text_order_it_replaces`.
  *Mutation:* reversed `Date`'s `Ord` — T4 went red on the first pair.
- [x] **T5** `truncate` is idempotent and monotone — the two properties that
  make a truncated value usable as a group key, since a key that moved under
  re-truncation would double-count and one that reordered would sort wrong. The
  extraction half checks `year`/`month`/`day` against the components of the
  value's own canonical text, which is the only independent statement of what
  those components are. `t5_truncation_is_idempotent_and_monotone`,
  `t5_extraction_agrees_with_the_printed_form`.
  *Mutation:* moved a month's period start back one day — T5 went red
  immediately. **Worth recording: the first mutation tried did not.** Shifting
  the week's origin from Monday to Sunday is *equally* idempotent and monotone,
  so the property is blind to it — and the example written to cover that gap
  (`a_week_starts_on_monday`) failed on the real implementation, which had the
  epoch weekday off by one. A property that cannot fail on a wrong answer is
  the case rule 3 exists to expose, and here it took the mutation to find that
  the property needed an example beside it.
- [x] **T7** **The temporal fold rules** (§9, 2026-08-20), reachable only after
  the value pools widened: **`avg` over durations is a `duration`, not a
  `float`** (`avg_over_durations_is_a_duration`) — the rule §9 argues for at
  length, which had exactly one hand test; and **`sum` over a point type is a
  type error** (`sum_over_a_point_type_is_rejected`), the rejection half the
  corollary asks for beside it. `min`/`max` over dates and timestamps come from
  `min_and_max_bound_every_value`, widened off its int-only pool to
  `arb_value` filtered to one type per fold.
  *Mutation:* removing `avg_values`' duration branch reddens
  `avg_over_durations_is_a_duration` — the message becomes "avg requires values
  it can fold and divide … got duration", which is the numeric branch refusing
  the vector case.

- [x] **T8** **The cast laws reach all eight types** (2026-08-20).
  `absent_survives_every_cast` now selects from eight `TypeName`s, and
  `casting_through_string_is_the_identity` carries the three temporal arms —
  the half **T1 cannot reach**, since T1 round-trips through
  `temporal::parse_temporal` directly while this goes through `apply_cast`, so
  the reading side is `lexer::classify_cell` and the writing side is the
  render-minus-`@`. `casting_to_a_values_own_type_is_the_identity` reached all
  eight **without an edit**, because it takes `arb_value` — which is the argument
  for fixing the pool rather than each property.
  *Mutation:* making the `date` render keep its `@` reddens
  `casting_through_string_is_the_identity` on the first date drawn.

- [x] **T6** Import anchoring (the rule-4 acceptance property for the widened
  generators): a CSV of temporal cells imports to exactly the values the
  corresponding `@`-sigilled literals denote — **F3**'s claim extended to the
  sigil, which it has to be, because inference now reads a form the lexer alone
  does not. `t6_temporal_cells_import_as_their_literals`, with
  `t6_generator_writes_unsigilled_cells_of_both_types` as the non-vacuity half
  — it asserts the generated cells carry **no** `@`, since a generator that
  emitted one would be testing the lexer rather than the reader.
  *Mutation:* made CSV inference produce `string` for an ISO cell — T6 went red
  on the first case.

- [x] **C16** **A rejected program is always branchable** (2026-08-25, §12's code
  vocabulary) — every diagnostic a corrupted program produces carries a code from
  `ErrorCode::ALL`, and the code alone decides the category
  (`c16_every_diagnostic_carries_a_pinned_code`, over
  `arb_corrupted_program_text`).

  What it catches is the **one place the compiler cannot look**. Adding an
  `ErrorCode` variant is forced through two exhaustive matches, but `ALL` is a
  hand-written list, and a variant missing from it is a code that reaches a
  consumer while §12 says the set is complete. Only a run that observes codes in
  the wild sees that.

  The generator is the first one in the suite that makes programs **fail**:
  everything before it generates programs that work, so nothing swept what a
  failure looks like. It prints a correct program and breaks one thing —
  a Prolog operator, an uppercase relation, a doubled comma, a truncation, a
  blind byte deletion.

  *Guard* — `c16_generator_rejects_and_reaches_several_families` asserts both
  halves: that corruptions are actually rejected (>64 of 256), and that they
  reach **six or more** families. Ten were observed across four categories when
  it was written. Without the second half a generator that collapsed to the
  parser's catch-all would sweep one arm of the vocabulary and call it the set.

  *Mutation:* removing `TrailingComma` from `ErrorCode::ALL` while a site still
  raises it reddens C16 on the first corrupted program that trips a trailing
  comma — which is mechanically the case the property exists for, a code emitted
  in the wild that the pinned set does not list. The guard stays **green** under
  it, which is the contrast: the guard counts families and the property checks
  membership, and neither substitutes for the other.
