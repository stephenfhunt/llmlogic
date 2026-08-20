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

## 2026-08-19 — Temporal values ship, and a builtin turns out to be a relation

S4 read "met **except dates**"; it now reads met. Three primitive types with
`@`-sigilled literals, one arithmetic rule, and `std/time` behind a gate. Design
and rejected alternatives in
[`notes/temporal-values.md`](../datalog/notes/temporal-values.md); two §17
decisions, both annotated with what building them taught the same day.

**Done**
- **`src/temporal.rs`** — civil dates, civil timestamps, exact durations, and
  Hinnant's calendar algorithms. **Zero new dependencies**, which is affordable
  only because the design excludes zones and calendar durations.
- **§8's algebra as one rule** — points and vectors — replacing a table to
  memorize. `duration / duration → float` is the only route from a duration to a
  number, so the sibling engine's `172800000` finding is excluded *by
  construction* rather than by a paragraph in a guide.
- **`std` modules**: `std/` is a reserved virtual path prefix; a builtin is a
  **relation**, which is what dodges the `ident (` ambiguity that ruled out
  `float(A)`. `std/time` ships; `std/math` and `std/text` are designed, not built.
- **§13 types temporal columns** — CSV by the literal grammar, Parquet from its
  declared type. §16.14 is the worked example, with a system test and a
  pipe-it-back-in test.
- **Properties T1–T6**, all six with mutations recorded.

**Decided**
- **The sigil is decided by §14's closure, not taste.** Output must re-parse, so
  a computed date needs a spelling. Bare ISO was rejected because `2026-08-19`
  already evaluates to `1999`.
- **`timestamp as date` stays a lossy error**, with `truncate` named as the fix —
  the one tension resolved *for* an existing rule. §16.14 records the cost.
- **The gate buys the short names**, not safety: `year`/`month`/`day` are the
  names a program wants *and* the names a column has.
- **A duration is never inferred from any source** — reversing what §13 said this
  morning about `INTERVAL`. Reading DuckDB's `1 day 02:00:00` would mean a second
  duration grammar, and one grammar is what keeps reading and rendering inverse.

**Removed**
- §13's "date/time-like types become their ISO text as strings", §4's *Not
  covered* temporal clause, `duckdb.rs`'s VARCHAR cast for `DATE`/`TIME*`, and
  §8's scan-ahead candidate for the `ident (` ambiguity — **withdrawn**, not
  parked: a relational spelling means the ambiguity never arises.
- `print_type`'s duplicate type-name list, now `TypeName::keyword`'s.

**Next up**
- **The profile**, with pillar 1's question and the row-provenance trade attached
  — the last of the stock-take's four, and `EXPERIMENTS.md` still sits alongside
  it as the thing v1 is defined against.
- Open, in §17: period arithmetic ("same day next month" is not expressible),
  whether a truncated value should print as its period, and the `avg`-over-mixed
  column question T5 does not reach.

## 2026-08-18 — The caller's contract ships, and the thing it was merged for has no trigger

The exit code now answers the question. Five axes taken one at a time
([`notes/callers-contract.md`](../datalog/notes/callers-contract.md)); **two were
settled by measurement, and both measurements contradicted a document**. 465 tests
pass, clippy and rustfmt clean.

**Done**
- **grep's vocabulary**: `0` rows found · `1` no rows · `2` did not answer, stated
  as a **range** (`≥ 2` did not answer) so a later code refines `2` rather than
  reinterpreting it. Program errors moved `1 → 2`. **§16.13** is the first example
  whose subject is the exit code; `tests/system.rs` pins every code, plus the
  boundary that "any query decides".
- **Two silences ended**: a conversion that loses a value reports *malformed, not
  missing* (§4/§12), and an aggregate in a **query** now reports its skipped
  absents — §9's documented blind spot, whose stated cause was wrong (`answer`
  builds the premises and discarded them).
- **Swept**: §9's exclusion, §12's severity and *Not covered*, §14's binary
  contract, §15's truncation paragraph, §4's cast section, `ROADMAP.md` ×4,
  `testing.md`, `skill/SKILL.md`, `docs/agent-skill.md`.

**Decided**
- **A constraint is not a construct.** The language already writes the check — a
  negation-only body answers `holds(true).` — so only the code was missing.
  *Rejected*: a `constraint` statement (buys no expressiveness) and the ASP denial
  `:- body.` (its meaning is a model filter; we compute one model). Phrase checks
  **affirmatively**: errors are `≥ 2`, so `&&` cannot fire on a broken program.
- **Withholding is specified and unnumbered.** §17 2026-08-16 named three live
  truncation sources; measured, **none is one** — §13's cells are structured
  errors, the round cap is a test oracle, and §9's skips are absent *values* in
  present rows. **Short by rows is not absent in a cell**, and the entry never drew
  that line. So the merge that made this one session was right for a reason it
  could not state: the two needs never contended for the code.
- **stdout stays a pure fact stream.** A `%` marker was rejected as visibility
  without capability — a downstream lexer skips comments, so it cannot make a pipe
  safe, only look safe. The residual hazard is recorded in §14 rather than left to
  lore: `a | b` still cannot see an exit code without `pipefail`.
- **The new report needed a silencer.** §16.9's guarded idiom (`V = X as int, V is
  absent`) is a *correct* program the warning fired on — the exact hazard the
  sibling engine measured — so a guarded conversion is silent.

**Removed**
- §9's "not covered by the warning: an aggregate appearing only in a query", §14's
  "what the exit code means beyond 'it ran'", §15's three-live-instances claim, and
  the ROADMAP's integrity-constraint and reclassification items. The 2026-08-18 §6
  entry rotated verbatim to `worklog-archive/2026-08.md`.

**Next up**
- **Temporal types**, then **the profile** — unchanged by this session.
- Filed while building: **a conversion inside a comparison is still silent**
  (`X as int > 5` narrows a filter with no premise to report on) — v1, §8/§12.

## 2026-08-18 — §1 is written, and three of §2's four principles ratify *scoped*

The definition of done, taken first on the stock-take's recommendation. §1 now has
goals, non-goals, target users and six success criteria; **v1 = S2–S6 hold and S1
has been measured at least once**; every open backlog item is ruled v1 or post-v1
against those criteria — **17 v1, 19 post-v1**, argued item by item in
[`datalog/notes/v1-scope.md`](../datalog/notes/v1-scope.md). **No `src/` change, by
design** — 386 lib tests unchanged, clippy and rustfmt clean.

**Done**
- **§2 ratified**, and the interesting part is *how*: three of the four went in
  **scoped to what the implementation delivers**, each checked against source
  before being written. The measurement that did the most work — spans are attached
  in the **lexer and parser only**, and *all* 46 `Error::semantic` and 33
  `Error::source` sites carry none. §12 said "many"; it is all of them, and §12 now
  says so.
- **§1 written** (12 → 94 lines): pillars unchanged, two goals beyond them, five
  non-goals each pointing at the §17 entry that settled it, three target users **in
  priority order**, and S1–S6 with an instrument and a today-status per row.
- **`ROADMAP.md`** tags each item v1/post-v1 on an axis orthogonal to status;
  **swept** §12's span claim and `EXPERIMENTS.md`'s header, now S1's instrument.

**Decided**
- **v1 requires S1 to have been *measured*, not to have come out favourably.** The
  repo exists to test a hypothesis, so a negative result is a finding; shipping
  having never run the experiment would leave the three pillars — cited throughout
  §17 as settled authority — resting on an unmeasured premise. That is what moves
  `EXPERIMENTS.md` from near the bottom of the backlog to a v1 item.
- **A principle is a claim about the implementation: ratify it scoped or not at
  all.** The sharp case — *explainability and the agent API are first-class* is
  true of the engine (all derivations, unconditionally, no flag) and **false at the
  surface** (`?why` unbuilt, `RunResult` carries none, the CLI's only flag is `-q`).
  As written it would have made §2 assert what §11 and `src/lib.rs` contradict.
  *Rejected: leaving the four as candidates* — a permanent candidate constrains
  nothing.
- **The criteria were tested against the ruling, not only used for it**: if a
  ruling cannot be derived from a stated criterion, the **criterion** is missing.
  It fired once — S4 went from "met" to "met **except dates**", which is what makes
  temporal types v1 rather than a preference.

**Removed**
- The whole *Ratify §1 and §2* ROADMAP item, superseded by shipped text; the
  backlog preamble's "**the order below is under review**"; §1's and §2's *Not
  covered* footers, which existed only to say the sections were unwritten.
- The 2026-08-18 termination entry rotated verbatim to `worklog-archive/2026-08.md`.

**Next up**
- **The caller's contract** — constraints + exit code + the truncation contract's
  open half; one session, one vocabulary. Now v1 by S1 and pillar 3 rather than by
  recommendation. Then **temporal types** (S4), then the **provenance surface** as
  one session with the recorder-cost and row-anchoring calls, which the **profile**
  feeds.
- **`EXPERIMENTS.md` as a harness** is v1 and sits *alongside* these, not after:
  until it runs, v1 is undefined rather than unfinished.
- Open, in §17: what a caller learns from a run that completed — unchanged, and the
  caller's-contract session is what answers it.
