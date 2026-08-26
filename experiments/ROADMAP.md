# experiments — roadmap & backlog

The single index of discrete work items for `experiments/`. One line per item, a
status, and a pointer. The other documents specialize: `decisions.md` holds the
rationale, `README.md` what the harness is, `../docs/worklog.md` the session
handoff.

Status vocabulary: **queued** (agreed, not started) · **designing** (needs a design
pass first) · **building** (underway) · **parked** (deferred, awaiting a trigger) ·
**shipped**.

Every item carries **v1** / **post-v1**, where v1 means *the first full run of the
grid* — the thing `datalog`'s S1 is waiting on. post-v1 means "does not block that
run", never "unwanted".

## Open backlog

### Core

- **Project skeleton** — docs, `pyproject.toml`, package layout. _shipped._
- **Source corpora** — pinned sdists fetched by `harness corpus fetch` into the
  cache outside the checkout, sha256-verified; a pack whose corpus is missing
  says so instead of vanishing. _shipped._ — `corpus.py`.
- **Core types + record store** — `Task`/`Episode`/`Cell`/`Record`, JSONL store,
  and a stub subject so the whole grid runs `--dry-run` offline. _shipped._ The
  offline grid is the CI gate: `harness run --dry-run --all`.
- **Subject driver** — Claude Agent SDK, both arms, first-program capture via a
  `PreToolUse` hook. _shipped_, verified on a real cell. — `agent.py`, `arms.py`.
- **Engine-use detection** — one home for "did it use the engine, and what did it
  ask?". _shipped._ — `engine_use.py`, `decisions.md` 2026-08-21.
- **Catalogue assembly** — the prompt's relation catalogue derived from each
  fixture's own schemas, pinned by a drift test. _shipped._ — `catalogue.py`.
- **Grading** — answers against the domain's independent truth, with format
  failures held apart from wrong answers. _shipped._ — `grade.py`, `decisions.md`
  2026-08-21 (*The answer contract*).
- **Process signals** — reached-for-it, `grep`-escapes, rounds-to-correct,
  silent-wrong. _shipped_ as counts; what *"reached for it"* should mean is an
  open question in `decisions.md`. — `signals.py`.
- **Report rendering** — a run's JSONL to markdown, controls tabled separately
  from the measured slate. _shipped._ — `report.py`.

### Domains

One pack each: `fixture.py` · `truth.py` · `tasks.py`. All **v1** — the slate is
what makes the result readable, and a grid of one domain measures one domain.

- **`access_control`** — _shipped._ Four pinned tasks over a generated policy
  graph; truth is a BFS, property-checked against a fixpoint formulation. The first
  pack with `generate(seed, difficulty, track)` beside `build()`.
- **`ontology`** — _shipped._ Multiple inheritance, property overriding, and a
  disjointness check over the same closure. Generated: diamonds spliced at a
  shared grandparent, declarations on one spine so *exactly one value* holds.
- **`imports`** — _shipped._ Dates, aggregates and missing amounts across CSV,
  JSONL and a redundant Parquet copy. — `decisions.md` 2026-08-22. Generated: the
  pinned seed search becomes a tie repair, and customers get a season.
- **`eligibility`** — _shipped._ Four criteria in prose, and the applicants a
  missing income leaves undecided rather than refused. Generated: difficulty is
  how many columns can be blank, and thresholds are calibrated to the draw.
- **`scheduling`** — _shipped._ Interval overlap: double bookings, unstaffable
  shifts, forced assignments, rest violations. Its roster and two of its
  questions were **repaired 2026-08-24** after the first grid found them
  ambiguous; the 2026-08-24 numbers for this domain are void. —
  `decisions.md` 2026-08-24. Generated: difficulty is how much the day overlaps
  itself, and `check` asserts every assignment is one its person could work.
- **`static_analysis`** — _shipped._ The agent extracts its own facts from a
  pinned `sqlparse`; carries the `grep`-escape finding. Its questions define
  their abstractions syntactically. — `../datalog/skill/recipes/source-analysis.md`,
  `decisions.md` 2026-08-22. **The one pack with no generator**, and
  `domains.NO_GENERATOR` says why: there is nothing to seed.
- **`controls`** — _shipped._ Four tasks: two single-hop lookups, two one-step.
  Without them a null result is indistinguishable from a broken instrument.
  Generated, but flat: no `at-scale`, and a row ceiling — a control that gets
  hard stops being a control.

### Making it discriminate

The 2026-08-24 grid could not be read: seven domains scored identically in both
arms, the engine arm reached in 9 of 56 cells, opus is 20/20 in prose, and there
is no statistics code. Three independent defects; fixing any one alone leaves the
grid unable to answer S1. The argument and the numbers are in
[`notes/discriminating-instrument.md`](notes/discriminating-instrument.md).

- **A third arm, `engine-forced`** — base prompt plus one mandate block, so
  *would an agent pick this up* and *does using it help* stop competing for one
  arm. Control 3 stays on `engine`, whose prompt remains byte-identical to
  `prose`; the mandate is a strict suffix on `engine-forced`, asserted as one.
  _shipped 2026-08-25._ — `cell.py`, `catalogue.MANDATE`, `tests/test_controls_hold.py`.
- **Reach as a three-valued outcome** — `none` / `invoked` / `answered-from`,
  closing the open question a `Skill` call carrying an unexecuted program raised.
  Only `answered-from` is engine use in the sense S1 means, and on `engine-forced`
  it doubles as the compliance check. _shipped 2026-08-25._ — `signals.EngineUse`,
  `decisions.md` 2026-08-25.
- **Two difficulty tracks** — `in-context` keeps `FIXTURE_TOKEN_BUDGET` and takes
  its difficulty from structure; `at-scale` exceeds the prose arm's window and is
  reported in its own table, never averaged with the first. _shipped 2026-08-25._
  — `task.Task.track`, `report.py`. The size cap is now conditional on the track
  rather than global.
- **Parameterised generators** — `generate(seed, difficulty, track)` beside each
  pack's `build()`; the 28 pinned tasks stay as the comparable slate, hashed in
  `tests/test_pinned_slate.py`. Absorbs the owed `scheduling` re-run, whose
  2026-08-24 numbers are void: the next grid replaces those numbers.
  _shipped 2026-08-26._ — six of seven packs; `static_analysis` has none by
  decision. `harness.generate` holds the degeneracy rules,
  `domains.generate(name, …)` is the single entry point, and a pack's `check`
  holds what only it can know. 994 tests over 12 seeds.
- **`harness calibrate`** — generate a large pool, run the prose arm at one weak
  strength, keep the items whose accuracy lands in the informative band, pin the
  slate to a manifest. The cheapest possible check for a ceiling.
  _shipped 2026-08-26._ — `calibrate.py`, `cli.cmd_calibrate`. Three trials per
  item, because one is 0 or 1 and the band is then empty by construction;
  `at-scale` is calibrated too and its band is a **floor**, since prose scoring
  ~0 is what that track claims. `harness run --slate <manifest>` runs what it
  selected, regenerating each item and refusing one whose fingerprint moved;
  `calibrate --from <run-dir>` moves the band without re-running the subject.
  **No paid pass has run**: the slate has still never met a real subject.
- **Statistics** — `--repeats N`, McNemar on the pairing the design already has,
  Wilson intervals, per-item F1 beside the binary verdict, and `harness power`
  before a grid is paid for. Pure stdlib. _shipped 2026-08-25._ — `stats.py`,
  `report.py`, `cli.cmd_power`. `power --effect 0.10` wants 155 paired items against
  a slate of 56 — the 2026-08-24 grid's third defect, as a number.
- **`hypotheses.md`** — the comparisons and the primary endpoint, written before
  the grid runs. _queued._ **Now also has to say which subject** — a local arm and
  an Anthropic arm are not comparable to each other, only within themselves.
- **A local-model subject** — `LocalSubject`, an OpenAI-compatible tool loop
  against ollama on the local GPU. _shipped 2026-08-26._ — `local.py`,
  `notes/a-local-subject.md`. Two tool protocols (`native`, `structured`), the
  skill as a `Skill` tool advertised from SKILL.md's own frontmatter, `bash` under
  `unshare -rn`, and `Strength` carrying the endpoint, protocol and reasoning
  effort so `harness run --local-model M --protocol P` sweeps them as strengths.
  `runner.FATAL` is behind a per-subject classifier. **Not yet answered: whether
  an 8B subject clears the negative controls at all** — the first sweeps found it
  below the floor, and the fixes for that are the three entries below.
- **What a weak subject needs before it can be measured** — _shipped
  2026-08-26_, all in `decisions.md`: the exit condition checked (at most twice,
  naming only the file), the answer format **shown** rather than only described,
  and `thought` on every structured action. The first two move the instrument and
  say so; the third is parity with what `native` already allowed.
- **Bounds that make an overnight run finishable** — wall clock per cell, tokens
  per completion, and a conversation that may not outgrow its window; each
  recorded as a *stopping rule*, because an ERROR cell is one `resume` owes
  forever. _shipped 2026-08-26._ — `local.py`, `runner.STOPPING_RULE`.
- **Preflight refuses a misconfigured server** — no server, an unpulled model, or
  a window smaller than the run assumes. The third instance of this harness
  reporting plausible numbers from a misconfigured instrument, so the rule is now
  stated: refuse, do not degrade. _shipped 2026-08-26._ — `local.preflight`,
  `decisions.md` 2026-08-26.
- **A local sweep of the calibrated slate** — the run the above exists for:
  models x protocols x arms over `controls` and `access_control`, three trials.
  _queued_ — the next session.
- **`harness score --task <id> --program <file>`** — run a program, grade its
  output. One command; also the natural home for the reference corpus's check.
  _shipped 2026-08-25._ — `score.py`. Exit 0/1/2; the wall-clock timeout is the
  harness's, since the engine has none by decision.

### Instrument hygiene

- **Cell containment** — workspaces outside the checkout, OS bash sandbox with the
  network denied, and a PreToolUse gate against paths that leave the workspace.
  _shipped_, verified on a real cell. — `confine.py`, `decisions.md` 2026-08-21.
  A cell also starts from an **empty** directory: the first pilot found the
  dry-run stub's `answer.txt` still in place for a paid cell, and graded two of
  them on it. — `decisions.md` 2026-08-23.

- **Reference corpus** — _shipped 2026-08-23._ Seven correct programs, one per
  domain, each answering that domain's four questions and pinned byte-exact; five
  malformed ones pinned to their diagnostic. The test checks both the pins *and*
  every relation against the domain's plain-Python oracle, because a pin over a
  wrong program defends the error. `harness reference [--repin]`.
  — `reference/README.md`, `src/harness/reference.py`. It earned itself on the
  first day: pinning the malformed half found `../datalog/bugs/008`.
- **Doc-line ablation** — _shipped 2026-08-23._ A paragraph of the skill is
  wrapped in `<!-- block: name -->`; `harness run --ablate <block>` cuts it from
  the engine arm's copy and runs those cells under a distinct id. Markers are
  stripped from **every** copy, ablated or not, so the two conditions differ by
  the cut alone — and `cargo package-skill` strips them too, so none ever ships.
  Four blocks marked; `harness blocks` lists them. — `ablate.py`,
  `../datalog/AGENTS.md`.
  The one it was wanted for: `count-wildcard` and `source-analysis-count-trap`
  are what turn *"is documenting the trap enough?"* into a measurement rather
  than a position. — `../datalog/ROADMAP.md`, *Count-distinct*.
- **The engine binary a run measures against is checked for staleness** —
  _shipped 2026-08-25._ Both call sites checked only that
  `datalog/target/release/datalog` *existed*, so the corpus could report green
  against a two-day-old engine and an earlier checkout reddened four pins until
  someone rebuilt. `arms.require_engine` refuses a binary older than `src/`,
  `Cargo.toml` or `Cargo.lock`, and ignores everything that changes without
  changing the binary. — `arms.py`, `decisions.md` 2026-08-25.
- **A resume should refuse a fixture that moved under it** — `resume` guards the
  slate by cell id and count, which catches a renamed or added task and is blind
  to the thing that actually changed on 2026-08-24: `scheduling`'s roster, under
  the same four task ids. Nothing owed by the open run was a `scheduling` cell,
  so it did not bite — but a resume that had owed one would have joined two
  different experiments with no signal. _shipped 2026-08-25._ `resume.fingerprint`
  writes a sha256 of each task's question, fixture files and truth rows into
  `run.json`; `resume.moved` refuses on a mismatch, and stays silent for a run that
  recorded none. — `resume.py`, `decisions.md` 2026-08-24.
- **`Signals.ran_engine` contradicted `engine_use`** — `measure` counted a
  `Skill` invocation as the engine having run while `classify` did not, so a cell
  whose only tool call was `Skill` recorded `invoked` and `ran_engine=True` in the
  same record. `ran_engine` and `rounds` are narrow now; `engine_calls` keeps the
  wide count and says so. Past records keep what they were written with.
  _shipped 2026-08-26._ — `signals.measure`, `tests/test_signals.py`.

- **A resume rebuilds the slate the run actually ran** — there are three
  provenances now, and `cmd_resume` knew one. A calibration pass rebuilds its
  pool from the spec in its own `run.json`; a calibrated run reloads its
  manifest and refuses one whose bytes moved since; everything else is the
  pinned slate. The same rebuild had also **dropped `repeats`**, so resuming a
  repeated run produced trial 0 only and then refused every later trial as a
  stranger — a resume that refused itself. Nothing had owed one, because nothing
  had run one. _shipped 2026-08-26._ — `cli._slate_of`.

- **TypeScript extraction** — a `tsc`-API fact extractor as a second
  `static_analysis` corpus. The better demo; blocked as a *control* by control 1,
  which wants a plain-Python oracle. _parked — **post-v1**._ — `decisions.md`
  2026-08-22.
- **A `static_analysis` ceiling case** — the pinned corpus is a whole small
  package (~36K tokens, inside the budget). _superseded_ — "a separate, stated
  run" is now the `at-scale` track, which states it for every domain.
- **Prompt caching** — stable fixture/system prefix behind a breakpoint; assert
  `cache_read_input_tokens` is non-zero across cells. _queued — **post-v1**
  (cost, not validity)._

- **Cheaper slices** — `harness run` takes `--strength` and (via `--ablate`)
  narrows to the engine arm, so a pilot need not be the whole crossing.
  _shipped 2026-08-23._ — `cli.py`, `cell.grid`.

- **A failed cell is not a wrong answer** — a subject that reported an error
  grades `ERROR` and leaves every denominator, and the rule is applied when
  reading a record as well as when writing one, so runs recorded before it are
  read correctly without `results/` being rewritten. _shipped 2026-08-24._
  — `runner.run_cell`, `resume.failed`, `decisions.md` 2026-08-24.

- **Sessions, not sittings** — a full grid does not fit in one five-hour window
  alongside the session driving it, so `run_grid` halts on a session or rate
  limit rather than recording phantom cells, `--resume <run-dir>` finishes a run
  into its own directory, and `--limit N` sizes a sitting to the window. The
  report counts each cell once and says how many were resumed.
  _shipped 2026-08-24._ — `resume.py`, `cli.cmd_resume`, `report.latest`.

- **`Record` should carry `cache_creation_tokens`** — `Usage` captures it and the
  record drops it, so a run cannot explain its own cost and the pilot's
  `$0.07/cell` took three fields and half an hour to re-derive as a warmed
  number. _shipped 2026-08-25._ — `record.Record`, `decisions.md` 2026-08-24.

### Later

- **The game arm** — an `Environment` (`reset`/`observe`/`step`/`score`) of which
  a single-shot task is an episode of length 1. Minesweeper first: deduction is
  provably the bottleneck, scoring is objective, no opponent to model. The core
  must not foreclose it; nothing more is built now. _parked — **post-v1**._
- **A local-model subject** — _superseded_ by the queued item under *Making it
  discriminate*: the reason to want it (the signal lives at the weak end) is now
  one of the three defects the grid has to fix, not a later nicety.

- **Second engine** — the harness measures one engine against its own absence. A
  second engine as a third arm is a different question. _parked — **post-v1**._
