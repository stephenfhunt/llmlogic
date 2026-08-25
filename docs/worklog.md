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

## 2026-08-24 (later) — Every diagnostic has a place, and the engine stops asserting what it never read

Three discovered defects, all fixed. **585 crate tests green** (+6), 230 harness
tests green, ruff clean, clippy clean — and the crate now passes
**`--no-default-features`** too, at 557.

**Done**
- **`bugs/008` closed**, and it was two defects. The suppression is the one the
  bug file asked for — skip the declared-vs-inferred sweep once inference is
  poisoned. The second was found while fixing the first and its own acceptance
  criteria missed it: the sweep says *"its values are T"* wherever inference
  contradicts a declaration, and inference reaches a column from **rules** as
  well as facts, so `declare p(x: int). s("a"). r(S) :- p(x: S), s(S).` made that
  claim about a relation holding **no facts at all**. The message now re-reads
  the facts before speaking about values. Both mutations recorded on **C15**.
- **Spans on semantic and source errors — the v1 gate.** All five malformed
  reference pins moved to carry a position, not the three the item asked for;
  **15 of 15** constructed diagnostic families carry one, against 0 of 113 sites
  that morning. Stages after parsing record a span and `api.rs` resolves it once.
- **`--no-default-features` is green**, and it was two `#[cfg(feature =
  "duckdb")]` attributes on tests that import date columns. Nothing else failed.

**Decided**
- **Resolve at the boundary, not by threading `&str`** (§17 2026-08-24). Lowering,
  inference and the evaluator never hold the program text; they record a span and
  `run_at_reporting` resolves every stage's errors.
- **A span stops at the file edge.** Spans are per-file byte offsets, so a program
  that spliced in a module **drops** them rather than print a confidently wrong
  line — `bugs/008`'s own class, freshly manufactured, and the reason to decline.
  Rebasing into one virtual text is the real fix; filed, not built.
- **The 113 sites were the wrong unit.** An error is raised deep and caught
  shallow: one wrapper around the evaluator's per-literal recursion covers
  `engine/`'s 35, one stamp in `load_imports` covers `sources/`'s 29. Six of the
  77 were `naive.rs`, which is `#[cfg(test)]` and reaches no user.

**Removed**
- `bugs/008` from the open set (→ `bugs/resolved/`), and `type-clash.dl`'s header
  note describing the false diagnostic it used to pin. §12's *Not covered* claim
  that no semantic error carries a span; §1's claim that spans are what scope S3.
  The oldest worklog entry rotated to the archive.

**Next up**
- **`harness reference` silently tested a two-day-old binary.** It runs
  `target/release/datalog` and only errors when that file is *missing*, so the
  first run this session reported 12/12 ok against yesterday's engine. A tripwire
  that can pass on stale evidence is the instrument defect class of the last
  three sessions — it wants a staleness check, or to build.
  **Shown twice, in both directions**: checking out an earlier commit and running
  `pytest` reddened 4 tests until the release binary was rebuilt, because the
  pins are versioned and the binary they are pinned against is not. So the corpus
  reports on whatever was last compiled, which at any commit may be neither that
  commit's engine nor the working tree's.
- **S3 now turns on the code vocabulary alone** — four `ErrorKind` variants where
  an agent wants to branch on `unsafe-aggregate`. That is the last thing between
  the criteria and v1.
- Still open: per-file error attribution, suggestion coverage (13 of 77), the
  cast-inside-a-comparison silence, `declare`-defines-a-predicate, and re-running
  the repaired `scheduling` domain under a new run id.

## 2026-08-24 — S1 is measured, the answer is null, and v1 moves to S3

The first full 112-cell grid finished, across three session windows. **S1 flips
to measured** (`datalog/spec.md` §1) — the criterion asks that the experiment was
*run*, and it was. 230 harness tests green (+7), 579 crate tests green, ruff clean.

**Done**
- **The grid ran to completion**: 112 cells, 0 errored. Both arms **41/48**, or
  **38/40** with the void `scheduling` domain struck out; controls 8/8 in both, so
  the null is not a broken instrument. — `results/run-20260824T104501Z/`.
- **Two instrument defects, both found by running it.** A session limit was filed
  as 46 ordinary `no-answer` cells under a report saying "0 errored" — fixed by an
  `ERROR` verdict, halt-on-fatal, `--resume` and `--limit`. That fix then
  over-reached onto the harness's own `max_turns` cap, which would have
  **rewarded a model that flails**; `FATAL` and `STOPPING_RULE` now differ.
- **`scheduling` was repaired.** 21 of its 29 assignments were ones the person
  could not work while every question's preamble stated that rule, so applying the
  rule first deleted every clash — three of four `double-booked` cells wrote an
  empty file, correctly.
- **S3 captured as the v1 gate**, re-measured rather than re-read: §2 and §12
  carried 46 + 33 spanless error sites from 2026-08-18; it is **77 + 36**, still
  0% against 3 of 3 for lex and parse.

**Decided**
- **A failed cell is `ERROR` and excluded**; the turn cap is not a failure and is
  graded on what it left behind. **The budget is the five-hour window, not
  dollars.** — `experiments/decisions.md` 2026-08-24.
- **The null is about *supplying* the engine, not using it.** The engine arm
  reached in **9 of 56 cells** — opus 1, haiku 8 — and every incorrect answer on
  the live slate came from a cell that wrote no program.
- **Hand-verifying 28 tasks against their oracles proved the oracle matched the
  *author's* reading**, not that there was only one reading. The subjects were the
  first readers who did not already know the answer.
- **v1 now turns on S3** (§17 2026-08-24): what scopes it is spans, so a semantic
  diagnostic points at a name — `variable Q in rule 0` — not a place.

**Removed**
- `ROSTER`, the glued-on preamble behind the `scheduling` defect, for two narrower
  constants. README's "$35–40 per grid", a bill that does not exist. The oldest
  worklog entry rotated to the archive.

**Next up**
- **Spans on semantic and source errors** — the v1 item: `lower.rs` (26 sites) and
  `engine/mod.rs` (35), done when the three `Semantic` programs in
  `experiments/reference/malformed/` gain a position.
- **Re-run `scheduling` under a new run id** — repaired, and its 2026-08-24
  numbers are void. Cannot be resumed into the finished grid.
- **The slate is at a ceiling**: opus is 20/20 in prose on the live domains, so no
  delta is detectable downstream of reach. Harder questions, or the parked
  **local-model subject** — the signal lives at the weak end.
