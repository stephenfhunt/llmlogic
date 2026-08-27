# run-20260827T015701Z

144 cells · 0.00 USD · 0 errored

### S1 — the measured slate (in-context)

| | qwen3-14b-native | qwen3-14b-structured | all | mean F1 |
|---|---|---|---|---|
| **prose** | 0/12 (0%) | 1/12 (8%) | 1/24 (4%) [1%, 20%] | 0.59 |
| **engine** | 0/12 (0%) | 2/12 (17%) | 2/24 (8%) [2%, 26%] | 0.69 |
| **engine-forced** | 0/12 (0%) | 0/12 (0%) | 0/24 (0%) [0%, 14%] | 0.71 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-forced − prose | does the engine make the agent right? | +0 pts | [+0, +0] | 0/0 | 1.000 | 8 |
| engine − prose | does *supplying* the engine help? | +12 pts | [+0, +38] | 1/0 | 1.000 | 8 |
| engine − engine-forced | what does not reaching for it cost? | +12 pts | [+0, +38] | 1/0 | 1.000 | 8 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

### Negative controls — the engine is *not* expected to help here

| | qwen3-14b-native | qwen3-14b-structured | all | mean F1 |
|---|---|---|---|---|
| **prose** | 7/12 (58%) | 10/12 (83%) | 17/24 (71%) [51%, 85%] | 0.99 |
| **engine** | 9/12 (75%) | 11/12 (92%) | 20/24 (83%) [64%, 93%] | 0.96 |
| **engine-forced** | 5/12 (42%) | 3/12 (25%) | 8/24 (33%) [18%, 53%] | 0.92 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-forced − prose | does the engine make the agent right? | -25 pts | [-62, +25] | 1/3 | 0.625 | 8 |
| engine − prose | does *supplying* the engine help? | +25 pts | [+0, +62] | 2/0 | 0.500 | 8 |
| engine − engine-forced | what does not reaching for it cost? | +50 pts | [+12, +88] | 4/0 | 0.125 | 8 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose | engine | engine-forced |
|---|---|---|---|---|
| `access_control` | negation, recursion | 1/24 (4%) | 2/24 (8%) | 0/24 (0%) |
| `controls` | one-step, single-hop | 17/24 (71%) | 20/24 (83%) | 8/24 (33%) |

### By question class

| class | prose | engine | engine-forced |
|---|---|---|---|
| negation | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) |
| recursion | 1/12 (8%) | 2/12 (17%) | 0/12 (0%) |

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none |
|---|---|---|---|---|
| engine | qwen3-14b-native | 2/24 [2%, 26%] | 10 | 12 |
| engine | qwen3-14b-structured | 2/24 [2%, 26%] | 5 | 17 |
| engine-forced | qwen3-14b-native | 6/24 [12%, 45%] | 14 | 4 |
| engine-forced | qwen3-14b-structured | 7/24 [15%, 49%] | 16 | 1 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on `engine-forced` it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

### Process signals

- **First program captured before feedback:** 43/96.
- **Switched back to search after using the engine:** 18. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 20. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 0, across 0 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
