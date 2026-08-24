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

A **cell** is one `(task, arm, strength)`:

- **arm** — `engine` or `prose`. Both are the same agent, same tools, same
  workspace, same files. The engine arm additionally has the `datalog` binary and
  its skill; the prose arm has neither and is free to write a Python script
  instead. That is the honest comparison: an agent's real alternative to the
  engine is not prose, it is ad-hoc code.
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

## Running it

```sh
python3 -m venv .venv && . .venv/bin/activate
pip install -e '.[dev]'
harness corpus fetch                           # the pinned source corpus, once

harness run --dry-run --all                    # full grid, stub subject, no API calls
harness run --domain access_control --smoke    # one real cell, both arms, both strengths
harness run --domain controls --strength haiku-4.5   # a cheaper slice
harness report results/<run-id>                # render to markdown

harness reference                              # the pinned corpus still answers the same
harness blocks                                 # doc blocks an ablation can cut
harness run --ablate count-wildcard --domain static_analysis   # cut one, engine arm only
```

A full grid is 112 cells and **$35–40** — cheap enough to re-run whenever
the skill's documentation changes, which is the point of building it rather than
eyeballing it.

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
