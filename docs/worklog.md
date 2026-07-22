# Worklog

A running handoff log for chaining agentic coding sessions. Each session ends by
adding an entry so the next session can get oriented in seconds — without re-reading
raw transcripts (Claude Code auto-saves those under
`~/.claude/projects/<repo-slug>/*.jsonl`; resume with `claude --resume`).

**Conventions**
- Newest entry on top (reverse-chronological).
- Keep each entry short and high-signal. Three fields:
  - **Done** — what changed this session (link commits/PRs where useful).
  - **Decided** — key decisions made (design decisions also go in `datalog/spec.md`
    §17; note them here too so the timeline is complete).
  - **Next up** — the concrete next threads, so the following session starts oriented.
- This is a curated summary, not a transcript. Don't paste raw output here.

---

## 2026-07-21 — `declare`-signature verification (closes §4)

**Done**
- **Declared column types now flow AST→IR and are verified.** `ir::PredicateInfo`
  gains `field_types: Option<Vec<Option<TypeName>>>` parallel to `fields`
  (invariant: `Some` iff `fields` is `Some`, same length; per-position `None` for a
  named-but-untyped field). `lower::collect_schema` records `FieldDecl.ty` and
  `attach_field_names` threads it onto the IR (13 `PredicateInfo` sites updated;
  §16.7 fixture now carries `person` and `employee` types). Two schemas agreeing on
  names but disagreeing on types now conflict, naming both origins.
- **Verification is a dedicated pass in `typecheck::finish`** (not via `set_type` in
  `gather`): each declared column type is compared against the resolved inferred
  type; a contradiction is a structured `Error::Semantic` naming the column
  ("declared as X but its values are Y"). A declared type inference never
  constrains is left unrefuted and seeds the column's type in `TypeEnv`. `eval`
  stays type-blind; no pipeline change.
- **Tests: 107 → 115.** typecheck units (match / conflict naming `person.age` /
  untyped field ignored / declared-only seeds type); lowering units
  (`declared_types_reach_the_ir` + the `field_types`↔`fields` invariant;
  `schemas_conflicting_only_on_types_are_reported`); **property C6** over
  `arb_well_typed_program` (correct signature always accepted; a self-contained
  wrong-typed probe column always rejected, naming it). Clippy/rustfmt clean.
- **Docs**: spec §4 status + `declare` paragraph + a §17 decision entry (2026-07-21);
  testing.md **C6** added, C5 deferral updated (only imported *inferred* types
  remain, §13).

**Decided** (also in spec §17, 2026-07-21)
- `field_types` reuses `ast::TypeName` — `ir` already imports `ArithOp`/`CmpOp`/
  `Span` from `ast`, so no new coupling. Verification lives in `typecheck::finish`
  for a tailored message and to keep declared types out of inference propagation.

**Next up**
- **Lexer + parser** (roadmap step 5, Phase D): source text → AST, golden tests for
  structured errors (testing.md D1–D4). Then wire the full `parse → lower →
  typecheck → eval` pipeline (today `typecheck` is only invoked from tests).
- Imported *inferred* column types remain deferred to §13.

## 2026-07-21 — §8 literal-value builtins, then §4 type inference

**Done**
- **§8 comparison & arithmetic builtins now evaluate** (engine + naive oracle).
  Comparisons are anti-join filters; `=` binds a bare unbound variable
  (assignment) else compares. Strict numerics — `int op int`/`float op float`
  only; truncating integer `/`; division-by-zero, integer overflow, NaN, and
  cross-type comparison are structured `Error::Semantic`. The join now threads
  `Result` (`enumerate_from`/`enumerate_matches`/`collect_rule_matches`/
  `eval_stratum`/`eval`/`Model::answer`) so runtime arithmetic errors propagate;
  comparisons are scheduled after positives/negations at their true `BodyIdx`.
  New `Premise::Builtin`/`ProofTree::Builtin` keep provenance complete.
- **Lowering assignment-safety exception** (`safe_bound_vars`/
  `assignment_target`): an `=`-target counts as range-restricted for head vars
  and comparison operands, computed in source order (negated-atom vars still
  require *positive* binding, since negations run before comparisons).
- **§4 type inference** — new `src/typecheck.rs`: a union-find over the five
  primitives (one class per column / per rule-or-query variable), gathering
  constraints from facts, variable flow, and §8 operand rules; conflicts
  collected (not fail-fast) and reported naming the column/variable.
  `typecheck` runs between `lower` and `eval`; `eval` stays type-blind.
- **Tests: 88 → 107.** §16.3 end-to-end; §8 error-path units (div-by-zero,
  overflow, NaN, mixed, cross-type); assignment lowers+evals; 5 typecheck
  units; B1 extended over `arb_comparison_program` (incl. error path); **C4/C5**
  over `arb_well_typed_program`; `injected_type_conflict_is_rejected`; coverage
  guards for both new generators. Mutation-checked (7 mutants, each reverted
  after confirming failure; no regression seeds kept). Clippy/rustfmt clean.
- **Docs**: spec §8 → Draft (semantics + the three resolved open questions),
  §4 status (inference implemented), §16.3 resolved, two §17 decision entries;
  testing.md coverage map + B1/B5 + C4/C5.

**Decided** (also in spec §17, 2026-07-21)
- The three §8 open questions: `=` is assignment-or-equality; strict numerics,
  no coercion; truncating int `/` with checked/structured arithmetic errors.
- **`eval` stays type-blind** (separation of concerns), and the **evaluation
  property generators migrated to well-typed programs**: `arb_program_with_edb`/
  `arb_extension_pair` relabel every constant to a `symbol` injectively
  (`monotype`) — isomorphic to the old programs, so no property changed
  behavior — with A1–A5 keeping cross-type `Value` coverage.
  `evaluation_generator_is_well_typed` pins the invariant.

**Next up**
- **`declare`-signature verification** (§4): thread declared `TypeName`s onto
  `ir::PredicateInfo` (a parallel `field_types`, ~15 construction sites) and
  verify inferred vs. declared — the one piece of §4 left. Then **lexer +
  parser** (roadmap step 5, Phase D). Imported column types wait on §13.

## 2026-07-20 (research-note session) — semiring provenance under negation, parked

**Done**
- Discussed the semiring-provenance-with-negation thread surfaced during §7
  and recorded it as a parked research note:
  `datalog/notes/semiring-provenance.md` (new `notes/` directory). Core
  observations: `Model.derivations` is already a boolean provenance circuit
  in DAG form (Deutch et al.'s representation at the boolean semiring);
  `Premise::Absent` is a *factored* dual token — the symbolic form of
  Grädel–Tannen's expanded ∀-product under CWA + active domain; strata are an
  iterated `X·X̄ = 0` quotient and first-round stamping a monomial selector.
  A semiring layer would therefore be an interpretation folded over the
  existing derivation store, not a redesign.
- Ranked opportunities recorded in the note: (1) `?whynot` with minimal
  repairs (natural landing spot: the step-6 provenance surface), (2) a
  `Semiring` trait over the derivation DAG with **tropical cheapest-proof
  selection for token economy** as the on-brand novel application, (3) a
  TaPP-sized theory write-up of the factored-dual-token observation.
  Non-goals and a verify-before-use reading list included.
- Pointers added: spec §17 open questions and references.md group 5 both
  link the note.

**Decided**
- The thread is **parked** — not on the roadmap; the §17 proof-tree decision
  stands unmodified. The note exists so a future session can reboot the
  thread without this conversation's context.

**Next up**
- Unchanged: **§8 builtins**, then type inference (§4, C4–C5).

## 2026-07-20 (audit session) — FO-algebra coverage audit; the query gap

**Done**
- Audited the property catalog and generators against the first-order algebra
  after the negation milestone. Core verdict: selection, projection, join,
  product, union, difference, recursion, and set semantics all had generated
  + oracle coverage. One systematic hole: **queries** — the generator never
  emitted them, `Model::answer` had exactly one call site, and query negation
  (claimed "free" in the negation session) had zero tests. Four unpinned
  edges: head wildcards, three-strata chains, empty-negated-relations,
  first-round well-foundedness (comment-only).
- **B8 — query/rule equivalence** (the centerpiece): `Model::answer(q)` must
  equal the relation of a synthesized rule projecting `q`'s named variables,
  run in a fresh final stratum. The generator now emits queries from the same
  `build_body` machinery as rule bodies (factored out; queries binding no
  named variable are skipped), `positionalize` rewrites query bodies (A13
  covers named query atoms), and a coverage guard pins queries + negated
  queries as generator outputs.
- Hand tests for every audited edge: negated query over §16.2 (first direct
  `Model::answer`-with-negation test), empty negated relation (trivially
  true), double negation across three strata (`c = d∖(d∖a) = a` within the
  domain), head wildcard unsafety, negated-query lowering shape, the `query`
  safety-context string (first assertion ever), three-strata lowering chain.
- **E1 extended**: every derived fact must have a *well-founded* derivation
  (fact premises strictly earlier; absences exempt) — the invariant
  `explain` selects by, now pinned directly.
- testing.md: B8 catalog entry, E1 update, generator-section bullets for
  negation/query generation, a **First-order algebra coverage map** table so
  the next audit starts from this frame, and the proptest-regressions policy
  clarified (commit genuine-failure seeds; delete mutation-check seeds).
- 88 tests, fmt/clippy clean.

**Decided**
- Deliberate non-gaps left open: §8 comparisons/arithmetic, C4/C5 type
  inference, D1–D4 parser, E5 — all roadmap-tracked, not oversights.
- Mutation checks: reversing `Model::answer`'s projection order fails **B8
  alone** (the query-only defect nothing else sees — proof the property earns
  its keep); dropping negated literals from evaluation order fails the
  negated-query hand test plus eleven others (B8 is blind to symmetric
  breakage — why the hand test exists); stamping derived facts round 0 fails
  E1's new assertion directly.

**Next up**
- Unchanged from the negation session: **§8 builtins**, then type inference
  (§4, C4–C5). The B8 synthesized-rule pattern also gives §8 a ready-made
  equivalence check for comparison-bearing queries when they land.

## 2026-07-20 (later session) — §7 stratified negation (roadmap step 4)

**Done** — four commits, tree green after each.
- **Spec first**: §7 drafted (perfect model per Apt/Blair/Walker; the
  independence theorem stated, making the numbering choice semantics-free);
  §10 narrowed to *named* variables in negated atoms; §11 premises became
  fact-or-absence; §16.2 resolved; six new §17 entries. Closed a real
  grounding gap: references.md group 5 was positive-programs-only — added
  Grädel–Tannen 2017 and Dannert–Grädel–Naaf–Tannen CSL 2021 as the
  principled semiring treatments of negation we deliberately do *not* adopt
  (`Premise::Absent` is a proof-tree-level why-not record).
- **Lowering**: `stratify` — Ullman relaxation, structured error naming a
  concrete cycle ("a -> not b -> a"), single-stratum fallback on the error
  path; safety narrowed in the `NegAtom` arm only; §16.2 became the
  `lower(ast) == ir` contract fixture, plus cycle and two-strata hand tests.
- **Engine/provenance/oracle**: anti-join filter in the join loop
  (`AbsentPattern`; binds nothing; always the full frozen relation),
  positives-then-negations evaluation order with premises recorded at their
  true `BodyIdx`, two static malformed-IR guards (named negated vars
  positively bound; negated preds defined strictly lower), the `Premise` enum
  + `ProofTree::Absent` leaves + round-guard exemption for absences, naive
  oracle iterating strata. §16.2 end-to-end (exact derivation + proof tree);
  negation over a recursive IDB across two strata, written
  negation-before-binder.
- **Phase C**: generator levels (stratifiable by construction — negated
  selectors draw strictly below the head's level), `NegativeCycle` /
  `UnsafeNegatedVar` defects, negation coverage guards. C1 (recomputed-graph
  check; reject side via A11), C2 (independent §16.2 set-difference oracle),
  C3 (B1–B6 over the negation-emitting generator — B1 is now the
  perfect-model differential). B4 fact-half restricted via
  `negation_independent_preds`; rule-half extends on fresh `ext_*`
  predicates. A9/A12 generalized. 79 tests, clippy/fmt clean.

**Decided** (all in spec §17, 2026-07-20): Ullman relaxation over Tarjan SCC;
`AbsentPattern` = `PredId` + `Vec<Option<Value>>`; evaluator-internal
negations-last ordering; the naive oracle is the per-stratum perfect-model
oracle; the engine statically validates the negation contract; negation
provenance is a why-not record, not a semiring construction. The previous
session's "`VarScope` must track wildcard-fresh slots" guess proved
unnecessary: `VarScope::fresh` never enters the name map, so a `None`-named
slot inside a negated atom was necessarily created there — the safety check
simply skips unnamed slots (argument recorded at the check site and in §17).

**Mutation checks** (each reverted after confirming failure): re-widened
safety arm → §16.2 contract test; dropped negative-edge increment → all three
stratification tests; raw body-order evaluation → negation-before-binder
test; absences failing the round guard → §16.2 proof tree; broken
wildcard-existential matching → B1 + C2 + §16.2 at once.

**Next up**
- **§8 builtins** (comparisons/arithmetic incl. the open `=` question), then
  **type inference** (§4, C4–C5) — the rest of roadmap step 4. The engine's
  evaluation-order scheduling (positives first, then filters) is the shape
  comparison literals will slot into.
- Provenance *surface* (`?why`, JSON encoding, provenance-as-facts, E5) stays
  parked until step 6; rendering `AbsentPattern` in named form (§4 field
  names) can ride along when it lands.

## 2026-07-20 — Named-argument lowering (pass 2)

**Done**
- Implemented lowering pass 2, the last unimplemented path in `lower()`.
  `src/lower.rs`: a `lower`-internal field registry (`Lowerer.schemas`,
  `FieldSchema`/`SchemaOrigin`) populated in pass 1 from `declare` statements
  and explicit import schemas; `lower_named_atom` resolving fields to schema
  positions (order-insensitive), filling omitted fields with fresh slots
  (partial selection) and enforcing §4's all-fields rule on heads via a new
  `AtomPos` parameter; structured errors for unknown field, duplicate field in
  a literal, missing schema, incomplete named head, duplicate field within a
  schema, and conflicting schemas.
- **Fixed a latent bug the stub was hiding**: fact grounding zipped the lowered
  head against *surface positional terms*, and `positional_terms` returned
  `&[]` for named args — so the first named fact to lower successfully would
  have been silently dropped with no fact and no error. Grounding now runs
  against the lowered head; `positional_terms`/`index_of` deleted.
- Fixtures: `ast::fixtures::example_16_7` / `ir::fixtures::example_16_7` with
  `lower(ast) == ir` as the contract test; the pre-existing §16.7 AST test now
  uses the fixture instead of rebuilding it inline. Eleven example-based tests
  (equivalence, order-insensitivity, partial selection, late `declare`, and one
  per error case).
- Testing: `testgen` now emits *both* argument forms — ~half the predicates get
  a `declare`, atoms over those may be named, partially selected in bodies and
  fully supplied in heads — so A6–A12 cover the named path. New `positionalize`
  + property **A13** (named/positional lower identically), three new
  `DefectKind`s feeding A11, and two generator-coverage guards so A13 cannot
  pass vacuously. 64 tests, clippy/fmt clean.
- Docs: spec §4 status, §16.7 implementation note, four new §17 decisions;
  testing.md generator policy + A13; AGENTS.md roadmap corrected (see below).

**Decided** (details in `spec.md` §17, 2026-07-20)
- **Named-argument resolution is IR-invisible** — named and positional forms
  lower to structurally identical IR. Stated as property A13, not a comment.
- **The field registry is `lower`-internal and program-wide**: field names never
  reach the IR, and a `declare` may appear *after* the rule that uses it.
  Duplicate fields within a schema, and two disagreeing schemas for one
  predicate, are errors.
- **Named access needs a known schema**, so a *schema-less* import cannot be
  accessed by field name until §13 fact sources land (header inference happens
  at load time, which lowering cannot see). Temporary consequence, not a
  language rule — §16.7's fixture uses the explicit-schema import meanwhile.
- **Fact grounding is checked against the lowered head**, so named and
  positional facts behave identically and only the error wording differs.

**Corrected from the previous handoff**
- The 2026-07-19 entry said "lowering already collects declare/import schemas
  into its registry". It did not — `collect_predicates` read `fields.len()` for
  arity and discarded the names; there was no registry. Building it was part of
  this task.
- `AGENTS.md` folded named-arg lowering into roadmap step 1 and marked that step
  done, which is why the gap went unnoticed. Named-arg lowering is now its own
  step 3 and the later steps are renumbered.

**Follow-up the same session — field names retained in the IR**
- Design review of the above asked whether desugaring named args to positional
  is still right. Answer: the desugar is right, but it had been bundled with a
  second decision — *discarding* the field names — that was not.
- `ir::PredicateInfo` gains `fields: Option<Vec<String>>` (invariant:
  `Some(f)` implies `f.len() == arity`), populated by a new
  `Lowerer::attach_field_names` step after pass 1. Atoms stay positional;
  evaluation is untouched. The §16.7 IR fixture now carries `employee`'s eight
  import-schema names and `person`'s two declared ones, so the existing
  `lower(ast) == ir` contract test asserts it. 67 tests.
- Rationale: type inference (§4, the *next* roadmap step after negation) runs
  over the IR and must name the conflicting column; provenance (§11) should
  render wide relations in named form; §14 output likewise. The IR already
  retains predicate names, `var_names`, and spans purely for rendering — field
  names are the same category, so this follows precedent rather than weakening
  the surface/core split.
- Guard tested where it is actually observable: `lower()` returns `Err` on an
  arity clash and never yields the table, so that test drives
  `collect_predicates` + `attach_field_names` directly and confirms a
  mismatched schema is not attached. Mutation-checked — removing the guard
  fails the test.

**Follow-up — review fixes: predicate-table snapshot moved after pass 2, A14**
- Review of the above found a latent dangling-`PredId` bug adjacent to the
  seam it touched: `lower()` snapshotted `out.predicates` before the statement
  loop, but pass 2 can still intern — `pred_id()` interns a schema-less import
  never used in a clause at arity 0. A program whose only mention of a
  predicate is such an import produced an `ImportSpec` pointing past the end
  of the table. Pre-existing (the snapshot placement predates field-name
  retention), but the field-name work added a second must-happen-before-the-
  snapshot step, making the ordering more fragile. Fix: `out.predicates` is
  now taken from the lowerer *after* the loop; pass 2 never reads it.
  Regression test lowers a lone schema-less import and indexes the table at
  the import's `PredId`.
- **A14** added to the Phase A property suite: field names attach to exactly
  the predicates the program gives a schema, matching it in order — so
  `Some(f)` implies `f.len() == arity` across all generated safe programs,
  not just the §16.7 fixture. The generator already emitted `declare`s, so
  the property cost only the assertion.
- Both fixes mutation-checked: reverting the snapshot placement panics the
  regression test on the dangling index; deleting the `attach_field_names`
  call fails A14. 69 tests.

**Next up**
- **§7 stratified negation** (now roadmap step 4). Pre-decisions already in spec
  §17 (2026-07-19): derivations record premises as a `BodyIdx`-aligned
  `Premise::Fact | Premise::Absent(pattern)` enum, and wildcards inside negated
  atoms are existential under the negation. Work: draft §7 (references.md group
  3 — Apt/Blair/Walker perfect model), real stratification in lowering
  (dependency graph, reject negative cycles), `NegAtom` evaluation in the
  engine's per-stratum loop (`eval_stratum` shape is already there; negated
  atoms filter rather than join, and never take a delta view), the `Premise`
  change in `src/provenance.rs`, and Phase C properties C1–C3.
- **Carry forward**: §16.2 currently fails safety even ignoring the "negation
  not yet supported" error. `not parent(_, X)` lowers `_` to a fresh slot and
  `check_body_safety` demands every variable in a negated atom be positively
  bound — including that fresh one. The wildcards-are-existential decision fixes
  it, but `VarScope` must start tracking which slots are wildcard-fresh, and
  §10's range-restriction wording ("every variable in a negated atom") needs
  narrowing to *named* variables to match.
- Then **§8 builtins** (comparisons/arithmetic incl. the open `=` question) and
  type inference (§4, C4–C5). Provenance *surface* (`?why`, JSON,
  provenance-as-facts, E5) stays parked until step 5.

## 2026-07-19 — Core evaluator: semi-naive fixpoint, provenance, Phase B/E

**Done**
- Implemented roadmap step 2, the core evaluator, to the ratified §17 contract.
  `src/engine/mod.rs`: `Model` (per-predicate `BTreeSet` storage — canonical
  §14 order for free; all-derivations map; base-fact set; per-fact first-round
  stamps), `eval()` (structured not-yet-supported errors for imports /
  comparisons / negation, strata-coverage check, then a per-stratum semi-naive
  fixpoint: naive seed pass + Full/Delta/Old views per body position), and
  `Model::answer()` query projection (deduped, canonically sorted).
- `src/provenance.rs`: `Derivation` (rule + premises, `BodyIdx`-aligned) and
  `ProofTree` with `explain()` — picks the `Ord`-least well-founded derivation
  per fact, so proofs are finite even over cyclic support.
- `src/engine/naive.rs`: the permanent naive oracle (~70 lines, facts-only,
  shares nothing with the semi-naive join loop).
- Testing: `testgen.rs` gained eval-bounds generators (`arb_program_with_edb`,
  `arb_parent_edges`, `arb_extension_pair`) and IR-level metamorphic mutators;
  properties B1–B7 and E1–E4 implemented and green, plus example-based engine
  tests (§16.1 end-to-end incl. its query, a diamond program asserting two
  recorded derivations for one fact, structured-error and edge cases) and
  proof-tree tests. 50 tests, clippy/fmt clean.
- Spec: §6 (least-model semantics) and §15 (evaluation strategy) drafted; §11
  data-model half drafted (query surface still TBD); four new §17 decisions.
  testing.md Phase B/E updated; AGENTS.md roadmap step 2 marked done.

**Decided** (details in `spec.md` §17, 2026-07-19)
- **First-round stamping**: every fact carries the fixpoint round it first
  appeared in; proof extraction requires strictly-decreasing rounds, which
  guarantees finite proofs under all-derivations storage.
- **Naive oracle is facts-only**; provenance correctness is checked by replay
  (E3) with the oracle's own matcher, not by a second recording evaluator.
- **Phase E properties E1–E4 pulled forward to step 2** (E5 still waits on the
  provenance surface design).
- Evaluator properties run on tighter generator bounds than lowering
  properties (`testgen::eval_bounds`); B4's cross-program comparisons are
  keyed by predicate name since interning order can differ.

**Next up** (scope ratified end-of-session: named-args first, then negation)
- **Named-argument lowering** (pass 2 of `lower()`) — the last
  not-yet-implemented lowering path. Unblocked: the AST already carries
  `declare`/`Args::Named`, and lowering already collects declare/import
  schemas into its registry; implement resolution + the §16.7 fixture
  end-to-end (A-property generators gain named-arg forms).
- **§7 stratified negation** (roadmap step 3): draft §7 (references.md group
  3), real stratification in lowering (dependency graph, reject negative
  cycles with a structured error), `NegAtom` evaluation in the engine's
  per-stratum loop (the loop shape is already there), Phase C properties
  (C1–C3). Two pre-decisions recorded in spec §17 (2026-07-19): derivations
  record negated premises as a `BodyIdx`-aligned `Premise::Fact |
  Premise::Absent(pattern)` enum, and wildcards inside negated atoms are
  existential under the negation (safety constrains only *named* variables).
- **§8 builtins** (comparisons/arithmetic incl. the open `=` question), then
  type inference (§4, C4–C5).
- Provenance *surface* (`?why` form, JSON encoding, provenance-as-facts, E5)
  stays parked until step 5.

## 2026-07-19 — AST/IR contract, lowering pass, property-based test layer

**Done**
- Implemented roadmap step 1: `src/ast.rs` (surface AST — span-carrying, mirrors
  the §5 grammar exactly; `Args` enum makes positional/named mixing
  unrepresentable), `src/ir.rs` (core IR — positional, interned `PredId`s,
  per-rule `Var` slots + `var_names` side table, `F64` float totality,
  non-optional `strata`; provenance coordinate guarantees in the module header),
  and `src/lower.rs` (minimal lowering: interning + arity checks, wildcard
  elimination + variable numbering, safety/range restriction, trivial
  single-stratum stratify; named-arg resolution stubbed as a structured
  not-yet-implemented error). Spec §16.1 is encoded as matching AST and IR
  fixtures; the contract test `lower(ast_16_1) == ir_16_1` passes end-to-end.
- Added the property-based test layer: `proptest` dev-dependency,
  `src/testgen.rs` (safe-by-construction program generator built body-first,
  single-defect injection), and Phase A properties A1–A12 (F64/Value laws,
  lowering totality/determinism/density/safety-both-directions). All green:
  25 tests.
- New `datalog/testing.md` — single source of truth for the test strategy with
  the full phased property catalog (A done; B–E specified with checkboxes).
  `references.md` group 9 (queryFuzz, QuickCheck, Csmith). AGENTS.md roadmap
  and test-pyramid updated; spec §10 gains the range-restriction draft; §4/§5
  marked prototype-validated.

**Decided** (details in `spec.md` §17, 2026-07-19 entries)
- **Surface AST / core IR split**: two plain Rust type hierarchies + a lowering
  pass, per compiler literature and Datalog-engine precedent (Soufflé AST→RAM,
  rustc AST→HIR); no phase-parameterized trees, no filled-in-later fields.
- Variable slots + name side table; float totality (no NaN, `-0.0` normalized,
  `total_cmp`); canonical value order symbol < string < int < float < bool;
  u32 byte-offset spans; symbol interning deferred.
- **PBT adopted**; proptest is the first dev-dependency; generators are coverage
  machinery, distinct from the still-banned ergonomic test DSLs; commit
  `proptest-regressions/` when they appear.
- **Naive reference evaluator ratified as a permanent differential oracle**
  (`naive(p) == seminaive(p)`, testing.md B1) — to be written with the evaluator.
- Handoff pre-decisions for the evaluator session (details in spec §17):
  **evaluator API** is `eval(&ir::Program) -> Result<Model, Error>` — `Model`
  holds all derived facts per predicate (set storage), queries answered as
  projections over it; **provenance records all derivations per fact**,
  deduped by rule instance (not first-witness-only); **comparison literals are
  a structured not-yet-supported error** in step 2 (§8 incl. the `=` question
  stays open). Also: imports at eval time error until sources land;
  `ir::Program.facts` duplicates collapse at engine load.

**Next up**
- **Core evaluator** (roadmap step 2) over `ir::Program`: semi-naive
  facts/rules/recursion implementing the ratified contract —
  `eval(&ir::Program) -> Result<Model, Error>`, all-derivations provenance in
  the fixpoint, query answering by projection. `ir::fixtures::example_16_1` is
  the first engine test (assert its query's expected answers); write the naive
  oracle alongside and implement testing.md Phase B properties (B1–B7, incl.
  `arb_edb` generators). Draft spec §6/§15 alongside (references.md groups 1–2).
- Provenance types (`Derivation`/`ProofTree`) against `ir::RuleId`/`BodyIdx`.
- Then: named-argument lowering (pass 2 of `lower()`), §7 negation with real
  stratification, §8 builtins (open `=` question).

## 2026-07-10 — References & implementation roadmap

**Done**
- Added `datalog/references.md`: annotated bibliography of the Datalog literature,
  grouped by topic (surveys, evaluation, negation, aggregation, provenance,
  implementations, language design, LLM+logic), each group cross-referenced to the
  spec section it informs. Linked from `spec.md`, `README.md`, and `AGENTS.md`.
- Recorded the implementation roadmap and testing conventions (AGENTS.md; decisions
  in spec §17).

**Decided**
- Papers are cited by title/authors/venue/year (stable, searchable); URLs only
  where long-lived. Consult the relevant group before drafting/implementing a spec
  section.
- **Implementation proceeds bottom-up, evaluation-first**: AST design → core
  evaluator (facts/rules/recursion, provenance hooks from the start) → stratified
  negation → builtins + type inference → lexer/parser → CLI/agent API. Rationale:
  the risky, novel design lives in the engine; the evaluator's natural interface is
  the AST, so semantics are unit-testable without a parser. (spec §17)
- **The AST is a designed contract; the engine core is positional-only** — named
  arguments and partial selection desugar to positional form during front-end
  lowering, using the predicate schema. (spec §17)
- **Test pyramid grows outward with the pipeline**: engine unit tests over
  hand-constructed ASTs (verbose construction is fine — agents write the tests; no
  macro-DSL infrastructure) → parser golden tests (text → AST, structured errors) →
  integration (text → results) → system tests running the binary over program
  files. The spec §16 worked examples are the canonical corpus at every level.
- **Set semantics**: relations are sets; duplicates collapse, including at import.
  Multiplicity-sensitive queries import the key column. (spec §17)
- **Licensing deferred** — leaning restrictive (AGPL) or no OSS license for now to
  preserve control/options. Removed the guessed `license`/`repository` metadata
  from `Cargo.toml`; set both before any publish. DuckDB added to the candidate
  import backends (§13/§17).
- **Agent interface: CLI-first, skill-driven, Datalog-in/Datalog-out** (spec §14
  now Draft; pillar 3 rewritten everywhere). Query results emit as ground facts,
  deterministically ordered — output is valid input, so runs compose over pipes
  (the jq pattern, Datalog-native). One-shot `-q` flag takes a bare atom or a
  define-and-select rule. Motivation: token economy — agents issue narrow queries
  over large fact bases instead of loading raw data into context. JSON reserved
  for errors (§12) and provenance (§11).

**Next up**
- **Implementation can begin**: design `src/ast.rs` against spec §3–§5, then the
  core evaluator (semi-naive facts/rules/recursion) with example 16.1 as the first
  engine test. Draft spec §6/§15 alongside (start from references.md groups 1–2).
- Then §7 negation, §8 builtins (open `=` question), aggregate-syntax revisit (§9).
- Later: §11/§14 (provenance surface, query-result / JSON shapes).

## 2026-07-03 — Core surface syntax ratified

**Done**
- Drafted spec §3 (lexical structure), §4 (data model & types), §5 (EBNF grammar),
  §13 (imports). Updated §16 examples to the ratified syntax, rewrote 16.5 to the
  new `import` form, added 16.7 (named arguments & partial selection). Recorded
  eight new decisions in §17 and pruned the resolved open questions.

**Decided** (details + rationale in `datalog/spec.md` §17)
- Strict Prolog casing: lowercase relations/symbols/fields, Capitalized variables.
- Named arguments alongside positional (`rel(field: X)`, `:` delimiter); a literal
  is all-positional or all-named; partial selection on named literals; requires
  known field names (import header or `declare`).
- **Static typing with full inference** — no annotations required; type errors
  flagged before evaluation (not dynamic typing).
- Optional `declare` statement (keyword, not `.decl`) for field naming and asserted
  signatures.
- Import syntax: `import "<path>" as <relation>.` with inferred schema + optional
  explicit override; CSV first.
- Flat terms in v1; single- or double-quoted strings; symbols ≠ strings.

**Next up**
- §6–§8: declarative semantics, stratified negation, arithmetic/comparison builtins
  (including the open `=` question and int/float division details).
- Revisit aggregate syntax — `count { Var : Goal }` collides with the named-arg `:`
  (§9).
- Then §11/§14: provenance query form and query-result / JSON shapes.

## 2026-07-03 — Bootstrap

**Done**
- Installed Rust toolchain (rustup; `cargo 1.96.1`, edition 2024).
- Scaffolded `datalog/` as a self-contained crate (library + thin CLI binary) with a
  module skeleton (`ast, lexer, parser, engine, provenance, sources, api, error`).
  Builds warning-free; unit + integration smoke tests pass; clippy/rustfmt clean.
- Wrote `datalog/spec.md` — living spec: full-featured v1 outline, spec-driven
  process, decisions/open-questions log (§17), and six provisional worked examples
  (§16: recursion, negation, arithmetic, aggregation, import, provenance).
- Added `AGENTS.md` (repo-wide guidance) and this worklog.
- Initial commit `18d68a9`.

**Decided**
- Language scope for v1: **full-featured** (facts, rules, recursion, stratified
  negation, arithmetic/comparison builtins, aggregation).
- LLM-targeting pillars: provenance/explainability, LLM-friendly syntax + structured
  errors, programmatic (JSON) agent API.
- `datalog/` is self-contained (no root Cargo workspace); library + thin binary.
- Session continuity: curated worklog (this file) + Claude Code's auto-saved
  transcripts; no raw-transcript directory in-repo.

**Next up**
- Formalize `spec.md` §3–§5 (lexical structure → data model → EBNF grammar) against
  the §16 examples.
- Ratify the **type-discipline** open question first (untyped terms vs declared
  predicate schemas) — it ripples into imports (§13), errors (§12), and the grammar.
- Then resolve the related open questions in §17 (meaning of `=`, aggregate syntax,
  import declaration syntax, provenance query syntax).
