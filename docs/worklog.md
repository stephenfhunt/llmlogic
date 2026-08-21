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

---

## 2026-08-21 — The seek lands, and body order becomes the thing that matters

The profile's finding, shipped: **the positive-atom arm and the anti-join seek a
bound prefix instead of scanning the relation**, in `src/engine/seek.rs` with
`testing.md` **B12** pinning the equivalence. Answers byte-identical on all 26
corpus programs, 553 tests green, clippy and rustfmt clean.

**Done**
- **Up to 13.4×, and an exponent on three shapes.** `join_4000` 13.4×,
  `sparse_800` 10.8×, `agg_50000` 9.3×, `chain_800` 2.4×, `negation_400` 2.85×.
  Sparse 400→800 goes n^4.25 → **n^2.17**, join 2000→4000 n^2.07 → **n^1.00**.
  Peak RSS identical to the megabyte — same model, same recorder.
- **The anti-join seeks too**, which the prototype did not. It needs the *opposite*
  `absent` rule: a refutation compares structurally (§4/§7), so `absent` is a legal
  key there where a join calls the atom impossible. Two prefix builders, one per
  notion of sameness. Worth a further 1.22× on `negation_400` — and recording that
  it is *small* is the point: the anti-join was not what was left in those shapes.
- **B12a/b/c**, each mutation-verified and each mutation written on its catalog
  line: `.skip(1)` on the range; a prefix that extends past an unbound variable;
  `map_while` → `filter_map` in the refutation's key.
- Swept: §17 (the decision, plus a ***Consequences*** on the 2026-07-19 recorder
  entry), `ROADMAP.md` ×5, `testing.md` (B12 + a coverage-map row), and an
  annotation on `notes/profile-2026-08-20.md`.

**Decided**
- **The differential is not the guard here.** Both callers re-check every
  candidate, so an over-yield is invisible and only an under-yield loses answers —
  B1 cannot see the failure mode this change has. B12b/B12c are the tests that
  fail if contiguity breaks, and every recorded mutation is under-yielding.
- **Secondary indexes are rejected for now, and priced.** The seek is the
  relation's own column order, so a bound column that is not *leading* still
  scans. Fixing it means a second copy of every relation on top of a recorder
  already at 78% of peak RSS — which is the next decision, not this one.
- **Body order is now 29×, where it was 2.0×.** `join_4000` written in the
  pessimal atom order takes 0.86 s against 0.03 s, and the seek buys that order
  *nothing*. The profile's "seeking makes body order matter more" has a number.
  `schedule.rs` is deliberately untouched: reordering positive atoms is observable
  on the error path, so it is its own session.

**Removed**
- **The per-group schedule hoist — written, measured, dropped.** `literal_order`
  runs once per group inside the aggregate arm, and the guess was that the seek
  would expose it. It is noise: 0.7% at `agg_50000`, unchanged at 50 000 groups of
  one row each. A `Plan` type for a non-existent win, so it went back out.
- `ROADMAP.md`'s aggregation item, subsumed and closed, and the interning item's
  "memory if the seek goes first, time if not" conditional — now just *memory*.

**Next up**
- **The three-way decision session** (recorder / query surface / row provenance),
  against this engine — but the 50–60% was measured on the prototype, so re-run it
  against the shipped code first.
- Still open: **E5**/**E6**, and `--no-default-features` does not pass its own
  test suite (two temporal tests).

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
