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
harness corpus fetch     # `static_analysis`'s corpus; needs the network, a cell does not
```

Auth: run `ant auth status` before assuming an API key is needed — a zero-arg
client picks up an active profile. Do not ask for a key that is already there.

## Build / test / run (from `experiments/`)

```sh
pytest                          # harness units + property tests
harness run --dry-run --all     # the full grid, stub subject, zero API calls
harness run --domain access_control --smoke   # one real cell, both arms
harness calibrate --dry-run     # the selection pass, offline, against the stub
harness calibrate --local-model qwen3:14b --protocol structured --yes  # a pass per subject
harness run --local-model qwen3:8b --protocol structured --yes  # a local sweep
harness run --slate slates/<run-id>.json      # the grid over a calibrated slate
harness run --resume results/<run-id>  # finish a run the session window cut off
harness report results/<run-id> # render a run to markdown
harness domains                 # the slate, and why a pack is unavailable
harness reference               # the pinned reference corpus; `--repin` to adopt a diff
harness blocks                  # the doc blocks `run --ablate` can cut
ruff check . && ruff format .   # lints + formatting — keep clean
```

**A full grid is run over several sessions, not one sitting.** The subject
authenticates through the Claude Code subscription, so the binding budget is the
account's five-hour window — shared with the session driving the run — and not
money. `run_grid` halts on a session or rate limit instead of recording phantom
cells; `--resume` re-runs what a run is missing or failed, appending to the same
directory; `--limit N` sizes a sitting to the window. See `decisions.md`
2026-08-24.

**A local run needs its server started deliberately.** ollama serves at 4,096
tokens unless told otherwise, and that silently invalidated a whole sweep:

```sh
LLAMA_ARG_FIT_TARGET=288 OLLAMA_CONTEXT_LENGTH=24576 OLLAMA_FLASH_ATTENTION=1 \
  OLLAMA_KV_CACHE_TYPE=q8_0 OLLAMA_KEEP_ALIVE=30m ~/.local/ollama/bin/ollama serve
harness run --local-model qwen3:14b --min-context 24576 --protocol structured \
  --max-output-tokens 4096 --max-cell-seconds 900 --yes
```

`--min-context` sets both what preflight holds the server to *and* what the
strengths claim, so the overflow guard and the server cannot disagree. What fits
on this card is measured in `notes/a-local-subject.md`. `calibrate` takes the same
flags, at **one** model and one protocol: a slate is calibrated for one subject.

**24k leaves 0.86 GiB spare, which is less than the desktop holds** — it assumes
nothing new is launched during the run. `LLAMA_ARG_FIT_TARGET=640` with a 20,480
window is the setting that tolerates a browser.

The pass this configuration exists for — 240 cells, **~7.5h**, one sitting.
Calibration draws the `prose` arm only, which is why it is not the ~17h the
engine-forced rate implies. Dry-run verified to record the whole subject
(`reasoning_effort: null`, 24576, 4096), so a `--resume` rebuilds it:

```sh
harness calibrate --domain access_control --domain ontology --domain imports \
  --domain eligibility --seed 20260826 --seed 20260901 \
  --difficulty 1 --difficulty 2 --local-model qwen3:14b --protocol structured \
  --min-context 24576 --max-output-tokens 4096 --max-cell-seconds 900 --yes
```

**Thinking is on** — no `--reasoning-effort` flag. Two bounds move with it, and
both were measured rather than guessed (`decisions.md` 2026-08-27):
`--max-output-tokens 4096`, because the cap bounds **reasoning plus answer** and a
thinking model that runs out mid-thought emits no parseable action; and
`--max-cell-seconds 900`, because thinking costs ~7.5× wall clock and 240s would
cap a cell that was going to succeed.

**A local run resumes like any other**, rebuilding its strengths from the `local`
block its `run.json` recorded — including the served window, without which it
refuses rather than guessing what the overflow guard bounded the first half by.

**A local run is bounded by wall clock, not by an account window.** It is the arm
that can be left running overnight: `--local-model` crossed with `--protocol`,
`--max-cell-seconds` per cell, and a preflight that refuses a server whose context
is smaller than the run assumes. Needs a model server — see
`notes/a-local-subject.md`.

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
| `hypotheses.md` | **append-only** | the pre-registration: what a grid measures, and the one number that decides it |
| `notes/` | long-form | this project's overflow, per the length caps |
| `results/` | **append-only** | what the instrument produced. Never edit a past run |
| `slates/` | current-state | the calibrated slates a grid runs — one manifest per pass |

`results/` is a record, not a workspace: a run that was made is a run that was
made, and re-rendering a report is fine while rewriting a verdict is not.
