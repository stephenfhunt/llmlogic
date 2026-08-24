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

## 2026-08-23 — The corpus that pins, the paragraph that can be cut, and a diagnostic that lies

Both remaining **v1** items for the first grid run, plus the bug the first one
found on its first day. 195 tests green (+31), ruff clean, the 112-cell offline
grid still renders.

**Done**
- **Reference corpus** (`experiments/reference/`) — seven correct programs, one
  per domain, and five malformed ones pinned to their diagnostics. The five
  domain programs from 2026-08-22 were **rescued out of a scratch directory**
  before it was swept; `access_control` and `controls` were written and verified
  this session. `harness reference [--repin]`, and a test that checks the pins
  *and* every relation against the domain's plain-Python oracle.
- **Doc-line ablation** — `<!-- block: name -->` in the skill, `harness run
  --ablate <block>`, `harness blocks` to list them. Four marked. Markers are
  stripped from every copy, ablated or not, and `cargo package-skill` strips them
  too, so none reaches a subject or a bundle.
- **`datalog/bugs/008`** — a rule-level type clash manufactures a *second*, false
  diagnostic asserting the fact table holds values it does not. `union()` merges
  the classes after reporting the conflict, and `finish()`'s declared-vs-inferred
  sweep then reads the poisoned class. The accusation follows operand order,
  which is the tell. Found while pinning the malformed half.
- **Cheaper slices** — `--strength`, and `--ablate` implying the engine arm.

**Decided**
- **Run the grid before fixing count-distinct**, which was the session's opening
  question. The trap is already documented in two places, and the ROADMAP's own
  open question is *"`count distinct`, or document it harder"* — the run and the
  ablation are the evidence that settles it. Fixing first spends a language-design
  session on a call the run would inform, then wants a re-run.
- **A pin is a tripwire, not an assertion of correctness** — so the corpus
  re-checks every relation against the oracle, and a moved pin is a change in the
  instrument to be read, never a red test that regenerates itself.
- **Measured, and it sharpens the item**: over `sqlparse`, `widely_used` returns
  **7 with a two-column `call` and 13 with a three-column one** — same rule text,
  the extra column a line number it never mentions, exit 0, no warning.

**Removed**
- One test of my own writing, before it landed: a text-scan for "datalog" in the
  oracles duplicated `test_truth_independence.py`, which already does it over the
  AST and does it properly. Nothing else — the session was almost all new.

**Next up**
- **The pilot**: `harness run --domain controls --smoke --yes` (4 cells), then
  `--domain access_control --yes` (16). Then read the transcripts and close the
  two questions `decisions.md` says to settle against real ones — what *"reached
  for it"* means, and whether `max_turns=30` ever binds.
- Then the full grid, then rule on count-distinct from what it shows.
- Still open: `008`, the JSON encoding, `--no-default-features`, §17's
  period-arithmetic questions.

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
