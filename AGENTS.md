# AGENTS.md

Guidance for AI agents working in the `llmlogic` repository.

## What this repo is

`llmlogic` hosts experiments in LLM agents that use **formal logic engines** for
better reasoning. It is a multi-project repo: each project lives in its own
top-level directory and is self-contained (its own build system, tests, and docs).

**Start here each session:** read [`docs/worklog.md`](docs/worklog.md) — the most
recent entry's *Next up* tells you where the last session left off — and, for a
project, its `ROADMAP.md` (e.g. [`datalog/ROADMAP.md`](datalog/ROADMAP.md)) for
the indexed backlog of open items. Open **defects** are tracked separately, one
file per defect, in the project's `bugs/` directory (`ls datalog/bugs/*.md` is the
open set; conventions in [`datalog/bugs/README.md`](datalog/bugs/README.md)).
ROADMAP holds what is *missing*; `bugs/` holds what is *wrong*.

**End your session** by updating any item whose status changed in `ROADMAP.md`
and adding a worklog entry with four fields:

- **Done** / **Decided** / **Next up** — as before.
- **Removed** — what you deleted, merged, or replaced. Docs and code both accrete
  by default because every other field rewards adding; this one is the
  counterweight. "Nothing" is a fine answer once you have actually looked.

Then **annotate any `spec.md` §17 decision this session taught you something
about** — see "Working style" below. Raw session transcripts are auto-saved by
Claude Code under `~/.claude/projects/<repo-slug>/*.jsonl` — don't commit
transcripts into the repo.

Current projects:
- **`datalog/`** — a Datalog engine in Rust, targeted at LLM/agent use, with
  convenient import of fact tables from external sources. This is the first project.

Future projects (other Rust crates, Python packages) will get their own top-level
directories. There is **no root Cargo workspace** — do not add one without being asked.

## Project: `datalog/`

A Rust library (the engine) plus a thin binary (CLI/REPL). Everything for this
project lives inside `datalog/`.

### Environment
Rust is installed via rustup. If `cargo` is not on `PATH` in a fresh shell, source
the env first:
```sh
source "$HOME/.cargo/env"
```

### Build / test / run (from `datalog/`)
```sh
cargo build          # compile
cargo test           # unit + integration tests
cargo run            # launch the (stub) CLI/REPL
cargo clippy --all-targets   # lints — keep clean, no warnings
cargo fmt            # apply formatting (run before committing)
```
The starter is warning-free and rustfmt-clean; keep it that way.

### Layout
- `src/lib.rs` — crate root and public API; the engine belongs here.
- `src/main.rs` — thin binary; delegates to the library.
- `src/{ast,lexer,parser,error,provenance,sources,api}.rs`, `src/engine/` — modules
  mirroring the intended architecture. Most are stubs pending the spec.
- `tests/` — integration tests.
- `spec.md` — see below.
- `references.md` — annotated bibliography of the Datalog literature, grouped by
  topic and mapped to spec sections. Consult the relevant group before designing or
  implementing a feature (evaluation, negation, aggregation, provenance all have
  well-established solutions in these papers).

## Working style: spec-driven

`datalog/spec.md` is a **living specification and the design workspace**. Before
implementing a language feature:
1. Read the relevant `spec.md` section and its status.
2. Prefer working from **canonical example programs** (spec §16) — let examples drive
   syntax/semantics rather than designing in the abstract.
3. Record non-obvious design choices in the **decisions log** (spec §17) with a date
   and rationale; track unresolved questions there too. When a decision's
   correctness rests on an invariant, **name the test that fails if the invariant
   does**. `bugs/001` is the cost of one without a guard: its §17 entry argued the
   invariant in prose at the check site, and a feature two days later falsified it
   with nothing to notice.
4. Validate risky sections (grammar, negation, provenance) with small prototypes and
   feed findings back into the spec.
5. **An equivalence claim ships as a property, not a unit test.** Any "these two
   spellings mean the same thing" claim — surface sugar, a desugaring, an
   IR-identity claim — gets a generated-input property. The record is exact: every
   such claim carrying a property has held (named ≡ positional, body order); both
   carrying only a unit test became defects (`bugs/001`, `bugs/002`).

### Changing what already exists

Most of the damage so far has come from editing, not building — three of the four
2026-07-25 defects were caused by a doc claim that had quietly stopped being true.

**Know which kind of document you are in.** They have opposite disciplines:

| | current-state | append-only record |
|---|---|---|
| what | `spec.md` §1–§16, `README.md`, `SKILL.md`, code and doc comments | `spec.md` §17, `docs/worklog.md`, `bugs/resolved/` |
| discipline | **rewrite** it to state present truth | **append**; never rewrite |
| history | *point* to the decision; never narrate the change | history is the payload |

The test for any sentence in a current-state document: *would this still be here
if the feature had always worked this way?* If not, it is narration — cut it and
leave the pointer. "See §17 2026-07-25" is fine; "relaxed from positively bound"
is not.

- **One normative home per rule.** State a rule in exactly one section; everywhere
  else cross-references it. `bugs/003` names this as *the drift mechanism* — the
  same safety rule lived in four sections, and updating three looked like done.
- **Changing a rule means sweeping §17** for entries resting on it. Fixing
  `bugs/001` turned up three needing amendment; that was diligence, not process.
- **Annotate a decision when its consequences land, not only when it is
  overturned.** The most valuable note has no change attached — that the
  2026-07-19 wildcard entry's instinct was right and the entry overturning it was
  wrong is something no diff can recover. Record what it actually cost, whether
  the stated rationale held, and what the *rejected* alternative would have done;
  the last is the part a later session cannot reconstruct.

Design pillars for `datalog` (they drive decisions): provenance/explainability,
LLM-friendly syntax + structured/actionable errors, and an agent-native CLI —
Datalog in, Datalog out (results are facts; output composes as input), with JSON
at the machine-readable edges (errors, provenance).

## Implementation roadmap & testing (`datalog/`)

Implementation proceeds **bottom-up, evaluation-first** (decided 2026-07-10;
rationale in spec §17):

1. **AST + IR** (`src/ast.rs`, `src/ir.rs`, `src/lower.rs`) designed against spec
   §3–§5 — done 2026-07-19. Two distinct plain type hierarchies (spec §17): the
   surface AST mirrors the grammar (spans, named args, wildcards); the core IR is
   **positional-only** and index-resolved. The evaluator consumes `ir::Program`
   only. (Named-argument resolution — lowering pass 2 — was stubbed here and
   landed 2026-07-20; see step 3.)
2. **Core evaluator** (`src/engine/`, `src/provenance.rs`) — done 2026-07-19.
   `eval(&ir::Program) -> Result<Model, Error>`: stratified semi-naive fixpoint
   recording **all derivations per fact** (deduped by rule instance) plus
   first-round stamps for finite proof extraction (`ProofTree::explain`);
   queries answered as projections (`Model::answer`). The naive reference
   evaluator (`src/engine/naive.rs`, test-only, permanent) is the differential
   oracle; testing.md Phase B (B1–B7) and Phase E (E1–E4, pulled forward) are
   green. Spec §6/§15 and the §11 data model are Draft.
3. **Named-argument lowering** (`src/lower.rs` pass 2) — done 2026-07-20.
   Named literals resolve against a `lower`-internal field registry collected
   from `declare` statements and explicit import schemas; omitted fields become
   fresh anonymous slots (partial selection) and named heads must supply every
   field (§4). Named and positional forms lower to identical IR (testing.md
   A13); §16.7 is the contract fixture.
4. **Stratified negation** (§7) — done 2026-07-20. Ullman relaxation numbering
   in lowering (`stratify`) with structured concrete-cycle errors; the engine
   evaluates negated atoms as anti-join filters over frozen lower strata and
   records `Premise::Absent` patterns for provenance (`ProofTree::Absent`
   leaves); the naive oracle iterates strata (perfect model). §16.2 is the
   contract fixture; testing.md C1–C3 are green. **Builtins + type inference**
   (§8/§4) remain in this step.
5. **Lexer + parser** (§3–§5), wired to the engine — done 2026-07-22.
   Hand-rolled zero-dep lexer (`src/lexer.rs`) + recursive-descent parser
   (`src/parser.rs`) producing the existing surface AST, with statement-level
   error recovery and did-you-mean messages for Prolog-prior near-misses; a
   canonical printer (`src/print.rs`) defining the §14 output form; and the
   first production pipeline `parse → lower → typecheck → eval` (`src/api.rs`,
   thin `src/main.rs`). Atom arguments widened `Term → Expr` for inline
   arithmetic (lowering hoists); disjunction `;` expands in the parser. The §16
   corpus is now source-text-first (golden AST fixtures); testing.md D1–D4 plus
   integration (`tests/pipeline.rs`) and system (`tests/system.rs`, compiled
   binary over `tests/programs/*.dl`) tests are green. A minimal binary contract
   (stdout/stderr, exit 0/1/2) was pulled forward here; the full agent CLI stays
   step 6.
6. **Agent CLI** (§14) — done 2026-07-23. One-shot `-q` queries: a bare atom /
   comma-body appends `?- <arg>.`, a `head :- body` rule appends the rule plus a
   synthesized `?- <head>.` (classified by *parsing* the arg, not splitting on
   `:-`). Logic lives in the library (`api::program_with_queries` /
   `run_with_queries`); `src/main.rs` grew a small hand-rolled arg loop (zero new
   deps, no clap); optional positional source (empty base when only `-q`). Agent
   guide: `docs/agent-skill.md`. **JSON output was deferred as low-value** — the
   data path is Datalog-native (`-q` over facts is the jq analog) and errors are
   already actionable prose; `--format json` stays a documented future edge only.
   Post-v1 threads (§9 aggregation, a first-class optional/absent value, §11
   provenance surface, §12 error taxonomy, §13 follow-ons, …) are tracked as a
   single indexed backlog in [`datalog/ROADMAP.md`](datalog/ROADMAP.md) — the
   *what's-open-and-what-state* view, with each item pointing to its §17 detail.

### Using `datalog` as an agent skill

`datalog/skill/` is a committed Claude Code skill (`SKILL.md` + a `datalog`
wrapper) — the first experiment in exposing the engine to an LLM agent
(2026-07-23; form + inline-facts-now decided with the user). Activate it in a
dev checkout by symlinking `datalog/skill` to `.claude/skills/datalog`; build a
standalone bundle with `cargo package-skill` (feature-gated build-tooling bin
`src/bin/package_skill.rs`, std-only, excluded from normal builds). Try-it tasks
are in `datalog/EXPERIMENTS.md`. A Claude API agent-loop harness and an MCP
server are possible later forms, deferred until the skill experiment tells us how
well the model uses the tool. The "big external fact base" demo waits on §13 CSV
imports.

The test pyramid grows outward with the pipeline:
- Engine unit tests over **hand-constructed IR** (`ir::Program`); lowering tests
  over **hand-constructed ASTs**. Verbose construction in tests is acceptable —
  do not build macro DSLs or builder frameworks for ergonomics.
- **Property-based tests** (proptest): generated programs checked against
  metamorphic relations and reference oracles. Strategy, generator policy, and
  the phased property catalog live in **`datalog/testing.md`** — consult it and
  implement the relevant phase's properties as each layer lands.
- Parser tests: source text → expected AST, plus golden tests for structured errors.
- Integration tests: source text → query results through the full pipeline.
- System tests: run the binary on program files and assert on output.
- The spec **§16 worked examples are the canonical test corpus** at every level:
  encoded as AST fixtures first, reused as source-text fixtures once the parser
  exists.

## Conventions
- Match the style of surrounding code; keep modules documented with `//!` headers.

### Git workflow — trunk-based, solo
- **Commit only when asked**, and commit **directly on `trunk`**. There is no
  `main` and no feature branch: this is a single-developer repository with no PR
  or review gate, so a branch would only add ceremony. (Revisit if the project
  gains other contributors or a CI review flow — feature branches are a fine
  answer to a problem this repo does not have yet.) The remote is `vault`.
- **Commit in small, self-contained steps as the work lands**, rather than
  accumulating a session's worth of change into one commit. Each commit should
  build, pass `cargo test`, and be one coherent idea — a fix, a property, a
  refactor. A session that ships five things is usually five commits.
  Reconstructing that split afterwards is not a cheap edit: the changes end up
  interleaved within `spec.md`, `lower.rs` and `engine/mod.rs`, so separating
  them means hunk-level surgery, and the intermediate states are no longer the
  ones that were actually verified.
- Design decisions still go to `spec.md` §17 and the worklog; a commit message
  should say *what changed*, and point at §17 for *why*.
- Don't introduce dependencies casually — dependency choices are tracked as decisions
  in `spec.md` §17 and made as the relevant section stabilizes. The core language
  engine (lexer/parser/lowering/eval) stays zero-dependency; §13 imports are powered
  by DuckDB (a default-on cargo feature; `--no-default-features` builds without it).
