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
| Aggregation (§9) | `aggregate_ir` (grouped, one relation); `grouped_ir` (group keys from a *second* relation, empty groups, absent witnesses); `aggregate_goal_ir` (multi-atom + negated goal, all 6 goal orderings) | **independent group-by oracle** (`aggregation_matches_an_independent_group_by`); B1 differentials (`b1_aggregate_programs_agree`, `b1_aggregate_goal_shapes_agree`); body-order invariance (`b5_aggregate_body_order_does_not_change_the_model`); fold laws (absent-skip, empty→absent, count=witnesses, sum oracle, min/max bounds); §16.4 hand test; query-position and nested/assignment-bound goals in `tests/pipeline.rs` |
| Absent value (§4/§8) | `arb_fact_constant` (absent in *data*, ~1 in 10) + `absent_ir` (join / self-join / anti-join / comparison / arithmetic / presence over a `{0, 1, absent}` pool) | B1 absent differential (`b1_absent_programs_agree`); value laws (annihilation, comparison-false, unify-vs-eq, sorts-first); `generator_emits_absent_in_facts_only` |

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
  cross-type order symbol < string < int < float < bool (§14).
- [x] **A5** `Fact` set semantics: `HashSet` size equals Ord-dedup size under
  arbitrary duplication.

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
- [x] **B8** Query/rule equivalence: `Model::answer(q)` equals the relation of
  a synthesized rule whose head projects `q`'s named variables over `q`'s
  body, evaluated in a fresh final stratum. A query is a rule plus projection,
  mechanically checked; under the Phase-C generator this extends to negated
  query bodies. (Added by the 2026-07-20 coverage audit — queries were the
  one near-untested half of the algebra.)

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
  `a_computed_negated_argument_is_recorded_as_a_closed_absence` pins the
  **provenance** half: the absence pattern must record `Some(2)`, not an open
  slot, or "why?" answers the far stronger "because no `q` fact exists".

  *Known gap:* derivation **replay** does not reach a deferred negation, whose
  absence pattern depends on an assignment-bound value — see **E6** below.
- [ ] **C8** **Surface-spelling equivalence** — the language's "form A means the
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

  - **One answer set, two output shapes**
    `filtered_atom_query_answers_like_its_unfiltered_shape` — **specified, not
    yet green**; the acceptance criterion for §14's widening (§17 2026-08-03,
    `notes/query-answer-shape.md`). For a body of one positive atom plus literals
    that bind nothing, the rows are the same set whichever shape §14 selects;
    only the functor differs. Written `#[ignore]`d and failing, the way
    `dash_q_rule_equals_the_same_rule_in_a_file` was written for `bugs/002` —
    deleting the `#[ignore]` is what closing the item looks like.

    Its generator must be built **backwards from a fact the EDB contains**, and
    then filtered by a comparison chosen to hold on that fact. This is the
    `bugs/005` lesson applied before the fact rather than after: three things
    must coincide before the shapes can differ — one positive atom, a
    non-binding literal beside it, and a matching fact — and over arbitrary
    programs the third almost never holds, so both shapes print nothing and
    agree. The non-vacuity guard asserts the generated queries **answer**, not
    merely that they type-check.

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
  red with `AbsentPattern::matches` reverted before being kept — `bugs/005`'s
  discipline.

  Its counterpart in the other direction is
  `repeating_a_body_literal_drops_absent_rows` (Phase B above): the law C9 does
  *not* generalize to, kept so the asymmetry is a decision on the record rather
  than a gap.

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
lists §3 states — the eight reserved words, and the eleven contextual ones
(`table`, the five type names, the five aggregate operators) — plus a field-name
case for the other half of "relation *or field* names". They exist because §3 had
silently fallen two words behind the lexer: `absent` and `is` were reserved with
no §3 edit, and nothing failed. A doc list a test walks is the only version of
that list that cannot drift.

- [x] **D1** **The §14 closure property**: print any IR fact set in canonical
  output form → parse → lower → identical `Fact` set
  (`d1_fact_set_closure` over `arb_printable_fact_set`). Datalog-out is
  Datalog-in, mechanically checked.
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
- [x] **D3** Canonical fixpoint: `print(parse(print(ast)))` equals
  `print(ast)` (`d3_canonical_print_is_a_fixpoint`) — the printed form is a
  stable canonical representative. Also `print::tests::corpus_round_trips`
  over §16.
  - Concrete acceptance partner: `grouping_survives_the_print_round_trip` pins
    the seven shapes by hand, both directions — five that must keep their
    parentheses and two that must not (testing rule 4, for the generator
    widening that grouping required).
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
  and exit-1 stderr rendering for type, runtime, and lexical near-miss errors
  (the `=<`→`<=` hint).
- Corpus lives in `datalog/tests/programs/`: the §16 examples as real `.dl`
  files, `features.dl` / `disjunction.dl` for feature wiring, and
  `broken_{multi,unsafe,types,arith}.dl` for the error paths.

Known remaining thin spots (deliberate, per the pyramid): the full did-you-mean
near-miss set is unit-tested, not each re-checked through the binary; provenance
output (§11) has no end-to-end path yet (step 6).

### Phase E — provenance (§11) — generalizes §16.6

E1–E4 were pulled forward to roadmap step 2 (decided 2026-07-19, spec §17):
provenance recording lands inside the evaluator's fixpoint, so its properties
are tested the session it is written. E3's replay deliberately reuses the
naive oracle's matcher, keeping the check independent of the semi-naive join
loop that recorded the derivation. E5 waits on the §11/§14 output surface, and E6
is a coverage hole in E3 rather than a new claim.

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
- [ ] **E5** **Comment-stripping is the closure guard.** Proof trees are *not*
  facts (§17, 2026-08-16 — decided in the negative), so there is no fact-shaped
  provenance output to run D1 over. What replaces it: a proof rides in `%`
  comments, so stripping every comment from a program's output must leave
  **byte-for-byte** what the same program prints without its goals. That is what
  keeps Datalog-out-is-Datalog-in true in the presence of provenance, and it is
  cheaper than the closure test it replaces. Waits on the §11/§14 output surface.
- [ ] **E6** Extend **E3 to §8 builtins**. E3 passes today only because
  `arb_program_with_edb` emits no comparisons — `monotype` makes every column a
  symbol, so arithmetic cannot appear — and its `replay` helper rebuilds the
  environment from *fact* premises alone, returning `None` on any
  `Premise::Builtin`. The strongest provenance property has therefore never seen
  an `=`-assignment, a presence test or an aggregate, and by extension never sees
  a **deferred negation**, whose absence pattern is closed by an assignment-bound
  value (§7/§10, 2026-07-25). The work: fold builtin premises into the replayed
  environment in schedule order, then run E3 over `arb_comparison_program`, which
  is int-typed and already emits them. Note what replay has to become — a
  `Premise::Builtin` records the *values*, not the expression, so replaying it is
  re-evaluation, not re-checking. Tracked in `ROADMAP.md` under "Provenance
  surface".

### Phase F — §13 imports (roadmap step 7) — generalizes §16.5, §16.7

Ratified 2026-07-23 with the §13 deep-dive. The reader is DuckDB (default-on
feature), so the default `cargo test` exercises it; a `--no-default-features`
lane pins the structured feature-error path. Temp files via a small hand-rolled
scratch-dir helper in tests (`std::env::temp_dir()` + pid + counter — no
`tempfile` dep). The URL happy path is `#[ignore]` (network).

- [ ] **F1** CSV round-trip: arbitrary cell strings (incl. quotes, commas,
  newlines, CRLF) → test-side RFC 4180 writer → import ≡ the original table.
  Doubles as a conformance check on the DuckDB read options (`all_varchar`,
  `header=false`).
- [ ] **F2** Inference oracle: generated text tables import with column types
  matching an independent in-test implementation of the §13 literal-grammar
  rules (lexer-classified cells, column unification).
- [ ] **F3** **Import ≡ inline facts** (the anchor property): a generated typed
  table, imported, evaluates to the same model and answers as the same facts
  written as in-program literals — the full pipeline run twice.
- [ ] **F4** JSONL round-trip: generated typed records → test-side writer →
  import ≡ the original values (JSON strings never re-inferred).
- [ ] **F5** Module diamond: root→{a,b}, a→c, b→c import graphs evaluate
  identically to the flat concatenation of the four files (once-only splice).
- [ ] **F6** Module cycles: mutually-importing files terminate and equal the
  union of their statements.
- [ ] **F7** Parquet round-trip: fixture written at test time via DuckDB `COPY`
  (no binary files in the repo), imported, ≡ the original typed table.
