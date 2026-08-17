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

## 2026-08-17 — Racing the sibling engine: the benchmark item closes, two of ours break

A for-fun cross-engine comparison against `~/code/tsdl` that came back with three
things reasoning would not have produced. No `src/` change (426 pass, 1 ignored);
the deliverable is `notes/cross-engine-benchmark.md`, with the harness kept outside
the repo at `~/datalog-cross-engine-bench-2026-08-17/`.

**Done**
- **A 26-program corpus both engines run byte-identically**, generated from a seeded
  script inside the two dialects' common subset. All 23 that ran on both agreed, and
  both agreed with a Python oracle that never calls either engine — **the first
  differential test between two independent implementations of this language**.
- **The measurement**: whole-process wall and RSS, best-of-3, 120 s cap, six shapes
  at four to six sizes. Validated against a tsdl figure measured by an unrelated
  route before the harness existed (15.79 s vs ~16 s).
- **ROADMAP's *Profile the engine* loses its blocking half** — the repeatable
  benchmark `performance-baseline.md` asked for exists, answered its "the fact base
  needs a home" the cheapest of the three ways it listed (regenerate, don't commit).
- **Two §17 ***Consequences*** annotations**: the 2026-08-16 naive-first decline
  (measured, and it was an exponent) and 2026-07-19's provenance recording (the
  third option now has a price tag, from the outside).

**Decided**
- **The chain gap is an exponent, not a constant**: ~n^2.4 here against ~n^3.8 there,
  with closure output itself n². Semi-naive vs naive doing what the textbook says,
  measured — and the front end is only ~4× apart (480k vs 110k facts/s), so the
  whole separation lives in the fixpoint and none of it in reading the program.
- **Two shapes where this engine is the bad one, neither visible without a second
  engine.** `agg` is the one cell tsdl *wins*: 2.5× the rows at fixed group count
  costs 9.6×, while groups scale sublinearly — a rescan of the aggregated relation,
  now its own ROADMAP item. And `sparse_800` is a 22× cliff at 1.2 GB, cycles not
  size: the chain at the same node count is 3.78 s against 65 s.
- **The provenance recorder is the top profiling target, now on outside evidence.**
  tsdl under `LINEAGE` costs **13×** on a cyclic graph, 1.3× on flat ones — and ours
  is unconditional, so every table *understates* this engine, worst on exactly the
  shapes that blew up. `performance-baseline.md` guessed it first and never measured
  it; still needs a temporary build, since there is no flag.
- Throughput is 5–14× that file's ~15k tuples/s on short values where its workload
  had long Rust identifiers — evidence *for* its interning lead, not proof.

**Removed**
- The 2026-08-16 *Building the decided backlog* entry rotated verbatim to
  `worklog-archive/2026-08.md`. Otherwise nothing — the one backlog item retired
  here was retired by doing it.

**Next up**
- **Profile**, now unblocked and with a ranked list: recorder, aggregation's rescan,
  value representation, cyclic graphs (`sparse_800`, not the chain everyone uses).
- Unchanged and still ahead of it: **named queries**, then **Termination** (blocks
  `bugs/004`), then **§6's extension**.
- Open, not taken: whether the harness belongs in the repo — it needs a `tsdl`
  checkout for the full sweep, though the generator and our half stand alone.

## 2026-08-17 — The answer shape settles, and axis 4 turns out to decide axis 2

The design session the last three entries deferred to the user. Five axes settled,
the shape built, and one axis deliberately left unbuilt with its brief written.
426 pass (was 421), 1 ignored, clippy and rustfmt clean, `--no-default-features`
still builds.

**Done**
- **The shape, in `answer_lines` alone** — set **equality** between the positive
  atoms' variables and the projection, replacing a one-directional check. Three
  forms: one atom substitutes beside any number of non-binding literals (the
  2026-08-03 widening, unfrozen); a **ground conjunction** emits all its atoms,
  sorted by name then value so body order cannot change the bytes; a body with no
  answer variables and nothing to substitute answers **`holds(true).`**
- **C8's property** (`arb_answer_shape_case`), oracle filtering the generator's own
  fact list. **Four mutations killed**, including restoring the one-directional
  guard — caught only because the generator has a fourth shape binding a variable no
  atom mentions. Without it the property restates the rule instead of guarding it.
- **§16.10 with its named test**, the second example done §16.9's way; §14's rule,
  hazard paragraph and footer; two §17 decisions and three amendments; the note's
  axis-by-axis outcome; `testing.md`; the two agent-facing restatements.

**Decided**
- **Axis 4 settled axis 2, which is why the axes were taken one at a time.**
  `-q 'flag(_, X), truthy(X)'` already prints `answer(true).`, so adopting tsdl's
  boolean `answer` would have made two meanings byte-identical — a collision they
  do not have (theirs is boolean-only) and we would have created. Hence `holds/1`,
  and **no `holds(false)`**: silence has meant no since 2026-07-22.
- **Axis 5 is the fix for axis 3**, which nobody in either project had connected:
  named output wears no source relation's name. A *language* form, not `-q` sugar —
  the name must survive lowering, and recomputing the projection in `api.rs` is the
  second classifier §13's lexer move exists to prevent. Its own session.
- **The lost question was three shapes wide, not one.** A bare comparison, a
  negation-only body, and `?- p(_).` all printed nothing either way; the last
  appears in neither project's discussion and is the likeliest to be hand-typed.
- **Rejected, reversing this session's own recommendation**: a warning when a
  program reads `answer` facts. The hazard is a two-run merge, indistinguishable
  inside one run from legitimate composition, so it would fire on correct code.

**Removed**
- §14's *multi-atom existence check* paragraph and its "the answer shape is under
  review" footer, both closed by what shipped; the note's *"no recommendation is
  recorded, deliberately"* and its frozen implementation brief.
- Two ROADMAP items closed as one (the widening and the existence check).
- The 2026-08-16 tsdl entry rotated verbatim to `worklog-archive/2026-08.md`.

**Next up**
- **Named queries**, brief written, and the **`%` comment** naming which query a
  synthesized answer answers — sequence the latter with §11's comment rendering so
  the format is designed once.
- Unchanged and now the head of the queue: **Termination** (blocks `bugs/004`),
  then **§6's extension**. Then the profile, which gates two more items.

---

## 2026-08-16 — The `as` cast ships, and its deferred question splits in two

Expressions is now empty: the last queued non-design item is built. 421 tests
pass (was 410), 1 ignored, clippy and rustfmt clean, `--no-default-features`
still builds.

**Done**
- **`Expr as type` end to end** — a `cast` precedence level between `mul` and
  `primary`, `ast::ExprKind::Cast` / `ir::Expr::Cast` (it survives lowering; a
  per-row conversion has nothing to hoist), a typecheck arm fixing the result
  type *without* unioning the operand, `apply_cast`, and a printer that wraps a
  cast's **operand** — the parenthesization case arithmetic alone cannot reach.
- **The literal-grammar classifier moved to `lexer.rs`** (its own commit) so
  `"30" as int` and an imported `30` cell are the same function — §13's "no
  second classifier exists" is now structural rather than a convention.
- **§16.9, the first example that names its test** and pins its output in a fence
  compared byte-for-byte — the shape the other eight should be retrofitted to.
- **Tests**: B9 (§8's table as an *independent* oracle — `naive.rs` deliberately
  calls `apply_cast`, so a differential would agree with a wrong table forever),
  three algebraic laws incl. the `as string as T` round trip, B10 as a
  biconditional, the printer's acceptance partner, and a non-vacuity guard. Seven
  mutations run and recorded.

**Decided**
- **A failed conversion splits, which the question's either/or did not admit**:
  `absent` where there is *no value to represent* (`"abc" as int`), an error
  where representing it would be **lossy** (`2.5 as int`, `as float` above 2⁵³).
  The second half extends §8's ratified widening rule to narrowing unchanged.
- **The deciding argument was expressibility, not reporting.** With an error rule
  a dirty column has *no writable query* — this language has no convertibility
  predicate, string ops having been rejected rather than deferred — so `absent`
  is what puts the guard back in existing vocabulary. Support: `eval_expr` is
  `?`-propagated, so an error emits nothing, not even facts already derived. The
  accepted cost, malformed reclassified as missing, is a new ROADMAP item.
- **The conversion table was as open as the failure mode**, and needed settling
  in the same sitting: text is the universal intermediary, and `bool as int` and
  friends have *no* conversion rather than a debatable one.

**Removed**
- **ROADMAP's Expressions essay**, 55 lines to 30. Its conclusion — "so the fix
  is an *explicit* `float(X)`" — had stopped being true the moment `as` was
  ratified: the exact stale-claim failure `docs/rules/editing-docs.md` exists for.
- §8's *Still open* bullet and §4's "deliberately still open"; two copies of the
  i128 exactness test, now one shared function; one of three `type_label`s.
- The 2026-08-03 entry rotated verbatim to `worklog-archive/2026-08.md` (new).

**Next up**
- Unchanged and still the user call: **the answer shape** (blocks the widening),
  then **Termination** (blocks `bugs/004`), then **§6's extension**.
- New here, both small: making the malformed/missing reclassification visible,
  and a type-clash diagnostic that names `as` — there is a concrete fix to
  suggest now, which there was not before. The **§16 retrofit** also got cheaper:
  §16.9 is a pattern to copy rather than a shape to invent.

