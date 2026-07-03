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
LLM-friendly syntax + structured/actionable errors, and a programmatic (JSON) agent
API.

## Conventions
- Match the style of surrounding code; keep modules documented with `//!` headers.
- Commit only when asked. Use a branch off `main` if committing.
- Don't introduce dependencies casually — dependency choices are tracked as decisions
  in `spec.md` §17 and made as the relevant section stabilizes.
