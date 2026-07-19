# Testing strategy

*Living document. This is the single source of truth for how the `datalog`
crate is tested; `AGENTS.md` and `spec.md` §17 point here. Update the property
catalog's checkboxes as layers land.*

## The pyramid

The test pyramid grows outward with the pipeline (AGENTS.md):

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

## Tooling

- **proptest** is the crate's first dev-dependency (§17, 2026-07-19). Decisive
  feature: integrated shrinking — counterexamples shrink *through the
  strategy* that produced them, which matters enormously for recursive program
  structures. Dev-dependencies do not affect the shipped library's
  zero-dependency posture.
- **Commit `proptest-regressions/` directories.** Persisted failing seeds are
  free regression tests.
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

**Policy — generators vs. the no-DSL rule.** The no-macro-DSL/no-builder rule
(AGENTS.md) is about ergonomic sugar for hand-written tests; generators are
*coverage machinery* that construct plain `ast::`/`ir::` values and return
them. They are `pub(crate)`, test-only, and must never be used to shorten
hand-written example tests — §16-derived example tests construct literal
structs verbatim. If a generator helper starts looking like a convenience API
for humans, it has crossed the line.

## Property catalog

Status: `[x]` implemented and green; `[ ]` specified, waiting on its layer.
Each phase names the roadmap step that unblocks it and the §16 examples it
generalizes.

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
  The anchor property — catches delta-bookkeeping bugs directly.
- [x] **B2** Fixpoint idempotence: re-running with `facts ∪ output` derives
  nothing new.
- [x] **B3** Set semantics: duplicating any subset of input facts leaves
  output unchanged.
- [x] **B4** Monotonicity for positive programs (the queryFuzz relation):
  `output(p) ⊆ output(p + fact)` and `⊆ output(p + fact + rule)`.
- [x] **B5** Body-reorder invariance: permuting a rule's body leaves output
  unchanged. (Safety is occurrence-based, so permutations of a safe rule stay
  safe; revisit when §8 mode/binding rules for arithmetic land.)
- [x] **B6** Rule-order invariance within a stratum.
- [x] **B7** Independent oracle for a fixed shape: random `parent` edge sets
  into the §16.1 ancestor program vs. a hand-rolled DFS transitive closure
  (independent of *both* evaluators).

### Phase C — negation + type inference (roadmap step 3) — generalizes §16.2, §16.3

- [ ] **C1** Stratification correctness: every negated dependency sits in a
  strictly lower stratum; generated negative-cycle programs are rejected.
- [ ] **C2** Brute-force perfect-model oracle on small domains (iterated
  fixpoint per stratum, computed directly).
- [ ] **C3** B1–B6 re-run over stratified programs (B4 restricted to fact
  additions in predicates no negation depends on — or skipped under negation).
- [ ] **C4** Type-inference soundness: any program inference accepts evaluates
  with no type-based runtime error; derived facts match inferred column types.
- [ ] **C5** Typed-generator completeness: well-typed-by-construction programs
  are always accepted by inference.

### Phase D — lexer + parser (roadmap step 4) — generalizes all §16 source texts

- [ ] **D1** **The §14 closure property**: pretty-print any IR fact set in
  canonical output form → parse → lower → identical `Fact` set.
  Datalog-out is Datalog-in, mechanically checked.
- [ ] **D2** `parse(print(ast)) == ast` modulo spans, over generated ASTs
  (including named-argument forms).
- [ ] **D3** Canonical fixpoint: `print(parse(t)) == t` for canonical-form
  text.
- [ ] **D4** Lexer/parser never panic on arbitrary byte strings.

### Phase E — provenance (§11) — generalizes §16.6

E1–E4 were pulled forward to roadmap step 2 (decided 2026-07-19, spec §17):
provenance recording lands inside the evaluator's fixpoint, so its properties
are tested the session it is written. E3's replay deliberately reuses the
naive oracle's matcher, keeping the check independent of the semi-naive join
loop that recorded the derivation. Only E5 waits on the step-5 surface design.

- [x] **E1** Every derived fact has at least one derivation.
- [x] **E2** Every fact has a proof tree, and every proof-tree leaf is a base
  (EDB/imported) fact.
- [x] **E3** Replay: each derivation node's rule instance applied to its child
  facts rederives exactly the fact (and every premise holds in the model).
- [x] **E4** A base fact's provenance is a leaf.
- [ ] **E5** Once provenance-as-facts is designed (§17 open question),
  provenance output itself satisfies D1 closure.
