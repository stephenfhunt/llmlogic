# run-20260828T025652Z

3 cells · 0.00 USD · 0 errored

### Negative controls — the engine is *not* expected to help here

| | qwen3-14b-structured | all | mean F1 |
|---|---|---|---|
| **prose** | 0/1 (0%) | 0/1 (0%) [0%, 79%] | 0.00 |
| **engine** | 1/1 (100%) | 1/1 (100%) [21%, 100%] | 1.00 |
| **engine-forced** | 0/1 (0%) | 0/1 (0%) [0%, 79%] | 0.00 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-forced − prose | does the engine make the agent right? | +0 pts | [+0, +0] | 0/0 | 1.000 | 1 |
| engine − prose | does *supplying* the engine help? | +100 pts | [+100, +100] | 1/0 | 1.000 | 1 |
| engine − engine-forced | what does not reaching for it cost? | +100 pts | [+100, +100] | 1/0 | 1.000 | 1 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose | engine | engine-forced |
|---|---|---|---|---|
| `controls` | single-hop | 0/1 (0%) | 1/1 (100%) | 0/1 (0%) |

### By question class

| class | prose | engine | engine-forced |
|---|---|---|---|

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none |
|---|---|---|---|---|
| engine | qwen3-14b-structured | 0/1 [0%, 79%] | 0 | 1 |
| engine-forced | qwen3-14b-structured | 1/1 [21%, 100%] | 0 | 0 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on `engine-forced` it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

### Process signals

- **Answers that would not parse:** none.
- **Cells that ended at their budget:** engine 0/1 (0%), engine-forced 0/1 (0%), prose 0/1 (0%). The arms do not share one budget (`cell.ARM_BUDGET`), so a difference between them is only readable while these are low — a capped cell measures the cap.
- **Declared the engine unusable:** 0. The subject tried, the engine would not run its program, and it said so instead of looping. Not a correct answer, but a different fact from an empty file — and evidence about the engine rather than a hole.
- **First program captured before feedback:** 1/2.
- **Switched back to search after using the engine:** 0. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 1. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 0, across 0 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
