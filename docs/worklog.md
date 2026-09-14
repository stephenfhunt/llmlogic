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

## 2026-09-14 (late morning) — fact store step 3: premises by row; a missed seek regression; interning next

Asked to approve step 3's design, then build it. A wider measurement found a
regression the gate had missed, and the user chose each direction as the evidence
came in.

**Done** — branch `fact-store`; per-commit gate green at every commit; harness
diffs empty at every engine commit.
- **Step 3.**
  - `7ae01a1`: seeks yield row ids.
  - `4a27aff`: recorder keyed by row, rounds from blocks.
  - `2950331`: premises as `FactRef` (16 bytes), derivations resolved at read.
  - `efda16b`: E12 (proofs least by content) and E10's match clause. Two
    mutations had survived the suite before these.
- **`bugs/015`:** an uncapped per-commit run drew a 19.8 GB runaway, likely B13 at
  `Large`.
- **The regression.** Seek-heavy library queries got slower on the branch; the
  four-program gate never timed them. Bisected to `d9a4c5e`, the runs.
  - Cheap levers measured (E8–E10): only merge factor 8 helped, and it was
    committed as `7ecfc33`.
  - The rest is locality, per `heaptrack`: the joins and copies are the same as
    `efcda71`'s, and cache misses are 2.5×.
- **At `7ecfc33`, against `efcda71`:**
  - `pointsto.dl` 10.5 → 10.0 s; its `?why` 9.2 → 7.7 s, 817 → 654 MB;
  - `callreach.dl` `?why` 1.46 → 0.91 s, 379 → 206 MB;
  - `sparse_800` 1.23 → 0.94 s;
  - `q_coh.dl` +26%, `lib/cohesion.dl` +24%, `lib/flow.dl` +32%.

**Decided**
- The user's: approve step 3's design; merge factor 8, then a B-tree. The B-tree
  was then replaced by value interning, designed first, on E9's and `heaptrack`'s
  evidence.
- The performance gate includes `q_coh.dl`, `lib/cohesion.dl` and `lib/flow.dl`.
  Step 2's acceptance rested on a gate without them (§17 2026-09-13 (later iv),
  consequences (later iii)).

**Removed** — `Model.first_round` (rounds come from blocks); `Premise::Fact`'s
copied fact; `ProofStep`'s borrowed derivation; the step-4 B-tree plan; the oldest
worklog entry.

**Next up**
- **Design value interning with the user.** The hard question: §14 orders symbols
  and strings by content, and runs, seeks and printing rely on that order.
- Record the deep gate on `7ecfc33` (running at session end).
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-14 (morning) — fact store step 2: runs, flat imports, hash membership; accepted at 2.6%

Asked to file `bugs/016` and move to step 2, then chose each fix as the
measurements came in.

**Done** — branch `fact-store`; per-commit gate green at every commit. At each code
commit, the harness diff is empty (1,109 cases) and deep seeds 2 and 3 pass.
- `bfc8324` (trunk): `bugs/016`, three import tests fail without `duckdb`. The
  branch was rebased onto it.
- `d9a4c5e` flat store and sorted runs, with B14c; B12a folded into B14a; nine
  mutations killed. Correct, and 30% slower.
- **Diagnosis** (`notes/fact-store.md` § Step 2, measured): profiles and
  scratch builds E1–E7.
  - Not the cause: cursor allocation, comparison style, merge factor.
  - The causes: the loader's freed per-row buffers, and binary-search membership.
- `5f27f41` imports flat (−79 MB, no time); `de461ce` raw table flat (`pointsto.dl`
  13.5 → 10.8 s); `29378cd` hash row index and B15, five mutations
  (`sparse_800` 1.64 → 0.92 s).
- **`29378cd` against `efcda71`:** `pointsto.dl` 10.45 → 10.72 s; its `?why`
  9.13 → 8.94 s and 825 → 809 MB; `callreach.dl` `?why` 1.44 → 1.30 s;
  `sparse_800` 1.24 → 0.92 s.
- `bugs/015`: an uncapped per-commit run drew a 19.8 GB runaway, likely B13 at
  `Large`.
- Step 3's design, written for review (§ Step 3 design).

**Decided**
- The user's: no per-row vectors first; membership by hash; accept
  `pointsto.dl`'s 2.6% and go to step 3.
- Review answers 3 and 4 were reversed on measurement: imports are flat blocks,
  not `Program.facts` (§17 2026-09-13 (later iv), consequences).
- The deep gate runs seeds 2 and 3 only; seeds 1 and random draw pathological B5.

**Removed** — `seek::tuples_with_prefix` and B12a's own property (B14a carries
it); `retire_delta` and `Model.delta_holders`; per-row vectors in `LoadedTable`
and `RawTable`; binary-search membership; the oldest worklog entry.

**Next up**
- **Review step 3's design with the user, then build it:** `Premise<FactRef>`,
  a row-keyed recorder, proof selection by content, B13 resolved.
- Re-measure `bugs/015`'s `Deep` draws after step 3.
- `pointsto.dl`'s 2.6% sits in seeks through runs. An unordered join seek is the
  separate, audited optimisation.
- **Open**: `datalog/bugs/015`, `016`.

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
