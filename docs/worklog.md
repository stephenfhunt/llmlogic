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

## 2026-09-14 (afternoon) — fact store step 4: values interned; faster than the baseline everywhere

Asked, in the interning review, for both kinds interned and interning alone,
measured before anything more is decided.

**Done** — branch `fact-store`; per-commit gate green; harness diff empty
(1,109 cases); deep seeds 2 and 3 pass 525/525 at 351 MB.
- `60a5495`: `Value::Symbol` and `String` hold a `Sym`, interned once for the
  process; a `Value` is 24 bytes; `Value::symbol`/`string` constructors.
  - **A17**, interned equality is content equality, mutation-verified. Ordering by
    pointer escapes every B-series differential; only A17 and hand tests catch it.
- **Measured against `efcda71`**, one sitting, median of 3:
  - `pointsto.dl` 10.6 → 7.4 s; its `?why` 9.1 → 6.0 s, 815 → 410 MB;
  - `callreach.dl` `?why` 1.46 → 0.60 s; `sparse_800` 1.22 → 0.41 s;
  - `q_coh.dl` 6.4 → 4.1 s, `lib/cohesion.dl` 7.8 → 4.8 s, `lib/flow.dl`
    23.2 → 12.6 s.
- **`bugs/015`:** seed 2 unskipped on `7ecfc33` failed an allocation at 14.2 GB,
  with only B13 at `Deep` still running. Which test holds the memory is now
  established.

**Decided**
- The user's, in review: accept the interner's process-lifetime leak; intern
  symbols and strings both; interning alone, measured first.
- §17 2026-07-19's "interning deferred" is marked ***Superseded***.

**Removed** — `Value`'s owned `String`s; `coerce_borrowed`, folded into `coerce`;
the oldest worklog entry (rotated).

**Next up**
- **The user's call: merge `fact-store`** (fast-forward), or more seek work first.
  No gate program is slower than `efcda71`.
- The gate's random-seed deep run on the interned tip (started at session end).
- **Open**: `datalog/bugs/015`, `016`.

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
