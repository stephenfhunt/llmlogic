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

## 2026-09-14 (early) — the fact store's branch, its gate, and step 1: a `Relation` type, nothing moved

Asked to set up the fact-ownership refactor on a branch, more rigorously than
usual. Planned; the user answered the design's review questions, then asked to
start on the relation type.

**Done** — branch `fact-store`; `cargo test`, clippy, fmt green at every commit
- `07d123d` the review's answers: runs, binary-search membership, imports stay in
  `Program.facts`, storage before provenance. §17 2026-09-13 (later iv), and
  `AGENTS.md`'s one branch exception.
- **The gate**, in `~/.cache/fact-store/` (its README): a frozen `efcda71`;
  `diff.py`, 1,109 cases (corpus, every grafana-ui library and query file asked
  for every relation it defines, 250 cross-engine and generated programs, 794
  goals), whose baseline self-diff is empty; `deep.sh`; `measure.py`.
- `4bc722b` **`Relation`**: facts and delta behind one type, views read by the
  collecting round, `apply_round` for both insert paths, rows as `&[Value]`.
- `1479c84` **B14a** (the relation against a ledger) and **B14b** (views against
  round stamps, via `eval_observed`). Five mutations killed.
- **Step 1's gate at `1479c84`:** harness diff empty; deep seeds 2 and 3 pass 514
  in 158 and 126 s (baseline 159, 130); `pointsto.dl` 10.55 → 10.37 s, its `?why`
  9.23 → 8.25 s; `sparse_800` 1.24 → 1.27 s, 126 → 133 MB.

**Decided**
- The user's: the branch in this checkout; storage first, staged; runs with
  binary-search membership; a capped deep gate run detached (foreground stops at
  10 min; background tasks were killed for "low memory" at 22 GB available).
- The user's: B5's pathological `Deep` draws are not chased (seed 1 took 6 h, the
  random seed was stopped at 72 min). The gate runs seeds 2 and 3.
- A delta is tagged with the round that wrote it. So a lower stratum's last block
  is never a higher stratum's delta.

**Removed** — `insert_derived`, `insert_unkept`, and the delta map threaded through
`JoinCx`; `recorder-wt`'s scratch instrumentation (saved as a patch) and
`proofs.sh` (replaced by `diff.py`); the oldest worklog entry.

**Next up**
- **Step 2: the flat store, sorted runs and watermark views, inside `Relation`**
  (`notes/fact-store.md` § Rules). It adds the property *a row never moves*, and
  B14a gains the runs half.
- `sparse_800`'s +0.03 s and +7 MB are inside run-to-run noise; re-measure at step 2.
- **`bugs/016`**, filed on trunk (the user's): `cargo test --no-default-features`
  fails three import tests that `694a6ff` added without the `duckdb` gate.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-13 (late night) — fact references: shared tuples built, slower, reverted; a store with one owner designed

Asked to start the fact references design. The user asked why a premise had to be
an id and not a `&`, chose design then build, and after the regression named the
defect: nothing owns a fact.

**Done** — datalog `cargo test`, clippy, fmt green at every commit
- `aa1ae1d` designed shared tuples (`Rc<[Value]>`), not ids (§17 2026-09-13
  (later iii)). `5035215` built them, with E12 and a killed mutation:
  - `?why` byte-identical on 33 corpus goals and both `@grafana/ui` goals;
  - `pointsto.dl` `?why` 836 → 474 MB, `callreach.dl` 389 → 196 MB.
- **Measured only after committing, and slower.** `pointsto.dl` with no goals
  went 8.4 → 12.7 s and `sparse_800` 1.83 → 2.19 s, on equal instructions and
  83% more cache misses.
- **Diagnosed** (`notes/recorder-at-scale.md` § Shared tuples, measured):
  - ruled out: the seek prefix's allocation, count traffic, placement at apply;
  - `Box<[Value]>` split the layout cost from sharing's;
  - leaking imported rows' freed buffers gave 6.55 s, which is `pointsto.dl`'s
    cost. `sparse_800`'s is not explained.
- `efcda71` reverts the code and E12; §17 keeps the entry, ***Falsified***.
- `notes/fact-store.md`:
  - each relation owns its facts in a flat, append-only store, written once;
  - everything else holds a `FactRef`, and order is kept as sorted runs of rows;
  - a measurement gate, and six questions for review.

**Decided**
- The user's: the defect is ownership. Facts need *a single source of truth that
  everything else refers back to*, designed and prototyped before trunk.
- The user's: revert `5035215` rather than keep its memory win.
- §17 2026-09-13 (later iii): ***Amended*** (a premise stays 32 bytes), then
  ***Falsified***.

**Removed** — `5035215`'s code and E12; the shared-tuple direction; the note's
zero-arity question (v1 has no nullary predicates); the oldest worklog entry.

**Next up**
- **Review `notes/fact-store.md`'s six questions with the user first:**
  - runs or a B-tree;
  - membership;
  - imports written into the store;
  - `Program.facts`;
  - the `?whynot` copy;
  - scope (storage before provenance, recommended).
- **Prototype in `~/.cache/recorder-wt`** against a frozen `efcda71` release
  binary. The proof harness is in `~/.cache/recorder-wt/harness/`.
- **Measure time, `perf stat` and memory before any commit.**
- Re-learned: `cargo build | grep | tail && cp` copied a stale binary after a
  failed build. Use `set -o pipefail`.
- **Open**: `datalog/bugs/015`.

## 2026-09-13 (later that night) — the recorder cut by constants: `pointsto.dl` `?why` 1,428 → 848 MB, every derivation kept

Asked where the derivation-tracking changes stood: references rather than copies
a clear win, one derivation per fact held back unless it is what code-analysis
hits. It is not. Planned, approved, and steps 1–3 built.

**Done** — datalog `cargo test` (every binary), clippy, fmt; each commit green
- **The answer.** Library programs store 1.11–1.12 derivations per fact, 95–97%
  of them first-round. They pay for representation and base-fact bookkeeping,
  not count, and the base set does not need the one-derivation direction.
- `91b730a` — no base set: a held fact with no round stamp is base. **E11**,
  stated from `program.facts`. Mutations killed: `insert_base` stamps round 0; a
  rediscovered fact is stamped. Guard: 4 of 48.
- `ef82398` — a premise is a fact wide (80 → 32 bytes). Every kind but `Fact` is
  boxed, `NoMatch` too, since two inline variants of one shape leave no niche for
  the tag (40). Pinned by `a_premise_is_a_fact_wide`.
- Against a frozen `8c6bc91` release binary:
  - `?why` is byte-identical on 43 §16-corpus goals and both `@grafana/ui` goals;
  - `pointsto.dl` 1,428 MB / 14.7 s → 848 MB / 9.1 s;
  - `callreach.dl` 466 → 389 MB.
- `Deep` draws, with the note's harness rebased in `~/.cache/recorder-wt`:
  idx 129 6.96 → 5.43 GB, idx 119 3.58 → 2.78 GB. Most of what remains is cloned
  premise tuples (3.06 GB on idx 129).
- **The deep run still does not finish**: it failed an allocation at 372 s and
  18.1 GB under a 20 GB cap. In `bugs/015`, with a 16 GB per-commit-tier draw
  seen once and not reproduced.

**Decided**
- datalog §17 2026-07-19 ***Consequences 2026-09-13 (later)***: the reopening
  stands, narrowed to what all-derivations costs a dense program.
- One derivation per fact is not built; the plan was the user's.
- The user's, after the measurements: fact references come next, designed first;
  `bugs/015` keeps waiting on the recorder design rather than a generator cap.

**Removed** — `Model::base`; base facts' round-0 stamps; `Premise`'s inline
payloads; the note's unverified `base` direction; the oldest worklog entry.

**Next up**
- **Fact references next, as a design pass** (the user's). Stable ids reach into
  evaluator storage: `BTreeSet<Tuple>` relations, the seek, the deltas. The
  numbers are in the note's § After the first two cuts.
- **`bugs/015` waits on the recorder design** (the user's): no generator cap.
- Not chosen for now: dropping `api::run`'s fact copy for a `?why` (233 MB on
  `pointsto.dl`).
- Re-learned: `ps -C cc,c++` misses DuckDB's `clang++` workers, so a working
  release build looked hung and was killed once.
- **Open**: `datalog/bugs/015`.
