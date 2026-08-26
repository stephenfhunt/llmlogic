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

## 2026-08-26 (later still) — A second subject, and four silent misconfigurations

`LocalSubject` ships: our own tool loop over an OpenAI-compatible endpoint, so the
weak end of the scale is reachable at last. **1,391 harness tests green (+19)**,
ruff clean, the 168-cell offline grid unchanged. A 432-cell local sweep is
**running as this is written** — `results/run-20260826T204936Z` — and the next session reads
it. Long form: [`experiments/notes/a-local-subject.md`](../experiments/notes/a-local-subject.md).

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

**Next up**
- **Read the sweep** in `results/run-20260826T204936Z`, then a local run of the calibrated
  slate. At 131 of 432 cells it stands at 11% correct against ~5% before this
  session's fixes, and `structured` leads `native` on all three models.
- **`hypotheses.md`** before any grid is paid for — and it now has to name the
  *subject*, since local and Anthropic numbers are not comparable to each other.
- **Open question:** whether an 8B subject clears the negative controls at all. If
  it cannot, a null on the measured slate stays unreadable however good the
  instrument is, and the answer is a larger model at a smaller window — the
  fixtures are ~275 tokens, so context is not the scarce resource here.

## 2026-08-26 (later) — The pass that picks the slate

`harness calibrate` — the consumer everything on 2026-08-26 was built for. A
generated pool runs once on the prose arm at the weak strength, the band keeps
what is not unanimous, and what it keeps is pinned to a manifest a grid runs
from. **1,353 harness tests green (+52)**, ruff clean, `harness run --dry-run
--all` still renders the 168-cell grid offline. **No paid pass has run**: the
slate has still never met a real subject.

**Done**
- **`calibrate.py`** — pool, tally, band, manifest. `harness calibrate` runs the
  pass and selects from it; `--from <run-dir>` selects again from a pass already
  paid for; `harness run --slate <manifest>` runs what it kept. The whole of it
  is exercised by `--dry-run` against the stub, which is what keeps a step this
  expensive tested.
- **Three trials per item**, because the band is empty at one: 0 and 1 are the
  only reachable accuracies, and both are outside [0.2, 0.8].
- **A pool that fails loudly**: building it is offline, so a degenerate item
  aborts rather than shrinking it, and two seeds agreeing in the four hex digits
  an id carries are refused by name before a cell runs.
- **A manifest holds provenance and a fingerprint, not fixtures**, and `load`
  refuses an item that no longer hashes to what the pass measured.
- **A resume knew one way to rebuild a grid and needed three** — pool, manifest,
  pinned slate (`cli._slate_of`). Writing it found that the rebuild had been
  **dropping `repeats`** all along: a repeated run resumed into trial 0 and then
  refused its own later trials as strangers. Nothing had owed one.

**Decided** (`experiments/decisions.md`, three entries)
- **A band needs three trials before it can be expressed at all**, and a failed
  cell is not a trial — *hard* is a keep on one of the two bands, so an ERROR
  counted as a miss selects an item because the instrument broke.
- **`at-scale` is calibrated too, and its band is a floor.** ~0 is what that
  track claims; the interesting direction is the rejection, since an at-scale
  item prose answers well did not defeat the arm it was built to defeat.
- **Selection is separable from collection**, so moving the band costs nothing.
  Re-running a stochastic subject to move it would be a different pass.
- The 2026-08-25 calibration entry is annotated with what building it taught:
  0.2–0.8 survived contact, the trial count and the second track did not.

**Removed**
- Nothing deleted: `cmd_resume`'s single rebuild path was *replaced* by
  `_slate_of`. The planned rule selecting `at-scale` by construction was dropped
  before it was written, in favour of calibrating that track too.

**Next up**
- **A paid calibration pass** — the default pool is 87 items in 261 cells at
  haiku. That is the first time the generated slate meets a real subject, and the
  first evidence about whether the items are informative rather than merely hard.
- Then **`hypotheses.md`** before a grid is paid for, and `LocalSubject`.
- **Open question:** if the pool comes back mostly *too hard* at difficulty 2–4,
  the knobs are mis-scaled rather than the band being wrong — and only the
  rejection histogram says which.

## 2026-08-26 — Five generators, and what only running them showed

The owed generators landed: **six of seven packs** now have
`generate(seed, difficulty, track)` beside `build()`, so `harness calibrate` has
a pool to select from. **1,301 harness tests green (+884)**, ruff clean, and
`harness run --dry-run --all` still renders the 168-cell grid offline. Long form:
[`experiments/notes/generating-the-slate.md`](../experiments/notes/generating-the-slate.md).

**Done**
- **The slate was pinned before anything moved.** All 28 fingerprints asserted in
  `tests/test_pinned_slate.py`: a fixture refactor changes what the comparable
  slate measures without changing a task id.
- **One home for the degeneracy rules** (`harness/generate.py`), a pack's
  `check` for what only it knows, and `domains.generate(name, …)` /
  `domains.validate(name, …)` as the entry points a calibration pass consumes.
- **`eligibility`, `ontology`, `scheduling`, `imports`, `controls`**, one commit
  each. Every fixture became a hashable value the oracle can be *handed* — which
  is what lets it be checked against a second formulation on generated data,
  rather than only on the fixture it was written for.
- **A fifth question per pack**, because every wrong answer to the pinned four is
  a **subset** of the right one and their shape cannot say which mistake was
  made. Each fifth is false in both directions.
- **`scheduling`'s `check` mechanises the 2026-08-24 defect**: every assignment
  is one its person could work. Hand-verification passed that fixture — proving
  the oracle matched the author's reading, not that there was one reading.

**Decided** (`experiments/decisions.md`, four entries)
- **`static_analysis` gets no generator**, and says so at the call. Its fixture
  is a fetched real package; a synthetic one trades away what the pack is for.
- **A control that gets hard stops being a control** — no `at-scale`, a row
  ceiling, four questions not five, and `validate` no longer calls a one-row
  truth guessable on a negative control.
- **A generator's defects are in its output distribution, not its control
  flow.** Nine degeneracies passed a reading of the code and were found by
  running the generators over 60 fixtures: `eligibility` d1 with one eligible
  applicant, `ontology` conflicts falling into classes reserved as empty,
  `imports`' universal quantifier satisfied by 864 of 864 customers,
  `scheduling` answering 3,534 rows at scale.
- **A big frozen value memoizes its hash** — an `at-scale` `Roster` rehashed
  60,000 rows per cached lookup; one test took eleven seconds, the file now 1.8s.

**Removed**
- `access_control`'s private `Degenerate`, `validate`, `_median_by` and its
  fingerprint test, folded into the shared homes. `imports`' seed-search comment,
  replaced by `_break_ties`. `scheduling`'s role-scoped `at-scale` helpers,
  unneeded once the background population worked one shift each. The 2026-08-24
  worklog entry, rotated to the archive.

**Next up**
- **`harness calibrate`** — the consumer. Everything it needs now exists.
- Then **`hypotheses.md`** before a grid is paid for, and `LocalSubject`.
- **Open question:** the generated slate has never met a real subject. The
  answer-size bands say the items are not trivially passable; only a calibration
  pass says whether they are *informative*.
