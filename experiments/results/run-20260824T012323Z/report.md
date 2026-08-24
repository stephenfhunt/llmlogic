# run-20260824T012323Z

4 cells · 0.38 USD · 0 errored

### S1 — the measured slate

| | opus-5 | haiku-4.5 | all |
|---|---|---|---|
| **engine** | — | — | — |
| **prose** | — | — | — |

### Negative controls — the engine is *not* expected to help here

| | opus-5 | haiku-4.5 | all |
|---|---|---|---|
| **engine** | 1/1 (100%) | 1/1 (100%) | 2/2 (100%) |
| **prose** | 1/1 (100%) | 1/1 (100%) | 2/2 (100%) |

**Delta: +0 points** (engine − prose).

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | engine | prose |
|---|---|---|---|
| `controls` | single-hop | 2/2 (100%) | 2/2 (100%) |

### Process signals

- **Reached for the engine:** 0/2 engine-arm cells.
- **First program captured before feedback:** 0/2.
- **Switched back to search after using the engine:** 0. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 0. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 0, across 0 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
