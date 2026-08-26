# experiments

**Does an agent actually reason better with a logic engine?**

This is the instrument that answers that question for `llmlogic`. It runs the same
task twice — once by an agent that has the [`datalog`](../datalog/) engine, once by
the same agent without it — and compares what comes back against ground truth
computed independently of the engine.

It is `datalog`'s success criterion **S1** made falsifiable:

> An agent answering multi-hop, recursive or constraint questions is measurably
> more accurate **with** the engine than reasoning in prose — at two model
> strengths, first program recorded before any feedback.
> — [`../datalog/spec.md`](../datalog/spec.md) §1

A negative result is a finding. The harness is built so one can actually happen:
the slate carries deliberate **negative controls** — single-hop lookups over tiny
fact bases, one-step arithmetic — where the engine should not help. Without them a
null result is indistinguishable from a broken instrument.

## The unit of measurement

A **cell** is one `(task, arm, strength, trial)`:

- **arm** — `prose`, `engine`, or `engine-forced`. All three are the same agent,
  same tools, same workspace, same files. The two engine arms additionally have the
  `datalog` binary and its skill; `prose` has neither and is free to write a Python
  script instead. That is the honest comparison: an agent's real alternative to the
  engine is not prose, it is ad-hoc code.

  `engine` and `prose` get a **byte-identical** prompt, so `engine` measures
  *adoption and capability together* — would an agent pick this up, and does it
  help. `engine-forced` appends a block mandating a program, so it measures
  capability alone. Without both, a null is uninterpretable: an engine arm that
  never reached for the engine is not a test of the engine.
- **track** — `in-context`, where both arms can read the whole fact base and the
  difficulty comes from logical structure, or `at-scale`, where the fixture
  deliberately exceeds the prose arm's window. They answer different questions and
  are reported in separate tables, never averaged into one headline.
- **strength** — Claude Opus 5 and Claude Haiku 4.5. The weaker arm is the
  informative one: the strongest model routes around gaps instead of falling into
  them, so a guide only it can follow is a guide that fails in production.

Each cell records the transcript, the **first program written before any
feedback**, the answer, the cost, and a verdict — plus process signals the answer
alone does not carry: did it reach for the engine at all, did it silently fall
back to `grep`, how many rounds to correct, and whether a wrong answer *looked*
wrong.

## Domains

Seven, chosen to vary the question class and the origin of the fact base rather
than to sample topics.

| domain | question class | fact base |
|---|---|---|
| `access_control` | multi-hop, negation | generated policy graph |
| `ontology` | recursion, subsumption | generated class/instance graph |
| `imports` | aggregation, arithmetic, dates | CSV + JSONL, with a redundant Parquet copy |
| `eligibility` | thresholds, negation, *why not* | generated rule set + applicants |
| `scheduling` | constraint / search | generated roster |
| `static_analysis` | recursion, negation | facts the agent extracts from a pinned real package |
| `controls` | single-hop, one-step | tiny and closed — the negative controls |

Each pack ships **two** fixtures: the pinned one its four questions are written
against, and `generate(seed, difficulty, track)` for a fresh one. The 28 pinned
tasks are the comparable slate and are hashed in `tests/test_pinned_slate.py`;
the generated ones are the pool a calibration pass selects from. Difficulty is
*structure* — closure depth, how many columns can be blank, how much the day
overlaps itself — and the `at-scale` track exceeds the prose arm's window on
purpose. `static_analysis` has no generator: its fixture is a fetched real
package, so there is nothing to seed. Long form:
[`notes/generating-the-slate.md`](notes/generating-the-slate.md).

Which of the pool becomes the slate is **selected, not designed**. `harness
calibrate` runs one cheap pass — the prose arm at the weaker model, three trials
an item — and keeps what lands in the informative band: an item the subject
always gets right, or never does, cannot show whether the engine helped. What it
keeps is pinned to a manifest in [`slates/`](slates/README.md), and `harness run
--slate` regenerates from it and refuses an item that has moved.

## Running it

```sh
python3 -m venv .venv && . .venv/bin/activate
pip install -e '.[dev]'
harness corpus fetch                           # the pinned source corpus, once

harness run --dry-run --all                    # full grid, stub subject, no API calls
harness run --domain access_control --smoke    # one real cell, both arms, both strengths
harness run --domain controls --strength haiku-4.5   # a cheaper slice
harness run --arm prose --arm engine-forced --repeats 3   # named arms, three trials each
harness run --resume results/<run-id>          # finish a run the window cut off
harness report results/<run-id>                # render to markdown

harness power --effect 0.10                    # how many paired items would it take?
harness score --task access_control/who-can-read-r03 --program q.dl   # run and grade one

harness reference                              # the pinned corpus still answers the same
harness blocks                                 # doc blocks an ablation can cut
harness run --ablate count-wildcard --domain static_analysis   # cut one, engine arm only
```

A full grid is 168 cells and roughly **50 minutes of wall time** — cheap enough to
re-run whenever the skill's documentation changes, which is the point of building
it rather than eyeballing it. What it is *not* cheap in is the account's
five-hour session window, which is the real budget: a grid does not fit in one
window alongside the session driving it, so a run is finished over several
sittings with `--resume`, and `--limit N` sizes a sitting to the window. A run
halts rather than recording cells it could not measure.

## Two things beside the grid

**The reference corpus** ([`reference/`](reference/)) pins what the engine prints
for a known-correct program in each domain, and what it prints for a malformed
one. The harness measures an agent against an engine that moves under it, so two
runs are the same measurement only if the engine answered the same way in
between; nothing else here checks that.

**Ablation** cuts one named paragraph out of the engine arm's documentation and
re-runs those cells. Skill guidance accretes — every trap anyone hit becomes a
paragraph, and none is ever removed, because nobody can show a line does *not*
carry weight. This is how a line earns its place with a number.

## What this is not

Not a benchmark, and not a leaderboard. It measures one engine against its own
absence on questions that engine claims to be good at. It says nothing about how
`datalog` compares to another Datalog — that is
[`../datalog/notes/cross-engine-benchmark.md`](../datalog/notes/cross-engine-benchmark.md).

Open items and their status: [`ROADMAP.md`](ROADMAP.md). Why each thing is the way
it is: [`decisions.md`](decisions.md).
