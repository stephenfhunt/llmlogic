# run-20260828T162946Z

16 cells · 0.00 USD · 0 errored

### S1 — the measured slate (in-context)

| | qwen3-14b-structured | all | mean F1 |
|---|---|---|---|
| **prose** | 2/8 (25%) | 2/8 (25%) [7%, 59%] | 0.55 |
| **engine-briefed** | 1/8 (12%) | 1/8 (12%) [2%, 47%] | 0.19 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-briefed − prose | and with the engine's manual in hand? | -12 pts | [-50, +25] | 1/2 | 1.000 | 8 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

### Negative controls — the engine is *not* expected to help here

No cells.

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose | engine-briefed |
|---|---|---|---|
| `access_control` | negation, recursion | 1/4 (25%) | 0/4 (0%) |
| `ontology` | constraint, negation, recursion | 1/4 (25%) | 1/4 (25%) |

### By question class

| class | prose | engine-briefed |
|---|---|---|
| constraint | 0/1 (0%) | 1/1 (100%) |
| negation | 1/3 (33%) | 0/3 (0%) |
| recursion | 1/4 (25%) | 0/4 (0%) |

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none | unusable |
|---|---|---|---|---|---|
| engine-briefed | qwen3-14b-structured | 8/8 [68%, 100%] | 0 | 0 | 0 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on the mandated arms it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

**`unusable` cells are out of the mandated arms' denominator, not in their numerator.** Declaring the engine unusable is the mandate's own legal exit, so it is not evidence the mandate failed to take — but no program ran, so it carries nothing about whether the engine helps. It answers neither question and is counted in neither (`decisions.md` 2026-08-28). Read the column: a high one is a finding about the engine, and it shrinks the n every other number rests on.

### Process signals

- **Answers that would not parse:** none.
- **Cells that ended at their budget:** engine-briefed 1/8 (12%), prose 2/8 (25%). The arms do not share one budget (`cell.ARM_BUDGET`), so a difference between them is only readable while these are low — a capped cell measures the cap.
- **Declared the engine unusable:** 0. The subject tried, the engine would not run its program, and it said so instead of looping. Not a correct answer, but a different fact from an empty file — and evidence about the engine rather than a hole.
- **Completions cut off at the output cap:** engine-briefed 4 in 4/8 cells, prose 10 in 5/8 cells. The whole budget went to reasoning and no action came out, so the turn bought nothing and cost its full decode. `n/a` means the run predates the counter, not that it was zero.
- **Cells ended by that:** prose 2. Stopped after two in a row, because the loop is deterministic — the reasoning is not fed back, so a third lap re-thinks the same thing from the same conversation.
- **First program captured before feedback:** 8/8.
- **Switched back to search after using the engine:** 2. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 7. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 0, across 0 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
