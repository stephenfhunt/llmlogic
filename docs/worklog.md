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
