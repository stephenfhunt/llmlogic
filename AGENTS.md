# AGENTS.md

Guidance for AI agents working in the `llmlogic` repository.

## What this repo is

`llmlogic` hosts experiments in LLM agents that use **formal logic engines** for
better reasoning. It is a multi-project repo: each project lives in its own
top-level directory and is **self-contained** — its own build system, tests, docs,
and its own `AGENTS.md` describing how to work in it.

| project | what it is |
|---|---|
| [`datalog/`](datalog/AGENTS.md) | a Datalog engine in Rust, targeted at LLM/agent use, with convenient import of fact tables from external sources. The first project. |
| [`experiments/`](experiments/AGENTS.md) | a Python harness that measures whether an agent answers more accurately **with** a logic engine than without one. The instrument for `datalog`'s S1, and the repo's own validity question. |

Future projects (other Rust crates, Python packages) get their own top-level
directories and their own `AGENTS.md`. There is **no root Cargo workspace** — do
not add one without being asked.

**This file stays repo-wide.** Anything true only of one project belongs in that
project's `AGENTS.md`, which loads when you read files in its directory. A new
project costs this file one row in the table above, not a section.

## Session protocol

**Start here each session:** read [`docs/worklog.md`](docs/worklog.md) — the most
recent entry's *Next up* tells you where the last session left off. Then, for the
project you're working in, its `AGENTS.md` and its `ROADMAP.md`, whose header
explains what each of that project's documents is for. Open defects are
`ls <project>/bugs/[0-9]*.md`.

**End your session** by updating any item whose status changed in the project's
`ROADMAP.md` and adding a worklog entry with four fields:

- **Done** / **Decided** / **Next up** — as before.
- **Removed** — what you deleted, merged, or replaced. Docs and code both accrete
  by default because every other field rewards adding; this one is the
  counterweight. "Nothing" is a fine answer once you have actually looked.

Then **annotate any decision this session taught you something about** in the
project's decisions log — the amendment vocabulary and the trigger are in that
log's preamble; the *why* is in
[`docs/rules/editing-docs.md`](docs/rules/editing-docs.md), which also sets the
length caps these records live under. Raw session transcripts are auto-saved by
Claude Code under `~/.claude/projects/<repo-slug>/*.jsonl` — don't commit
transcripts into the repo.

## Git workflow — trunk-based, solo

- **Commit only when asked**, and commit **directly on `trunk`**. There is no
  `main` and no feature branch: this is a single-developer repository with no PR
  or review gate, so a branch would only add ceremony. (Revisit if the project
  gains other contributors or a CI review flow — feature branches are a fine
  answer to a problem this repo does not have yet.)
- **Commit in small, self-contained steps as the work lands**, rather than
  accumulating a session's worth of change into one commit. Each commit should
  build, pass its project's tests, and be one coherent idea — a fix, a property, a
  refactor. A session that ships five things is usually five commits.
  Reconstructing that split afterwards is not a cheap edit: the changes end up
  interleaved within large files, so separating them means hunk-level surgery, and
  the intermediate states are no longer the ones that were actually verified.
- A commit message should say *what changed*, and point at the project's decisions
  log for *why*.

## Claude Code setup

`.claude/` holds per-checkout state and is mostly gitignored; the exceptions are
tracked, so a fresh clone loads its guidance with no setup:

- `CLAUDE.md → AGENTS.md` (this file) and each project's `CLAUDE.md → AGENTS.md`
  — Claude Code reads `CLAUDE.md`, not `AGENTS.md`. Nested ones load on demand
  when you read files in that directory.
- `.claude/rules/` — path-scoped rules, symlinked from `docs/rules/`.
- `.claude/settings.json` — shared permissions. `settings.local.json` stays local.

Skills are *not* activated by default; see the project's `AGENTS.md` for why and
how. Confirm what actually loaded with `/context` under **Memory files**.
