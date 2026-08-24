# run-20260824T013829Z

16 cells · 1.08 USD · 0 errored

### S1 — the measured slate

| | opus-5 | haiku-4.5 | all |
|---|---|---|---|
| **engine** | 4/4 (100%) | 4/4 (100%) | 8/8 (100%) |
| **prose** | 4/4 (100%) | 4/4 (100%) | 8/8 (100%) |

**Delta: +0 points** (engine − prose).

### Negative controls — the engine is *not* expected to help here

| | opus-5 | haiku-4.5 | all |
|---|---|---|---|
| **engine** | — | — | — |
| **prose** | — | — | — |

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | engine | prose |
|---|---|---|---|
| `access_control` | negation, recursion | 8/8 (100%) | 8/8 (100%) |

### Process signals

- **Reached for the engine:** 4/8 engine-arm cells.
- **First program captured before feedback:** 4/8.
- **Switched back to search after using the engine:** 1. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 0. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 4, across 2 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
