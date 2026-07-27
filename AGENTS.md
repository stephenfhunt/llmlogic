# AGENTS.md

Guidance for AI agents working in the `llmlogic` repository.

## What this repo is

`llmlogic` hosts experiments in LLM agents that use **formal logic engines** for
better reasoning. It is a multi-project repo: each project lives in its own
top-level directory and is self-contained (its own build system, tests, and docs).

Current projects:
- **`datalog/`** — a Datalog engine in Rust, targeted at LLM/agent use, with
  convenient import of fact tables from external sources. This is the first project.

Future projects (other Rust crates, Python packages) will get their own top-level
directories. There is **no root Cargo workspace** — do not add one without being asked.

## Session protocol

**Start here each session:** read [`docs/worklog.md`](docs/worklog.md) — the most
recent entry's *Next up* tells you where the last session left off. Then, for a
project, [`datalog/ROADMAP.md`](datalog/ROADMAP.md), whose header explains what
each document in the project is for. Open defects are `ls datalog/bugs/[0-9]*.md`.

**End your session** by updating any item whose status changed in `ROADMAP.md`
and adding a worklog entry with four fields:

- **Done** / **Decided** / **Next up** — as before.
- **Removed** — what you deleted, merged, or replaced. Docs and code both accrete
  by default because every other field rewards adding; this one is the
  counterweight. "Nothing" is a fine answer once you have actually looked.

Then **annotate any `spec.md` §17 decision this session taught you something
about** — the vocabulary and the trigger are in §17's preamble; the *why* is in
[`docs/rules/editing-docs.md`](docs/rules/editing-docs.md). Raw session
transcripts are auto-saved by Claude Code under
`~/.claude/projects/<repo-slug>/*.jsonl` — don't commit transcripts into the repo.

## Project: `datalog/`

A Rust library (the engine) plus a thin binary (CLI/REPL). Everything for this
project lives inside `datalog/`.

Design pillars (they drive decisions): provenance/explainability, LLM-friendly
syntax + structured/actionable errors, and an agent-native CLI — Datalog in,
Datalog out (results are facts; output composes as input), with JSON at the
machine-readable edges (errors, provenance).

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
cargo run            # launch the CLI/REPL
cargo clippy --all-targets   # lints — keep clean, no warnings
cargo fmt            # apply formatting (run before committing)
```
Keep the tree warning-free and rustfmt-clean.

Implementation proceeds **bottom-up, evaluation-first** (decided 2026-07-10;
rationale in spec §17). Shipped milestones and the open backlog are in
[`ROADMAP.md`](datalog/ROADMAP.md); the test strategy and the phased property
catalog are in [`datalog/testing.md`](datalog/testing.md), which is their single
normative home — consult it and implement the relevant phase as each layer lands.

### Using `datalog` as an agent skill

`datalog/skill/` is a committed Claude Code skill (`SKILL.md` + a `datalog`
wrapper that builds the release binary on first use). Build a standalone bundle
with `cargo package-skill`. Try-it tasks are in
[`datalog/EXPERIMENTS.md`](datalog/EXPERIMENTS.md).

**Activation is deliberate, and `.claude/skills/` is gitignored so it stays that
way.** A project's skill is a deliverable, not development infrastructure —
building the engine needs `cargo`, not a logic engine in context — and a skill's
description loads in *every* session, so auto-activating one per project does not
scale as this repo grows. Turn it on when running experiments:

```sh
mkdir -p .claude/skills && ln -s ../../datalog/skill .claude/skills/datalog
```

The truer test of the skill is `cargo package-skill` installed into an unrelated
repo, where an agent with no knowledge of this project either reaches for it or
does not.

## Working style: spec-driven

`datalog/spec.md` is a **living specification and the design workspace**. Before
implementing a language feature:
1. Read the relevant `spec.md` section and its status.
2. Prefer working from **canonical example programs** (spec §16) — let examples drive
   syntax/semantics rather than designing in the abstract. They are also the
   canonical test corpus at every level of the pyramid.
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

**Editing what already exists** is where most of the damage has come from — three
of the four 2026-07-25 defects were a doc claim that had quietly stopped being
true. The discipline (current-state vs append-only documents, one normative home
per rule, sweeping §17 when a rule changes) lives in
[`docs/rules/editing-docs.md`](docs/rules/editing-docs.md). Read it before editing
`spec.md`, `README.md`, `SKILL.md`, the worklog, or a doc comment.

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
