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

- **`access_control`** — _shipped._ Four tasks over a generated policy graph;
  truth is a BFS, property-checked against a fixpoint formulation.
- **`ontology`** — _shipped._ Multiple inheritance, property overriding, and a
  disjointness check over the same closure.
- **`imports`** — _shipped._ Dates, aggregates and missing amounts across CSV,
  JSONL and a redundant Parquet copy. — `decisions.md` 2026-08-22.
- **`eligibility`** — _shipped._ Four criteria in prose, and the applicants a
  missing income leaves undecided rather than refused.
- **`scheduling`** — _shipped._ Interval overlap: double bookings, unstaffable
  shifts, forced assignments, rest violations.
- **`static_analysis`** — _shipped._ The agent extracts its own facts from a
  pinned `sqlparse`; carries the `grep`-escape finding. Its questions define
  their abstractions syntactically. — `../datalog/skill/recipes/source-analysis.md`,
  `decisions.md` 2026-08-22.
- **`controls`** — _shipped._ Four tasks: two single-hop lookups, two one-step.
  Without them a null result is indistinguishable from a broken instrument.

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
- **TypeScript extraction** — a `tsc`-API fact extractor as a second
  `static_analysis` corpus. The better demo; blocked as a *control* by control 1,
  which wants a plain-Python oracle. _parked — **post-v1**._ — `decisions.md`
  2026-08-22.
- **A `static_analysis` ceiling case** — the pinned corpus is a whole small
  package (~36K tokens, inside the budget). A corpus that does *not* fit the prose
  arm's context is a separate, stated run. _queued — **post-v1**._
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
  number. _queued — **post-v1** (legibility, not validity)._
  — `decisions.md` 2026-08-24.

### Later

- **The game arm** — an `Environment` (`reset`/`observe`/`step`/`score`) of which
  a single-shot task is an episode of length 1. Minesweeper first: deduction is
  provably the bottleneck, scoring is objective, no opponent to model. The core
  must not foreclose it; nothing more is built now. _parked — **post-v1**._
- **A local-model subject** — `Subject` is a protocol with two implementations
  already (`StubSubject`, `AgentSubject`); a locally-hosted model is a third, and
  costs a window of nothing to run. The reason to want it is what the grid keeps
  showing: **the signal lives at the weak end.** Opus answers correctly without
  reaching for the engine, so a strength below haiku is where "does the engine
  help?" has room to be answered at all — and small distilled models are the
  cheapest check that the instrument discriminates rather than measuring a
  ceiling. Hardware is limited; a run that takes all night costs nothing but the
  night. _parked — **post-v1**._

- **Second engine** — the harness measures one engine against its own absence. A
  second engine as a third arm is a different question. _parked — **post-v1**._
