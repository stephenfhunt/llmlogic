# run-20260828T012126Z

12 cells · 0.00 USD · 0 errored

### Negative controls — the engine is *not* expected to help here

| | qwen3-14b-structured | all | mean F1 |
|---|---|---|---|
| **prose** | 3/4 (75%) | 3/4 (75%) [30%, 95%] | 0.96 |
| **engine** | 4/4 (100%) | 4/4 (100%) [51%, 100%] | 1.00 |
| **engine-forced** | 2/4 (50%) | 2/4 (50%) [15%, 85%] | 0.75 |

| comparison | question | delta | 95% CI | wins/losses | p | paired tasks |
|---|---|---|---|---|---|---|
| engine-forced − prose | does the engine make the agent right? | -25 pts | [-100, +50] | 1/2 | 1.000 | 4 |
| engine − prose | does *supplying* the engine help? | +25 pts | [+0, +75] | 1/0 | 1.000 | 4 |
| engine − engine-forced | what does not reaching for it cost? | +50 pts | [+0, +100] | 2/0 | 0.500 | 4 |

**wins/losses** are the discordant pairs — the tasks the two arms disagreed on, and the only ones carrying information about a difference. `p` is McNemar's exact test. A wide interval around a small delta is *not* a null: it is the slate saying it was too small or too easy to tell.

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose | engine | engine-forced |
|---|---|---|---|---|
| `controls` | one-step, single-hop | 3/4 (75%) | 4/4 (100%) | 2/4 (50%) |

### By question class

| class | prose | engine | engine-forced |
|---|---|---|---|

### Reach — did the subject actually use the engine?

| arm | strength | answered-from | invoked | none |
|---|---|---|---|---|
| engine | qwen3-14b-structured | 0/4 [0%, 49%] | 0 | 4 |
| engine-forced | qwen3-14b-structured | 3/4 [30%, 95%] | 1 | 0 |

`invoked` is the middle case: it reached for the engine and no program ran. Only `answered-from` is engine use in the sense S1 means (`decisions.md` 2026-08-25). On the `engine` arm this is a measurement; on `engine-forced` it is a **compliance check** — a low number there means the mandate did not take, and the comparison it feeds is void.

### Process signals

- **Answers that would not parse:** none.
- **First program captured before feedback:** 4/8.
- **Switched back to search after using the engine:** 1. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 0. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 0, across 0 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
