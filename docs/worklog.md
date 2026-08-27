# Worklog

A running handoff log for chaining agentic coding sessions. Each session ends by
adding an entry so the next session can get oriented in seconds — without re-reading
raw transcripts (Claude Code auto-saves those under
`~/.claude/projects/<repo-slug>/*.jsonl`; resume with `claude --resume`).

**Conventions**
- Newest entry on top (reverse-chronological).
- Keep each entry short and high-signal. Four fields:
  - **Done** — what changed this session (link commits/PRs where useful).
  - **Decided** — key decisions made (design decisions also go in `datalog/spec.md`
    §17; note them here too so the timeline is complete).
  - **Removed** — what you deleted, merged, or replaced.
  - **Next up** — the concrete next threads, so the following session starts oriented.
- This is a curated summary, not a transcript. Don't paste raw output here.
- **Entries stay under ~50 lines**, and **this file keeps the most recent three.**
  Older entries rotate verbatim into [`worklog-archive/`](worklog-archive/) by
  month, so session-start orientation stays a fixed cost instead of a growing one.
  A session needing more room than that is describing work that wants its own
  document — put the long form in `datalog/notes/` and link it from the entry.
  (50 rather than 40 because the entry that set this rule landed at 48, and a cap
  nobody meets gets ignored — cf. the `Stable` rung, deleted for the same reason.)

---

## 2026-08-27 (later) — The first paid pass, and the rung that is not there

The calibration pass ran 408 of 1,125 cells and was **stopped deliberately**
(`results/cal-20260827T035804Z`). It answers the question it was launched to
answer, just not the way it was meant to: the generated pool is too hard for this
subject at **every difficulty the generators reach**, so there was nothing in the
band to select.

**Done**
- **The pass, read.** correct by difficulty: **d1 14%, d2 7%, d3 5%**. At the
  easiest setting `eligibility` is 0/25, `scheduling` 2/25, `imports` 3/25,
  `ontology` 4/25. 72% of two-trial items sit at zero correct, which rejects as
  *too hard*. Difficulty 1 is the floor, so no knob remains.
- **23% of cells bought no measurement** — 95 of 408 hit a stopping rule, and
  `scheduling` ran a median 234.6s against a 240s cap, 43 of 75 capped.
- **Stopped at ~22h remaining**, projecting ~75 kept items against the 155 the
  +20-point endpoint needs — and precondition 2 already fails, so the grid it
  would feed could not read the primary endpoint. No slate written: a halted pass
  refuses selection, which is the rule working.
- **The format fix looks to have taken.** `unparseable` is 9/408 = **2.2%**, five
  of them `wrong-arity`, against 15.4% of single-column cells before it. Different
  item mix and one run, so it is a direction and not yet a result.

**Decided** (`experiments/decisions.md`, one entry)
- **The gap is between the rungs, not inside them.** The same subject scores 83%
  on `controls` and 14% on the easiest generated item; the slate has nothing in
  between, and calibration cannot select what was never generated. That is now a
  measurement rather than a suspicion, and it is the blocker.

**Removed**
- Nothing deleted. The previous entry's *Next up* — resume the pass — is
  superseded by this one; "The pass that picks the slate" rotated to the archive.

**Next up**
- **Build the missing rung, or change the subject.** Either the generators grow a
  form easier than today's difficulty 1, or the calibrating subject is stronger
  than a 14B. A slate cannot be calibrated into a gap.
- **`scheduling` needs its own look** before any re-run: capped in 43 of 75 cells,
  it spends the most time and returns the least evidence of any pack.
- **Still owed from the earlier entry:** the mandate arm (`invoked` 16/24), the
  prompt-blind `resume.fingerprint`, and writing the partial-read rule down.

## 2026-08-27 — The gate that inverted its own interim read

`qwen3:14b` at 16k with q4 KV **clears the controls floor on `structured`** — the
precondition every 8B failed. 144 cells, 2h, $0.00
(`results/run-20260827T015701Z`). The mandate still does not take, on either
protocol. **1,410 harness tests green (+19)**, ruff clean, 52 datalog tests green.
A 1,125-cell calibration pass is running detached.

**Done**
- **The gate, per protocol.** Precondition 1: `structured` **10/12 = 83% PASS**,
  `native` 7/12 = 58% FAIL. Precondition 2: **25% / 29%** against an 80% floor.
  The measured slate is on the floor — `access_control` 0/36 and 3/36.
- **`harness calibrate` takes the local seam** (`cli._local_sitting`), refusing two
  strengths by name — `tally` counts by task key, so a two-protocol pass would put
  two subjects in one band. The same work made a local run **resumable at all**:
  `cmd_resume` rebuilt strengths from the two Anthropic ones and always built
  `AgentSubject`, so a halted local run refused itself.
- **The pipe-joined answer was the prompt's fault**: for one column the bullet
  said *"the fields `order_id`, separated by `|`"*. **15.4% of single-column cells
  `unparseable` against 1.9% of two-column ones.** `catalogue._fields_line` splits
  by arity; `unparseable` now records which of three ways it failed.
- **`datalog/bugs/009`** — a column type clash names one occurrence, sometimes no
  span at all, found by a local subject looping 15 rewrites against it. Two
  `#[ignore]`d criteria: the instance, and the property.
- **The card is power-bound, not thermally bound; the desktop holds 742 MiB** —
  the margin the 16k/q8 row spills by. — `notes/a-local-subject.md` (also vLLM).

**Decided** (`experiments/decisions.md`, four entries)
- **Fix the instruction, not the ruler.** Tolerating `|` would turn 119 answers
  into 104 `wrong` and 15 `correct`, the wrong-flips falling 47/34/23 across the
  arms — bias toward the engine, which `grade.py` cannot afford.
- **A pass is one subject and the manifest says which** (`spec` had hardcoded
  haiku); **a calibrated grid carries the pinned controls**, the pool draws none.
- **`hypotheses.md` addendum: a local grid is powered for +20 points, not +10.**
  Paired items grow as the baseline nears 50%, and the band puts it there by
  construction — +10 there wants 705 paired items, not 155.

**Removed**
- Nothing deleted. The 4-cell first gate run was dropped before it entered the
  record — launched without `--reasoning-effort none`, so it was thinking-on and
  not the subject the smoke measured. "Five generators" rotated to the archive.

**Next up**
- **Resume the pass** — `harness run --resume results/cal-20260827T035804Z --yes`;
  ~5–10h. Then `calibrate --from` selects; no slate is written until every item is
  measured, by design.
- **The mandate is the blocker, not the slate.** `invoked` dominates on
  `engine-forced` (16/24). Read those transcripts before designing a grid around
  that arm; `bugs/009` is one cause and probably not the only one.
- **A partial read has pointed the wrong way three times** — trial 0 said native
  4/4 and structured 3/4; the full run said 58% and 83%. Wants to be a rule.
- **Open question:** `resume.fingerprint` does not cover the prompt, so today's
  `_fields_line` change is invisible to `resume.moved`.

## 2026-08-26 (later still) — A second subject, and four silent misconfigurations

`LocalSubject` ships: our own tool loop over an OpenAI-compatible endpoint, so the
weak end of the scale is reachable at last. **1,391 harness tests green (+19)**,
ruff clean, the 168-cell offline grid unchanged. The 432-cell local sweep it was
built for **finished, and is uninterpretable by its own pre-registration** —
`results/run-20260826T204936Z`, 3.5h, $0.00. Long form: [`experiments/notes/a-local-subject.md`](../experiments/notes/a-local-subject.md).

**Done**
- **`local.py`** — the loop, the tools spelled as the SDK spells them, the skill
  as a `Skill` tool advertised from SKILL.md's own frontmatter, `bash` under
  `unshare -rn`, stdlib only. `confine.violation` and
  `engine_use.program_from_call` are imported, so controls 3 and 4 hold by
  construction. Verified: `/etc/passwd` denied, the `truth.py` answer key denied,
  no network inside a cell.
- **Two tool protocols, crossed as strengths.** `harness run --local-model M
  --protocol P` sweeps them through the existing grid; `Strength` carries the
  endpoint, protocol and effort, so one subject instance covers the sweep.
- **The weak end needed three things before it could be measured at all**: the
  exit condition checked, the answer format *shown* rather than described, and
  `thought` on every structured action. Two move the instrument and say so.
- **Bounds that make an overnight run finishable** — wall clock, tokens per
  completion, conversation size — each a *stopping rule*, never an ERROR.
- **`Signals.ran_engine` contradicted `engine_use`** on any transcript that
  invoked the skill without running anything. Narrow now.

**Also written:** `experiments/hypotheses.md`, the pre-registration — one primary
endpoint (paired `engine-forced` − `prose`, `in-context`, weaker strength) and the
preconditions under which a run is *unreadable* rather than null.

**Decided** (`experiments/decisions.md`, five entries)
- **The tool protocol is a property of the model.** `structured` is a grammar the
  model cannot leave; `native` is a description it may follow — `qwen3:8b` called
  a tool whose only required parameter was `zebra` with `{"file": …}`.
- **The completion reminder and the format example**, both recorded as instrument
  changes: the first is an asymmetry with the SDK subject, the second changes the
  shared prompt for everyone.
- **A misconfigured instrument must refuse, not degrade** — the third instance,
  so it is a pattern.

**Removed**
- Nothing deleted. `runner.FATAL` was *replaced* as the only fatal classifier by a
  per-subject one, and the 2026-08-25 (later) worklog entry rotated to the archive.
  The plan's `at-scale`-by-construction rule was dropped before it was written.

**The sweep, read**
- **Both preconditions failed**: negative controls 18/72 = **25%** against the 75%
  floor, and mandate compliance 21/144 = **15%** against 80%. `hypotheses.md`
  fired on its first use and refused the endpoint. 10% correct overall.
- **`engine-forced` is catastrophic for a weak subject** — 2/144 correct, 96
  `no-answer`. Told it must run a program it burns the budget trying: 96 cells
  `invoked` the engine, 21 got an answer out of it. Left unprompted the `engine`
  arm reaches essentially never (1/144 `answered-from`, 113 `none`).
- **Structured decoding is a floor, not an improvement**: `llama3.1` and
  `qwen2.5-coder` go 0% native → 15%/12% structured; `qwen3` is 17% either way.
  Exactly what the enforcement asymmetry predicts.
- **Two claims of mine that the full run contradicted**, both made from ~16 cells:
  `no-answer` did not collapse (47%, against 55% before), and the format example
  moved failures from `unparseable` to `no-answer` rather than resolving them.

**Next up**
- **An 8B subject is not viable for this instrument**, and it is a capability
  limit, not a prompt one: of 205 `no-answer` cells, 76 took zero turns and 75
  looped to the cap. Neither responds to more nudging.
- **Run `qwen3:14b` at 16k with q4 KV** — pulled, measured at 9.07 GiB fully
  resident, and smoke-tested: **3/4 on the negative controls in one trial**,
  against 25% across 72 cells for the 8B models. Promising, and explicitly *not*
  the measurement precondition 1 asks for — this session twice read a trend off a
  handful of cells and the full run contradicted it both times. The server settings are in `experiments/AGENTS.md`; `--min-context
  16384` makes preflight and the overflow guard agree. **Correction to what this
  session said twice:** context is *not* free to trade for model size. The
  conversation sets the window, not the fixture, and the 8k a 14B fits in without
  KV quantization cannot hold SKILL.md (~3,200 tokens) plus a working
  conversation.
- Then **`harness calibrate` against whichever subject clears the controls**, at
  ~6 `(seed, difficulty)` combinations, before any grid.
- **A calibration pass that clears power**, which is the real blocker: 78 tasks
  are needed for a 10-point effect and the pinned slate is 28, `--repeats` does
  not buy paired items, and the pass has to be run **per subject** — a slate
  calibrated on haiku is not calibrated for an 8B model.
- **Open question:** whether an 8B subject clears the negative controls at all. If
  it cannot, a null on the measured slate stays unreadable however good the
  instrument is, and the answer is a larger model at a smaller window — the
  fixtures are ~275 tokens, so context is not the scarce resource here.
