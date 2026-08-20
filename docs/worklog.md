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

## 2026-08-19 — Temporal values ship, and a builtin turns out to be a relation

S4 read "met **except dates**"; it now reads met. Three primitive types with
`@`-sigilled literals, one arithmetic rule, and `std/time` behind a gate. Design
and rejected alternatives in
[`notes/temporal-values.md`](../datalog/notes/temporal-values.md); two §17
decisions, both annotated with what building them taught the same day.

**Done**
- **`src/temporal.rs`** — civil dates, civil timestamps, exact durations, and
  Hinnant's calendar algorithms. **Zero new dependencies**, which is affordable
  only because the design excludes zones and calendar durations.
- **§8's algebra as one rule** — points and vectors — replacing a table to
  memorize. `duration / duration → float` is the only route from a duration to a
  number, so the sibling engine's `172800000` finding is excluded *by
  construction* rather than by a paragraph in a guide.
- **`std` modules**: `std/` is a reserved virtual path prefix; a builtin is a
  **relation**, which is what dodges the `ident (` ambiguity that ruled out
  `float(A)`. `std/time` ships; `std/math` and `std/text` are designed, not built.
- **§13 types temporal columns** — CSV by the literal grammar, Parquet from its
  declared type. §16.14 is the worked example, with a system test and a
  pipe-it-back-in test.
- **Properties T1–T6**, all six with mutations recorded.

**Decided**
- **The sigil is decided by §14's closure, not taste.** Output must re-parse, so
  a computed date needs a spelling. Bare ISO was rejected because `2026-08-19`
  already evaluates to `1999`.
- **`timestamp as date` stays a lossy error**, with `truncate` named as the fix —
  the one tension resolved *for* an existing rule. §16.14 records the cost.
- **The gate buys the short names**, not safety: `year`/`month`/`day` are the
  names a program wants *and* the names a column has.
- **A duration is never inferred from any source** — reversing what §13 said this
  morning about `INTERVAL`. Reading DuckDB's `1 day 02:00:00` would mean a second
  duration grammar, and one grammar is what keeps reading and rendering inverse.

**Removed**
- §13's "date/time-like types become their ISO text as strings", §4's *Not
  covered* temporal clause, `duckdb.rs`'s VARCHAR cast for `DATE`/`TIME*`, and
  §8's scan-ahead candidate for the `ident (` ambiguity — **withdrawn**, not
  parked: a relational spelling means the ambiguity never arises.
- `print_type`'s duplicate type-name list, now `TypeName::keyword`'s.

**Next up**
- **The profile**, with pillar 1's question and the row-provenance trade attached
  — the last of the stock-take's four, and `EXPERIMENTS.md` still sits alongside
  it as the thing v1 is defined against.
- Open, in §17: period arithmetic ("same day next month" is not expressible),
  whether a truncated value should print as its period, and the `avg`-over-mixed
  column question T5 does not reach.
