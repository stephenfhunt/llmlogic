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

## 2026-08-25 (later) — The instrument learns to discriminate

The 2026-08-24 grid returned +0 with every domain scoring *identically* in both
arms — not a null, three instrument defects at once. This session built the three
independent fixes. 417 harness tests green (+180), ruff clean, and
`harness run --dry-run --all` is now a **168-cell three-arm grid** rendering
intervals, paired tests and partial credit offline; nothing paid for yet. Long form:
[`experiments/notes/discriminating-instrument.md`](../experiments/notes/discriminating-instrument.md).

**Done**
- **A third arm, `engine-forced`.** `engine` keeps a byte-identical prompt to
  `prose` and so measures adoption *and* capability together; the new arm appends a
  mandate and measures capability alone. The report renders all three pairwise
  comparisons, each a different question.
- **Reach became an outcome, not a footnote** — `none` / `invoked` /
  `answered-from`, closing the open question an unexecuted-program `Skill` call
  raised. On `engine-forced` the same number is the compliance check.
- **`stats.py`, pure stdlib** — Wilson intervals, McNemar's exact test on the
  pairing the design already had, a bootstrap on the delta, per-item F1, `harness
  power`. It priced the third defect at once: a 10-point effect wants **155 paired
  items** against a slate of 56.
- **Two tracks and the first generator.** `access_control` gained
  `generate(seed, difficulty, track)`, `validate` rejecting degenerate items, and
  110 tests over 12 seeds — including the oracle against a second formulation *on
  generated graphs*, which the by-eye check on the pinned fixture never was. The 28
  pinned tasks are untouched, fingerprints asserted.
- **`harness score`**, `--repeats N`, resume refusing a fixture that moved, and
  `cache_creation_tokens` on the record.

**Decided** (`experiments/decisions.md`, five entries)
- **A delta with no interval is not a null.** The old report computed a raw
  percentage-point difference and stopped.
- **A slate is selected by calibration, not designed** — guessing which knob is
  hard is how the ceiling got built in the first place.
- **The token cap is one track's definition, not a global rule**, so `at-scale` can
  exceed the prose arm's window on purpose and is never averaged with the rest.
- **Control 3 keeps the arm it was written for**, which is what makes a third arm
  right rather than a prompt edit.

**Removed**
- `report.py`'s raw-difference rendering, and `_by_task` pooling, which averaged
  opus with haiku — the two subjects whose difference is why both are in the grid.
- `access_control/truth.py` rewritten whole after splicing left duplicate
  definitions; two ROADMAP items superseded by the `at-scale` track and the queued
  local subject; the 2026-08-24 S1 entry rotated to the archive.

**Next up**
- **Generators for the other six packs**, absorbing the owed `scheduling` re-run;
  then `harness calibrate`, and `hypotheses.md` before any grid is paid for.
- `LocalSubject` — the weak end is where the signal is, and two of the three
  in-scope claims are unmeasurable without it.
- **Open question:** `at-scale` answers run 1,800–6,300 rows, which measures
  transcription as much as querying. An aggregate variant was considered, not built.

## 2026-08-25 — Every diagnostic gets a code, and the last criterion closes

The §12 code vocabulary — designed, built and pinned in one session. **S3 flips
to met**, and with it **all six of §1's criteria hold**. 590 crate tests green
(+5), 568 under `--no-default-features`, 237 harness tests green (+7), clippy and
ruff clean.

**Done**
- **38 error codes over 153 emission sites**, by census rather than by naming
  what was convenient (`datalog/notes/error-codes.md`). The code is a **required
  constructor argument**, so every site had to be read — which is what produced
  the census — and the category derives from it. `Warning::code` too, one flat
  namespace across both.
- **53 messages stopped faking a code** — *"type error: "*, *"malformed IR: "*
  and two more: prefixes §12 forbade and nothing enforced, because until there
  was a code they were the only way to tell an overflow from a type clash.
- **Property C16** — every diagnostic carries a code from the pinned set — over
  `arb_corrupted_program_text`, the suite's **first generator that makes programs
  fail**; ten families across four categories. Plus the pinned code-set test.
- **The harness refuses a stale engine.** Both call sites checked only that
  `target/release/datalog` existed; `arms.require_engine` refuses one older than
  `src/`. Done first: the corpus re-pin depended on it.

**Decided**
- **A code exists where the *fix* differs in kind**, not where a message differs
  (§17 2026-08-25) — `Semantic` split into fifteen, four field diagnostics folded
  into one.
- **One fold was wrong and a test found it within the hour.** The conversion
  table classifies *no such conversion* and *would lose the value* differently,
  and went red the moment it read `error.code` instead of prose;
  `lossy-conversion` split back off. Nine assertions changed the same way — the
  crate's own tests are the first consumer to stop reading English.
- **No generic fallback code**, planned and then declined: there was no tail, and
  a generic code is where the next diagnostic goes without thinking.

**Removed**
- The four `Error::lex`/`parse`/`semantic`/`source` constructors, for one
  `Error::new(code, …)`. The 53 message prefixes. §2's stale claim that spans are
  lexer/parser-only — true until 2026-08-24, and not swept then. §12's *Not
  covered* clause about the code. `reference.py`'s duplicate `EngineMissing`. The
  2026-08-23 worklog entry, rotated to the archive.

**Next up**
- **Five items still carry a v1 tag** while §1 says the criteria are met — and
  three are about programs the engine **accepts**, while S3 is about *rejected*
  ones. Retag them or reopen the criterion: a user call, recorded in
  `datalog/ROADMAP.md`'s preamble. Nothing was retagged to make the flip clean.
- **Does the skill tell a subject the codes exist?** It does not, and adding it
  changes the experiment's instrument — a deliberate call, not an edit, and
  `--ablate` is how it would be measured.
- Still open: the `scheduling` re-run, count-distinct, `internal-error` under the
  wrong category, suggestion coverage (13 of 77), per-file attribution, §16/§17
  hygiene.
