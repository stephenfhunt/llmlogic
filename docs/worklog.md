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

## 2026-08-18 — Termination ships as a *warning*, and the roadmap's own direction loses

The last blocking design session, and it reversed the decision it was convened to
implement. `bugs/004` closes; the open-defect set is **empty for the first time
since the 2026-07-25 spec review**. 386 lib tests (was 383), clippy/rustfmt clean.

**Done**
- **The classification** — `schedule::computed_vars` finds head variables bound to
  an arithmetic-computed value, **transitively**; `lower::value_creating_recursion`
  pairs it with a positive-cycle walk over the stratification edges and warns. Ten
  hand cases pin the table.
- **Eager emission**, without which the design does not work. `api::run_at_reporting`
  reports static warnings *before* `eval` — answers print after the fixpoint, so
  `bugs/004`'s "no output at all" applied to its own diagnostic. A system test reads
  stderr off a live non-terminating process.
- **C10 + a C8 spelling equivalence**, both mutation-verified. C10's oracle is a
  **test-only round cap** (`eval_capped`): a wrong certification fails instead of
  hanging the suite — the shape for any property whose negation diverges.
- **§10's Termination section** (its largest hole), §6's premise made conditional,
  §15's stopping condition, **§2's pillar ratified** scoped, and **§16.12** — the
  first example to exercise a *diagnostic*, half its output on stderr.
  `notes/termination.md` has the proof; `references.md` gains limit Datalog.

**Decided**
- **Classify, do not reject** (user call, reversing 2026-07-25). `path_cost` is
  valid on every acyclic graph, so rejecting it is a false positive on a property
  of the **data**. Three supports found rather than assumed: `bugs/004`'s criteria
  always offered "rejected **or** documented as in-scope"; 2026-08-16's "a slow
  program stays slow" already discounts the DoS argument; and the only checkable
  line between `nat` and `path_cost` over-accepts `p(N) :- p(M), q(_), N = M+1.`
- **The roadmap's rule sketch was unsound**, not merely imprecise:
  `nat(N) :- nat(M), K = M + 1, N = K.` binds the head from a bare variable and
  still diverges. Taint must be transitive; that is what both properties guard.
- **The hatch is deferred with a name**: limit predicates (Kaminski et al., IJCAI
  2017), `declare path_cost(from, to, min cost)` as the surface — a milestone,
  since §6's `T_P`, §9 and §11 all move. A bounded-counter recognizer was rejected
  (cannot reach `path_cost`). Soufflé, checked rather than recalled, is the
  opposite pole: `.limitsize` is documented as a *debugging* directive.

**Removed**
- The whole "Termination & value-creating recursion" ROADMAP item — sketch,
  discrimination table and open question, all superseded by shipped text. §6's
  false finiteness bullet; §10's, §15's and §2's *Not covered* paragraphs on
  termination; §16's "no example exercises a diagnostic" claim.
- `bugs/004` to `bugs/resolved/`; the 2026-08-17 answer-shape entry rotated
  verbatim to `worklog-archive/2026-08.md`.

**Next up**
- **§6's extension** — the last design session, three unknowns lighter and down to
  aggregation alone. Then the **profile**, still gating two items.
- Open: **limit predicates** (new, queued); the benchmark harness's home.

## 2026-08-17 — The named query ships, and one of its planned sites turns out not to be one

The brief frozen earlier the same day, built. 439 pass (was 426), 1 ignored, clippy
and rustfmt clean, `--no-default-features` still builds. The projection hazard —
recorded 2026-08-16 as unsolved in both engines — is closed for any query that
takes a name.

**Done**
- **`?- adult: person(N, A), A >= 18.`**, desugared in `lower_query` to the rule
  `adult(N, A) :- …` plus the one-atom query `?- adult(N, A)`, reusing the same
  `VarScope`. One `Option<Ident>` on `ast::Query`, one parser arm, one printer arm.
- **`ir::Query` and `api.rs` were not touched**, which is the finding: a one-atom
  query already prints under its atom's name, so the answer-shape rule settled
  hours earlier needed **no third arm** — empty projection included, where the
  ground head `name(true)` rides the same path.
- **C8 gains `c8_a_named_query_matches_its_desugared_rule`**, oracle = the
  desugaring as program text, over `arb_answer_shape_case` widened to carry the
  projection *it* emitted. Eight lowering unit tests, three parser tests, **§16.11**
  with its named system test and output pinned byte-for-byte.
- **Swept**: §5's grammar, §14's shape rule + both hazard paragraphs + footer,
  §16.10's closing line, `testing.md` C8, `ROADMAP.md`, the note, both agent docs.

**Decided**
- **The name is a *definition*, not a label**, and that is the whole feature.
  Carrying an `Option<Ident>` to the printer emits the same bytes while the relation
  does not exist — so `eligible(N) :- adult(N, _).` could not read it and provenance
  would have nothing to explain. §16.11 pins that rule.
- **The guard's line is *defined or declared*, not *mentioned*.** A name merely
  referenced in a body stays takeable, defining it being exactly what the equivalent
  hand-written rule does; rejecting it would make the sugar inexact. Needed a
  `defined` set in pass 1, `by_name` conflating the two.
- **Four mutations, two worth keeping**: a head one column short of the projection
  is caught by the **existing** IR well-formedness check, not by anything added
  here; and dropping the empty-projection `true` prints `ans().`, which §5 does not
  accept — so the argument keeping output re-parseable is §5's own, now measured.
- **A named query publishes *every* variable its body binds** (`adult/2` vs a
  rule's `adult/1`). Verified, and documented for consumers rather than filed.

**Removed**
- The 2026-08-16 *`as` cast* entry rotated verbatim to `worklog-archive/2026-08.md`.
- §14's *Not covered* claim that a query "cannot yet be given a name", and the
  standing "this hazard is unsolved" sentence — both had stopped being true. The
  synthesized-answer-collision item narrowed to the *unnamed* case instead of being
  deleted: naming already tells two queries apart.

**Next up**
- **Termination** (blocks `bugs/004`), then **§6's extension** — now the head of the
  queue. **Profile** is unblocked with a ranked list and gates two further items.
- Newly unblocked, *not* taken: **an unnameable query as an error** (axis 1).
  Recovery is now "prepend a word", which is what made strictness unaffordable.

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