# Making the instrument discriminate

Long-form for the 2026-08-25 decisions on arms, difficulty tracks, statistics and
a local subject. `decisions.md` holds the rulings; this file holds the argument
and the numbers behind them. `ROADMAP.md` holds the work items.

## What the first grid actually measured

`results/run-20260824T104501Z/` — 112 cells, engine 41/48, prose 41/48, delta +0,
and **every domain identical in both arms**:

| domain | engine | prose |
|---|---|---|
| access_control | 8/8 | 8/8 |
| controls | 8/8 | 8/8 |
| eligibility | 7/8 | 7/8 |
| imports | 8/8 | 8/8 |
| ontology | 8/8 | 8/8 |
| scheduling | 3/8 | 3/8 (void — fixture defect) |
| static_analysis | 7/8 | 7/8 |

Seven domains agreeing to the cell is not a null result. It is the signature of
two conditions that were not different. Recomputed from `records.jsonl`, the
engine arm reached for the engine in **9 of 56 cells** — opus **1/28**, haiku
**8/28**. In the other 47 the arms differed only in which files were on disk.

Three defects, and they are independent — fixing any one alone leaves the grid
unable to answer S1.

1. **The independent variable was barely manipulated.** Reach is upstream of
   everything the criterion asks.
2. **The slate is at a ceiling.** Opus is 20/20 in prose on the live domains.
   Even at reach 56/56 there is no room for a positive delta.
3. **There is no resolution.** n=48 per arm, one trial per cell, binary set
   equality, and no statistics in the codebase at all — `report.py` subtracts two
   percentages and stops. A delta of +0 with no interval is not evidence of no
   effect; it is the absence of a measurement.

## Why a third arm, and why control 3 survives

Control 3 — *the subject is never told to use datalog* — is what makes "reached
for it?" a measurement rather than an instruction. It is also what made the grid
uninterpretable, because the same run has to answer two questions that pull in
opposite directions:

- *Would an agent pick this up on its own?* — needs control 3.
- *Does the engine make the agent right?* — needs the engine to be used.

One arm cannot do both. So there are three, and control 3 is retained exactly
where it belongs:

| arm | prompt | question it answers |
|---|---|---|
| `prose` | base | the baseline; free to write Python |
| `engine` | base, byte-identical | **adoption**: does supplying it help? |
| `engine-forced` | base + mandate block | **capability**: does using it help? |

`engine-forced` vs `prose` is S1's sentence read literally. `engine` vs
`engine-forced` is the adoption gap, and it is a finding about the skill, not
about the engine — the thing the ablation machinery already exists to act on.

The cost is a 50% larger grid. `--strength` and `--arms` slicing already exists,
and the calibrated slate (below) is what keeps the total honest.

### Reach becomes a reported outcome

`decisions.md` 2026-08-21 left *what counts as reaching for it* open, and the
first real transcript split it three ways: a haiku cell invoked the `Skill` tool
with an entire program in `args` and **nothing ran**. So:

- `none` — no program, no invocation
- `invoked` — asked for the engine, but no program executed
- `answered-from` — a program ran and its output is in the answer

Only the third is engine use in the sense S1 means. Recording the middle one
separately is what stops that transcript being counted as a success.

## Two difficulty tracks, and why they never share a number

`cell.FIXTURE_TOKEN_BUDGET = 100_000` exists for a real reason, stated in
`domains/access_control/fixture.py`: *"a fixture that defeats it on context alone
would measure the context window."* That is correct, and it is also why the slate
has no headroom.

The resolution is not to relax the cap. It is to admit there are two questions:

- **`in-context`** — both arms see everything. The cap holds. Difficulty comes
  from *structure*: depth, negation, distractors. A win here is a claim about
  reasoning.
- **`at-scale`** — the fact base does not fit the prose arm's window. The cap does
  not apply. A win here is a claim about *scale*, and saying so plainly is what
  makes it honest rather than rigged.

They are reported in separate tables and never averaged. `Task.track` carries
which, and the size guard in `tests/test_controls_hold.py` becomes conditional on
it — the cap is the **definition** of the first track, not a global rule.

### What actually makes an item hard

Structure, not size. The knobs, each chosen against a known failure mode:

| knob | why |
|---|---|
| closure depth ≥ 6, with cycles | prose reasoning slips a generation; this is the canonical case |
| answer sets of 30–200 rows | the *silent subset* needs room to happen — 9 occurred in the last grid |
| two-level stratified negation | "X with no Y that has no Z" is where chain-of-thought inverts |
| negation over a **derived** relation | forces the closure to be completed before it can be negated |
| aggregation over a derived relation, with a threshold | combines two strata |
| planted near-miss distractors | entities one hop or one attribute from qualifying |
| an override layer applied **after** closure | the order of operations is the trap |
| the count-over-wildcard trap | `datalog/ROADMAP.md` names this harness as the instrument meant to rule on it |
| a fraction with empty true answers | so writing nothing never pays |

All seeded, with identifiers regenerated per seed, so nothing is memorised and a
re-run is a different sample of the same difficulty rather than the same items.

### Scale, bounded by what the engine measurably does

`datalog/notes/performance-baseline.md`: 27,957 facts import in 0.32s; a
5,248-edge closure yields 173,177 tuples in 11.6s. `at-scale` targets **10k–100k
rows**. The same note records what not to build: mutual recursion self-joining a
173k derived relation was killed at two minutes, and merely *importing* an unused
rule library took a run from 0.33s to 11.5s, because evaluation is bottom-up with
no demand-driven pruning. Anchor joins on the small base relation.

## Calibration, and why guessing the knob settings is not allowed

An item on which the subject scores 0% or 100% carries almost no information
about whether the engine helped. The informative band is the middle. So the slate
is **selected, not designed**: generate a large candidate pool, run a cheap pass
(prose arm, one weak strength) and keep items whose prose accuracy falls in
roughly 0.2–0.8.

This is the step that would have caught the 2026-08-24 ceiling before the grid
was paid for, and it is cheap because it is one arm at one strength.

The selected slate is pinned to a manifest, so the measured grid is reproducible
and the calibration pass is not silently re-run with different results.

## Item validation, from the `scheduling` lesson

`decisions.md` 2026-08-24: hand-verifying 28 tasks against their oracles proved
the oracle matched the **author's** reading, not that there was only one reading.
21 of 29 `scheduling` assignments violated the rule its own questions stated.

Generation multiplies that risk by the number of items, so validation has to be
mechanical:

1. **The oracle agrees with a second, independent formulation.**
   `access_control` already does this — BFS against a fixpoint.
2. **The fixture obeys every rule its question states.**
   `tests/test_scheduling_truth.py:174,190` are the template; they exist because
   nothing compared the fixture against the question text.
3. **Two independent readings agree.** The question is prose, and prose is where
   the defect lives. Divergent items are quarantined, not shipped.

## Statistics

Pure stdlib. Wilson and an exact binomial are about twenty lines each, and
`AGENTS.md` is explicit that a dependency is a decision.

- **Repeats.** The subject is stochastic and every cell has been run exactly once.
  `--repeats N` is the only way to get a variance estimate; the trial index joins
  the cell identity so `resume` can count them.
- **Paired tests.** The design is paired by task, so McNemar's exact test is the
  right one and is far more powerful here than comparing two independent rates.
  A bootstrap gives the delta an interval.
- **Partial credit.** F1 over the answer set, reported beside the binary verdict.
  `Grade` already carries `missing`/`extra` and the truth size is known, so this
  is **recoverable from the existing `records.jsonl`** — the 2026-08-24 run gets a
  finer read without re-running anything.
- **Power.** A run that could not have detected the effect it was looking for is
  the most expensive kind of null. `harness power` says the item count up front.
- **Pre-registration.** `hypotheses.md`, written before the grid. The comparisons
  multiplied from one to three this session; naming the primary endpoint in
  advance is what keeps that from becoming three chances to find something.

## A local subject

Two of the three claims in scope cannot be measured without one:

- *the engine helps at the weak end* — the worklog's own conclusion, and there is
  no subject below haiku today;
- *a small model with the engine matches a frontier model without it* — the
  strongest single number this repo could produce, and it needs the small model.

`Subject` is already the right seam: one method, and everything downstream depends
on `Transcript`, not on the SDK. Hardware is one RTX 3060, 12 GB — Qwen2.5-Coder-7B
and Llama-3.1-8B at 4-bit fit, 1.5B/3B comfortably. An OpenAI-compatible client
against llama.cpp keeps vLLM a substitution rather than a rewrite.

What Claude Code was supplying for free, and now has to be built:

- the tool loop itself (`Read/Write/Edit/Bash/Grep/Glob` as schemas);
- **network isolation** — `unshare -n` or equivalent. A subject that can reach the
  network is a validity problem before it is a security one;
- **a wall-clock timeout on every bash call.** The engine has no fuel, cap or
  timeout **by decision** (`datalog/spec.md` non-goals, rejected twice), and a weak
  model will write value-creating recursion. The cap belongs in the harness, on
  the tool call — wrapping the engine binary in a `timeout` shim would change the
  engine arm's environment, which is the one thing the design holds fixed.

Two Anthropic-shaped assumptions have to move behind the seam: `Strength`
hardcodes per-MTok prices and a context window, and `runner.FATAL` is a regex over
Anthropic error text. The second matters — a misclassified local failure is
recorded as an ordinary wrong answer, which is precisely the defect that produced
46 phantom `no-answer` cells on 2026-08-24.

`confine.violation` and `engine_use.program_from_call` are already pure functions
and are reused as-is, so controls 3 and 4 hold for the local subject by
construction rather than by reimplementation.

## Fine-tuning: tabled, not foreclosed

Out of scope this cycle. Two things keep the door open and are wanted for their
own sake anyway:

- the generators emit arbitrarily many items with independent oracles — that is a
  training corpus the day one is wanted, and today it is how the slate is built;
- `harness score --task <id> --program <file>` runs a program and grades its
  output. One command, verifiable, and the natural home for the check the
  reference corpus already performs.

No trainer, no rollout server, no dataset export.
