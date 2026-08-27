# run-20260827T012630Z

4 cells · 0.00 USD · 0 errored

### Negative controls — the engine is *not* expected to help here

| | qwen3-14b-structured | all | mean F1 |
|---|---|---|---|
| **prose** | 3/4 (75%) | 3/4 (75%) [30%, 95%] | 0.95 |

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | prose |
|---|---|---|
| `controls` | one-step, single-hop | 3/4 (75%) |

### By question class

| class | prose |
|---|---|

### Process signals

- **First program captured before feedback:** 0/0.
- **Switched back to search after using the engine:** 0. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 1. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 0, across 0 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
