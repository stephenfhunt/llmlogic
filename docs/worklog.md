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

## 2026-08-31 — The ladder found no rung: Haiku is over the whole generator range

Asked for a small `prose` vs `engine` head-to-head on Haiku, to find where the
engine starts to pay along size and difficulty. The axis does not exist for this
subject.

**Done** — **1,508 tests** (+28), ruff clean, offline grid renders
- **`harness ladder`** — a slate chosen by construction and screened by nothing,
  the opposite of `calibrate`: *which* items land in the band is itself a fact
  about the rung. `MANIFEST_VERSION` 3 adds `selection`, so an unscreened slate's
  `trials: 0` cannot be read as *the subject answered none of these*.
- **`Record.difficulty`** and a **by-difficulty table** in every report, with
  median wall and USD per rung — the number that sizes the next grid.
- **Three instrument defects, each found by trying to use the thing:**
  **`ontology` generated a different fixture in every process** (a dict built
  from a set of strings fed `rng.choice` — the premise under *regenerated, not
  stored*, invisible to 1,480 tests because a process agrees with itself);
  **`--limit` and `--resume` could not be used together** though they are the
  documented shape of a run, the staged sitting recording its own size as the
  grid's; **the per-cell USD ceiling recorded nothing**, so `error_max_budget_usd`
  would have graded `ERROR` and `resume` would have owed the cell forever.
- **Three runs**: a controls gate (4/4 prose, 4/4 compliance, **no fabricated
  import syntax** — the 14B's failure did not transfer), rung 1 staged, and the
  88-cell ladder, halted by a session limit at 40 and resumed to 88.

**Decided** (`decisions.md`, five entries + one open question;
`hypotheses.md` addendum)
- **80/80 in both arms on the measured slate**, 8/8 controls, **44/44
  compliance**, zero cap-hits, zero truncations, $6.92. **`briefed − prose` is
  +0 points at every rung** — and it is the pre-registered *upper bound*.
- **Not a null about the engine; a statement about the slate.** Real, not a
  grading artefact: d5 `delete-without-read` is a 262-row closure-then-negation
  answer graded `missing=0 extra=0` against the plain-Python oracle, in 10 turns.
- **The blocker is bracketed on both sides now.** The 14B is under the whole
  range (14/7/5%), Haiku is over all of it, and there is no subject between them.
- **Prose holds 5–6 turns from d1 to d5 while the engine arm climbs 8 → 12.5.**
  Accuracy did not move along the axis; cost did.
- **Open: nothing records how the `prose` arm answered.** The founding decision
  names ad-hoc code as the honest counterfactual; `signals.py` counts the engine
  and the `grep` escape and not that. On this run it is the whole mechanism.

**Removed**
- Nothing. The 16-cell staged sitting (`run-20260831T011829Z`) stays on disk
  rather than being patched resumable: `results/` is append-only, and $1.26 is
  the whole cost of that rule.

**Next up**
- **`at-scale` is the only axis left untested** — the track that defeats the
  prose arm by construction rather than by structure, and the one place this
  subject cannot bring its own script to bear on the whole fact base.
- **A `wrote_script` signal before the next run**, not after: transcripts are not
  persisted, so it is unrecoverable for anything on disk.
- Unchanged: the `SKILL.md` gap, the mandate arm's `invoked` split, `scheduling`.

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
