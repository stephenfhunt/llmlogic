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
