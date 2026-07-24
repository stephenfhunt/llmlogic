# datalog — roadmap & backlog

The single index of discrete work items for `datalog/`. This is the *what's
open and what state is it in* view; the other docs specialize:

- **`spec.md` §17** — design rationale (why/what for each decision & open question).
- **`docs/worklog.md`** — session handoff (what happened, newest first).
- **this file** — the item index: one line per item, a status, and a pointer.

When an item's design or status changes, update it here *and* its §17 detail.
Status vocabulary: **queued** (agreed, not started) · **designing** (needs a
design pass before it can be built) · **building** (implementation underway) ·
**parked** (deliberately deferred, low priority / awaiting a trigger) ·
**shipped**.

## Shipped milestones

The evaluation-first roadmap (decided 2026-07-10; rationale in `AGENTS.md` and
§17). One line each; detail in `AGENTS.md` "Implementation roadmap".

1. **AST + IR** — surface AST + positional core IR + lowering. ✅ 2026-07-19
2. **Core evaluator** — stratified semi-naive fixpoint with provenance recording. ✅ 2026-07-19
3. **Named-argument lowering** — named literals desugar to positional IR. ✅ 2026-07-20
4. **Stratified negation** — Ullman relaxation numbering + anti-join eval. ✅ 2026-07-20
5. **Lexer + parser** — hand-rolled, zero-dep; canonical printer; `parse→lower→typecheck→eval`. ✅ 2026-07-22
6. **Agent CLI** — one-shot `-q` queries; the Claude Code skill. ✅ 2026-07-23
7. **§13 imports** — data (CSV/JSONL/Parquet/URL via DuckDB) + module imports. ✅ 2026-07-23
8. **First-class absent value** — two-valued `absent`; `is [not] absent`; uniform null→absent imports (type-neutral). ✅ 2026-07-24

## Open backlog

### Next up (highest-signal)

- **§9 aggregation** — count/sum/min/max + grouping. The paired expressivity
  pillar for source analysis; every dogfood analysis so far was a threshold or
  existence check for lack of it. The absent interaction is settled
  (skip-but-report, §9/§17 2026-07-24) but **not yet built** — it lands with the
  aggregates; the absent *value* itself shipped (milestone 8). Aggregate
  syntax/grouping still open. _queued._ — §9.

### Aggregation (§9)

- **Aggregate expression syntax** — `count { Var : Goal }` is provisional, and
  `:` now also delimits named args, so it will be revisited. _queued (with §9)._ — §9.
- **Aggregation vs recursion** — how far to take recursive aggregation
  semantics. _queued (with §9)._ — §9.

### Provenance surface (§11)

- **Provenance query syntax** — `?why <fact>` is provisional across CLI + API;
  also decide the proof-tree JSON encoding. _queued._ — §11/§14.
- **Provenance as facts** — emit `?why` output as ground derivation-edge facts
  so provenance itself pipes back in (the Datalog-in/out closure). _queued._ — §11/§14.
- **Semiring provenance under negation** — `?whynot` with minimal repairs,
  tropical cheapest-proof selection; sketch in `notes/semiring-provenance.md`.
  _parked (research)._ — §11.

### Import follow-ons (§13)

- **Database loading** — SQLite/DuckDB files via the reserved `table "…"`
  grammar; Postgres via DuckDB attach. Grammar ratified, loading deferred until
  a real consumer. _queued._ — §13.
- **TSV** — an easy format add, deferred with database loading. _queued._ — §13.
- **Filter pushdown for large sources** — v1 eagerly materializes every import;
  push selections into SQL when a consumer hits the wall (the path/`table`
  syntax leaves room). _queued._ — §13.
- **Module namespacing** — v1 module imports share one global namespace;
  qualified names / visibility deferred until needed. _queued._ — §13.

### Performance (after §9 + optional/absent)

Deliberately sequenced **after** aggregation and the optional/absent value: tune
a feature-complete surface rather than re-profiling as core semantics change
(both add evaluation paths that would move the hotspots).

- **Profile the engine** — it has never been profiled. The USDA dogfood put
  `foundation × 170k-measurement` joins at ~11–20 s in release, but the cause is
  unmeasured (join strategy? the semi-naive fixpoint? hashing? provenance
  recording? import vs. eval split?). Stand up a repeatable benchmark and a
  profile *before* optimizing anything. _queued (after feature-complete)._ — engine.
- **Parallelism** — assess how much of semi-naive evaluation and joins can go
  parallel (independent rules within a stratum, partitioned/hash joins) while
  preserving the deterministic canonical output and full provenance recording,
  which are load-bearing guarantees. Scope follows from the profile. _queued
  (after profiling)._ — engine.

### Errors & API edges (§12/§14)

- **Machine-readable error taxonomy** — flesh out §12: codes, spans, suggested
  fixes over the current structured-prose errors. _queued._ — §12.
- **`--format json` scope** — a documented future *edge* feature (structured
  errors, provenance); the data path stays Datalog-native. _parked (low value)._ — §14.
- **`serde` for the API** — decide as §14 stabilizes. _queued._ — §14.

### Agent skill

- **`skill/recipes/source-analysis.md`** — a source-analysis recipe, now
  unblocked by §13 imports. _queued._ — skill.
- **"Big external fact base" demo** — the motivating import demo; unblocked by
  §13 (the USDA dogfood is a first pass). _queued._ — skill.
