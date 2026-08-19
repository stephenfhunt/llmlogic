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

## 2026-08-18 — Taking stock at feature-complete: three holes the backlog could not list

Asked, after §6 shipped and `bugs/` went empty, whether the language has major
design holes. The method was to **not** read `ROADMAP.md` first — a backlog lists
only holes someone already noticed — and to read §§1–15, `src/main.rs` and the
exported API instead. Seven findings in `notes/taking-stock-2026-08-18.md`; three
were untracked, four tracked and mis-ranked. No `src/` change, nothing ratified.

**Done**
- **`ROADMAP.md`**: a new section, **the caller's contract (§12/§14/§15)**, holding
  the two untracked items; **§1/§2 promoted out of "spec hygiene"** into a section
  of its own; five annotations (the truncation merge, provenance's joint decision,
  import row anchoring, temporal's ranking, the EXPERIMENTS reframe); the preamble
  now says the order is under review rather than "the profile is the next item".
- **`spec.md`**: §14's *Not covered* gains **what the exit code means** (every run
  that completes exits `0`, so "no rows" and "answered no" are indistinguishable
  without parsing stdout), §11's that an imported fact is anchored by its
  **relation, not its row**, and §17 one open question covering both.
- **`datalog/AGENTS.md`'s "CLI/REPL" corrected** — there is no REPL, and it was
  claimed twice in the file every session loads.

**Decided**
- **Nothing, deliberately.** The findings are recorded as a recommendation because
  ratifying **§1** is itself step one: "are we feature complete?" is not answerable
  against goals, non-goals and success criteria that have never been written.
  Filing §1 under *spec hygiene* was the category error that hid this — it is not
  tidiness, it is the definition of done, and its absence is why "is X in scope"
  keeps getting settled ad hoc at the moment it is asked.
- **Integrity constraints and the truncation contract are one design.** There is no
  way to say *this must never happen*, and the contract's one open piece already
  says withholding "has to mean an exit code and a stdout discipline". Two needs,
  one exit code; designing them apart gives it two vocabularies.
- **Pillar 1 pays full price and returns nothing at the surface it exists for.**
  The recorder is unconditional and the top profiling target (13× measured on a
  sibling engine's cyclic graph); `?why` is unbuilt and absent from the exported
  API. The backlog carried this as two items in two sections and never as one fact,
  so whichever gets settled first would have silently constrained the other.
- **The project's hypothesis has never been measured**, while §1 cites the three
  pillars as "settled authority" for every §17 decision. `EXPERIMENTS.md` was filed
  as skill polish; it is the validity question, and could reorder all of the above.

**Removed**
- The backlog preamble's "**the profile is the next item**", and the *Ratify §1 and
  §2* bullet out of the Spec hygiene section — both moved rather than deleted.
- `AGENTS.md`'s two "CLI/REPL" claims, which had stopped being true.
- The 2026-08-17 named-query entry rotated verbatim to `worklog-archive/2026-08.md`.

**Next up**
- **§1 (and §2's remaining four)** — short, and it makes every other ranking here
  decidable rather than arguable. Then **the caller's contract**, then **temporal
  types**, then **the profile** with pillar 1's question attached rather than
  trailing it. The sequence itself wants ratifying first.
- Open, in §17: whether a constraint is a language construct or a query convention,
  how many exit codes the vocabulary needs, and whether stdout stays a pure fact
  stream when a run has something to say and no rows to say it in.
