# Decisions log & open questions — `experiments/`

An **append-only record**: history is what it is for. Amend entries in place,
never rewrite them — a rationale that turned out wrong is the most useful thing
here, because it shows where the reasoning misleads. Everything outside this file
states present truth and merely *points* here (`../docs/rules/editing-docs.md`).

**Newest first.** **Amendment markers** are the repo's one vocabulary, defined in
[`../datalog/spec.md`](../datalog/spec.md) §17 — ***Falsified***,
***Superseded by***, ***Amended***, ***Reopened***, ***Consequences***,
***Answered*** — so the sweep after a rule changes is a grep across both logs and
not a judgement call. Not restated here: one normative home per rule.

Entries cap at ~15 lines; long-form goes to `notes/` and is linked.

## Decisions

- **2026-08-23** — **The reference corpus pins the engine's output *and* re-checks
  it against the oracle.** A pin alone records what the engine did; it cannot say
  whether that was right, and a pin over a wrong program actively defends the
  error. So `tests/test_reference_corpus.py` runs each program once and asserts
  twice: every relation against the domain's plain-Python truth, then stdout and
  stderr byte-for-byte.
  - **What it is a tripwire for**: the harness measures an agent against an
    engine that moves under it, and two grid runs are the same measurement only
    if the engine answered the same way in between. Nothing else here checked
    that. A pin that moves is therefore *not* a regression by default — it is a
    change in the instrument, to be read and then re-pinned deliberately
    (`harness reference --repin`), never by a red test regenerating itself.
  - **The malformed half is the half that earned it.** Correct programs cannot
    pin what the tool does when a run goes wrong, which is most of what a subject
    reads while getting its program right — and pinning five of them on the first
    day turned up `../datalog/bugs/008`, a diagnostic that makes a false claim
    about the fact table.

- **2026-08-22** — **The `static_analysis` corpus is fetched and pinned, not
  vendored, and it is Python.** A cell is sealed, so the tree has to be on disk
  before the run starts; `harness corpus fetch` downloads a pinned sdist
  (`sqlparse` 0.6.0, sha256-checked) into `~/.cache`, outside the checkout for
  the same reason workspaces are. Vendoring would put a third-party licence in a
  repo whose own licensing is deliberately unsettled.
  - **`sqlparse` over `requests`**: a heavily memorized codebase lets a subject
    answer from training instead of from the files, and nothing in the transcript
    would distinguish the two.
  - **Python rather than TypeScript**, which was the better *demo*: `tsc`'s API
    is the nicer extractor, but control 1 requires a plain-Python `truth.py`, so
    a TS corpus needs its oracle written over a hand-rolled TS parse. Stdlib
    `ast` gives an oracle that is right by inspection, and the sealed workspace
    already has it. Left as a post-v1 item.
  - **The questions define their abstractions syntactically** — "called" means
    the name is the callee of a call expression. Real name resolution is
    ambiguous (`../datalog/skill/recipes/source-analysis.md` says so at length),
    and a semantic oracle would be a guess the answers were then graded against.

- **2026-08-22** — **Parquet ships as a redundant copy of a text table, never as
  the only spelling of one.** The sealed workspace has system `python3` and
  nothing else — no pyarrow, no duckdb, no network — so a Parquet-only relation
  is a table the *prose arm cannot open*. Those cells would be decided by file
  format, and the delta they contributed would not be a reasoning delta.
  - So `imports` ships `order` as CSV **and** Parquet, the same rows in each, and
    `catalogue.verify` checks the schema of every spelling and requires the row
    counts to agree — otherwise the copy is decoration that can go stale.
  - *Cost, accepted:* `pyarrow` becomes a dependency, for a file the experiment
    never requires anyone to read.
  - *Rejected:* dropping Parquet from v1 (§13's Parquet path then goes untested by
    the instrument that exists to exercise it), and accepting the asymmetry.

- **2026-08-22** — **A question is tuned in the fixture, never in the grader.**
  Four cases turned up while building the five packs, each of which would have
  produced a plausible number instead of an answer: a roster whose shifts tiled
  the day cleanly had no double bookings at all; a region's "busiest month" was a
  three-way tie, so the question had no single answer; every applicant with a
  missing income also met every other criterion, so listing the blanks scored
  correct; and no applicant was under age, so one criterion of four never decided
  anything.
  - **Two instruments for it**: values *planted* over the seeded ones where the
    case is coverage (`eligibility`'s under-age applicant), and a *seed chosen by
    search* where the case is a property of the whole draw (`imports` re-draws
    until every region's busiest month is a strict maximum).
  - **The tests carry the conditions**, so a reseed cannot quietly lose them —
    `busiest_month_per_region` raises on a tie rather than picking one.

- **2026-08-21** — **Cells are contained, and containment is a validity control
  before it is a safety one.** The first build had none: `permission_mode` was
  `bypassPermissions`, `sandbox` was unset, and workspaces sat in
  `experiments/.workspaces/`. Verified by hand from a workspace, two things were
  reachable — `../../src/harness/domains/*/truth.py`, **the answer key**, and
  `../../../datalog/target/release/datalog`, **executable by absolute path**.
  Scrubbing `PATH` does nothing against an absolute path, so the arm separation
  was decorative.
  - **Three layers.** Workspaces moved out of the checkout (`~/.cache/...`); the
    OS bash sandbox is enabled with `allowUnsandboxedCommands: False` and the
    network denied outright; and a **PreToolUse hook** denies any tool call whose
    path leaves the workspace. The hook is the gate because it is the only one
    that fires: under `bypassPermissions` the SDK auto-approves every call before
    `can_use_tool` is consulted, and its own guidance says to use a PreToolUse
    hook instead. That also answers the open question about `allowed_tools` — it
    is not a whitelist under this mode, which is why `Skill` ran without being in
    it.
  - **The engine arm is self-contained**: the binary is hardlinked *into* the
    workspace, so confinement and the engine arm are not in conflict. A hardlink
    because 50 MB across a grid would be gigabytes.
  - **Denials are recorded, not swallowed.** A cell that kept trying to leave is
    a fact about the run, and a spike means the prompt or fixture made leaving
    look necessary — which is exactly what the first contained cell showed.
  - **The workspace directory is a hash, not the cell id.** Named after the cell
    it read `...who-can-read-r03.engine.haiku-4.5`, so a subject running `pwd`
    learned it was the engine arm of an experiment. Control 3 assumes it cannot
    know that.
  - **The subject is told its working directory** in the system prompt. Without
    it, it guessed — `/grant.csv`, then `/home/stephen/grant.csv` — burning five
    turns and five denials before asking. Telling it took the same cell from **20
    turns to 11**, with no denials and the same correct answer. The friction was
    the harness's fault, and left in place it would have been charged to the
    subject.

- **2026-08-21** — **Engine use is recognised by parsing the command, not by
  looking for the word.** The first real cell falsified the obvious
  implementation immediately: the subject ran
  `ls .../.claude/skills/datalog/`, and a substring test recorded that as **the
  first Datalog program it wrote**. Control 4 measures the program written before
  any feedback, so a false positive there does not add noise — it replaces the
  measurement with a directory listing.
  - **The same cell falsified the other half.** It wrote no `.dl` file at all,
    passing programs to the binary inline, so a `.dl`-suffix test read "never
    wrote a program" for a subject that wrote several. Both spellings now count:
    a file write, and source carried on the command line.
  - **One home for the predicate** (`engine_use.py`). It had been two — `agent.py`
    and `signals.py` — and the two disagreed, which is the drift mechanism
    `datalog/bugs/resolved/003` names.
  - **Invoking the skill counts as reaching for the engine**, and is the strongest
    form of it. The observed cell used a `Skill` call before ever running the
    binary.
  - *Consequence for the design:* the instrumentation could not have been settled
    on paper. One cell, twelve cents, falsified two assumptions — which is the
    argument for a smoke cell before a slate, not after.

- **2026-08-21** — **The answer contract is arm-neutral, and format failures are
  not wrong answers.** Both arms write `answer.txt`: one result per line, fields
  separated by `|`, order insignificant. Asking for facts would have handed the
  engine arm its native output format; asking for prose would have handed it to
  the other.
  - **`UNPARSEABLE` is kept apart from `WRONG`**, and the reason is bias, not
    tidiness. The prose arm writes sentences more often than the engine arm, so
    counting a sentence as a wrong answer would inflate the engine's margin —
    the one direction of bias this harness cannot afford. Found by a test, not by
    reasoning: the arity check that catches prose in a two-column answer cannot
    catch it in a one-column answer, where `o2` and a whole sentence are both a
    single field.
  - **So the fallback is the shape of the truth**: if no true value contains a
    space, a submitted value that does is prose. The guard disables itself on any
    domain whose answers legitimately contain spaces.
  - **An empty file is an empty answer, not a parse failure.** "There are none" is
    the correct answer to a whole question class — negation over a closed set —
    and grading it as malformed would have penalized exactly the questions S1 is
    about. This was live for one dry run before the grid showed it.

- **2026-08-21** — **Both arms are agents; the engine is the only difference.**
  S1 asks whether an agent is more accurate *with* the engine than reasoning in
  prose. The tempting shape — an agent for the engine arm, a text-in/text-out
  prompt for the prose arm — measures **tools vs. no tools** and answers nothing
  about the engine. So both arms are the same Claude Agent SDK subject, same
  tools, same workspace, same fixture files; the engine arm additionally has the
  `datalog` binary and skill.
  - **The prose arm keeps `bash` and may write a Python script.** That is the
    honest counterfactual: an agent's real alternative to a logic engine is not
    careful prose, it is ad-hoc code. If ad-hoc code wins, that is the finding,
    and a harness that forbade it would have hidden it.
  - *Rejected:* the `tsdl` shape (text-in/text-out, batchable at 50% cost, fully
    reproducible). It cannot produce our most valuable finding to date — the three
    questions answered with `grep` because the engine could not express them —
    because their subject has no `grep`
    (`../datalog/notes/tsdl-cross-project-review.md`).

- **2026-08-21** — **Ground truth is computed independently of the engine.** Each
  domain ships a plain-Python `truth.py`, and a test asserts none of them imports
  or shells out to `datalog`. If the engine grades itself, the engine arm is
  correct by construction and the entire run is void while still producing
  plausible numbers — the failure mode is silent, which is why it gets a test
  rather than a convention.
  - *Cost, accepted:* every domain is implemented twice, once as a Datalog
    question and once as a Python oracle. That is the price of the arm being
    measurable at all.

- **2026-08-21** — **Negative controls are part of the slate.** §1 says a negative
  S1 result is a finding rather than a failure to ship. That is only true if the
  instrument can produce one, and a slate of transitive-closure questions cannot:
  the engine wins by construction and the number means nothing. So `controls`
  ships single-hop lookups, tiny closed fact bases and one-step arithmetic, where
  the engine is expected **not** to help.
  - They also calibrate the null: without them, "no difference on this domain" is
    indistinguishable from a broken harness.

- **2026-08-21** — **Opus 5 and Haiku 4.5 as the two strengths.** The weaker arm
  is the informative one — the strongest model routes around gaps instead of
  falling into them, so a guide only it can follow is a guide that fails in
  production. Haiku 4.5 is also the cheapest arm ($1/$5 per MTok against Opus's
  $5/$25), so the informative half of the grid is the cheap half.
  - **Watch the context asymmetry.** Haiku 4.5 is 200K, Opus 5 is 1M, and on a
    large fact base the prose arm must hold the facts in context while the engine
    arm does not. A 22k-fact task would measure context, not reasoning. Fixtures
    are capped so the prose arm is never defeated by context alone;
    `static_analysis` at full size runs separately as a stated ceiling case.
  - *Rejected:* Opus 5 + Sonnet 5 (too close — a ceiling effect would tell us
    nothing about which doc lines carry weight), and one model at two efforts
    (cleanest control, but it does not answer whether a weaker model needs better
    docs, which is what the two-strengths control exists for).

- **2026-08-21** — **The harness is its own top-level project.** `experiments/`
  sits beside `datalog/`, not inside it: the repo has no shared build, `AGENTS.md`
  already anticipates Python projects getting their own directory, and a Python
  package nested in a self-contained Rust crate breaks that crate's own rule.
  - **The instrument's normative home moves with it.** `datalog/EXPERIMENTS.md`
    stops being the instrument and becomes the record of what the instrument
    produced; `spec.md` §1's S1 row points here.
  - *Noted:* the thesis is repo-wide — whether *agents reason better with formal
    logic engines* is not a `datalog` question — so measuring it from inside one
    project would have been the wrong altitude even if the build had allowed it.

## Open questions

- **What counts as "reached for it"?** Writing a `.dl` file is clear; asking the
  engine one question and then answering from `grep` is the case the signal exists
  for, and a boolean will not carry it. `signals.py` therefore records counts and
  no classification. Likely a small enum, decided against real transcripts rather
  than in advance.
- **How many rounds does a cell get?** The first program is recorded before any
  feedback regardless, but the *final* answer needs a stopping rule, and an
  unbounded agent loop makes cost unpredictable. Candidates: a fixed round cap, a
  token task budget, or the agent's own declaration that it is done.
- **Does a full run get committed?** Rendered reports and per-cell verdicts,
  clearly. Full transcripts are large and the repo already forbids committing
  session transcripts — but a verdict nobody can audit is a weak record.
