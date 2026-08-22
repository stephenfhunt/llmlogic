# AGENTS.md — `experiments/`

Guidance for the `experiments` project. Repo-wide guidance (session protocol, git
workflow, the doc-editing discipline) is in [`../AGENTS.md`](../AGENTS.md) and
[`../docs/rules/editing-docs.md`](../docs/rules/editing-docs.md); this file covers
only what is specific to this project.

A Python harness that measures **whether an agent answers questions more
accurately with a logic engine than without one**. It is the instrument for
`datalog`'s success criterion **S1** (`../datalog/spec.md` §1), and the repo's own
validity question: `llmlogic` exists to test that hypothesis, and until this runs
the hypothesis is assumed rather than measured.

Everything for the project lives inside `experiments/` — its own `pyproject.toml`,
no shared build with the Rust crate next door.

## What it measures, and the controls that make that true

Each **cell** is one `(task, arm, strength)` triple. Both arms are the same agent
with the same tools in the same workspace; the **engine arm** additionally has the
`datalog` binary and skill, and the **prose arm** does not. That single difference
is the independent variable — everything else is held.

Four controls carry the validity of the whole run. Breaking any one silently
turns a measurement into a rehearsal:

1. **Ground truth never comes from the datalog engine.** Every domain ships a
   plain-Python `truth.py`. If the engine grades itself the engine arm is correct
   by construction. A test asserts no `truth.py` reaches for the binary.
2. **The prompt's relation catalogue is derived from the fixture's own schemas**,
   so a renamed field cannot quietly change what the experiment measures.
3. **The subject is never told to use datalog.** The engine arm must reach for it
   unprompted — that is what makes *"reached for it?"* a measurement.
4. **The first program is recorded before any feedback.** That one reads the
   skill; every later one reads the diagnostics.

And one finding this project must not lose: our subject has `grep`. The most
valuable thing the old checklist produced was the three questions the model
answered with `grep` because the engine could not express them — **a skill that
cannot say something loses the question silently, and the model does not announce
the switch.** `signals.py` exists to catch that.

## Environment

Python 3.13, stdlib `venv` (`uv` is not installed on this machine; the
`pyproject.toml` is plain PEP 621 and works with either).

```sh
python3 -m venv .venv && . .venv/bin/activate
pip install -e '.[dev]'
```

Auth: run `ant auth status` before assuming an API key is needed — a zero-arg
client picks up an active profile. Do not ask for a key that is already there.

## Build / test / run (from `experiments/`)

```sh
pytest                          # harness units + property tests
harness run --dry-run --all     # the full grid, stub subject, zero API calls
harness run --domain access_control --smoke   # one real cell, both arms
harness report results/<run-id> # render a run to markdown
ruff check . && ruff format .   # lints + formatting — keep clean
```

**`--dry-run` is the CI gate.** The whole grid must execute offline against the
stub subject and render a report without one API call; a change that can only be
tested by spending money is a change that stops being tested.

Don't introduce dependencies casually — dependency choices are decisions, and go
in `decisions.md`.

## The document map

Which discipline each document follows — the repo-wide rule for *how* to edit each
kind is in [`../docs/rules/editing-docs.md`](../docs/rules/editing-docs.md).

| document | kind | holds |
|---|---|---|
| `README.md` | current-state | what the harness is and how to run it |
| `AGENTS.md` | current-state | how to work in this project (this file) |
| `ROADMAP.md` | current-state | the item index: one line per item, status, pointer |
| `decisions.md` | **append-only** | decisions + rationale, and open questions |
| `notes/` | long-form | this project's overflow, per the length caps |
| `results/` | **append-only** | what the instrument produced. Never edit a past run |

`results/` is a record, not a workspace: a run that was made is a run that was
made, and re-rendering a report is fine while rewriting a verdict is not.
