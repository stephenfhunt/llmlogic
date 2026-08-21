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

## 2026-08-20 — The profile, and the scan that was hiding everything else

The last of the stock-take's four. It answered pillar 1's question and then
falsified the ranking that made the question urgent: **the recorder is a memory
cost, and the thing actually burning the clock is a full-relation scan nobody had
named.** Long form, with method and every table, in
[`datalog/notes/profile-2026-08-20.md`](../datalog/notes/profile-2026-08-20.md).
No engine code changed.

**Done**
- **The recorder is priced** — 70–78% of peak RSS, 2–24% of wall clock, its time
  share *falling* as the workload grows (2% at `sparse_800`). Four scratch builds in
  a throwaway worktree, each guarded by byte-identical answers on all 26 programs.
- **The finding neither note had**: a relation's `BTreeSet` is already ordered by
  column, so a bound prefix is a contiguous range — and nothing seeks it. A ~20-line
  prototype is **10.2× on `sparse_800`, 12.7× on `join_4000`, 9.0× on `agg_50000`**,
  takes sparse from n^4.16 to n^2.14, and passes all 422 library tests.
- **Aggregation**: mechanism confirmed (goal rescans per group), quantification
  corrected; under the seek, groups become free and rows go linear.
- **`samply` sampling** put ~70% of `agg_50000` in value comparison, collapsing to
  2% under the seek.

**Decided**
- **The recorder question must be re-measured against the post-seek engine.** Its
  share there is **50–60%**, not 2–24%. Deciding today would price pillar 1 at 2% of
  a run that will not exist. §17's 2026-07-19 entry is annotated with both numbers.
- **Interning is a memory item only if the seek goes first** — a nuance the A/B
  builds got wrong and the sampler corrected. Symbol *length* costs 7.5% at 16× the
  width, but value *comparison* is 70% of an aggregate today and 2% after the seek.
- **`cross-engine-benchmark.md`'s wall clock does not reproduce at the top end.**
  Rebuilding `0356b04` on the same machine: `sparse_800` 43.25 s vs its 65.14 s,
  `agg_50000` 1.28 s vs its 3.77 s, while peak RSS reproduces to 0.3%. So **`tsdl`
  does not win `agg` outright** — 1.73 s ours vs 2.92 s theirs today — and that claim
  was cited in `ROADMAP.md`, `v1-scope.md` and §17.
- **Measure with `min`-of-N, not best-of-3-and-a-spread-claim.** `agg_50000` spans
  61% across consecutive runs of one binary — not thermal, not core placement, and
  it hits every build equally.

**Removed**
- The Performance section's framing that ranked the recorder first, and the "one
  shape a sibling engine wins outright" clause from `ROADMAP.md` and `v1-scope.md`
  both. Nothing deleted from `notes/`: both earlier performance notes are
  append-annotated, since their numbers record what was believed when.

**Next up**
- **Land the seek** — unreviewed, and it does not touch the negated-atom or
  aggregate-goal arms, which still scan. Seeking makes body order matter *more*, so
  its interaction with join-order selection needs looking at.
- **Then** the three-way decision session (recorder / query surface / row
  provenance), against the post-seek engine.
- Still open: **E5**/**E6**, and `--no-default-features` does not pass its own test
  suite (two temporal tests, found while profiling, now a ROADMAP item).

## 2026-08-20 — `bugs/007` closes, and the aggregate becomes a fold over a multiset

The defect the morning's audit filed, fixed the same day in four commits.
**538 tests green**, clippy and rustfmt clean, `--ignored` back to no known
failures, and the open defect set empty again.

**Done**
- **The sort** — `fold_aggregate` folds its present values in §14 order, so an
  aggregate is a function of its witness multiset and the goal's literal order
  cannot reach the answer. `fold_is_permutation_invariant` loses its `#[ignore]`.
- **Compensated floats** — Neumaier summation, shared structurally by
  `sum_values` and `avg_values`, with a finiteness guard: `F64::new` permits
  infinities, so an overflowing sum is already an answer and `inf + -inf` in the
  correction term would have made it an error.
- **Wide ints** — `sum` over ints and durations accumulates in `i128`, so
  overflow is a property of the total: `{ -MAX, -MAX, MAX, MAX }` sums to `0`
  where every fixed association, the sorted one included, errored.
- **Swept**: §9 (the fold-order rule, a new normative home), §17 ×2, `bugs/007`'s
  resolution and `git mv`, `ROADMAP.md` ×3, `testing.md`'s B11 and two coverage
  rows.

**Decided**
- **Sorting alone was rejected** — the bug file's own recommendation, and enough
  to close the defect. §14's order is by value, not magnitude, so it would have
  canonised `0.0` for `{ 1e16, -1e16, 0.1 }`, the worse of the two answers the
  defect reported. The sort carries determinism, compensation accuracy.
- **The `i128` half is not a bug fix**, and took its own commit: spec and code
  agreed that the fold inherited §8's operand-pair overflow, so it is a §9
  widening — and the one option none of the file's four candidates named.
- **The float half was not split into a second defect.** One contradiction, one
  root cause, one fix, one acceptance test; two files sharing a fix is the
  stale-cross-reference failure mode `bugs/README.md` exists to prevent.
- **B5 cannot see this defect, and its doc comment now says so.** Float-pooling
  it was still right, but with the sort deleted it stays green: a single-atom
  goal enumerates in the relation's own order, which *is* the sorted order. An
  equivalent mutant, measured rather than assumed.

**Removed**
- ROADMAP's one-open-defect block and its known-`--ignored`-failure paragraph,
  including the parenthetical recording which window the previous line was true
  for — narration a current-state document should not carry.
- `avg_values`' private float accumulator, which duplicated `sum_values`' fold.
- The 2026-08-18 entry, rotated verbatim to `worklog-archive/2026-08.md`.

**Next up**
- **The profile** — unblocked, and now genuinely next: the defect that ran ahead
  of it is closed.
- Still open: **E5**/**E6**, and §17's period arithmetic, whether a truncated
  value should print as its period, and the `avg`-over-mixed-column question.

## 2026-08-20 — A coverage audit finds `bugs/007`, and six laws that were never stated

No feature work: an audit of the property suite, asked for before the profile,
which turned up one wrong-answer defect and six algebraic laws with nothing
asserting them. **533 tests green**, clippy and rustfmt clean; the one
`--ignored` failure is `007`'s acceptance criterion, by design.

**Done**
- **`bugs/007`** — `sum`/`avg` fold in witness-*enumeration* order, so two
  spellings of one goal give `r(0.0)` and `r(0.1)` over floats, and an answer
  versus **exit 2** on the int overflow check. §17's 2026-07-25 entry is
  ***Falsified***: its standard holds, its generalisation does not — body order
  is unobservable only where the fold is associative over the value type.
- **The root cause**: `arb_constant` never followed the language past five
  types. Widened to eight, and *measured* — swapping `Date` and `Bool` in
  `ir::Value`'s variant order is a mutation **A4 passes on the old generator and
  fails on the new**.
- **Six properties**: **C11** negation is antitone by parity (the half B4
  excludes rather than covers), **C12** join/union idempotence, **C13** statement
  order changes nothing — verdict, types *and* model, **C14** the derivations are
  order-invariant too, **B11** the aggregate monoid laws, **D5** §14's closure at
  the program level. Plus §9's temporal folds, §8's cast table widened 5×5 → 8×8,
  and C10's eleventh shape for §10's `std`-builtin exemption.
- **`testing.md` swept**: F1–F7 and C8 were green and still `[ ]`, and one entry
  described a test that does not exist (`filtered_atom_query_…`, superseded
  2026-08-17). Coverage map gained four rows.

**Decided**
- **Widen the generator globally, then narrow deliberately.** The `unreachable!`
  that hid temporal was an accident of ordering; a narrowing with a comment is a
  decision. It found no defect in §8/§9's temporal rules — the design was right
  and only the coverage was thin.
- **Record a mutation that could not be aimed, rather than a tidier one.** C12,
  D5 and C11 each have no unique kill and say why. A plausible-sounding mutation
  nobody ran is what rule 3 exists to stop.
- **An equivalent mutant is evidence, not a gap** (C13's reversed gather order
  reddens *nothing* — which is what confluence means); and **mutation-verify
  against the full suite**, since the `holds.` mutant is invisible to `--lib`.

**Removed**
- Ten proptest regression seeds recording deliberate mutants rather than defects
  (kept: `007`'s). The stale `filtered_atom_query_…` entry, replaced by what
  shipped. ROADMAP's "no known `--ignored` failures" and "defect set is empty"
  lines, both now false.

**Next up**
- **`bugs/007`'s fix is a user call** — sort witnesses before folding (the bug
  file's recommendation) versus documenting the restriction. Blocked on it: the
  float widening of B1's and B5's aggregate arms, `007`'s second acceptance half.
- **The profile**, unchanged and unblocked — what this session ran ahead of.
- Still open: **E5**/**E6**, whose hole now covers every temporal derivation.
