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

## 2026-08-28 (later) — A 32-cell A/B, three instrument defects, and a floor that was not one

Asked for a medium A/B of `prose` against `engine-briefed`. Ran it — and getting
there cost three defects in the harness's own recording, all the same shape: a
real event the record could not express. Long form:
`experiments/notes/a-local-subject.md`.

**Done** — 10 commits, **1,480 tests** (+23, one red on trunk before today),
ruff clean, offline grid renders
- **The output cap was a stopping rule that recorded nothing.** A completion that
  spends its budget reasoning returns empty content; the loop called it a
  *malformed call*, said so — false — and retried into an identical lap at ~150s
  each. `finish_reason` decides now, `TRUNCATION_LIMIT` stops at two.
  **`malformed_calls` had been collected and dropped since it was written.**
- **Mean per-item F1 scored an empty cell 1.0**, flattering whichever arm fails
  by writing no file: prose read 0.92 against 0.19 on 2/8 against 1/8.
- **A subject that walks away read as one that finished** — fixed, and the
  reminder re-aimed. Not only a recording change: the next cell went
  `no-answer` → `correct`. Amended in place.
- **`engine_unusable` leaves the compliance ratio**, decided on the rule first.
- **Three runs**: a controls gate (prose 3/4, compliance 4/4, zero truncations —
  the counter's first check), sitting 1 (stopped by its gate), the clean 32.

**Decided** (`experiments/decisions.md`, six entries + two amendments;
`hypotheses.md`, two addenda)
- **prose 9/16 (56%) against `engine-briefed` 4/16 (25%)** — **−31 pts [−62, +0],
  1 win / 6 losses**, p = 0.125. Exploratory by pre-registration; reported, not
  claimed. `briefed − prose` is the recorded **upper bound**, so a negative one
  says the engine is losing to `grep` *with the manual in hand*. Worst on
  `recursion`, 0/4.
- **The pinned slate is not the floor it was written off as**: prose 1/24 on
  these exact items thinking-off, 9/16 thinking-on. **The missing rung may be a
  thinking-effort question, not a generator one** — the cheaper answer, and it
  was never on the table.
- **The subject is deterministic** (temperature 0.0), so `--repeats` buys
  neither paired items *nor* reliability. **Sitting 1 failed its own gate and
  was stopped**: half a run on a patched instrument is two experiments.
- **Not a `datalog` bug.** The briefed arm's worst cells burn out on the engine's
  silence, but `spec.md` §14 specifies exit 1 with no output deliberately and
  anticipates the case. It is a **`SKILL.md`** gap: what exit codes mean without
  *look at them*, and transitive closure with no worked example and no warning
  that the reflexive case is unsafe as a bare fact.

**Removed**
- Sitting 1's cap-hit as a difficulty signal, and the 42h projection's standing
  as a measurement of the subject — amended, not deleted.

**Next up**
- **Re-open the missing rung as a thinking-effort question**, before any
  generator work. The session's cheapest open lead.
- **A calibration pass is plausible now** — 56% is a band, 14% was not.
- **The `SKILL.md` gap**, both halves: most likely to move the briefed arm.
- Unchanged: the mandate arm's `invoked` split, the prompt-blind
  `resume.fingerprint`, `scheduling`'s own look.

## 2026-08-28 — The fixes took, and what they uncovered was tool use

Re-gated `engine-forced` on all three arms, launched the 240-cell pass, **stopped
it at 13 cells**, and built a fourth arm instead. The session's finding is one
sentence: *both arms are failing on tool use before the engine is in play.*

**Done**
- **The gate says the three fixes took** (`results/run-20260828T104124Z`, 12
  cells): **no cell at its budget in any arm** against `engine-forced`'s 48%,
  4–9 turns of 48, no fact-shaped answers, `engine_unusable` used once. prose
  3/4 = **75%**, precondition 1 at its boundary.
- **And what was under them**: all four `engine-forced` first programs invented a
  CSV loader — `read_csv/4`, `csv_load/3`, `csv_read_line/2`, `csv_read/4` —
  against `import "f.csv" as r.`, with **zero `Skill` calls** and the manual in
  the workspace.
- **A fourth arm, `engine-briefed`** — `engine-forced` plus `SKILL.md` in the
  prompt (`479067e`), gated at 16 cells (`run-20260828T132615Z`): **import syntax
  0/4 → 3/4, compliance 3/4 → 4/4**, accuracy 2/4 against 1/4 at n=4. Prompts
  nest as strict suffixes, same 2.0 budget: the pair differs by the briefing.
- **The slate-subject guard** (`faa8375`, `7060097`), the open question that said
  *do it before the grid*: five fields compared, `endpoint` excluded by name,
  containment not equality — and the manifest could not name the output cap at
  all until now (`MANIFEST_VERSION` 2). Plus: the mandate says the quotes come
  off. 1,457 tests green (+28), ruff clean, offline grid renders.

**Decided** (`experiments/decisions.md`, four entries; `hypotheses.md` addendum)
- **The 240-cell pass is parked**: 7 of the first 12 prose cells ended at the
  900s cap, projecting **42h not 7.5h** — the 7.5h had extrapolated four
  `controls` prose cells at 113s to a multi-hop pool. *A rate measured on the
  controls is not a rate for the slate.* A band selected now would rank items by
  how often prose runs out of clock.
- **The primary endpoint does not move onto the new arm.** `briefed − prose` is
  an **upper bound** on what the engine buys this subject, not S1: an agent
  handed the manual is more equipped than the one S1 describes.
- **Open**: whether `engine_unusable` counts as complying with the mandate —
  precondition 2 read 75% against an 80% floor and the missing cell was the
  mandate's own legal exit. *Decide on the rule, not on the number already seen.*

**Removed**
- The *"`MANDATE` should name the skill?"* open question, answered by building
  the arm instead. The ROADMAP's *"engine-forced writes Datalog it has invented"*
  item, superseded the same night. Nothing else: the parked pass's 13 cells are
  kept as the evidence for parking it.

**Next up**
- **The blocker moved.** It is no longer *a rung between controls and the slate*;
  it is that this subject's tool use eats the variable. A briefed cell still
  fabricated its fact base inline with the CSV beside it (22 turns, no answer).
- **Nothing is calibrated**, and no grid should run until the above is settled.
- Unchanged: the mandate arm's `invoked` split, the prompt-blind
  `resume.fingerprint`, `scheduling`'s own look.

## 2026-08-27 (later still, iv) — engine-forced was measuring its own turn cap

Asked why `engine-forced` could possibly score below `engine` when it has the
same tools and more instruction. It could not — the number was an artefact. Over
every run on disk the arm ended at its **turn cap in 48% of cells** against 16%
for `prose`, 75% of those writing nothing, **and nothing recorded it**. Its 5%
correct was a reading of the cap. Long form:
`experiments/notes/a-local-subject.md` (*What is wrong with engine-forced*).

**Done** — three fixes, `1,429` tests green (+17), ruff clean, offline grid renders
- **A per-arm budget** (`cell.ARM_BUDGET`): `engine-forced` gets 2× the turns and
  wall clock, because it must write, run, read, repair and re-run before it can
  answer. Its *successful* cells took a median 13 turns against `engine`'s 4 and
  topped out at 21 against a 24 cap — truncated, so 2.0 is a floor.
- **The mandate names the answer format.** `datalog` prints `answer("x").`; the
  contract wants `x`. Fact-shaped answers were 3.7% of `engine-forced` answers and
  0% of both other arms, all `wrong`. *Fix the instruction, not the ruler.*
- **`engine_unusable`**, a legal move in the engine arms with a required reason,
  graded `ENGINE_UNUSABLE`. The mandate forbids answering from anything else, so a
  subject whose program will not run previously had no move but to loop.
- **A capped cell now says so.** `_OUT_OF_TURNS`, matching `runner.STOPPING_RULE`.
  The wall clock and context guard always recorded theirs; the turn cap — the most
  fired of the three — recorded nothing, which is why this took three sessions.
- **Two hypotheses checked and killed**: the arm does *not* under-read the skill
  (203/343 vs `engine`'s 149/417), and it is not mis-built (base prompt plus
  `MANDATE`, test-pinned as a strict suffix).
- **First real cell after the fix**: 593s/12 turns → **183s/6 turns**, no cap, and
  a bare value instead of fact syntax. Still wrong; n=1.

**Decided** (`experiments/decisions.md`, two entries, two open questions answered)
- **A stopping rule that does not record itself is a silent truncation** — the
  fourth instance of this project's standing failure mode, and the first found in
  its own bounds rather than in a vendor default.
- ***Amended*: dropping `engine-forced` was the wrong call, on a premise that was
  the bug's own symptom.** Yesterday's costing made it 4.7× the items per night;
  617s/cell was a defect, not a price. It would have retired the primary endpoint
  to work around a bug. `engine − prose` stays a real secondary question.
- **The cost, stated:** an unequal budget is arm-asymmetric and sits on the arm
  carrying the primary endpoint, so a gain there now has two candidate causes.
  `report.py` prints cap-hit rate per arm; a result is readable only while it is
  low.

**Removed**
- Every prior `engine-forced` accuracy number. All three fixes change the arm, and
  those cells were measuring a cap — so nothing is lost that was worth keeping.
  The 2026-08-27 (later) entry rotated to the archive.

**Next up**
- **The arm's accuracy is now unmeasured.** Re-gate `controls` on all three arms
  before the pass, and read cap-hit rate first.
- Unchanged: the slate-subject guard before any `run --slate`; the 240-cell
  calibration; the missing rung; the mandate arm's `invoked` split.
