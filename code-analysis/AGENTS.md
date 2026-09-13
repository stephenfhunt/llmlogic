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
| `skill/reference/` | the skill's own references: TypeScript, Python, the Datalog language, extracting another language. `datalog.md` and `bring-your-own.md` began as the datalog skill's guide and recipe and are maintained separately for this skill's reader |
| `tools/code-facts/` | the extractor: TypeScript source run by Node, a Python frontend, the rule library (`lib/`), tests, `bench/` |
| `package.sh` | builds the standalone bundle into `dist/` |
| `decisions.md` | append-only decisions log (the amendment vocabulary is `../datalog/spec.md` §17's) |
| `ROADMAP.md` | the item index |
| `testing.md` | the property catalog |
| `notes/` | overflow: `notes/code-facts.md` is the extractor's design |

## The skill's texts are published

Everything the skill ships — `SKILL.md`, `reference/`, the headers of `lib/*.dl`,
and what `code-facts` writes into an output directory or prints — is read by an
agent in someone else's project, with its own setup and conventions and none of
this repository's history. Write it for that reader: state a case, never its
provenance (no defect ids, dates, subject codebases, session narrative or paths
into this repo — those live in `decisions.md`, `bugs/` and `notes/`).
`test/published-text.test.ts` catches the common leaks; it cannot catch prose
that only makes sense with the background, so read a change as a stranger would.
Why: [`decisions.md`](decisions.md) 2026-09-13 (night ii).

## Build / test / run

```sh
cd tools/code-facts
npm ci                          # typescript + fast-check (dev)
npm run typecheck               # tsc over src/ and test/
npm test                        # fixtures, properties, checks.dl on tsdl if present
CODE_FACTS_RUNS=500 npm test    # more cases per property
npm run bench -- --facts <dir>  # time every library over an extracted fact
                                # directory, and digest its answers
```

**A change to `lib/` is a performance change until the bench says otherwise.**
`bench/` prints wall clock, peak RSS and a sha256 of each library's answers to
its own documented relations; the digest is what separates a speed-up from a
different answer. Extract a fact directory outside the checkout first
(`./skill/code-facts <tsconfig> -o ~/.cache/code-facts-bench/<name> --no-git`).
The numbers to beat are in `notes/code-facts.md` § Calibration (tsdl) and § At a
million facts (VS Code's `vs/base`).

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
