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

- **2026-08-26** — **A local subject, built; and the tool protocol is a property
  of the model, not a design choice.** `LocalSubject` is our own loop over an
  OpenAI-compatible endpoint, stdlib only. `confine.violation` and
  `engine_use.program_from_call` are imported rather than reimplemented, so
  controls 3 and 4 hold by construction, and the tools are spelled as the SDK
  spells them so `signals.py` transfers untouched.
  - **Two protocols, both kept.** `structured` constrains the decoder to a
    discriminated union and is a **grammar the model cannot leave**; `native`
    sends a `tools` array, which is a **description it may follow** — given a
    tool whose sole required parameter was `zebra`, `qwen3:8b` called it with
    `{"file": …}`. `qwen2.5-coder:7b` cannot emit a parseable native call at all.
  - **The skill is a `Skill` tool advertised with SKILL.md's own frontmatter,
    verbatim.** Writing a fresh description would tell the local subject
    something the SDK subject was never told — control 3, lost where nobody would
    look. — `notes/a-local-subject.md`

- **2026-08-26** — **The harness checks the exit condition, and that is a
  deliberate asymmetry with the SDK subject.** Read the transcripts and the weak
  models mostly are not failing to reason: on `qwen3:8b` the thought is *"Carol
  is listed under the 'engineering' department"* and then it finishes without a
  `Write`, because in conversation saying the answer **is** delivering it. 55% of
  a sweep graded `no-answer`, much of it right and filed nowhere.
  - **At most twice, naming only the file.** A subject that ignores two reminders
    will not write it on the third, and the turn cap should not be spent finding
    out. The text says nothing about the answer.
  - *Consequence, accepted:* the SDK subject gets no such nudge, so local and
    Anthropic numbers are not comparable **to each other**. They were not anyway —
    different models, different drivers. What must stay comparable is prose
    against engine *within* a subject, and the reminder is identical on every arm.

- **2026-08-26** — **The answer format is shown, not only described, and that
  changes the prompt for everyone.** *"Each line has the fields `order_id`,
  separated by `|`"* names one field and implies several; a frontier model papers
  over it and a weak one cannot. `qwen3:8b` found the right three orders and wrote
  `o2|150`; `llama3.1:8b` wrote the field name as the value six times.
  - **The example is derived from each task's `answer_shape`**, not fixed: a
    two-field example shown to a one-field question is an invitation to add a
    field, which is the mistake being fixed.
  - *Consequence, accepted:* runs before this are not comparable with runs after.
    There is no valid measured grid to lose — 2026-08-24 is void for three other
    reasons — and control 3 is untouched, since the example names no engine and no
    value from any fixture.

- **2026-08-26** — **Every bound on a local cell is a stopping rule, never an
  error.** A cell is bounded in wall clock, in tokens per completion, and in how
  much of the window its conversation may fill. All three record like the turn
  cap: the cell is graded on what it left behind.
  - **Because an ERROR cell is one `resume` owes forever.** It would hit the same
    bound on the next sitting, and the run could never finish. That is the trap
    the wall clock fell into first and the request timeout fell into second, in a
    place the first fix had not looked.
  - **The conversation is what fills the window, not the fact base.** Largest
    fixture in the sweep, ~275 tokens; worst cell, 762,441 input tokens over 30
    turns. A `Skill` call appends ~3,200 tokens and a `Grep` up to 6,000, so a
    loop re-appends them until the server shifts the question out of the far end —
    and the subject then answers a question it can no longer see, which reads
    like reasoning. — `notes/a-local-subject.md`

- **2026-08-26** — **A misconfigured instrument must refuse, not degrade.**
  Models declaring 32,768 tokens were served at ollama's default 4,096, applied at
  load time and invisible from the request side. Every cell of a sweep, and the
  whole model survey before it, ran in a quarter of the assumed window and
  produced plausible, worthless numbers. `preflight` now reads what a model is
  **actually loaded with** and refuses below what the run assumes.
  - **Best-effort, and it says so.** vLLM has no `/api/ps`; a check that cannot be
    answered returns nothing rather than inventing a refusal.
  - **This is the third instance, so it is a pattern and not luck** — the stale
    engine binary (2026-08-25) and the fixture that moved under a resume
    (2026-08-24) are the others. This harness's expensive failures are silent, and
    a preflight check pays for itself the first time it fires.

- **2026-08-26** — **A band needs three trials before it can be expressed at
  all.** `harness calibrate` keeps items whose prose accuracy lands in
  [0.2, 0.8], and at one trial per item the reachable accuracies are 0 and 1 —
  both outside it, so the pass would have selected nothing and said the pool was
  degenerate. Three trials make the band reachable at ⅓ and ⅔, which is the
  cheapest arithmetic that expresses *not unanimous*.
  - **Coarse on purpose.** This is a screen run before the money, not the
    measurement: the grid is where `--repeats` and `stats.mcnemar` live, and
    spending five trials to sharpen a decision that only has to sort items into
    two piles buys precision the next step does not use.
  - **A failed cell is not a trial**, and this is where that rule bites hardest.
    An API error counted as a miss makes an item look hard, and *hard* is a keep
    on one of the two bands — an item selected because the instrument broke.
    `Outcome.rate` raises on zero trials rather than returning 0.0 for the same
    reason.

- **2026-08-26** — **`at-scale` is calibrated too, and its band is a floor.**
  That track exceeds the prose arm's window by construction, so the middle band
  would reject the whole of it. The rule there is `rate ≤ ⅓`: prose scoring ~0 is
  what the track *claims*, and the pass is what turns the claim into evidence.
  - **The rejection is the interesting direction.** An at-scale item the prose
    arm answers well is a defective at-scale item — its fixture did not defeat
    the arm it was built to defeat — and nothing but running the pass can say
    which items those are. Selecting the track by construction was the
    alternative, and it would have kept exactly those items.
  - *Consequence, accepted:* the two tracks are selected by different rules and
    reported in separate tables, which is the same split `decisions.md`
    2026-08-25 made for the grid. A single pooled number across both would mean
    nothing in either direction.

- **2026-08-26** — **A slate is a manifest of provenance, not a copy of the
  fixtures.** What `calibrate` pins is `(pack, seed, difficulty, track, id)` plus
  a `resume.fingerprint`; `harness run --slate` regenerates each item and refuses
  one that no longer hashes to what the pass measured. Storing the fixtures would
  make a slate reproducible without making it *checkable* — the copy would still
  be there after a generator's threshold moved, and the grid would measure a slate
  nobody calibrated.
  - **The rejected items are recorded too, with the rate that rejected them.** A
    pass that keeps nothing is a finding about the pool, and a manifest that says
    only what it kept cannot distinguish *too easy* from *never ran*.
  - **Selection is separable from collection.** `calibrate --from <run-dir>`
    re-reads a pass with a different band. Re-running the subject to move a band
    would quietly be a different pass, since the subject is stochastic.
  - *Consequence:* `cmd_resume` had one way to rebuild a grid and now needs three
    (`cli._slate_of`). Finding that turned up a second defect it had all along —
    the rebuild dropped `repeats`, so a repeated run resumed into trial 0 and
    then refused its own later trials as strangers.

- **2026-08-26** — **`static_analysis` is the one pack with no generator, and
  says so.** Its fixture is a fetched, pinned copy of `sqlparse`; there is
  nothing to seed. A synthetic package would have given real difficulty knobs and
  an `at-scale` track, and would have traded away the property the pack exists
  for — *real third-party code*, which is where the `grep`-escape finding came
  from and where "the agent extracts its own facts" stops being a simulation.
  - **Stated rather than discovered at call time.** `domains.NO_GENERATOR` holds
    the reason and `domains.generate` raises it; `domains.generators()` reports
    presence of `generated`, so a pack that is *deliberately* without one reads
    differently from one that has not got round to it. A calibrated slate quietly
    missing a domain is the 2026-08-24 failure mode in different clothes.
  - *Consequence, accepted:* `static_analysis` keeps its four pinned tasks and is
    outside whatever `harness calibrate` selects. The comparable slate still
    carries it.

- **2026-08-26** — **A control that gets hard stops being a control.**
  `controls` gets a generator — a calibrated slate needs controls of matching
  provenance, or a null is again indistinguishable from a broken instrument — but
  it is flat by construction: names and values vary, the table grows by a few
  rows to `MAX_ROWS`, and the number of hops never moves.
  - **No `at-scale` track, refused with a message.** A single-hop lookup in a
    fact base nobody can hold is a question about *retrieval*, and a positive
    delta there has an innocent explanation the null exists to rule out. It was
    considered as a genuinely interesting third thing to measure, and declined
    because it would be measured in the column that reads as the null.
  - **Four questions, not five**, and `generate.validate` stopped calling a
    one-row truth guessable on a negative control: the rule exists so an item
    carries information about whether the engine helped, and a control is the
    item that is deliberately trivial. — `notes/generating-the-slate.md`

- **2026-08-26** — **One home for the degeneracy rules; a pack adds only what
  only it knows.** `access_control` proved *empty truth*, *the whole universe*
  and *a single guessable row* generic, and six more copies of one rule is the
  drift mechanism `../datalog/bugs/resolved/003` names. They live in
  `harness/generate.py`; `<pack>.tasks.check` holds the rest, and
  `domains.validate(name, task)` runs both.
  - **A pack's `check` holds what a person used to supply by reading it.**
    `scheduling` asserts every assignment is one its person could work — the
    2026-08-24 defect, mechanised. Each such invariant was true of the
    hand-written fixture only because somebody had read it.
  - **The second formulation is the emitted file wherever a format is the
    trap** — `applicant.csv`, `shipment.jsonl`, the Parquet copy — not a second
    pass over the tuples that wrote them.
  - **A generator's defects are in its output distribution, not its control
    flow.** Every degeneracy fixed this session passed a reading of the code and
    was found by running it over 60 fixtures and printing the answer sizes.
    The list, and the per-pack knobs: `notes/generating-the-slate.md`.

- **2026-08-26** — **A big frozen value memoizes its hash.** The oracles take
  their fixture as an argument and reach an `lru_cache` on it; a frozen
  dataclass rehashes its tuple fields on **every** call, so an `at-scale`
  `Roster` rehashed 60,000 rows per lookup and one test took eleven seconds
  against 1.8s for the file. `__hash__` computes once and caches; `__eq__` still
  decides equality, so value semantics are unchanged.
  - *Where:* `scheduling.Roster` and `imports.Ledger`, the two whose oracle API
    is per-lookup rather than per-question. Not added speculatively to the
    others — the cost showed up in a profile, and that is the evidence it wants.

- **2026-08-25** — **A local subject, and the two Anthropic-shaped assumptions in
  the way.** Two of the three claims now in scope need a model weaker than haiku,
  and there is none. `Subject` is already the seam — one method, and everything
  downstream depends on `Transcript`. `confine.violation` and
  `engine_use.program_from_call` are pure functions and are reused, so controls 3
  and 4 hold by construction rather than by reimplementation.
  - **`runner.FATAL` is a regex over Anthropic error text.** A misclassified local
    failure records as an ordinary wrong answer — the exact defect that filed 46
    phantom `no-answer` cells on 2026-08-24. It moves behind a per-subject
    classifier before any local cell is graded. `Strength`'s per-MTok prices and
    context window move with it.
  - **The timeout is ours, and it goes on the tool call.** The engine has no fuel,
    cap or timeout by decision (`../datalog/spec.md` non-goals, rejected twice) and
    a weak model will write value-creating recursion. A `timeout` shim around the
    binary was rejected: it changes the engine arm's environment, which is the one
    thing this design holds fixed. — `notes/discriminating-instrument.md`

- **2026-08-25** — **A delta with no interval is not a null.** `report.py`
  subtracted two percentages and stopped; there is no statistics code in the
  project, no repeats, and n=48 per arm. So: `--repeats N` with the trial index in
  the cell identity, McNemar's exact test on the pairing the design already has,
  Wilson intervals on every printed rate, and `harness power` before a grid is
  paid for. Pure stdlib — Wilson and an exact binomial are ~20 lines and a
  dependency is a decision (`AGENTS.md`).
  - **Partial credit is recoverable, not a re-run.** `Grade` already carries
    `missing`/`extra` and the truth size is known, so per-item F1 comes out of the
    existing `records.jsonl`. The 2026-08-24 grid gets a finer read for free, and
    `results/` is not touched.
  - **`hypotheses.md`, written before the grid.** The comparisons went from one to
    three this session. Naming the primary endpoint in advance is what keeps that
    from being three chances to find something.

- **2026-08-25** — **A slate is selected by calibration, not designed.** An item
  the subject scores 0% or 100% on carries almost no information about whether the
  engine helped; the informative band is the middle. So a large candidate pool is
  generated, a cheap pass runs it on the prose arm at one weak strength, and items
  landing in roughly 0.2–0.8 are kept and pinned to a manifest.
  - **This is the step that would have caught the ceiling before the grid was paid
    for.** Opus was 20/20 in prose on the live domains and nothing said so until
    112 cells had run. One arm at one strength is the cheapest possible version of
    that check.
  - **Generation multiplies the `scheduling` risk**, so validation is mechanical:
    the oracle agrees with a second independent formulation, the fixture obeys
    every rule its question states, and two independent readings of the question
    agree. Divergent items are quarantined, not shipped — hand-verification proved
    the oracle matched the author's reading and could not prove there was only
    one. — `notes/discriminating-instrument.md`
  - ***Consequences*** 2026-08-26: built as `harness calibrate`, and the band
    turned out to need two things this entry did not say. It is not expressible
    below three trials per item, and it does not apply to `at-scale` at all —
    both above, dated today. The 0.2–0.8 figure survived contact unchanged.

- **2026-08-25** — **Two difficulty tracks, and the token cap is one track's
  definition rather than a global rule.** `FIXTURE_TOKEN_BUDGET` exists because a
  fixture that defeats the prose arm on size alone measures the context window;
  that reasoning holds, and it is also why the slate has no headroom. The
  resolution is two questions, not a relaxed cap: `in-context` keeps the budget and
  takes its difficulty from structure — depth, negation, distractors, the
  count-over-wildcard trap; `at-scale` deliberately exceeds the prose arm's window.
  - **They are reported in separate tables and never averaged.** A win in the first
    is a claim about reasoning; a win in the second is a claim about scale, and
    saying which is what keeps the second from reading as rigged.
  - **Difficulty is structure, not size.** Closure depth ≥ 6 with cycles, answer
    sets of 30–200 rows so the silent subset has room to happen, negation over a
    *derived* relation, an override layer applied after closure. `at-scale` is the
    same generators at 10k–100k rows, bounded by what
    `../datalog/notes/performance-baseline.md` measured.
    — `notes/discriminating-instrument.md`

- **2026-08-25** — **Three arms, and control 3 keeps the arm it was written for.**
  The first grid could not be read: seven domains scored identically in both arms
  because the engine arm reached for the engine in **9 of 56 cells**, so in 47 of
  them the two conditions differed only in which files were on disk. One arm
  cannot answer both *would an agent pick this up* (which needs control 3) and
  *does using it help* (which needs it used). So `prose`, `engine` — prompt
  byte-identical, control 3 intact — and `engine-forced`, whose prompt is the base
  plus one appended mandate block.
  - **`engine-forced` vs `prose` is S1's sentence read literally.** `engine` vs
    `engine-forced` is the adoption gap, which is a finding about the skill, and
    the ablation machinery already exists to act on it.
  - **Reach becomes a three-valued outcome**, closing the open question below: the
    first real transcript was a `Skill` call carrying a whole program with nothing
    executed. `none` / `invoked` / `answered-from`, and only the third is engine
    use in the sense S1 means. — `notes/discriminating-instrument.md`

- **2026-08-25** — **The harness refuses an engine binary older than the source
  it was built from, and refuses rather than builds.** `datalog/target/release/`
  is not versioned and everything measured against it is: the corpus pins live in
  git, so checking out an earlier commit reddens four of them until someone
  rebuilds, and a corpus run straight after a source edit reports green against
  yesterday's engine. Both were seen, in that order. Checking only that the file
  *exists* — which both call sites did — cannot tell the two apart, so
  `arms.require_engine` compares the binary's mtime against `src/`, `Cargo.toml`
  and `Cargo.lock`, and raises `EngineStale` naming the newest offender.
  - **Refusing, not building.** Building on demand would make the coupling
    invisible again, and this is the third instrument defect in four sessions
    whose whole cost was that nothing said the measurement had moved. A release
    build is also a minute the caller should choose to spend.
  - **The guard's usable half is what it ignores.** `spec.md`, `notes/`, `bugs/`
    and the crate's own `tests/` change constantly without changing the binary; a
    tripwire that fired on those would be routed around within a week, so a test
    pins that they do not trip it.
  - Prerequisite for re-pinning the corpus under the §12 error-code work, which
    moves every pinned diagnostic — a re-pin against a stale binary bakes a lie
    into the tripwire the pins exist to be.

- **2026-08-24** — **A fixture has to obey the rules its questions state, and the
  subjects are the first readers who do not already know the answer.**
  `scheduling`'s roster assigned people to shifts at random, ignoring the
  qualification and availability rule its own preamble stated: **21 of 29
  assignments were ones the person could not work**, the planted double booking
  among them. A reader who applied that rule to `assignment.csv` before looking
  for clashes got the empty set, and three of the four `double-booked` cells
  answered with an empty file — correctly. `forced-assignments` had the other
  half of the same fault: all four cells omitted `('s10','p09')`, reading "could
  work them" as excluding someone already on an overlapping shift.
  - **Hand-verifying 28 tasks against their oracles proved the oracle matched the
    *author's* reading. It could not prove there was only one reading.** Nothing
  in the suite compared the fixture against the question text, because the
  question text is prose. The tests that now exist are the closest mechanical
  proxy: every assignment is one the person could work, and nobody is rostered
  onto a shift `unstaffable-shifts` reports as impossible.
  - **A rule stated where it does nothing is not neutral** — a careful reader
    goes looking for the use. `ROSTER` was glued onto all four questions; it is
    now `OVERLAP` and `ELIGIBILITY`, each on the questions that turn on it, and
    `forced-assignments` says outright that an existing assignment does not
    disqualify.
  - **The cost of coherence is density.** No two overlapping shifts share a role,
    so an honest clash needs one person holding both roles of an overlapping
    pair — rare. Coherence alone cut `double-booked` to the planted pair and
    `forced-assignments` to one row, which is a question that measures whether
    the subject found the thing we hid. Paid for by `ROLES_EACH = 2` and a second
    planted `FORCED` shift, with a test that no answer is a single row.
  - The 2026-08-24 grid's `scheduling` numbers (3/8 both arms) are **not
    evidence about the models** and cannot be repaired in place: changing a task
    changes the slate, and `resume.strangers` refuses. Re-run under a new run id.

- **2026-08-24** — **Running out of turns is not an instrument failure: that cell
  is graded on what it left behind.** The SDK reports the harness's own
  `max_turns` cap as an error result like any other, so the `ERROR` rule above
  swallowed it — and discarding those cells would have **rewarded a model that
  flails**, by removing its failures from the denominator. That is bias in the
  opposite direction from the one `ERROR` exists to prevent, so the two are
  matched apart: `FATAL` (session/rate limit, `429`) is the instrument failing,
  `STOPPING_RULE` (`maximum number of turns`) is the experiment working. A
  turn-capped cell keeps the verdict it earned, carries its error text alongside,
  does not halt the grid, and is not owed again by a resume.
  - The distinction is only visible because the error text says which happened.
    A subject that reported failure without saying why would collapse the two,
    and the safe reading would then be `ERROR` — losing the flailing signal.

- **2026-08-24** — **A cell whose subject reported an error is `ERROR`, and is
  excluded — even when an `answer.txt` on disk would parse.** The first full grid
  ran out of the account's five-hour session window at cell 67. `AgentSubject.run`
  catches the SDK failure into `transcript.error` and returns normally, so the
  runner graded the empty workspaces it left behind: **46 cells recorded as
  `no-answer`**, which counts against the arm in every denominator.
  - **The report said "0 errored" while 46 had.** `controls` and
    `static_analysis` read `0/8` in both arms — a whole domain of the negative
    controls reading as total failure — and the headline delta was computed over
    denominators padded with cells that never ran. Same species as the two pilot
    defects: **the run went on producing plausible numbers after it stopped
    measuring anything.**
  - **Excluded rather than salvaged**, because grading an answer left by a cell
    that then failed makes the verdict depend on where in the turn sequence the
    failure landed. One cell today would have been salvageable. The raw text is
    kept on the record so a discarded cell can still be read.
  - **The rule is applied when reading, not only when writing** (`resume.failed`):
    a record carrying an `error` is failed whatever its verdict says. That is what
    lets the 46 be read correctly without rewriting one line of `results/`.
  - ***Amended*** 2026-08-24 by the entry above: *whatever its verdict says* was
    too wide. Only a `FATAL` error condemns a record; the turn cap does not.

- **2026-08-24** — **The pilot's `$0.07/cell` was a *warmed* number, and a record
  cannot explain its own cost.** Identical `access_control` cells cost 2–4× more
  in the full grid than in the 2026-08-23 pilot at the *same* turn counts and
  *lower* cache-read tokens — `who-can-read-r03.engine.opus-5` went `$0.051` to
  `$0.182`, turns 2 both times. The pilot ran ~14 minutes after the void run over
  the same grid, so its prefixes were already written and it paid reads where a
  cold run pays writes.
  - The residual has to be cache **creation**, and nothing in `results/` can show
    that: `Usage` captures `cache_creation_tokens` and `Record` drops it. Half an
    hour went into re-deriving from three fields what one recorded field would
    have stated. Queued on the ROADMAP.
  - **Consequences** for the parked *prompt caching* item: asserting
    `cache_read_input_tokens` is non-zero is the wrong assertion. The reads were
    never the cost.

- **2026-08-24** — **A session limit ends the run, not the cell — and the budget
  is a window, not dollars.** There is no `ANTHROPIC_API_KEY` here; the subject
  authenticates through the Claude Code subscription, so a run's `USD` figure is
  notional accounting and the real constraint is the five-hour window it shares
  with the session driving it. 66 cells consumed one window in **35 minutes** of
  wall time. Wall time was never the limit.
  - `run_grid` halts on a fatal error (`session limit`, `rate limit`, `429`) and
    records nothing after it, so those cells stay *owed* rather than being filed
    as phantom verdicts. Anything unmatched stays per-cell: guessing an unknown
    error is fatal would stop a run that could have finished.
  - **`--resume` is therefore the ordinary shape of a run, not a recovery path.**
    A 112-cell grid does not fit in one window alongside a working session. It
    re-runs the cells a run is missing or failed, appending to the same run
    directory — one run id stays one grid — and rebuilds the slate from that
    run's own `run.json`, because resuming with today's flags could silently join
    two different experiments. `--limit N` sizes a sitting to the window.
  - **Append-only survives it**: every attempt stays in `records.jsonl` in the
    order it happened, and the report reads the last record per cell and says how
    many were resumed. A grid measured across two windows is still one grid, but
    it was not one sitting.

- **2026-08-23** — **A cell starts from an empty directory.** Found by the first
  paid pilot, which is what a pilot is for. A workspace is named by a hash of the
  cell id, and `build` created it with `exist_ok=True` — so `--dry-run` and a paid
  run derive the *same* directory, and the stub's `answer.txt` was still sitting
  in it when the real subject arrived.
  - **Two of sixteen cells were graded on the stub's answer.** One scored `wrong`
    on the stub's deliberately-truncated truth; one scored `correct` without
    doing the work. Both were engine-arm cells, so the contamination moved the
    S1 delta in both directions at once.
  - **The shape of it is the dangerous part**: a stale answer only survives when
    the subject *fails to write its own*. So contamination appears exactly in the
    cells where the subject did not do the work — the ones the verdict is about.
    A run could look entirely healthy and be reporting the stub's numbers.
  - `build` now clears the directory first, guarded on the path being inside the
    workspace root and named like a cell (a 16-hex hash), because the only thing
    between that `rmtree` and a home directory is a name derived a moment ago.
  - **Not fixed by separating dry from paid roots**, which was the tempting
    smaller change: re-running the same paid cell twice inherits just as badly,
    and the failure would have come back the first time a run was repeated.

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

- **2026-08-23** — **A doc block is addressed by a marker, and no subject ever
  sees one.** Ablation cuts a named paragraph from the engine arm's skill and
  re-runs; the paragraph is named by `<!-- block: name -->` in the skill source
  rather than by line numbers, which rot on the first edit — and a rotted
  ablation cuts the wrong paragraph and reports a result anyway.
  - **Markers are stripped from every copy, ablated or not.** Otherwise the two
    conditions differ by a comment as well as by the cut, and every engine cell
    in every ordinary run carries a stray token nobody meant to ship.
    `cargo package-skill` strips them on the way into the distributed bundle for
    the same reason, so the annotation stays tooling and never becomes content.
  - **An ablation is engine-arm only, and a distinct cell id.** Cutting the
    engine's documentation cannot move an arm that never had it, so a prose cell
    would be paying to re-measure the control; and the workspace directory is a
    hash of the cell id, so without the ablation in that id the two conditions
    would share a directory.
  - **A cut that hits nothing raises.** A missing block would otherwise run the
    control twice at twice the price and report a null result that reads as *the
    doc line does not matter* — the exact conclusion the ablation exists to test.

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

- **"Reached for it" now has its first real transcript, and it splits the
  question.** A haiku engine cell invoked the **`Skill` tool** with its whole
  Datalog program in `args` — and nothing ran. `engine_use.uses_engine` counts
  that as engine use, deliberately ("the strongest possible form of reaching"),
  but the cell produced no answer of its own and was then graded on a file left
  behind by an earlier run. So *reached for it*, *ran a program*, and *got an
  answer out of it* are three signals, and the first one on its own is the least
  informative of the three. Decide the enum on more than this one cell — but the
  Skill-without-execution arm is now known to exist, and it is not a mis-parse.
  - ***Answered*** 2026-08-25: three values, and this transcript is the middle
    one. `none` / `invoked` / `answered-from`; only the third is engine use in the
    sense S1 means, and recording `invoked` separately is what stops a program
    that never ran being counted as a success.

- **What counts as "reached for it"?** Writing a `.dl` file is clear; asking the
  engine one question and then answering from `grep` is the case the signal exists
  for, and a boolean will not carry it. `signals.py` therefore records counts and
  no classification. Likely a small enum, decided against real transcripts rather
  than in advance.
  - ***Answered*** 2026-08-25: the small enum, decided against the transcript
    above. `grep` after `answered-from` is the escape this was written for and is
    now expressible; a count alone never was.
- **How many rounds does a cell get?** The first program is recorded before any
  feedback regardless, but the *final* answer needs a stopping rule, and an
  unbounded agent loop makes cost unpredictable. Candidates: a fixed round cap, a
  token task budget, or the agent's own declaration that it is done.
- **Does a full run get committed?** Rendered reports and per-cell verdicts,
  clearly. Full transcripts are large and the repo already forbids committing
  session transcripts — but a verdict nobody can audit is a weak record.
