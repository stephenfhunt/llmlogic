# run-20260831T014254Z

88 cells · 6.92 USD · 0 errored

**Resumed**: 1 cell were run a second time after the first attempt was cut short, and are counted once, at their later attempt. The cost above is everything the run spent, including the attempts that produced nothing. A grid measured across more than one session window is still one grid, but it was not one sitting.

### S1 — the measured slate (in-context)

| | haiku-4.5 | all | mean F1 |
|---|---|---|---|
| **prose** | 40/40 (100%) | 40/40 (100%) [91%, 100%] | 1.00 |
| **engine-briefed** | 40/40 (100%) | 40/40 (100%) [91%, 100%] | 1.00 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-briefed − prose | and with the engine's manual in hand? | +0 pts | [+0, +0] | 0/0 | 1.000 | 40 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

### By difficulty — the ladder

| rung | items | prose | engine-briefed | engine-briefed − prose | 95% CI | wins/losses | median wall | median USD |
|---|---|---|---|---|---|---|---|---|
| **d1** | 8 | 8/8 (100%) [68%, 100%] | 8/8 (100%) [68%, 100%] | +0 pts | [+0, +0] | 0/0 | 38s / 43s | 0.04 / 0.05 |
| **d2** | 8 | 8/8 (100%) [68%, 100%] | 8/8 (100%) [68%, 100%] | +0 pts | [+0, +0] | 0/0 | 36s / 47s | 0.07 / 0.09 |
| **d3** | 8 | 8/8 (100%) [68%, 100%] | 8/8 (100%) [68%, 100%] | +0 pts | [+0, +0] | 0/0 | 46s / 50s | 0.08 / 0.09 |
| **d4** | 8 | 8/8 (100%) [68%, 100%] | 8/8 (100%) [68%, 100%] | +0 pts | [+0, +0] | 0/0 | 52s / 45s | 0.08 / 0.09 |
| **d5** | 8 | 8/8 (100%) [68%, 100%] | 8/8 (100%) [68%, 100%] | +0 pts | [+0, +0] | 0/0 | 35s / 61s | 0.07 / 0.12 |

**No single rung is testable here.** A rung holds a handful of paired items, so every interval in this table spans zero and then some; what a ladder can say is the **ordering** across rungs, and even that is a direction to aim a powered pass at rather than a result. The rung difficulty is the generator's, and it moves fact-base size and structure **together** — so a turn in the column does not say which of the two caused it.

**Median wall and USD are per cell, in the column order above**, and they are the number that sizes the next grid. A rate measured on one rung is not a rate for the next one: that extrapolation is what made a 42-hour pass look like a 7.5-hour one.

### Negative controls — the engine is *not* expected to help here

| | haiku-4.5 | all | mean F1 |
|---|---|---|---|
| **prose** | 4/4 (100%) | 4/4 (100%) [51%, 100%] | 1.00 |
| **engine-briefed** | 4/4 (100%) | 4/4 (100%) [51%, 100%] | 1.00 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-briefed − prose | and with the engine's manual in hand? | +0 pts | [+0, +0] | 0/0 | 1.000 | 4 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose | engine-briefed |
|---|---|---|---|
| `access_control` | negation, recursion | 10/10 (100%) | 10/10 (100%) |
| `controls` | one-step, single-hop | 4/4 (100%) | 4/4 (100%) |
| `eligibility` | aggregation, negation | 10/10 (100%) | 10/10 (100%) |
| `imports` | negation, temporal | 10/10 (100%) | 10/10 (100%) |
| `ontology` | constraint, recursion | 10/10 (100%) | 10/10 (100%) |

### By question class

| class | prose | engine-briefed |
|---|---|---|
| aggregation | 5/5 (100%) | 5/5 (100%) |
| constraint | 5/5 (100%) | 5/5 (100%) |
| negation | 15/15 (100%) | 15/15 (100%) |
| recursion | 10/10 (100%) | 10/10 (100%) |
| temporal | 5/5 (100%) | 5/5 (100%) |

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none | unusable |
|---|---|---|---|---|---|
| engine-briefed | haiku-4.5 | 44/44 [92%, 100%] | 0 | 0 | 0 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on the mandated arms it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

**`unusable` cells are out of the mandated arms' denominator, not in their numerator.** Declaring the engine unusable is the mandate's own legal exit, so it is not evidence the mandate failed to take — but no program ran, so it carries nothing about whether the engine helps. It answers neither question and is counted in neither (`decisions.md` 2026-08-28). Read the column: a high one is a finding about the engine, and it shrinks the n every other number rests on.

### Process signals

- **Answers that would not parse:** none.
- **Cells that ended at their budget:** engine-briefed 0/44 (0%), prose 0/44 (0%). The arms do not share one budget (`cell.ARM_BUDGET`), so a difference between them is only readable while these are low — a capped cell measures the cap.
- **Declared the engine unusable:** 0. The subject tried, the engine would not run its program, and it said so instead of looping. Not a correct answer, but a different fact from an empty file — and evidence about the engine rather than a hole.
- **Completions cut off at the output cap:** engine-briefed 0 in 0/44 cells, prose 0 in 0/44 cells. The whole budget went to reasoning and no action came out, so the turn bought nothing and cost its full decode. `n/a` means the run predates the counter, not that it was zero.
- **First program captured before feedback:** 44/44.
- **Switched back to search after using the engine:** 10. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 0. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 1, across 1 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
