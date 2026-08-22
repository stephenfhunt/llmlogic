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

## 2026-08-22 — Five domains, and the grid reaches 112 cells

Phase B: the five queued domain packs, so the slate stops being one measured
domain plus its controls. **164 tests green** (+85), ruff clean, and the full
112-cell grid runs offline against the stub subject.

**Done**
- **`ontology`** (multiple inheritance, property overriding, disjointness),
  **`imports`** (dates, aggregates, missing amounts; CSV + JSONL + a Parquet
  copy), **`eligibility`** (four criteria in prose, and what a missing value
  leaves undecided), **`scheduling`** (interval overlap four ways), and
  **`static_analysis`** (a pinned `sqlparse`, facts the subject extracts itself).
- **Every one of the 28 tasks answered by hand with the real engine** in a
  scratch directory, and compared row-for-row against its oracle. All 28 match.
- **Plumbing**: a `Fixture` can now carry several spellings of one relation,
  binary contents and nested paths; `catalogue.verify` checks CSV headers, JSONL
  keys and Parquet schemas alike and requires the copies to agree on row count.
  `corpus.py` fetches a pinned sdist outside the checkout, and a pack whose
  corpus is missing says so rather than vanishing from the slate.
- **`FIXTURE_TOKEN_BUDGET` is enforced**, having only been stated.

**Decided**
- **Parquet is a redundant copy, never a relation's only spelling.** The sealed
  workspace has system `python3` and nothing else, so a Parquet-only table is one
  the *prose arm cannot open* — those cells would be decided by file format.
- **The `static_analysis` corpus is fetched and pinned, not vendored, and it is
  Python.** `tsc`'s API is the better extractor and the worse control: control 1
  wants a plain-Python oracle, and a TS corpus would need one over a hand-rolled
  parse. `sqlparse` over `requests` because a memorized codebase can be answered
  from training rather than from the files.
- **The questions define their abstractions syntactically** — "called" is the
  callee of a call expression. A semantic oracle would be a guess the answers
  were then graded against.
- **A question is tuned in the fixture, never in the grader**, by planting a row
  or by choosing the seed by search. Four near-misses caught that way, including
  a roster whose shifts tiled the day so cleanly that nobody could be
  double-booked.

**Removed**
- The unsound half of two property tests: overlap-by-distance and
  overlap-by-extremes disagree on a zero-length interval, and nearest-ancestor
  read as shortest path is wrong under multiple inheritance. Both replaced with
  formulations that are independent *and* sound.

**Next up**
- **The reference corpus** — the 28 verified programs written this session are
  most of it, and they are sitting in a scratch directory.
- Then the **doc-line ablation**, and the first paid run.
- Still open: what *"reached for it"* should mean, the per-cell stopping rule,
  and whether a full run's transcripts get committed.

## 2026-08-21 — The instrument gets built, and the first real cell falsifies it twice

S1's harness, as a new top-level project. `experiments/` runs each task twice —
once by an agent that has the engine, once by the same agent without it — and
grades both against truth computed in plain Python. 79 tests green, ruff clean,
and the full grid runs offline against a stub subject with no API calls.

**Done**
- **The project**: `AGENTS.md`, `ROADMAP.md`, `decisions.md`, and a package —
  core types, record store, grading, process signals, report, CLI. The offline
  `--dry-run` grid is the CI gate, so a change that can only be tested by
  spending money is a change that stops being tested.
- **Both arms are the same Claude Agent SDK agent**, same tools, same
  byte-identical prompt; the engine arm additionally has the binary and skill.
  The prose arm keeps `bash` and may write a script — the honest counterfactual.
- **Two domain packs**: `access_control` (four tasks over a generated policy
  graph, truth a BFS property-checked against a fixpoint formulation) and
  `controls`, the negative controls that make a null result readable.
- **Containment** — workspaces outside the checkout, OS bash sandbox with the
  network denied, a PreToolUse gate against paths that leave the workspace.

**Decided**
- **Ground truth never comes from the engine**, enforced by an AST test rather
  than a convention: if the engine grades itself the engine arm is correct by
  construction, and the run is void while still producing plausible numbers.
- **`UNPARSEABLE` is kept apart from `WRONG` because of bias, not tidiness.** The
  prose arm writes sentences more often, so counting a sentence as a wrong answer
  inflates the engine's margin — the one direction of bias this cannot afford.
- **Containment is a validity control before a safety one.** Verified by hand:
  from a workspace inside the checkout, `truth.py` — the answer key — and the
  engine binary were both reachable, the latter executable by absolute path.
  Scrubbing `PATH` does nothing against `/abs/path/to/datalog`.

**Removed**
- `experiments/.workspaces/` as a location — it was inside the repo, which is how
  the answer key was two directories up. Nothing else: this session was almost
  entirely new, and the deletions it did make were of its own first drafts.

**Next up**
- **Five domain packs**: `ontology`, `imports`, `eligibility`, `scheduling`,
  `static_analysis`. Then the reference corpus, the doc-line ablation, and the
  first full run.
- **`spec.md` §1 is deliberately untouched** — S1's instrument does not move to
  `experiments/` until the harness can actually measure. `datalog/ROADMAP.md`
  says *building*, which is what is true.
- Still open: the JSON encoding, `--no-default-features` failing two temporal
  tests, and §17's period-arithmetic questions.

## 2026-08-21 — The surface that asks, and provenance stops being free of charge

The three-way session `ROADMAP.md` had been sequencing — asking form, derivation
store, row provenance — held as one and then built. **S5 is met**, and S1 is now
the only unmet v1 criterion. 578 tests green, clippy and rustfmt clean.

**Done**
- **`?why` / `?whynot`** as §5 statements, in a file or inside a `-q` (one more
  arm in the classifier, no new flag). Lexer sigils, a fifth `StatementKind`, a
  groundness check in lowering, `RunResult.explanations`, `§16.6` and a new
  `§16.15`, both pinned byte-for-byte by system tests.
- **The failure trace** — `FailureTrace`/`NearMiss`/`Repair`, one entry per rule
  whose head unifies, re-solved through `schedule.rs` via a probe threaded into
  the *same* join the fixpoint runs. Five repair arms, four of which name no fact.
- **Demand-provisioned recording** (`engine::Provenance`). Measured, `sparse_400`:
  no goals **0.23 s / 44 MB**, `?why` **0.55 s / 202 MB** — 78% of peak RSS and
  58% of wall clock, matching the profile's projection. A `?whynot` over an absent
  fact pays nothing; over one that holds it re-runs the fixpoint and says so.
- **Four properties**: **E9** (provisioning changes no answer, and an unrecorded
  model says `Unrecorded`), **E10** (a near-miss holds against the model),
  **E5** unblocked at last (comment-stripping), **E6** closed (E3 through
  builtins, replaying in schedule order). All mutation-verified.

**Decided**
- **The sigil's real job is provisioning, not the cost hint.** Post-fixpoint the
  engine knows whether the fact holds and needs no hint; *before* it, the sigil is
  the only thing that says whether to record. That is the first argument for two
  forms this project generated rather than adopted — and it makes the asking form
  and "does the store earn its cost" **one decision**, not two.
- **A query cannot stand in for `?whynot`.** The commonest why-not is about a
  query that *succeeded*; the expectation is nowhere in the program, so only a
  goal naming the missing fact carries it. §16.13 forecloses the implicit version
  besides.
- **Explanations are exit-code-neutral** — the exit-code twin of E5.
- **Backwards extraction demoted to post-v1**: gating answers the store's cost by
  proportioning it, at a fraction of the risk.

**Removed**
- ROADMAP's provenance section shrank from ~90 lines to ~55: the query-syntax and
  derivation-store items collapsed into shipped entries, the row-anchor item's
  stale "decide with the two items above" (both now ruled), and E3-over-builtins.
- `api::expr_casts`, folded into `Program::reports_through_provenance` so the scan
  and the provisioning test cannot drift.
- §2's engine/surface scoping sentence and §16's "except §16.6's `?why` form".

**Next up**
- **S1** — `EXPERIMENTS.md` rebuilt as a harness, now the only unmet criterion.
  Task 7 was added for the goals themselves; the predicted failure is not sigil
  confusion but never asking.
- Still open: the **JSON encoding** (parked, low value), `--no-default-features`
  failing two temporal tests, and §17's period-arithmetic questions.
