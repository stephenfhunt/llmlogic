# run-20260831T212012Z

20 cells · 2.57 USD · 0 errored

### S1 — the measured slate (in-context)

| | haiku-4.5 | all | mean F1 |
|---|---|---|---|
| **prose** | 5/6 (83%) | 5/6 (83%) [44%, 97%] | 0.83 |
| **engine-briefed** | 6/6 (100%) | 6/6 (100%) [61%, 100%] | 1.00 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-briefed − prose | and with the engine's manual in hand? | +17 pts | [+0, +50] | 1/0 | 1.000 | 6 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

### By difficulty — the ladder

| rung | items | prose | engine-briefed | engine-briefed − prose | 95% CI | wins/losses | median wall | median USD |
|---|---|---|---|---|---|---|---|---|
| **d3** | 3 | 2/3 (67%) [21%, 94%] | 3/3 (100%) [44%, 100%] | +33 pts | [+0, +100] | 1/0 | 123s / 134s | 0.16 / 0.19 |
| **d5** | 3 | 3/3 (100%) [44%, 100%] | 3/3 (100%) [44%, 100%] | +0 pts | [+0, +0] | 0/0 | 205s / 72s | 0.21 / 0.11 |

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
| `controls` | one-step, single-hop | 4/4 (100%) | 4/4 (100%) |
| `provenance` | provenance | 5/6 (83%) | 6/6 (100%) |

### By question class

| class | prose | engine-briefed |
|---|---|---|
| provenance | 5/6 (83%) | 6/6 (100%) |

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none | unusable |
|---|---|---|---|---|---|
| engine-briefed | haiku-4.5 | 10/10 [72%, 100%] | 0 | 0 | 0 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on the mandated arms it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

**`unusable` cells are out of the mandated arms' denominator, not in their numerator.** Declaring the engine unusable is the mandate's own legal exit, so it is not evidence the mandate failed to take — but no program ran, so it carries nothing about whether the engine helps. It answers neither question and is counted in neither (`decisions.md` 2026-08-28). Read the column: a high one is a finding about the engine, and it shrinks the n every other number rests on.

### Provenance — did it ask the engine *why*?

| arm | strength | acted-on | asked | none | goals run |
|---|---|---|---|---|---|
| engine-briefed | haiku-4.5 | 0/10 [0%, 28%] | 0 | 10 | 0 |

`asked` is the middle case and merges two: a goal written down but never run, and one run on the way out the door. **`goals run` tells them apart** — `asked` with a zero there is the first.

**The baseline is zero.** Across every run on disk before 2026-08-31 — 1,812 transcripts, `engine-briefed` cells included, with `SKILL.md` and its worked `?whynot` examples in the prompt — there was not one invocation. So a non-zero number in this table is the thing the `engine-briefed-provenance` arm was built to produce, and reading the accuracy delta without reading this column first is a mistake: they answer different halves of one question.

### The counterfactual — did it write its own code?

| arm | strength | answered from a script | wrote one | none |
|---|---|---|---|---|
| prose | haiku-4.5 | 4/10 [17%, 69%] | 2 | 4 |
| engine-briefed | haiku-4.5 | 2/10 [6%, 51%] | 0 | 8 |

**Read this table before the accuracy one.** A null delta between an engine arm and `prose` means something different depending on what `prose` did: if it answered from a script, the comparison was script-against-engine and the slate could not have separated them; if it answered from reading, the engine genuinely bought nothing. The accuracy table alone cannot tell those apart.

### Process signals

- **Answers that would not parse:** none.
- **Cells that ended at their budget:** engine-briefed 0/10 (0%), prose 0/10 (0%). The arms do not share one budget (`cell.ARM_BUDGET`), so a difference between them is only readable while these are low — a capped cell measures the cap.
- **Declared the engine unusable:** 0. The subject tried, the engine would not run its program, and it said so instead of looping. Not a correct answer, but a different fact from an empty file — and evidence about the engine rather than a hole.
- **Completions cut off at the output cap:** engine-briefed 0 in 0/10 cells, prose 0 in 0/10 cells. The whole budget went to reasoning and no action came out, so the turn bought nothing and cost its full decode. `n/a` means the run predates the counter, not that it was zero.
- **First program captured before feedback:** 10/10.
- **Switched back to search after using the engine:** 3. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 1. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 4, across 1 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
