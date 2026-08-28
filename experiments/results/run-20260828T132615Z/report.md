# run-20260828T132615Z

16 cells · 0.00 USD · 0 errored

### Negative controls — the engine is *not* expected to help here

| | qwen3-14b-structured | all | mean F1 |
|---|---|---|---|
| **prose** | 3/4 (75%) | 3/4 (75%) [30%, 95%] | 0.75 |
| **engine** | 3/4 (75%) | 3/4 (75%) [30%, 95%] | 0.96 |
| **engine-forced** | 1/4 (25%) | 1/4 (25%) [5%, 70%] | 0.50 |
| **engine-briefed** | 2/4 (50%) | 2/4 (50%) [15%, 85%] | 0.50 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-forced − prose | does the engine make the agent right? | -50 pts | [-100, +0] | 0/2 | 0.500 | 4 |
| engine-briefed − prose | and with the engine's manual in hand? | -25 pts | [-100, +50] | 1/2 | 1.000 | 4 |
| engine − prose | does *supplying* the engine help? | +0 pts | [-75, +75] | 1/1 | 1.000 | 4 |
| engine − engine-forced | what does not reaching for it cost? | +50 pts | [+0, +100] | 2/0 | 0.500 | 4 |
| engine-briefed − engine-forced | what does *finding the manual* cost? | +25 pts | [+0, +75] | 1/0 | 1.000 | 4 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose | engine | engine-forced | engine-briefed |
|---|---|---|---|---|---|
| `controls` | one-step, single-hop | 3/4 (75%) | 3/4 (75%) | 1/4 (25%) | 2/4 (50%) |

### By question class

| class | prose | engine | engine-forced | engine-briefed |
|---|---|---|---|---|

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none |
|---|---|---|---|---|
| engine | qwen3-14b-structured | 0/4 [0%, 49%] | 0 | 4 |
| engine-forced | qwen3-14b-structured | 3/4 [30%, 95%] | 1 | 0 |
| engine-briefed | qwen3-14b-structured | 4/4 [51%, 100%] | 0 | 0 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on the mandated arms it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

### Process signals

- **Answers that would not parse:** none.
- **Cells that ended at their budget:** engine 0/4 (0%), engine-briefed 0/4 (0%), engine-forced 0/4 (0%), prose 0/4 (0%). The arms do not share one budget (`cell.ARM_BUDGET`), so a difference between them is only readable while these are low — a capped cell measures the cap.
- **Declared the engine unusable:** 1. The subject tried, the engine would not run its program, and it said so instead of looping. Not a correct answer, but a different fact from an empty file — and evidence about the engine rather than a hole.
- **First program captured before feedback:** 8/12.
- **Switched back to search after using the engine:** 4. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 3. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 0, across 0 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
