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
- **`ontology`**, **`imports`**, **`eligibility`**, **`scheduling`** — _queued._
- **`static_analysis`** — _queued._ The agent extracts its own facts; carries the
  `grep`-escape finding. — `../datalog/skill/recipes/source-analysis.md`.
- **`controls`** — _shipped._ Four tasks: two single-hop lookups, two one-step.
  Without them a null result is indistinguishable from a broken instrument.

### Instrument hygiene

- **Cell containment** — workspaces outside the checkout, OS bash sandbox with the
  network denied, and a PreToolUse gate against paths that leave the workspace.
  _shipped_, verified on a real cell. — `confine.py`, `decisions.md` 2026-08-21.

- **Reference corpus** — correct programs pinned byte-exact **and** a malformed
  corpus with expected diagnostics. A corpus of correct programs cannot pin what a
  tool does when a run goes wrong. _queued — **v1**._
- **Doc-line ablation** — cut one named block from one cell's assembled workspace
  and re-run. _queued — **v1**: this is how a doc line is shown to carry weight
  rather than asserted to._ — `ablate.py`.
- **Prompt caching** — stable fixture/system prefix behind a breakpoint; assert
  `cache_read_input_tokens` is non-zero across cells. _queued — **post-v1**
  (cost, not validity)._

### Later

- **The game arm** — an `Environment` (`reset`/`observe`/`step`/`score`) of which
  a single-shot task is an episode of length 1. Minesweeper first: deduction is
  provably the bottleneck, scoring is objective, no opponent to model. The core
  must not foreclose it; nothing more is built now. _parked — **post-v1**._
- **Second engine** — the harness measures one engine against its own absence. A
  second engine as a third arm is a different question. _parked — **post-v1**._
