# run-20260824T104501Z

112 cells · 16.69 USD · 0 errored

**Resumed**: 48 cells were run a second time after the first attempt was cut short, and are counted once, at their later attempt. The cost above is everything the run spent, including the attempts that produced nothing. A grid measured across more than one session window is still one grid, but it was not one sitting.

### S1 — the measured slate

| | opus-5 | haiku-4.5 | all |
|---|---|---|---|
| **engine** | 23/24 (96%) | 18/24 (75%) | 41/48 (85%) |
| **prose** | 22/24 (92%) | 19/24 (79%) | 41/48 (85%) |

**Delta: +0 points** (engine − prose).

### Negative controls — the engine is *not* expected to help here

| | opus-5 | haiku-4.5 | all |
|---|---|---|---|
| **engine** | 4/4 (100%) | 4/4 (100%) | 8/8 (100%) |
| **prose** | 4/4 (100%) | 4/4 (100%) | 8/8 (100%) |

**Delta: +0 points** (engine − prose).

A delta on the controls is a warning about the instrument, not a result: these are single-hop lookups and one-step arithmetic.

### By domain

| domain | class | engine | prose |
|---|---|---|---|
| `access_control` | negation, recursion | 8/8 (100%) | 8/8 (100%) |
| `controls` | one-step, single-hop | 8/8 (100%) | 8/8 (100%) |
| `eligibility` | aggregation, negation | 7/8 (88%) | 7/8 (88%) |
| `imports` | aggregation, negation, temporal | 8/8 (100%) | 8/8 (100%) |
| `ontology` | constraint, negation, recursion | 8/8 (100%) | 8/8 (100%) |
| `scheduling` | constraint, negation, temporal | 3/8 (38%) | 3/8 (38%) |
| `static_analysis` | aggregation, negation, recursion | 7/8 (88%) | 7/8 (88%) |

### Process signals

- **Reached for the engine:** 9/56 engine-arm cells.
- **First program captured before feedback:** 8/56.
- **Switched back to search after using the engine:** 0. This is the silent one — the subject had the engine, tried it, and went back to text.
- **Answers that were a strict subset of the truth:** 9. Rows dropped, and nothing in the output says so.
- **Tool calls denied for leaving the workspace:** 26, across 14 cells. A spike here is friction, not an attack — it usually means the prompt or the fixture made leaving look necessary. **A floor, not a count**: only the PreToolUse gate records a denial, and it flags an absolute path that already *exists*, so a write to a new path outside the workspace is stopped by the OS sandbox and never counted (seen on the first pilot). A low number is not evidence the subject stayed put.
