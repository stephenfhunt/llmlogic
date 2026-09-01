# What the `prose` arm is actually doing

Long-form for the 2026-09-01 backfill. `decisions.md` holds the ruling; this file
holds the numbers, the method, and the two caveats that keep them honest.

The founding decision (2026-08-21) says an agent's real alternative to a logic
engine **is not careful prose, it is ad-hoc code**. `signals.ScriptUse` was built
to measure that and, on the day it landed, was backfilled over one run — the
88-cell ladder, where `prose` answered from a script in 33 of 44 cells. That
produced a tempting reading: *the ladder's `+0 at every rung` was a Python script
tying with the engine, not prose tying with it.*

This is that reading, checked against every `prose` cell on disk.

## Method

`classify_script` over the archived transcript of each `prose` cell, joined to
that cell's recorded verdict. Every run in `results/` carries its transcripts, so
nothing here needs re-running the subject:

```python
from harness.signals import classify_script
from harness.transcript import Transcript, ToolCall
# for each results/run-*/records.jsonl row with arm == "prose" and verdict != "error":
#   load results/run-*/transcripts/<cell_id>.json into a Transcript
#   bucket classify_script(transcript) against verdict == "correct"
```

**558 prose cells, every one with a transcript**, across 23 runs and 10 subjects.

## The aggregate: the engine has never been ahead

Every paired comparison on disk — any engine arm against `prose`, same run, same
task, same subject:

| | count |
|---|---|
| engine arm correct, `prose` wrong | **18** |
| `prose` correct, engine arm wrong | **38** |
| agreed | **382** |

Descriptive, not a test: it pools runs, arms and subjects, and it includes items
later voided (`scheduling`'s 2026-08-24 questions, `critical-grant`'s wording).
The shape is not subtle, though — no subject, at any strength, has an engine arm
ahead of its own `prose` arm.

## Within a subject, the script does not explain the accuracy

| subject | answered from a script | did not |
|---|---|---|
| `haiku-4.5` | 62/74 (84%) | 45/51 (88%) |
| `opus-5` | 23/32 (72%) | 12/17 (71%) |
| `qwen3-14b-structured` | 3/7 (43%) | 36/65 (55%) |
| `qwen2.5-coder-7b-structured` | **0/11 (0%)** | 6/37 (16%) |
| `qwen3-14b-native` | 0/1 | 7/23 (30%) |
| `qwen3-8b-structured` | 0/1 | 6/47 (13%) |
| `qwen3-8b-native` | — | 12/48 (25%) |
| `llama3.1-8b-structured` | — | 5/48 (10%) |
| `llama3.1-8b-native` | — | 0/48 (0%) |

Two things fall out. **The strong subjects score the same either way** — haiku is
~85% whether or not it wrote code. And **the weak subjects barely write code at
all**, yet still beat their own engine arms: the 14B is 56% in `prose` against
25% on `engine-briefed`. A coder-tuned 7B did write scripts, in 11 cells, and got
**none** of them right.

So *"prose is really code"* describes the ladder's mechanism and does not
generalize into an explanation of the arm's accuracy. What separates the arms is
not that one has a better tool — it is that the engine arm has to **write a
correct program in a language the model knows less well**, which is a failure
surface `prose` does not have. Both of 2026-08-31's engine losses were Datalog
encoding bugs, not reasoning errors.

## The two caveats

**Script use is subject-chosen.** Conditioning on it conditions on the subject's
own read of the difficulty, so equal accuracy either way is consistent with *"it
writes a script exactly when it needs one"*. This is an observation over archived
runs, not a natural experiment; the experiment would randomize the tool, which
the arms already do for the engine and have never done for the interpreter.

**The pooling hides the difficulty axis.** Every one of these cells is
`in-context`. `at-scale` has never been run, and `scheduling` — the pack whose
questions are search rather than deduction — is parked. Those are the two places
this picture could still change, with the honest note that a *script* also reads
files it cannot fit in its context, so scale alone may not separate the arms
either.

## What this suggests the instrument is measuring

Haiku is 100% across the whole generator range, d1 through d5. The 14B is at
14/7/5% under all of it. `ROADMAP.md` has called the gap between them **the**
blocker since 2026-08-27, and no subject sits in it. An engine cannot show a
delta on an axis where the subject is already saturated or already lost — so what
four grids have produced looks less like a null about the engine and more like a
**bracketing of where these models' own reasoning stops**, with the engine as the
probe. That is a reframing of the project's question, not a finding, and it is
carried as an open item rather than adopted.
