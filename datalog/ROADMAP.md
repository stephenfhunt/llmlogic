# datalog — roadmap & backlog

The single index of discrete work items for `datalog/`. This is the *what's
open and what state is it in* view; the other docs specialize:

- **`spec.md` §17** — design rationale (why/what for each decision & open question).
- **`docs/worklog.md`** — session handoff (what happened, newest first).
- **`bugs/`** — one file per **defect**; `bugs/*.md` is exactly the open set
  (conventions in `bugs/README.md`). This file holds what is *missing*; `bugs/`
  holds what is *wrong*.
- **this file** — the item index: one line per item, a status, and a pointer.

When an item's design or status changes, update it here *and* its §17 detail.
Status vocabulary: **queued** (agreed, not started) · **designing** (needs a
design pass before it can be built) · **building** (implementation underway) ·
**parked** (deliberately deferred, low priority / awaiting a trigger) ·
**shipped**.

Every open item also carries a **v1** / **post-v1** tag — an orthogonal axis to
status, and a *classification*, not a restatement: **`spec.md` §1 defines what v1
means** (S2–S6 hold and S1 has been measured at least once), and
[`notes/v1-scope.md`](notes/v1-scope.md) holds the per-item argument. **post-v1
means "does not block v1", never "unwanted".**

## Shipped milestones

The evaluation-first roadmap (decided 2026-07-10; rationale in §17). One line
each; detail in §17 and `docs/worklog.md`.

1. **AST + IR** — surface AST + positional core IR + lowering. ✅ 2026-07-19
2. **Core evaluator** — stratified semi-naive fixpoint with provenance recording. ✅ 2026-07-19
3. **Named-argument lowering** — named literals desugar to positional IR. ✅ 2026-07-20
4. **Stratified negation** — Ullman relaxation numbering + anti-join eval. ✅ 2026-07-20
   Two follow-ons, both shipped: negated atoms **join the dependency schedule**,
   so a computed argument means the same in all three spellings (✅ 2026-07-25,
   closed `bugs/001`, property **C7**); and the anti-join is a **structural
   membership test**, so `p(X), not p(X)` derives nothing (✅ 2026-07-29, property
   **C9**). Both changed §7/§10 normative text — §17 has the rationale, including
   why the second was resequenced behind the first.
5. **Lexer + parser** — hand-rolled, zero-dep; canonical printer; `parse→lower→typecheck→eval`. ✅ 2026-07-22
6. **Agent CLI** — one-shot `-q` queries; the Claude Code skill. ✅ 2026-07-23
7. **§13 imports** — data (CSV/JSONL/Parquet/URL via DuckDB) + module imports. ✅ 2026-07-23
8. **First-class absent value** — two-valued `absent`; `is [not] absent`; uniform null→absent imports (type-neutral). ✅ 2026-07-24
9. **§9 aggregation** — the canonical five (`count`/`sum`/`min`/`max`/`avg`), set-builder `op { Expr | Goal }` syntax, implicit grouping, skip-but-report via provenance, stratified (recursion rejected). ✅ 2026-07-24

## Open backlog

> **Open defects live in [`bugs/`](bugs/)** — the set is **empty as of
> 2026-08-18**, for the first time since the 2026-07-25 spec review. **No design
> session blocks anything**: §6's extension, the last one, shipped 2026-08-18.
>
> **§1 is written and §2 ratified (2026-08-18), so every item below is now ruled
> v1 or post-v1** against §1's success criteria — the argument per item is in
> [`notes/v1-scope.md`](notes/v1-scope.md), and the evidence that prompted the
> exercise in [`notes/taking-stock-2026-08-18.md`](notes/taking-stock-2026-08-18.md).
> The stock-take's recommended order survives the ruling — the caller's contract
> (✅ 2026-08-18), **temporal types (✅ 2026-08-19), then the profile** — with one
> change:
> **S1's harness (`EXPERIMENTS.md`) now sits alongside them** instead of near the
> bottom, since §1 names it as the instrument v1 is defined against.
>
> Six are resolved in `bugs/resolved/`: `004` (§6 asserted a finiteness arithmetic
> falsified), **fixed 2026-08-18** by the Termination session — §6's premise is now
> conditional on the fragment §10 certifies, and the non-terminating program takes
> the criteria's *documented-as-in-scope* branch rather than being rejected; `001` (a compound argument in a negated
> atom silently misread as a wildcard), **fixed 2026-07-25** by milestone 4's
> scheduling follow-on; `002` (`-q` rejected a disjunctive rule), **fixed 2026-07-26**;
> `005` (a ground query with a computed argument printed nothing), **fixed
> 2026-07-27** by folding a ground compound argument in a query rather than
> hoisting it (§17 — the bug file's plumb-the-IR sketch was rejected for
> colliding with A15); `003` (three normative errors in `spec.md`), **fixed
> 2026-07-27** — §10 is now the single normative home of range restriction, which
> had been restated in *six* sections, two of them falsely; and `006` (`<`
> rejected the types §8 orders), **fixed 2026-07-27** by widening the type
> checker to match the spec, which needed no change.
>
> `cargo test -- --ignored` should report **no** known failures since 2026-07-29,
> when the two absent × negation tests were closed out (one passes, one was
> converted to pin the asymmetry it turned out to describe). The only `#[ignore]`d
> test left is `url_csv_import_reads_over_httpfs`, which needs the network and
> passes. Any *failure* under `--ignored` now means something regressed.

### Goals and principles (§1/§2) — shipped

**✅ 2026-08-18.** §1 now states goals, non-goals, target users and six success
criteria, and defines v1 as **S2–S6 hold and S1 has been measured at least once**;
§2's remaining four principles are ratified, three of them **scoped to what the
implementation delivers** rather than to what the sentence claimed. The ruling that
tags every item below is in [`notes/v1-scope.md`](notes/v1-scope.md); §17
2026-08-18 carries the argument. No `src/` change.

### Expressions (§5/§8)

**Both findings from the 2026-07-25 spec review have shipped.** Parenthesized
expressions ✅ 2026-08-16 (`primary → "(" expr ")"`, precedence-aware printing,
`grouping_survives_the_print_round_trip`), and the **`as` cast** ✅ 2026-08-16 —
`Expr as type`, postfix, binding tightest, chaining left-to-right, with the
result type fixed unconditionally and the operand left unconstrained. The two
were deliberately not bundled, one being a small task and the other carrying an
open design question.

**Strict numerics stand, and now have their escape hatch in-language**: `int` and
`float` still never meet implicitly — `1 / 3` is `0`, and `V = A + B` across an
int and a float column is a type error — but `(A as float) / (B as float)` asks
for the ratio, keeping the widening visible in the source text. Two decisions
came with the implementation (§17, 2026-08-16): the **conversion table** (§8),
and **`absent` for an unrepresentable conversion, an error for a lossy one**.

- **Make the malformed/missing reclassification visible.** ✅ **2026-08-18.** A
  conversion that loses a value reports *malformed, not missing* at the
  `=`-assignment — which lowering hoists every cast into, so one site covers them
  all — and a **guarded** conversion (§16.9's idiom) stays silent. The same session
  fixed §9's other blind spot: an aggregate in a **query** now reports its skips.
  — §4/§9/§12, [`notes/callers-contract.md`](notes/callers-contract.md).
- **A conversion inside a comparison is still silent.** `X as int > 5` drops the
  malformed rows: a failed conversion makes the operand `absent`, every comparison
  with an absent operand is false (§8), and the row is filtered out before a
  premise exists to carry the count — so a dirty column silently *narrows* a filter
  where it visibly widens an assignment. Found 2026-08-18 while building the report
  above. _queued — **v1** (S3: the same silence the report exists to end)._ — §8/§12.
- **A type-clash diagnostic that names the conversion.** `V = A + B` over mixed
  numerics reports the clash and stops; now that `as` exists there is a concrete
  fix to suggest, which there was not when this was first noted. _queued — **v1** (S3)._ — §12.

*Superseding the former "scalar-function call form" item*: user-defined scalar
functions were **declined** 2026-07-25 (a rule already is one; §17), and
conversion is the cast. What remains is only *builtin* scalars (`abs`, `length`,
`lower`, `substr`), still deferred until a consumer needs them — but their
**shape is settled 2026-08-19**: a builtin is a relation from a gated `std`
module (§13), so the `ident (` ambiguity that ruled out `float(A)` never arises
and the scan-ahead candidate is withdrawn.

### The value model (§4) — shipped

**✅ 2026-08-19. Temporal types — `date`, `timestamp`, `duration`.**
`@`-sigilled literals, points-and-vectors arithmetic with `duration / duration
→ float` as the only route to a number, CSV and Parquet inference, and
extraction/truncation as gated `std/time` relations. **S4 now reads met.**
— §3/§4/§8/§9/§13, §16.14, properties T1–T6,
[`notes/temporal-values.md`](notes/temporal-values.md).

### Termination & value-creating recursion (§6/§10) — shipped

**✅ 2026-08-18.** A static classification, shipped as a **warning** and not an
error: a rule binding a head variable to an arithmetic-computed value while its
head predicate lies on a positive cycle is named before the run, and still runs.
Certified programs terminate (§6/§10; proof in `notes/termination.md`), which
closed `bugs/004` and ratified §2's *predictable evaluation* pillar. The
2026-07-25 direction — a static *error* — was reversed on the user's call, since
`path_cost` is valid on every acyclic graph; §17 2026-08-18 carries the argument.

### An accumulating recursion that terminates: limit predicates (§4/§6/§9/§10)

**The escape hatch the warning above leaves open.** `declare path_cost(from, to,
min cost)` would keep only the extremum per key group, making cost-accumulating
transitive closure finite rather than merely warned about — Kaminski et al.'s
limit Datalog (references.md group 1). A milestone, not a rider: §6's `T_P`, §9's
aggregation and §11's provenance all move — and §6 now states the premise it would
give up (a fixpoint accumulating a set, not a per-group extremum), so the change
has a written baseline to be measured against. _queued (own session) — **post-v1**: the 2026-08-18 warning is v1's answer._ — §17 open
questions, `notes/termination.md`, `notes/declarative-semantics.md`.

### The truncation contract (§9/§13/§15) — decided, not built

**An incomplete model does not answer, and the whole-model surface survives it**
(§17, 2026-08-16). Decided, with three live instances that report through a stderr
warning today: §9's skipped aggregate rows, §13's unrepresentable import cells, and
an external signal. The discriminating rule is how the program reads the short
relation — projected, one row short and say so; folded or negated, withhold.

**The CLI shape is settled and withholding has no trigger** (2026-08-18): the
caller's-contract session measured all three of the decision's "live sources" and
none is one — §13's cells are structured errors, the round cap is a test oracle,
and §9's skips are absent *values* in present rows, not short relations. §15 says
so, and §14's range invariant reserves the next code above `2` for the day
something does truncate. What is left is a **trigger**, not a design.
_parked (awaiting a trigger: a hosted surface's budget, an external signal) — **post-v1**: the vocabulary it needed shipped._ — §15, `notes/callers-contract.md`.

### The caller's contract, and what a run leaves behind (§12/§14/§15)

Both found by the 2026-08-18 stock-take and neither previously listed;
[`notes/taking-stock-2026-08-18.md`](notes/taking-stock-2026-08-18.md) has the
evidence, and [`notes/callers-contract.md`](notes/callers-contract.md) the design
that settled the first.

- **Integrity constraints, and an exit code that carries an answer.** ✅
  **2026-08-18.** A constraint needs **no construct**: a negation-only body already
  answers `holds(true).`, so only the code was missing. Exit codes are now grep's —
  `0` rows · `1` no rows · `2` did not answer — **stated as a range** so a later
  code refines `2` rather than reinterpreting it, and `datalog check.dl -q 'not
  conflict(_, _)' && deploy` means what it looks like. Example §16.13. — §12/§14/§15,
  [`notes/callers-contract.md`](notes/callers-contract.md).
- **Nothing is reusable across runs.** Every invocation re-parses, re-imports (§13
  materializes eagerly) and re-runs the fixpoint from zero, and there is no REPL —
  so an agent's write-run-read-fix loop pays a full reload each iteration. Distinct
  from pushdown and parallelism, which make *one* run faster. Invisible to
  `notes/cross-engine-benchmark.md`, which times whole processes by construction.
  _queued — **post-v1**, with a trigger: S1's harness showing the reload change what an agent does._ — §13/§14/engine.

### Aggregation follow-ons (§9)

- **Count-distinct, and the invisible wildcard.** `count { S | e(_, S) }` returns
  the edge count, not the distinct-`S` count, because a wildcard inside a goal is
  a witness dimension — the one place `_` does not mean "don't care" (§9). Over an
  **imported table the wildcard is not written at all**: named-argument syntax
  leaves every unmentioned column implicitly wildcarded. Measured 2026-07-27 on a
  7-column table — `count { C | calls(caller: C, callee: "bump") }` = 36 call
  sites where the question wanted 20 callers. Decide whether v1 gets
  `count_distinct` / a `distinct` modifier, or whether the trap is documented
  harder; §13's wide machine-generated tables are the case that makes it urgent
  (§17, 2026-07-24 *Consequences*). A sibling engine spelled `count distinct`
  citing this measurement, then measured the **warning** beside it firing on
  correct programs and leading with a destructive fix — read
  `notes/tsdl-cross-project-review.md` before designing the diagnostic, not after.
  _queued — **v1** (S1: it returns wrong answers silently)._ — §9/§13.
- **Statistical reducers** — `median`/`stddev`/`variance`/`percentile`. The
  aggregate node reserves a parameter slot for `percentile(p)`; each is a reducer
  registration + a typecheck arm, no evaluator restructure. _queued — **post-v1** (additive)._ — §9.
- **Collection-valued reducers** — `collect`/`string_agg`. Blocked on a
  first-class collection value (the value model is flat, §4); its own design
  session. _queued (blocked) — **post-v1**._ — §9/§4.
- **Recursive/monotonic aggregation** — v1 rejects recursion through an aggregate
  via stratification; how far to take a fixpoint semantics (Zaniolo et al.,
  `references.md`) is open. _parked (research) — **post-v1**._ — §9.

### Surface uniformity & the agent edge (§5/§14)

Items landing on the agent surface, the one consumer least able to work around
them. Except where noted these are documented v1 limits rather than defects.

- **The answer shape — what a query prints, and under what relation name.** ✅
  **2026-08-17**, closing the widening *and* the multi-atom existence check it had
  absorbed. The rule is set **equality** between the positive atoms' variables and
  the answer variables (§14): the atoms substitute when they account for every
  answer variable — one atom beside any number of non-binding literals, or a
  **ground conjunction** — and a body with no answer variables and nothing to
  substitute answers `holds(true).`, which the lost question needed in *three*
  shapes and not one. `answer/N` stays; silence still means no. Property **C8**,
  example **§16.10**, four mutations recorded. — §5/§14,
  `notes/query-answer-shape.md`.
- **Naming a query where it is asked** — ✅ **2026-08-17**, closing the projection
  hazard that 2026-08-16 recorded as unsolved in both engines. `?- adult: p(X), q(X).`
  desugars **in lowering** to a rule whose head is the projection, so the name is a
  real relation a later rule can read and the answer-shape function needed no new
  arm; `ir::Query` and `api.rs` were untouched. Arity is the projection's length, an
  empty projection heads `name(true)`, and a name the program *defines* is rejected
  while one merely *referenced* is free. Property **C8**
  `c8_a_named_query_matches_its_desugared_rule` (oracle = the desugaring), example
  **§16.11**, four mutations recorded. — §5/§14, `notes/query-answer-shape.md`.
  - Now unblocked, and *not* taken: **making an unnameable query an error** (axis 1).
    Recovery is "prepend a word" rather than "rewrite it as a rule", which is what
    made strictness unaffordable before. Naming stays optional today. _queued — **post-v1**._
  - A named query publishes **every** variable its body binds, so it does not
    replace a rule that projects fewer columns. Documented, not a defect.
- **A synthesized answer does not say which query it answers.** Two boolean queries
  in one run both print `holds(true).` and `answer/N` collides the same way across
  same-width projections. **Narrowed 2026-08-17 to the *unnamed* case** — naming
  both queries already tells them apart, so this is now an ergonomic gap rather
  than the only route. The fix is still a **`%` comment**, not an extra argument —
  the text is unusable by any program (string ops rejected) and an argument is the
  same move 2026-08-16 rejected for provenance-as-facts. **Sequence with §11's
  comment rendering** so the format is designed once. _queued — **post-v1** (naming resolves it today)._ — §11/§14.
  - *Rejected 2026-08-17, and recorded so it is not re-proposed:* a **warning when
    a program reads `answer` facts**. The hazard is a two-run merge, which inside a
    single run is indistinguishable from legitimate single-pipe composition — so it
    would fire on correct programs and be silent on the case that matters. Cf. the
    inverted `invisible-witness` warning below.
- **Three different body grammars.** Rule bodies are DNF; queries and aggregate
  goals are conjunction-only (all three verified). So a disjunctive filter can be
  expressed only by detouring through the rule form — which `bugs/002` made
  unusable via `-q` until 2026-07-26, and the detour is still the only route.
  Decide whether `;` extends to queries and goals, or whether the asymmetry is
  deliberate and gets stated as such in §5 (it currently reads as an aside:
  "Queries stay conjunctive"). One datum on how much it matters: in a sibling
  engine's experiment **neither subject reached for `;` at all**, both writing two
  rules over one head, so the case for uniformity is consistency rather than demand
  (`notes/tsdl-cross-project-review.md`). _queued — **post-v1**; §5's footer already states the asymmetry._ — §5.
- **No string operations.** No prefix, split or concat, so reducing `lower::tests`
  to `lower` is unwritable and the fact producer must do it (2026-07-27). Closed:
  string construction fails §5's own termination test and would widen the hole the
  Termination session closed on 2026-08-18 — a string-building recursion has no
  `i64` ceiling at all, so the finite-state-space argument that keeps arithmetic
  honest would not apply to it. _rejected — §17, 2026-07-27._ — §8/§10.
  (Ordered comparison over strings was the separate, opposite call: widened,
  `bugs/006`, closed 2026-07-27.)
- **`declare` does not count as defining a predicate.** `declare banned(name:
  string).` plus `not banned(X)` still warns "referenced but never defined"
  (verified). Declaring a schema is the user stating a relation exists and may be
  empty, and there is no way to suppress the warning. Either `declare` should
  define, or there should be a way to say "intentionally empty". _queued — **v1** (S3, inverted: it fires on correct programs)._ — §10/§12.

### Provenance surface (§11)

- **Provenance query syntax — designed 2026-08-16, not built.** One union of
  `proof` / `underivable` / `unknown`; the sigil is a cost hint; a near-miss is a
  *rule*, not a binding; a repair is a step, not a promise; the trace re-solves
  through the scheduler the fixpoint uses. §17 has the decision. What remains open
  is the rendering, the JSON encoding, and sequencing against the truncation
  contract, whose distinction `unknown` is. **Decide it together with "does the
  derivation store earn its cost" below, and with the profile** (2026-08-18): the
  recorder runs unconditionally and is the top profiling target, while its only
  consumer is unbuilt — so pillar 1 pays full price on every run and returns
  nothing at the surface it exists for. Whichever is settled first constrains the
  other. _queued — decided, not built; **v1** (S5)._ — §11/§14, [`notes/taking-stock-2026-08-18.md`](notes/taking-stock-2026-08-18.md).
- **First appearance, not first round** — **closed 2026-08-16 without building it:
  the rationale was adopted from a sibling engine and does not hold here.** Ours
  *batches* application, so the derivation that first produces a fact always has
  strictly-earlier premises and the round bound never fails to find a proof —
  measured, `explain` returned `None` zero times over 275 derived facts, and every
  same-round premise sat on a redundant rediscovery. A sequence number would admit
  those and change which proof is printed, for no correctness gain (§17,
  ***Falsified 2026-08-16***). What remains is a **forward risk, not an item**:
  interleaving collection with insertion — streaming, or the parallelism item below
  — breaks the round bound silently. E1 is the test that fires, and
  `eval_stratum`'s apply loop now says so. _rejected._ — §11.
- **Provenance as facts.** A proof tree is not a fact, so emitting it as ground
  derivation-edge facts either invents relations the program never declared or
  flattens a tree into rows that no longer compose. It rides in `%` comments
  instead, which keeps Datalog-out-is-Datalog-in intact byte-for-byte.
  _rejected — §17, 2026-08-16._ — §11/§14.
- **E3 replay does not cover §8 builtins** — a coverage hole, not a defect, but
  the one place provenance is least checked. `e3_derivations_replay` reapplies
  each derivation's rule instance to its premises and asserts it rederives the
  fact; its `replay` helper rebuilds the environment from **fact premises only**
  and returns `None` on any `Premise::Builtin`. It passes solely because its
  generator (`arb_program_with_edb`) emits no comparisons — every column is a
  symbol after `monotype`, so arithmetic cannot appear. So the strongest
  provenance property has never seen an `=`-assignment, a presence test, or an
  aggregate, and by extension never sees a deferred negation, whose absence
  pattern is closed by an assignment-bound value (§7/§10, 2026-07-25).

  The work: teach `replay` to fold builtin premises into the environment in
  schedule order, then run E3 over a generator that emits them —
  `arb_comparison_program` already exists and is int-typed. Expect it to surface
  the same class of question `Premise::Builtin` raised when it was added: a
  self-justifying leaf records the *values*, not the expression, so replay has to
  re-evaluate rather than re-check. Catalogued as **E6** in `testing.md`.
  _queued — **v1**: S5 ships a proof surface, so its strongest property must have seen a builtin._ — §11/§15.
- **Imported facts are anchored by *relation*, not by row.** `engine::validate`
  records that an import's facts "are ordinary base facts by the time the engine
  runs… `ImportSpec` survives only as provenance/definedness metadata", so `?why`
  will answer "because `employees.csv`" and never "because row 4,182" — the answer
  §13's workloads actually want, and §11's "anchored by the program text (or,
  later, the import)" reads as a granularity that does not exist. Retaining the row
  costs memory on exactly the shapes already at 1.2 GB, so it is a **trade to
  decide with the two items above**, not a default to add. _queued — **v1** as a ruling, not necessarily as a feature._ — §11/§13,
  [`notes/taking-stock-2026-08-18.md`](notes/taking-stock-2026-08-18.md).
- **Semiring provenance under negation** — tropical cheapest-proof selection and
  the algebra behind it; sketch in `notes/semiring-provenance.md`. **`?whynot` with
  minimal repairs is no longer part of this item**: it was designed 2026-08-16
  without a semiring, a near-miss being a *rule* rather than a binding, which is
  what bounds it. What stays parked is the algebra. _parked (research) — **post-v1**._ — §11.
- **Does the derivation store earn its cost?** A sibling engine extracts a proof
  *backwards* from the retained model — no recorder in the fixpoint, no re-run — so
  a run nobody questions pays one integer per row. It is not a free swap: we record
  *all* derivations (§17, 2026-07-19) and backwards extraction yields one. But
  `notes/performance-baseline.md` names this recorder as its top hypothesis for the
  35× cliff and has never measured it, so **the profiling item below now has a
  concrete architecture to profile against.** Sequence after the profile, never
  before — and **settle it in the same session as the query surface above**
  (2026-08-18), since a profile is under pressure to make the recorder optional and
  pillar 1 is the only argument that it should not be. _queued (after profiling) — **v1**, decided with the query surface._
  — §11/engine, [`notes/taking-stock-2026-08-18.md`](notes/taking-stock-2026-08-18.md).

### `std` modules (§8/§12/§13) — shipped

**✅ 2026-08-19.** A builtin is a *relation* from a gated module; `std/` is a
reserved virtual path prefix; a name collision is an error naming both origins,
and an unimported use is §12's warning carrying the import line. `std/math` and
`std/text` are **designed, not built** — the shape is settled, so they stay
deferred until a consumer needs them (§8's *Not covered*). — §13,
[`notes/temporal-values.md`](notes/temporal-values.md).

### Import follow-ons (§13)

- **Database loading** — SQLite/DuckDB files via the reserved `table "…"`
  grammar; Postgres via DuckDB attach. Grammar ratified, loading deferred until
  a real consumer. _queued — **post-v1** (awaiting a consumer)._ — §13.
- **TSV** — an easy format add, deferred with database loading. _queued — **post-v1**._ — §13.
- **Filter pushdown for large sources** — v1 eagerly materializes every import;
  push selections into SQL when a consumer hits the wall (the path/`table`
  syntax leaves room). _queued — **post-v1** (awaiting a consumer)._ — §13.
- **Module namespacing** — v1 module imports share one global namespace;
  qualified names / visibility deferred until needed. _queued — **post-v1**._ — §13.

### Performance (now unblocked)

Deliberately sequenced **after** aggregation and the absent value (both shipped
2026-07-24): tune a feature-complete surface rather than re-profiling as core
semantics change (both added evaluation paths that would move the hotspots). Now
the highest-signal next item.

- **Profile the engine** — never profiled. The benchmark half is now done:
  [`notes/cross-engine-benchmark.md`](notes/cross-engine-benchmark.md) stands up a
  regenerating corpus and ranks the leads, first among them the derivation recorder
  (a second engine measures 13× for its own on a cyclic graph). Earlier one-off
  numbers: [`notes/performance-baseline.md`](notes/performance-baseline.md).
  _queued — **v1** by dependency: the input the recorder decision is sequenced behind, not on its own merits (S6 is met)._ — engine.
- **Aggregation does not scale with the aggregated relation** — 2.5× the rows at a
  fixed group count costs 9.6×, and it is the one shape where `tsdl` wins outright;
  groups scale sublinearly, so the suspect is a rescan rather than a group-key
  reach. Measured in [`notes/cross-engine-benchmark.md`](notes/cross-engine-benchmark.md).
  _queued (after profiling) — **v1** (S6: the one shape a sibling engine wins outright)._ — §9/engine.
- **Parallelism** — assess how much of semi-naive evaluation and joins can go
  parallel (independent rules within a stratum, partitioned/hash joins) while
  preserving the deterministic canonical output and full provenance recording,
  which are load-bearing guarantees. Scope follows from the profile. _queued
  (after profiling) — **post-v1**: S6 is about exponents, which parallelism does
  not change._ — engine.

### Declarative semantics (§6) — shipped

**✅ 2026-08-18.** §6 accounts for the whole language: `T_P` is defined over a
**match relation** rather than substitution (binding is total, matching is
semantic), builtins are interpreted predicates, an aggregate is a fixed function
from group keys to values because its goal reads a strictly lower stratum, and the
two finiteness claims `bugs/004` had conflated are separated. One decision came
with it — **a run that raises an error has no model**, the limiting case of the
truncation contract. Proof and operator in
[`notes/declarative-semantics.md`](notes/declarative-semantics.md); §17 2026-08-18
carries the argument. No `src/` change: every rule was already guarded
(`testing.md`'s coverage map).

### Spec hygiene (the document itself)

From the 2026-07-25 style review. The *false assertions* were `bugs/003`, closed
2026-07-27; what follows is staleness, unwritten sections, and structure — real,
but a different kind of work.
- **The provenance "absence pattern" is now the `no-match pattern`** ✅ 2026-08-16,
  with the explanation of a missing answer named a **failure trace** (§17 — three
  names, adopted verbatim from a sibling engine). The decision's site count was
  wrong in both directions: ~40 non-frozen lines over 9 files, not 31 over 8, and
  **§17 was not "nothing frozen"** — six of its entries hold the old identifier and
  keep it, an append-only record being exactly what a rename does not touch. The
  2026-07-20 entry carries the pointer instead. §4's and §11's two standing
  "shares a word, not a concept" disclaimers are gone, which was the point.
- **Name a test per §16 example.** **§16.9 (the cast) is the worked pattern** ✅
  2026-08-16, with **§16.10 (the answer shape)** and **§16.11 (the named query)**
  following ✅ 2026-08-17 — each names its `tests/system.rs` test and pins its
  output in a fence compared byte-for-byte, and §16.11 was written that way from
  the start rather than retrofitted. The other **eight** still carry prose in
  comments and no named test, so a block nobody wired up cannot fail; retrofitting them to that shape is
  the item. The rule going forward is **no example means no feature**. Add a
  diagnostic example while doing it: none of them exercises an error, so nothing
  checks what the engine prints when a program is wrong. _queued — **v1**: §16 is where S2–S6 are demonstrated._ — §16,
  `testing.md`.
- **Normalize §17's chronology.** Reverse-chronological for 08-16 back to 07-25,
  then it jumps to 07-03 and runs *forward* through 07-21, so a reader cannot tell
  which end is current. _queued — **v1** (trivial; every ruling cites §17)._ — §17.
- **Restructure the §17 decisions log** — **53% of `spec.md`** as of 2026-08-16
  (1 538 lines against §§1–16's 1 390; it was 48% on 2026-07-26), growing far faster
  than the body, driven by oversized entries rather than by decision count.
  Topic-keyed rewrite proposed; measurements, what is already decided, and the
  open questions are in
  [`notes/decisions-log-restructure.md`](notes/decisions-log-restructure.md).
  **Moving it to its own file** is the option a sibling engine took and this should
  weigh — their spec holds no decisions at all. _queued (user call; own session) — **post-v1**._ — §17.

*Closed 2026-08-16, by the hygiene pass in §17's entry of that date:* the status
vocabulary (deleted — §§1–16 carry no status markers, and each ends with a single
***Not covered*** footer, which is also where the six different deferral labels went);
the implementation audit trail (moved to
[`notes/spec-traceability.md`](notes/spec-traceability.md)); and the stale forward
pointers, five of which lived in the deleted status lines, with §16.7's dissolved
workaround and §16's preamble-vs-§16.4 contradiction fixed alongside.

### Errors & API edges (§12/§14)

- **Machine-readable error taxonomy** — §12 is now Draft: `Error` is a struct
  (category, message, span, line/column position, suggestion) with `Display`
  composed from the fields, and lexer/parser diagnostics carry real positions
  (2026-07-25). Two pieces remain: a stable **code** vocabulary so an agent can
  branch on `unsafe-aggregate` without matching prose, and **spans on semantic
  errors** — lowering reports many from points where the responsible span is not
  threaded, so choosing one per diagnostic is a design pass. _queued (partially
  shipped) — **v1** (S3, and the reason §2 ratified scoped)._ — §12.
- **`--format json` scope** — a documented future *edge* feature (structured
  errors, provenance); the data path stays Datalog-native. _parked (low value) — **post-v1**._ — §14.
- **`serde` for the API** — decide as §14 stabilizes. _queued — **post-v1**._ — §14.

### Agent skill

- **`skill/recipes/source-analysis.md`** — the source-analysis recipe: extract
  with a real parser, import, ask; the five traps. Bundled by `cargo
  package-skill`, so `recipes/` is now a shipped part of the skill. ✅ 2026-07-27.
- **"Big external fact base" demo** — the motivating import demo; unblocked by
  §13 (the USDA dogfood is a first pass). _queued — **post-v1**._ — skill.
- **Make `EXPERIMENTS.md` a measuring instrument.** It is an honest checklist
  eyeballed in a session, run twice — which is not enough to gate the decision below
  that explicitly waits on it. The controls that transfer from a sibling engine's
  harness (`notes/tsdl-cross-project-review.md`): a fixture in a domain the skill's
  docs never use; a prompt assembled by a tool from the fixture's own schemas; the
  **first program recorded before any feedback**; every task run at **two model
  strengths**, since the strongest subject routes around gaps instead of falling
  into them; a doc line measured by *cutting* it from one cell's prompt; reference
  programs pinned byte-exact in CI. What must **not** be lost in the redesign is the
  finding their setup cannot produce — the questions our subject answered with
  `grep` because the engine could not express them. A skill that cannot say
  something loses the question silently. **Reframed 2026-08-18: this is not skill
  polish but the project's own validity question.** The repo exists to test whether
  an agent reasons better with a logic engine, and §1 calls the three pillars
  "settled authority" for every decision in §17 — authority resting on a premise
  measured twice by eyeball. It is also the item most able to reorder everything
  else here, which argues for early rather than first. _queued — **v1**: S1 names this as its instrument, so until it exists v1 is undefined rather than unfinished._ — skill,
  `EXPERIMENTS.md`, [`notes/taking-stock-2026-08-18.md`](notes/taking-stock-2026-08-18.md).
- **Other agent-exposure forms** — a Claude API agent-loop harness, and an MCP
  server. Both are deliberately waiting on a trigger: the skill (2026-07-23) is
  the first experiment, and how well a model actually drives it is what should
  decide whether a second form is worth building — **which is a decision gated on
  the item above**, not on elapsed time. A hosted surface is also the one place the
  no-budget termination call does not cover — and now the one place the
  warn-don't-reject call does not cover either, since a hosted caller cannot press
  `^C` (§17, 2026-08-18). _parked (awaiting the
  skill experiment) — **post-v1**, gated on S1._ — skill.
