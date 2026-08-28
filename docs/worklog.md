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

## 2026-08-27 (later still, iii) — Thinking back on, gated before it was adopted

`qwen3:14b` runs with `reasoning_effort` unset from the next pass on. **Gated
first**, all 4 controls × 3 arms (`results/run-20260828T012126Z`, 12 cells, 52
min): **9/12 = 75% per-trial against thinking-off's 24/36 = 67%** on the same
units, and the gain is in **`engine-forced`, 25% → 50%** — the arm the earlier
entry named the blocker. Long form: `experiments/notes/a-local-subject.md`.

**Done**
- **`--max-output-tokens`** (`ec7bdba`), because `max_tokens` bounds **reasoning
  plus answer**: at the 2,048 default a five-constraint puzzle spent the budget
  thinking and returned content that was not an action — invisible in the record,
  since ollama reports `completion_tokens` *excluding* the reasoning it charged
  against the cap. Recorded in the run's `local` block so a resume cannot truncate
  the second half. +2 tests, 1,412 green, ruff clean.
- **The window and the cap are one setting.** `CONTEXT_BUDGET` leaves 6,144 tokens
  for the reply at 24k against a 4,096 cap. At 16k it would be 4,096 against
  4,096 — the overflow guard and the output cap arriving together. Raising the cap
  without the window trades one silent truncation for another.
- **Thinking costs ~7.5× wall clock** — 605s against 80s for three `controls`
  cells. `engine-forced` measured 268/593/704/904s, the last being the 900s cap.
- **Not one of the three gate failures is a wrong conclusion**: a Datalog-shaped
  answer, a prose answer carrying the CSV header row, and the capped cell.

**Decided** (`experiments/decisions.md`, two entries + three open questions)
- **The next pass is 240 cells, not 1,125** — 4 packs × 2 seeds × difficulties
  1–2 × 3 trials, stub-verified. **~7.5h**: calibration draws the `prose` arm
  only (113s average) where engine-forced averages 617s. **`scheduling` is
  dropped** — slowest pack, capped in 43 of 75. Both difficulties kept to bracket
  the band, since thinking may make 1 too easy.
- **Compare per-trial with per-trial.** Precondition 1's recorded *"10/12 = 83%
  PASS"* is an **any-of-3** figure; those cells are 67% majority-of-3. Reading a
  one-trial 75% against it as a failed floor is a category error — made in this
  session before it was caught, and amended in place rather than quietly fixed.

**Removed**
- Nothing. The interim single-cell "prose regressed" read was contradicted by its
  own re-run and is kept as an amendment, not deleted — it is the session's one
  wrong turn and the reason the aggregation trap is now written down.

**Next up — the pass is ready to launch.** Server line and the 240-cell
`calibrate` line are in `experiments/AGENTS.md`, dry-run verified to record the
whole subject (`reasoning_effort: null`, 24576, 4096) so a resume rebuilds it.

- **Do the slate-subject guard first** — new open question, cheapest insurance
  here: a manifest records its calibrating subject and *nothing checks it*, so
  `run --slate` would run the grid under a different one silently. Four
  subject-bearing flags now, up from one.
- **The blocker is half-touched.** Thinking is the subject's side of the missing
  rung; whether it lifts difficulty 1 out of 14% is what this pass asks. If not,
  the generators still owe an easier form.
- **Still owed:** the mandate arm, the prompt-blind `resume.fingerprint`,
  `scheduling`'s own look, and the partial-read rule.

## 2026-08-27 (later still, ii) — q8 KV fits, and the ceiling was a default

A system update was reason to re-measure and the answer moved twice: `qwen3:14b`
holds **q8_0 KV** — the aggressive q4_0 is retired — at **24,576 tokens**, because
the ceiling that looked like the card was a default. Two `controls` smokes, 3/3
each, $0.00. Measurements: `experiments/notes/a-local-subject.md`.

**Done**
- **The residency probe said 16k, and the probe was reading a default.** At
  ollama's stock fit margin: 16k ✓ (9.70 GiB, 41/41, 31.1 tok/s); 18k, 20k, 24k
  all spill. That ~1.15 GiB the fitter declines to spend is
  **`LLAMA_ARG_FIT_TARGET`** — at 288 MiB, **24k q8 loads 41/41** (10.37 GiB) and
  holds under load. **24k is the config**: it leaves 0.86 GiB, narrower than the
  desktop, and the environment is being held still for the pass. `640` with a
  20,480 window is the setting that tolerates a browser.
- **The block formats were the other arithmetic error.** `q8_0` costs 1.0625 bytes
  an element — 32 values plus a 2-byte scale — so KV is **85 KiB a token**, not
  80. With weights at 8.23 GiB the model predicts every resident row to within
  0.03 GiB.

**Decided** (`experiments/decisions.md`, one entry, amended once in session)
- **A KV type is part of the subject** — as are the window, protocol and output
  cap. `run-20260827T015701Z` and the halted `cal-20260827T035804Z` are q4
  thinking-off: the next pass is not paired with them and cannot resume into one.
  Both were already rejected, so this costs nothing.
- **A vendor default read as a hardware limit is the fifth silent
  misconfiguration here**, after `OLLAMA_CONTEXT_LENGTH=4096` and the note's
  three. Corollary: grep the server's own `--help` before believing a limit.

**Removed**
- The note's Xorg desktop-VRAM table (`Xorg`'s 451 MiB is gone — Wayland now), its
  10.18 GiB q8 row, its "moving the display buys the q8 KV cache" conclusion (that
  lever now buys *window*, ~32k, and stays unmade), and the vLLM row's ollama
  column. Two older entries rotated to the archive.

**Next up**
- Superseded by the entry above.

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
