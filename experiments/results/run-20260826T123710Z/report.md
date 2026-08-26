# run-20260826T123710Z

432 cells · 0.00 USD · 0 errored

### S1 — the measured slate (in-context)

| | llama3.1-8b-native | llama3.1-8b-structured | qwen2.5-coder-7b-native | qwen2.5-coder-7b-structured | qwen3-8b-native | qwen3-8b-structured | all | mean F1 |
|---|---|---|---|---|---|---|---|---|
| **prose** | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/72 (0%) [0%, 5%] | 0.68 |
| **engine** | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/72 (0%) [0%, 5%] | 0.74 |
| **engine-forced** | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/72 (0%) [0%, 5%] | 0.82 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-forced − prose | does the engine make the agent right? | +0 pts | [+0, +0] | 0/0 | 1.000 | 24 |
| engine − prose | does *supplying* the engine help? | +0 pts | [+0, +0] | 0/0 | 1.000 | 24 |
| engine − engine-forced | what does not reaching for it cost? | +0 pts | [+0, +0] | 0/0 | 1.000 | 24 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

### Negative controls — the engine is *not* expected to help here

| | llama3.1-8b-native | llama3.1-8b-structured | qwen2.5-coder-7b-native | qwen2.5-coder-7b-structured | qwen3-8b-native | qwen3-8b-structured | all | mean F1 |
|---|---|---|---|---|---|---|---|---|
| **prose** | 0/12 (0%) | 2/12 (17%) | 0/12 (0%) | 3/12 (25%) | 6/12 (50%) | 0/12 (0%) | 11/72 (15%) [9%, 25%] | 0.85 |
| **engine** | 0/12 (0%) | 3/12 (25%) | 0/12 (0%) | 1/12 (8%) | 6/12 (50%) | 0/12 (0%) | 10/72 (14%) [8%, 24%] | 0.82 |
| **engine-forced** | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 0/12 (0%) | 1/12 (8%) | 1/72 (1%) [0%, 7%] | 0.90 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-forced − prose | does the engine make the agent right? | -17 pts | [-33, -4] | 0/4 | 0.125 | 24 |
| engine − prose | does *supplying* the engine help? | -4 pts | [-12, +0] | 0/1 | 1.000 | 24 |
| engine − engine-forced | what does not reaching for it cost? | +12 pts | [+0, +29] | 3/0 | 0.250 | 24 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose | engine | engine-forced |
|---|---|---|---|---|
| `access_control` | negation, recursion | 0/72 (0%) | 0/72 (0%) | 0/72 (0%) |
| `controls` | one-step, single-hop | 11/72 (15%) | 10/72 (14%) | 1/72 (1%) |

### By question class

| class | prose | engine | engine-forced |
|---|---|---|---|
| negation | 0/36 (0%) | 0/36 (0%) | 0/36 (0%) |
| recursion | 0/36 (0%) | 0/36 (0%) | 0/36 (0%) |

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none |
|---|---|---|---|---|
| engine | llama3.1-8b-native | 0/24 [0%, 14%] | 8 | 16 |
| engine | llama3.1-8b-structured | 10/24 [24%, 61%] | 8 | 6 |
| engine | qwen2.5-coder-7b-native | 0/24 [0%, 14%] | 0 | 24 |
| engine | qwen2.5-coder-7b-structured | 0/24 [0%, 14%] | 3 | 21 |
| engine | qwen3-8b-native | 0/24 [0%, 14%] | 12 | 12 |
| engine | qwen3-8b-structured | 1/24 [1%, 20%] | 6 | 17 |
| engine-forced | llama3.1-8b-native | 3/24 [4%, 31%] | 21 | 0 |
| engine-forced | llama3.1-8b-structured | 8/24 [18%, 53%] | 13 | 3 |
| engine-forced | qwen2.5-coder-7b-native | 0/24 [0%, 14%] | 0 | 24 |
| engine-forced | qwen2.5-coder-7b-structured | 1/24 [1%, 20%] | 23 | 0 |
| engine-forced | qwen3-8b-native | 1/24 [1%, 20%] | 23 | 0 |
| engine-forced | qwen3-8b-structured | 7/24 [15%, 49%] | 17 | 0 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on `engine-forced` it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

### Process signals

- **First program captured before feedback:** 121/288.
- **Switched back to search after using the engine:** 29. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 81. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 15, across 5 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
