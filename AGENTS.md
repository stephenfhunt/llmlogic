# AGENTS.md

Guidance for AI agents working in the `llmlogic` repository.

## What this repo is

`llmlogic` hosts experiments in LLM agents that use **formal logic engines** for
better reasoning. It is a multi-project repo: each project lives in its own
top-level directory and is self-contained (its own build system, tests, and docs).

**Start here each session:** read [`docs/worklog.md`](docs/worklog.md) — the most
recent entry's *Next up* tells you where the last session left off. End your session
by adding a new worklog entry (Done / Decided / Next up). Raw session transcripts are
auto-saved by Claude Code under `~/.claude/projects/<repo-slug>/*.jsonl` — don't
commit transcripts into the repo.

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
1. Read the relevant `spec.md` section and its status (`TBD → Draft → Stable`).
2. Prefer working from **canonical example programs** (spec §16) — let examples drive
   syntax/semantics rather than designing in the abstract.
3. Record non-obvious design choices in the **decisions log** (spec §17) with a date
   and rationale; track unresolved questions there too.
4. Validate risky sections (grammar, negation, provenance) with small prototypes and
   feed findings back into the spec before marking a section *Stable*.

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
6. **CLI/REPL + agent API** (§14): `-q` one-shot flag, `--format json` for the
   machine-readable edges (§12 errors, §11 provenance), the agent skill
   definition. Extend the system-test harness landed in step 5.

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
- Commit only when asked. Use a branch off `main` if committing.
- Don't introduce dependencies casually — dependency choices are tracked as decisions
  in `spec.md` §17 and made as the relevant section stabilizes.
