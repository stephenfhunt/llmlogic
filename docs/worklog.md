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
## 2026-08-18 — §6 is written, and the gap nobody listed is the one that moved the operator

The last blocking design session: §6 goes from an account of positive programs to
an account of the language, with `notes/declarative-semantics.md` carrying the
formal half. **No `src/` change, by design** — 386 lib tests unchanged, clippy and
rustfmt clean, which is the check that a descriptive session stayed descriptive.

**Done**
- **§6 rewritten** (37 → 86 lines): `T_P` over a **match relation** rather than
  substitution; §8 literals as **interpreted predicates**; an aggregate as a
  **fixed function from group keys to values**; the two finiteness claims
  separated, PTIME restored with them; the error rule; a footer down to what is
  genuinely out of scope.
- **`notes/declarative-semantics.md`** (199 lines) — match table, witness set and
  fold, the assembled operator, the PTIME argument; the §10/`termination.md`
  pattern, second use. **`testing.md`** gains a §6 row indexing the properties its
  claims already rest on, and the note that §6 adds none.
- **Swept**: `spec-traceability.md`'s §6 row (its "no date" paragraph was stale for
  §10 too); `termination.md`'s "two consequences worth stating in §6", discharged;
  §15's "least model" widened to "least, or perfect"; three `ROADMAP.md` sites.

**Decided**
- **A run that raises an error has no model** — the one real decision, describing
  `eval`'s `Result<Model>` rather than changing it. Not a smaller model and not a
  hole: the error is neither a truth value nor a missing fact. It is the **limiting
  case of the truncation contract**, and what stops the semantics erasing §8's line
  between an *unrepresentable* conversion (`absent`) and a *lossy* one. *Whether* a
  run errors is the program's; *which* error it names is the schedule's (B1).
- **`T_P` needs a match relation**: `p(absent)` is in `I`, yet a *bound* occurrence
  must never match it — **binding is total, matching is semantic**. One split, from
  which `p(X), p(X)` selecting less than `p(X)` and `p(X), not p(X)` deriving
  nothing both fall out rather than being rules of their own.
- **Two finiteness claims had been one** — what `bugs/004` found without naming it:
  `T_P(I)` is finite for *every* program; the **fixpoint** is reached only inside
  §10's certified fragment.
- **§6's footer ranked its own gaps backwards.** Aggregation, carried since
  2026-07-25 as the last unknown, cost one sentence; `absent`, listed as a peer,
  rewrote the **operator**, and nothing had flagged it. Annotated onto the
  2026-07-29 deferral: a footer ranks by what was noticed.

**Removed**
- §6's three-gap *Not covered* footer, the **"§6 was never extended"** ROADMAP item,
  `§6` from the hygiene section's title, and the backlog preamble's "the one
  remaining design session" — none is blocking now.
- The 2026-08-17 cross-engine benchmark entry rotated verbatim to the archive.

**Next up**
- **Profile the engine** — head of the queue, ranked leads in
  `notes/cross-engine-benchmark.md` (the derivation recorder first). Gates the
  aggregation rescan, the derivation-store swap, and parallelism.
- Elective, none blocking: **limit predicates** (now with a written baseline),
  **temporal types**, the §17 restructure.
- Open, *not* filed: two §6 claims have no property and cannot — `T_P(I)` finite,
  and PTIME. Metatheoretic; recorded in `testing.md` rather than softened away.
