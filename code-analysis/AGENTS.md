# AGENTS.md — `code-analysis/`

Guidance for the `code-analysis` project. Repo-wide guidance (session protocol,
git workflow, the doc-editing discipline) is in [`../AGENTS.md`](../AGENTS.md) and
[`../docs/rules/editing-docs.md`](../docs/rules/editing-docs.md); this file covers
only what is specific to this project.

A **Claude Code skill for analysing a codebase's design** with the `datalog`
engine: an extractor (`code-facts`) turns source and git history into fact
tables, a rule library derives the measures, and the skill's `SKILL.md` is a
playbook for exploring them. It is an *application* of the engine — the general
reasoning skill is `../datalog/skill/`, and is kept free of this domain so that
the experiments measuring it measure one thing. Why the split:
[`decisions.md`](decisions.md) 2026-09-11.

## Layout

| path | what |
|---|---|
| `skill/` | the skill: `SKILL.md` (the playbook), `./datalog` and `./code-facts` wrappers, `reference/` |
| `skill/reference/datalog.md` | **symlink** to `../datalog/skill/SKILL.md` — the language guide has one home |
| `skill/reference/bring-your-own.md` | **symlink** to `../datalog/skill/recipes/source-analysis.md` — the experiments' `static_analysis` pack ships that file, so it stays in the datalog skill |
| `tools/code-facts/` | the extractor: TypeScript source run by Node, a Python frontend, the rule library (`lib/`), tests |
| `package.sh` | builds the standalone bundle into `dist/` |
| `decisions.md` | append-only decisions log (the amendment vocabulary is `../datalog/spec.md` §17's) |
| `ROADMAP.md` | the item index |
| `testing.md` | the property catalog |
| `notes/` | overflow: `notes/code-facts.md` is the extractor's design |

## Build / test / run

```sh
cd tools/code-facts
npm ci                          # typescript + fast-check (dev)
npm run typecheck               # tsc over src/ and test/
npm test                        # fixtures, properties, checks.dl on tsdl if present
CODE_FACTS_RUNS=500 npm test    # more cases per property
```

Needs **Node ≥ 22.18** (the source is run by type stripping); the Python
frontend needs **Python ≥ 3.11**. Tests find the engine at `DATALOG_BIN` or
`../datalog/target/release/datalog` — build it first with
`cargo build --release --offline` in `../datalog`. **Gate every commit on
`npm test`**: a fixture file once matched the test glob and two commits went in
red because the commit did not wait for the result.

**`tools/code-facts/src/schema.ts` is the one home of the fact schema**, for
every language: change a relation there and `schema/*.dl` and `SCHEMA.md`
follow. A new layer or relation is also a new half of `lib/checks.dl`, and a new
row in `testing.md` if a property covers it.

## Activating the skill

As with the datalog skill, activation is deliberate (`.claude/skills/` is
gitignored). From the repo root:

```sh
mkdir -p .claude/skills && ln -s ../../code-analysis/skill .claude/skills/code-analysis
```

The truer test is the bundle: `./package.sh`, then drop `dist/code-analysis-skill`
into an unrelated repo's `.claude/skills/code-analysis`.
