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

## 2026-08-21 — A proof gets a shape, and depth gets two channels

§11's rendering — the half of the provenance surface the 2026-08-16 decision left
open. **566 tests green** (+13), clippy and rustfmt clean.

**Done**
- **`print_proof`** (`src/print.rs`) renders a `ProofTree` as a `%`-comment block:
  a header, then one line per node carrying depth as a leading integer *and* as
  indentation. Nine unit tests, one per premise kind, plus the omitted-argument
  and hoisted-temporary cases.
- **An IR printer**, which did not exist: `print.rs` printed `ast`, and the rule a
  proof cites has to be the lowered one. Reuses `print_operand`'s parenthesization
  rule and `stdlib`'s name tables (new `stdlib::op_name`, with the test its
  totality `expect` was otherwise only claiming).
- **E7/E8** in `testing.md`, both mutation-verified, with a shared non-vacuity
  guard. **§16.6 is now a real block with a test that pins it** — one of the eight
  §16 examples that named none.
- **`ProofTree::Builtin` carries `lost`**, which `explain` had been dropping — so
  §12's *malformed* can reach a proof at all.
- **Swept**: §11 (rendering is normative there now), §16.6, §9's skip-report
  bullet, §17 ×2, `ROADMAP.md` ×2, `testing.md` ×4.

**Decided**
- **Depth rides in two channels because it has two readers.** Box-drawing is a
  *two-dimensional* encoding — `│`/`└─` mean something to an eye tracking a column
  and nothing to an agent reading a linear token stream. Bare indentation is worse
  again: depth becomes a whitespace-run *length*. E8 makes the integer
  load-bearing; it catches the mutant E7 cannot.
- **The cited rule is the lowered one.** Premises align index-for-index with the
  *lowered* body, so a source slice would list a body whose literal count does not
  match the premises under it. This was the finding that settled the question.
- **No "1 of N".** The rendering never says how many other derivations exist —
  which is exactly what makes it indifferent to the derivation-store question, and
  the reason it could be taken before that decision rather than after.
- **Named form wherever fields are known**, available only because a proof rides
  in comments and never re-parses.

**Removed**
- §16.6's box-drawing sketch, and §11's "its rendering is open" clause.
- §11's two-places-for-one-rule on the import anchor: the closing "coarser than
  the sentence suggests" narration is gone and the anchor bullet now just says
  *relation*. The rendering bullet that restated named form lost the restatement.
- The 2026-08-20 profile entry, rotated verbatim to `worklog-archive/2026-08.md`.

**Next up**
- **The form that asks.** §5 has no goal production, the lexer one `?-` token, the
  CLI no flag, `RunResult` no field — and the exit-code ruling for a run that
  explains but returns no rows is unmade. E5 unblocks with it.
- **Then** the recorder / derivation-store decision, now genuinely unconstrained
  by the surface.
- Still open: **E6**, `?whynot`'s near-miss rendering, the JSON encoding, and
  `--no-default-features` failing its own suite.
