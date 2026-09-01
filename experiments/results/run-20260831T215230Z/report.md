# run-20260831T215230Z

5 cells · 0.49 USD · 1 errored

### S1 — the measured slate (in-context)

| | haiku-4.5 | all | mean F1 |
|---|---|---|---|
| **prose** | 2/2 (100%) | 2/2 (100%) [34%, 100%] | 1.00 |
| **engine-briefed** | 0/2 (0%) | 0/2 (0%) [0%, 66%] | 0.86 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-briefed − prose | and with the engine's manual in hand? | -100 pts | [-100, -100] | 0/2 | 0.500 | 2 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

### Negative controls — the engine is *not* expected to help here

No cells.

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose | engine-briefed |
|---|---|---|---|
| `provenance` | provenance | 2/2 (100%) | 0/2 (0%) |

### By question class

| class | prose | engine-briefed |
|---|---|---|
| provenance | 2/2 (100%) | 0/2 (0%) |

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none | unusable |
|---|---|---|---|---|---|
| engine-briefed | haiku-4.5 | 2/2 [34%, 100%] | 0 | 0 | 0 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on the mandated arms it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

**`unusable` cells are out of the mandated arms' denominator, not in their numerator.** Declaring the engine unusable is the mandate's own legal exit, so it is not evidence the mandate failed to take — but no program ran, so it carries nothing about whether the engine helps. It answers neither question and is counted in neither (`decisions.md` 2026-08-28). Read the column: a high one is a finding about the engine, and it shrinks the n every other number rests on.

### Provenance — did it ask the engine *why*?

| arm | strength | acted-on | asked | none | goals run |
|---|---|---|---|---|---|
| engine-briefed | haiku-4.5 | 0/2 [0%, 66%] | 0 | 2 | 0 |

`asked` is the middle case and merges two: a goal written down but never run, and one run on the way out the door. **`goals run` tells them apart** — `asked` with a zero there is the first.

**The baseline is zero.** Across every run on disk before 2026-08-31 — 1,812 transcripts, `engine-briefed` cells included, with `SKILL.md` and its worked `?whynot` examples in the prompt — there was not one invocation. So a non-zero number in this table is the thing the `engine-briefed-provenance` arm was built to produce, and reading the accuracy delta without reading this column first is a mistake: they answer different halves of one question.

### The counterfactual — did it write its own code?

| arm | strength | answered from a script | wrote one | none |
|---|---|---|---|---|
| prose | haiku-4.5 | 2/2 [34%, 100%] | 0 | 0 |
| engine-briefed | haiku-4.5 | 0/2 [0%, 66%] | 0 | 2 |

**Read this table before the accuracy one.** A null delta between an engine arm and `prose` means something different depending on what `prose` did: if it answered from a script, the comparison was script-against-engine and the slate could not have separated them; if it answered from reading, the engine genuinely bought nothing. The accuracy table alone cannot tell those apart.

### Process signals

- **Answers that would not parse:** none.
- **Cells that ended at their budget:** engine-briefed 0/2 (0%), prose 0/2 (0%). The arms do not share one budget (`cell.ARM_BUDGET`), so a difference between them is only readable while these are low — a capped cell measures the cap.
- **Declared the engine unusable:** 0. The subject tried, the engine would not run its program, and it said so instead of looping. Not a correct answer, but a different fact from an empty file — and evidence about the engine rather than a hole.
- **Completions cut off at the output cap:** engine-briefed 0 in 0/2 cells, prose 0 in 0/2 cells. The whole budget went to reasoning and no action came out, so the turn bought nothing and cost its full decode. `n/a` means the run predates the counter, not that it was zero.
- **First program captured before feedback:** 2/2.
- **Switched back to search after using the engine:** 0. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 1. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 0, across 0 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
