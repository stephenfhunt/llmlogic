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

## 2026-08-27 (later still) — `bugs/009`, the half that needed no new machinery

The first defect this harness produced *about the engine* gets its first fix. A
column-level type clash now names both terms as they were written, which is the
part that cost a local subject fifteen rewrites. **590 datalog tests green**,
clippy and fmt clean, and the pinned reference diagnostic is byte-identical.

**Done**
- **`Fixed { ty, witness }`** replaces a bare `TypeName` in the typechecker's
  union-find, so a class remembers the literal that pinned it. The three call
  sites with a literal — facts, constant atom arguments, literal operands — pass
  it through `print_value`, which already spells a symbol bare and a string
  quoted. That spelling difference *is* the diagnosis:

  ```
  - `employee` column 0 is used as both symbol and string (at 3:4)
  + `employee` column 0 is used as both symbol `name` and string `"Carol"` (at 3:4)
  ```
- **Three tests pin the new shape**, including the deliberate *exclusion* below.

**Decided** (`datalog/bugs/009`, not §17 — one normative home, and the bug file
is where a reader of this message will look)
- **The `union` form does not name terms, on purpose.** Its two sides are two
  slots, not two terms: a variable's class inherited its type from whatever
  pinned the column, so *"variable `Q` has type int `4`"* would read as Q's own
  value. It was written that way, seen to be wrong, and backed out;
  `a_variable_type_clash_names_types_without_borrowing_a_literal` fails if it
  returns.
- **The bug's own root-cause note was wrong and is corrected in place.** "The fix
  is one field" holds for the *term*, not the *position*: `Error` carries exactly
  one span and renders one `(at …)`, and the typechecker never sees the source,
  so a second position cannot go in the message text. And `ir::Fact` carries no
  span at all — it derives `Eq`/`Hash` as a set member (§17), so a span cannot go
  on `Fact` without changing fact identity or breaking 108 uses of `.facts`.

**Removed**
- Nothing. `bugs/009` stays **open** and both `#[ignore]`d criteria stay red:
  they assert positions, which this does not deliver. The 2026-08-26 (later
  still) worklog entry rotated to the archive.

**Next up**
- **A design pass on the diagnostic model**, which is what closes 009: related
  spans on `Error`, how `locate_all` resolves them, how they render and serialize
  for a future `--format json`, and what locates an **imported** row — §12 says
  source name plus row and column, not a program span. Needs a §12 amendment and
  a §17 entry, so it is a design session and not a patch.
- Unchanged from earlier today: the mandate arm, the missing rung between
  `controls` and the measured slate, and the prompt-blind `resume.fingerprint`.

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
