# AGENTS.md — `datalog/`

Guidance for the `datalog` project. Repo-wide guidance (session protocol, git
workflow, the doc-editing discipline) is in [`../AGENTS.md`](../AGENTS.md) and
[`../docs/rules/editing-docs.md`](../docs/rules/editing-docs.md); this file
covers only what is specific to this project.

A Rust library (the engine) plus a thin one-shot CLI binary. Everything for the
project lives inside `datalog/` — no root Cargo workspace.

**Design pillars** (they drive decisions): provenance/explainability,
LLM-friendly syntax + structured/actionable errors, and an agent-native CLI —
Datalog in, Datalog out (results are facts; output composes as input), with JSON
at the machine-readable edges (errors, provenance).

## Environment

Rust is installed via rustup. If `cargo` is not on `PATH` in a fresh shell,
source the env first:
```sh
source "$HOME/.cargo/env"
```

## Build / test / run (from `datalog/`)
```sh
cargo build          # compile
cargo test           # unit + integration tests
cargo run -- p.dl -q 'q(X)'  # run a program (no REPL: one shot, then exit)
cargo clippy --all-targets   # lints — keep clean, no warnings
cargo fmt            # apply formatting (run before committing)
```
Keep the tree warning-free and rustfmt-clean.

Don't introduce dependencies casually — dependency choices are tracked as
decisions in `spec.md` §17 and made as the relevant section stabilizes. The core
language engine (lexer/parser/lowering/eval) stays zero-dependency; §13 imports
are powered by DuckDB (a default-on cargo feature; `--no-default-features`
builds without it).

Match the style of surrounding code; keep modules documented with `//!` headers.

## The document map

Which discipline each document follows — the repo-wide rule for *how* to edit
each kind is in [`../docs/rules/editing-docs.md`](../docs/rules/editing-docs.md).

| document | kind | holds |
|---|---|---|
| `spec.md` §1–§16 | current-state | the language as it is now |
| `spec.md` §17 | **append-only** | decisions + rationale, and open questions |
| `ROADMAP.md` | current-state | the item index: what's open, its state, a pointer |
| `testing.md` | current-state | test strategy + the phased property catalog |
| `references.md` | current-state | annotated bibliography, grouped by topic |
| `bugs/[0-9]*.md` | current-state | exactly the open defect set (location is status) |
| `bugs/resolved/` | **append-only** | defects with their resolution notes |
| `notes/` | — | **the overflow target**: long-form design that would burst a §17 entry or a ROADMAP item |

`notes/semiring-provenance.md` is the pattern working: a nine-line §17 decision
pointing at ~185 lines of parked research.

Implementation proceeds **bottom-up, evaluation-first** (decided 2026-07-10;
rationale in §17). Consult `references.md`'s relevant group before designing a
feature — evaluation, negation, aggregation and provenance all have
well-established solutions in that literature.

## Working style: spec-driven

`spec.md` is a **living specification and the design workspace**. Before
implementing a language feature:
1. Read the relevant `spec.md` section and its status.
2. Prefer working from **canonical example programs** (§16) — let examples drive
   syntax/semantics rather than designing in the abstract. They are also the
   canonical test corpus at every level of the pyramid.
3. Record non-obvious design choices in the **decisions log** (§17) with a date
   and rationale; track unresolved questions there too. When a decision's
   correctness rests on an invariant, **name the test that fails if the invariant
   does**. `bugs/001` is the cost of one without a guard: its §17 entry argued the
   invariant in prose at the check site, and a feature two days later falsified it
   with nothing to notice.
4. Validate risky sections (grammar, negation, provenance) with small prototypes
   and feed findings back into the spec.
5. Follow **`testing.md`'s four rules** — an equivalence claim ships as a
   property; every generator carries a non-vacuity guard; mutation-verify a
   property and record the mutation; widening a generator needs a matching
   acceptance property. Each was paid for by a `bugs/` file, and `testing.md` is
   their normative home along with the two corollaries about oracle construction.

Testing conventions and the property catalog live in `testing.md`, their single
normative home — consult it and implement the relevant phase as each layer lands.

## Using `datalog` as an agent skill

`skill/` is a committed Claude Code skill (`SKILL.md` + a `datalog` wrapper that
builds the release binary on first use). Build a standalone bundle with `cargo
package-skill`. Try-it tasks are in [`EXPERIMENTS.md`](EXPERIMENTS.md).

**Activation is deliberate, and `.claude/skills/` is gitignored so it stays that
way.** A project's skill is a deliverable, not development infrastructure —
building the engine needs `cargo`, not a logic engine in context — and a skill's
description loads in *every* session, so auto-activating one per project does not
scale as this repo grows. Turn it on when running experiments, from the repo root:

```sh
mkdir -p .claude/skills && ln -s ../../datalog/skill .claude/skills/datalog
```

The truer test of the skill is `cargo package-skill` installed into an unrelated
repo, where an agent with no knowledge of this project either reaches for it or
does not.

**Named blocks.** A paragraph of the skill may be wrapped in
`<!-- block: some-name -->` … `<!-- /block -->`. The name makes that paragraph
addressable so `experiments/` can **cut it from one cell's workspace and re-run**
— which is how a doc line is shown to carry weight rather than asserted to
(`experiments/src/harness/ablate.py`; `harness blocks` lists them).

The markers are structure for tooling, never content: **every consumer strips
them**, so no subject and no bundle ever sees one. `cargo package-skill` strips
them on the way into the bundle and the harness strips them on the way into a
workspace. Adding or renaming a block is therefore free here and only ever
changes what an ablation can name — but *deleting* one silently un-measures it,
so delete the marker only along with the paragraph.

### `ts-facts`, the skill's TypeScript extractor

`skill/tools/ts-facts` (run as `skill/ts-facts`) is a Node program inside the
skill: TypeScript source run by Node's type stripping (**Node ≥ 22.18**), on its
own pinned TypeScript 6.0. Design and rationale: [`notes/ts-facts.md`](notes/ts-facts.md);
agent guide: `skill/recipes/typescript.md`.

```sh
cd skill/tools/ts-facts
npm ci                     # typescript + fast-check (dev)
npm run typecheck          # tsc over src/ and test/
npm test                   # node:test; fixtures, properties P1–P8, checks.dl on tsdl if present
TS_FACTS_RUNS=500 npm test # more cases per property
```

Tests find the engine at `DATALOG_BIN` or `../../../target/release/datalog` —
build it first (`cargo build --release --offline`). **`src/schema.ts` is the one
home of the fact schema**: change a relation there and `schema/*.dl` and
`SCHEMA.md` follow. A new layer or relation is also a new half of
`lib/checks.dl`. Its property catalog is in `testing.md`, *ts-facts*.
