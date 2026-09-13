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

> **Testing comes first** (the user's, §17 2026-09-12): property tests grow their
> inputs before more refactoring or performance work — § Testing. Answer
> streaming and every § Performance item wait on it. Every § Performance item is
> gated by a deep run (`DATALOG_PBT=deep cargo test --lib`) before and after it
> (the user's).
>
> **Open defects live in [`bugs/`](bugs/)** — `013` opened 2026-09-12, `014` the same day.
> **No design session blocks anything** either: §6's extension, the last one,
> shipped 2026-08-18.
>
> **§ Performance's two pruning items shipped 2026-09-12** — rule pruning and
> relation pruning, closing `bugs/012`. *Column projection and magic sets are the
> bigger chunk behind them and were deliberately not next*; nothing is ruled next
> yet. The early check they left at lowering is *a column's type is known before
> its rows* (§ Import follow-ons, `bugs/014`).
>
> **§1's six criteria all hold as of 2026-08-25** — S3, the last, closed with the
> error code vocabulary.
>
> **Five items below still carry a v1 tag, and that is now a contradiction to
> resolve, not a backlog.** The *cast inside a comparison* (§8/§12),
> *`declare` does not define a predicate* (§10/§12) and *a type-clash diagnostic
> that names the conversion* (§12) are all tagged **v1 (S3)** — but S3 reads *"a
> **rejected** program can be repaired from the diagnostic alone"*, and all three
> are about programs the engine **accepts**. Either they are misclassified or S3
> is broader than its sentence. The other two are *count-distinct* (v1, S1) and
> the §16/§17 hygiene pair. **A user call**: retag them, or reopen the criterion.
> Nothing here was retagged to make the flip look clean.
>
> **§1 is written and §2 ratified (2026-08-18), so every item below is now ruled
> v1 or post-v1** against §1's success criteria — the argument per item is in
> [`notes/v1-scope.md`](notes/v1-scope.md), and the evidence that prompted the
> exercise in [`notes/taking-stock-2026-08-18.md`](notes/taking-stock-2026-08-18.md).
> The stock-take's recommended order survives the ruling — the caller's contract
> (✅ 2026-08-18), **temporal types (✅ 2026-08-19), the profile (✅ 2026-08-20) and
> the seek it found (✅ 2026-08-21)** — with one change:
> **S1's harness (`EXPERIMENTS.md`) now sits alongside them** instead of near the
> bottom, since §1 names it as the instrument v1 is defined against.
>
> Nine are resolved in `bugs/resolved/`: `012` (a program error paid for the whole
> fact load), **fixed 2026-09-12** by relation pruning, which lowers before it
> reads — its syntax-error half never reproduced, and the early check stops at
> lowering because of `014`; `008` (a rule-level type clash
> manufactured a second, *false* diagnostic asserting the fact table held values
> it does not), **fixed 2026-08-24** — the declared-vs-inferred sweep is skipped
> once inference is poisoned, and its message re-derives the column's type from
> the facts before claiming them; the second half was found while fixing the
> first, and neither alone is the fix (§17, property **C15**); `007` (`sum`/`avg` folded its witnesses
> in enumeration order, so two spellings of one goal gave two answers), **fixed
> 2026-08-20** — an aggregate is a fold over a **multiset** now, sorted into §14
> order, compensated over floats and accumulated wide over ints and durations
> (§9, §17), and the `i128` half is a semantic widening none of the bug file's
> four candidates proposed; `004` (§6 asserted a finiteness arithmetic
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
> `cargo test -- --ignored` reports **no known failures**. The only `#[ignore]`d
> test is `url_csv_import_reads_over_httpfs`, which needs the network and passes.
> Any failure under `--ignored` means something regressed.

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
  sites where the question wanted 20 callers. **Measured again 2026-08-23, on the
  shape that decides it**: over `sqlparse`, `widely_used` returns **7 functions
  with a two-column `call` relation and 13 with a three-column one** — same rule
  text, same corpus, the extra column a line number the rule never mentions.
  Exit 0, no warning. A relation is a set, so whether the answer is right turns
  on a projection choice made in the *extractor*, which is where nothing checks
  it. Decide whether v1 gets
  `count_distinct` / a `distinct` modifier, or whether the trap is documented
  harder; §13's wide machine-generated tables are the case that makes it urgent
  (§17, 2026-07-24 *Consequences*). A sibling engine spelled `count distinct`
  citing this measurement, then measured the **warning** beside it firing on
  correct programs and leading with a destructive fix — read
  `notes/tsdl-cross-project-review.md` before designing the diagnostic, not after.
  **The evidence to rule on it is being collected, not argued for**: the guidance
  is marked `count-wildcard` (§9's own wording) and `source-analysis-count-trap`
  (the recipe's worked fix), and `experiments/` can now cut either from one cell
  and re-run — so "document it harder" gets a number instead of a defence. Rule
  after the first grid, not before it.
  _queued — **v1** (S1: it returns wrong answers silently)._ — §9/§13,
  `experiments/src/harness/ablate.py`.
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

- **No string predicate of any kind** — _queued, post-v1_. There is no
  `contains` / `prefix` / `suffix`, and no `like`; `contains(P, ".x.")` is an
  undefined predicate and `P like "%.x.%"` a parse error. A fact base whose keys
  are *paths* — which every source-analysis fact base is — therefore cannot
  express a convention the schema did not anticipate, and the workaround is to
  leave the engine, generate a table in another language and import it. Measured
  cost when it bit: a dead-export query over a real library returned 594 rows of
  which 453 were one file-naming convention, and filtering them meant a Python
  script writing a fact file, so the query stopped being self-contained and
  re-runnable. The design question is not whether but *how much*: one predicate
  closes the case, a pattern language is a language. — found dogfooding
  (`../code-analysis/notes/code-facts.md` § Dogfooding — Grafana).
- **No `ORDER BY` / `LIMIT`, and no top-N** — _queued, post-v1_. Ranking is the
  most common thing an agent does with an answer, and the documented idiom is to
  compute a maximum and threshold on it, which costs a rule per ranking and
  cannot express "the ten largest". Every top-N in two real reviews was produced
  by piping output through `sort -rn | head`. Whether this belongs in the
  language or in the *printer* is the open question — a sort is not a relational
  operation, and `?- top 10 by N: p(X, N).` is a query-surface feature, not a
  semantics one.
- **A long run says nothing while it runs** — _queued, post-v1_. A library that
  took 304 s at a constant 9 GB produced no output until it finished, so
  "working" and "wedged" are indistinguishable and the only recourse is to `ps`
  the process. A heartbeat on stderr (facts derived, current stratum) every N
  seconds would close it. Interacts with the `--progress` question nobody has
  needed at fixture scale.

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

- **Provenance surface — ✅ shipped 2026-08-21.** `?why` / `?whynot` over a ground
  goal, as §5 statements a `-q` may also carry; one union of `proof` /
  `underivable` / `unknown` with neither sigil a selector; a near-miss per rule,
  a repair that is a step; explanations exit-code-neutral. Recording is
  **provisioned per sigil** — measured on a 400-node sparse closure: a run with no
  goals is **2.4× faster and 4.5× smaller**, and a `?whynot` over an absent fact
  pays nothing. **Still open: the JSON encoding** (§14, low value, parked).
  _shipped; JSON parked._ — §5/§11/§14,
  [`notes/provenance-asking-form.md`](notes/provenance-asking-form.md).
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
- **E3 replay now covers §8 builtins — ✅ shipped 2026-08-21 as E6.** `replay`
  folds builtin premises into the environment in **schedule order** and
  re-evaluates rather than re-checks them, and E3's claim runs over
  `arb_comparison_program`, which reaches an `=`-assignment, a presence test, an
  aggregate and a deferred negation. An aggregate's value is bound, not
  recomputed — a stated limit, the fold having B11. _shipped._ — §11/§15,
  `testing.md` E6.
- **Imported facts are anchored by *relation*, not by row.** ✅ **Ruled 2026-08-21**
  — §11 states it where the anchor is defined, and a proof prints
  `[fact from "employees.csv"]`. §13 materializes an import into ordinary base facts
  before lowering, so `ImportSpec` is the finest anchor there is. A row-level anchor
  costs memory on exactly the shapes already at 1.2 GB and stays a **trade, not a
  gap** — now standalone, both items it was to be decided with having been ruled the
  same day. _ruled; the row-level trade parked — **post-v1**._ — §11/§13.
- **Semiring provenance under negation** — tropical cheapest-proof selection and
  the algebra behind it; sketch in `notes/semiring-provenance.md`. **`?whynot` with
  minimal repairs is no longer part of this item**: it was designed 2026-08-16
  without a semiring, a near-miss being a *rule* rather than a binding, which is
  what bounds it. What stays parked is the algebra. _parked (research) — **post-v1**._ — §11.
- **Does the derivation store earn its cost? — ✅ Ruled and built 2026-08-21: yes,
  once it is only paid by the run that asks.** It was 70–78% of peak RSS on every
  run while nothing could ask ([`notes/profile-2026-08-20.md`](notes/profile-2026-08-20.md));
  gated, the question is answered by proportioning rather than replacing.
  Measured after the gate: 202 MB → 44 MB and 0.55 s → 0.23 s on `sparse_400`, and
  the profile's scratch build is no longer needed — the A/B is two ordinary
  invocations. _shipped._ — §11/engine, §17 2026-08-21.
- **Backwards proof extraction** — no recorder in the fixpoint, one proof extracted
  from the retained model (tsdl's answer). Optimises the *explaining* path only, and
  gives up all-derivations, the round-stamp cycle guard, and possibly which proof
  prints (§16.6 and E7/E8 pin it byte for byte). _parked — **post-v1**._ — §11/engine.

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
- **`--no-default-features` builds and tests green — ✅ 2026-08-24.** The two
  `system.rs` temporal tests import date columns from a file, so they carry
  `#[cfg(feature = "duckdb")]` like every other import-dependent test. 557 green
  there against 585 with defaults, and nothing else failed. _shipped._ — §13/§15.
- **TSV** — an easy format add, deferred with database loading. _queued — **post-v1**._ — §13.
- **Filter pushdown for large sources** — v1 eagerly materializes every import;
  push selections into SQL when a consumer hits the wall (the path/`table`
  syntax leaves room). **The consumer arrived 2026-09-12**: a 4.04M-fact base
  where importing every schema costs 8.03 GB to answer a 16-row question, and a
  program that does not compile still pays 2.79 GB. **Re-priced
  2026-09-12**, after relation pruning: the floor is now what a goal *reaches* —
  on `vs/base`, a 1-row question over `schema/all.dl` is 0.01 s, and counting all
  179,748 `var` rows 0.71 s / 0.21 GB. What is left is a reached relation read
  whole to answer a few of its rows. _queued — **post-v1**._
  — §13.
- **A column's type is known before its rows** — a declared type (`declare`, or
  an import's `as rel(f: t)`) is checked after inference, never used by it, so
  only a fact fixes a column's type; that is `bugs/014`, and it is why checking a
  program before its facts load stops at lowering. Declarations as constraints
  first; a source's own types second (a Parquet footer is free, JSONL/CSV a
  streaming pass). Changes what `bugs/resolved/008`'s wording rests on.
  _design session — post-v1._ — §4/§12/§13.
- **Module namespacing** — v1 module imports share one global namespace;
  qualified names / visibility deferred until needed. _queued — **post-v1**._ — §13.

### Testing

- **Property tests grow their inputs** — _building_ (the user's, §17 2026-09-12;
  **post-v1**). Tiers, the deep run and growth guards shipped for
  `arb_program_with_edb` (§17 2026-09-12, sized generators); next the generator
  audit, a tiered shaped generator, and the non-engine generators:
  [`notes/growing-inputs.md`](notes/growing-inputs.md). — testing.md § Generator sizes.
- **Metamorphic properties on sized, shaped generators** — _building_: B1, B5 and
  B13 run at `Medium` and `Large`; B2–B4, B6, B8, C11, C13, C14, E1–E10 next, each
  re-verified by mutation; then the scaling oracles (staged evaluation, renaming,
  rounds, an all-paths differential). **post-v1**. — testing.md.

### Performance

**Profiled ✅ 2026-08-20**, in [`notes/profile-2026-08-20.md`](notes/profile-2026-08-20.md).
The profile falsified the ranking both earlier notes gave, so the items below are
its ranking, not theirs.

**The first two items come from a different measurement, and both shipped
2026-09-12** — a 4.04M-fact base (`code-analysis` on Grafana's frontend, 2026-09-12),
where the binding constraint is **memory, not time**. The 2026-08-20 profile is a
CPU profile over corpus programs; neither it nor the seek work touched what a run
*materialises*, which is where a real fact base spends its resident set. Both items
are the same analysis — reachability over the rule graph — applied once to rules
and once to relations, and both are **pruning, not pushdown**: nothing about how a
join is executed changes.

- **Do not evaluate a rule no goal depends on** — **shipped ✅ 2026-09-12** (§17
  that date; `lower::live_predicates`, `engine::eval_pruned`; `testing.md` **B13**).
  On `vs/base`, `callreach.dl` imported beside a question that never reads it:
  **37.4 s / 4.20 GB → 3.1 s / 0.70 GB**, same answer. A pruned rule can no longer
  fail or hang a run, static warnings still cover the whole program, and a program
  with no goals prunes nothing. `code-analysis`'s `reach.dl` split was unwound the
  same day, answers byte-identical. — §15/engine.

- **Load only the relations the program names** — **shipped ✅ 2026-09-12** (§17
  that date; `api::lower_and_load`, `sources::load_imports_where`; `testing.md`
  **B13**). With every schema explicit the program is lowered before any import is
  read, and only reached imports are. On `vs/base`: `schema/all.dl` counting all
  179,748 `var` rows **6.7 s / 1.64 GB → 0.71 s / 0.21 GB**; an unknown field
  **5.5 s / 1.10 GB → 0.00 s**. Closed `bugs/012`; the early check stops at
  lowering because of `bugs/014`. — §13/§15/engine.

- **Hold the base facts once; build a derivation only if it is kept** — **shipped ✅
  2026-09-12** (§17 that date; `testing.md` **E9**). Heaptrack found an import
  held three times while typed, the base facts twice for the whole run, and every
  match's proof built and dropped under `Unrecorded`. Grafana's `orient.dl`
  **224 s / 9.46 GB → 87 s / 4.94 GB**, its runtime closure alone 133 s / 6.73 GB →
  44 s / 3.45 GB; `vs/base` `checks.dl` 1,265 → 699 MB,
  answers identical ([`notes/memory-profile-2026-09-12.md`](notes/memory-profile-2026-09-12.md)).
  — §13/§15/engine.

- **Profile `pointsto.dl` as a vehicle for the engine** — **profiled ✅ 2026-09-12**
  (§17 that date; [`notes/pointsto-profile-2026-09-12.md`](notes/pointsto-profile-2026-09-12.md),
  which ranks what is left). A round's unkept facts are a set (E9): on a 35% cut
  of `vs/base` evaluation takes 2.67 → 0.97 GB, and the 70% cut now finishes. The full base still does not fit. — §15/engine.
- **Stream a query's answer rather than copying it out of the model** — _queued,
  after § Testing_ (the user's). Printing 3.40M rows holds them twice more:
  `Model::answer`'s owned rows and `RunResult.answers`' lines, ~820 MB of a 1.69 GB
  peak. Both are API shapes, so a short design pass first. — §14/api.

- **Real pushdown: column projection, then demand transformation** — _designing,
  post-v1._ The two items above are pruning — they decide *whether* to read a
  relation or run a rule. These decide what a scan and a recursion actually
  compute, and they are a bigger chunk: both change evaluation rather than
  scheduling, and the second changes it under negation and aggregation, where this
  engine's stratification guarantees live.
  - **Column projection.** `symbol` has 18 columns and a typical rule binds two
    or three. Whether that pays cannot be answered from outside the engine — it
    depends on row-vs-column storage and on whether strings are interned — so the
    measurement came first: **measured 2026-09-12**, Grafana's `symbol` trimmed
    to the 4 of 18 columns `orient.dl` reads loads in **582 MB / 3.2 s against
    1,374 MB / ~11 s** ([`notes/memory-profile-2026-09-12.md`](notes/memory-profile-2026-09-12.md)).
    The win is real; the design is not started.
  - **Demand transformation (magic sets).** The only one of the four that helps a
    closure that *is* wanted: `modgraph.dl`'s `file_reaches` asked for one file's reachability still
    derives all 17.45M pairs. `callreach_seeded.dl` is the hand-rolled version and
    is evidence of the demand — a `seed/1` the caller supplies, which is exactly
    the binding a magic-set rewrite would infer. `references.md`'s evaluation
    group covers it; the delicate part is soundness under stratified negation,
    where the rewrite can move a literal across a stratum boundary.

- **Seek the bound prefix instead of scanning the relation** — **shipped ✅
  2026-08-21** (§17 that date; `src/engine/seek.rs`; `testing.md` **B12**). Up to
  **13.4×**, and an *exponent* on three shapes — sparse 400→800 n^4.25 → **n^2.17**,
  join 2000→4000 n^2.07 → **n^1.00** — at unchanged peak RSS, answers byte-identical
  on all 26 corpus programs. Subsumed the aggregation item, closed with it.
  — §9/engine.
- **A bound column that is not *leading* still scans** — the seek is the relation's
  own column order, so `p(X, Y) :- q(A, X), r(Y, X)` gets nothing for `r`. Priced
  while measuring the item above: the `join_4000` body written in the pessimal atom
  order takes **0.86 s against 0.03 s**, and the seek buys it *nothing* (0.84 s
  before). Fixing it means secondary indexes on the binding patterns a program
  actually uses — a second copy of every relation. The memory argument against it
  weakened 2026-08-21: the recorder is now paid only by a run that asks for a
  proof, so an index would no longer be stacked on 78% of peak RSS in the common
  case. **This is the whole of `code-analysis`'s million-fact problem** (§17
  2026-08-21, consequences 2026-09-11): every library over 30 s on VS Code's
  `vs/base` was over it for this reason, and each was fixed *in the program* by
  re-keying the relation — `checks.dl` 199.6 s → 13.7 s on two rules. So the
  index stays rejected and the 29× is if anything low; what a program cannot
  re-key is the case that reopens this — and since 2026-09-12 the `code-analysis`
  skill no longer teaches re-keying (a library free of engine workarounds is the
  user's direction), so every rule an agent writes is that case. _queued —
  **post-v1**._ — §15/engine.
- **Seeking makes body order matter more** — the same measurement, read the other
  way: good-vs-pessimal atom order cost **2.0×** before the seek and **29×** after.
  The scheduler runs positive atoms in strict source order with no cost model
  (`schedule.rs`), and reordering them is observable on the error path — whether a
  runtime error fires at all — so this is its own design session, not a patch.
  B5/C14 already assert the *answer* is order-invariant. Semi-naive makes it
  worse: a delta round walks the Full atoms before its delta position in full,
  which is 14 s of `pointsto.dl`'s 28 s on a 35% `vs/base` cut
  ([`notes/pointsto-profile-2026-09-12.md`](notes/pointsto-profile-2026-09-12.md)).
  _queued — **post-v1**._
  — §15/engine.
- **An aggregate runs once per binding of the atom beside it, not per distinct
  group key** — `calls(K, N) :- call_site(dispatch: K), N = count { … }` folds once
  per call site. Projecting the key first (`dispatch_kind(K) :- call_site(dispatch:
  K).`) took `code-analysis`'s `orient.dl` from **16.9 s to 1.2 s** on 205k facts
  (2026-09-11); the engine could deduplicate group keys itself. Answers are
  unaffected. **Not closed by 2026-09-11's reporting recorder**, which changes what
  an aggregate *stores*, not how often it folds. _queued — **post-v1**._ —
  §9/engine.
- **A reporting run records only the derivations the reports read** — **shipped ✅
  2026-09-11** (§17 that date; `Provenance::Reports`, `Derivation::reports`;
  `testing.md` **E9**, now three-way). An aggregate or a cast in any rule body used
  to provision the *full* store for the whole program to carry §9's skip count and
  §12's malformed count. On 1.33M facts one cast rule cost **+4.6 s and +1.36 GB**;
  `checks.dl` is **9.3 s / 1.8 GB** against 13.8 s / 3.2 GB. Counts byte-identical
  — the deduplication key is unchanged. It buys **memory**, and not always time:
  `coupling.dl` pays 27.5 s against 23.0 for half the residency. — §9/§12/engine.
- **Interning / `Rc<str>` for values** — a **memory** item, not a time one.
  `Value::eq` plus libc `memcmp` is 2–3% of a run: the seek deletes the comparisons
  rather than making each cheaper, and symbol *length* was never the driver (16× the
  width costs 7.5% of wall clock, but 75% more allocated bytes and 32% more RSS).
  [`notes/profile-2026-08-20.md`](notes/profile-2026-08-20.md). On Grafana's
  `symbol` the 5.8M string cells hold 263 MB of text, 96 MB of it distinct (2.7×),
  each cell a 32-byte `Value` plus a heap block
  ([`notes/memory-profile-2026-09-12.md`](notes/memory-profile-2026-09-12.md)). _queued — **post-v1**: memory only, and S6 is about exponents._ — §4/engine.
- **Parallelism** — assess how much of semi-naive evaluation and joins can go
  parallel (independent rules within a stratum, partitioned/hash joins) while
  preserving the deterministic canonical output and full provenance recording,
  which are load-bearing guarantees. Scope follows from the profile, which now
  exists and puts the prefix seek ahead of it, which shipped 2026-08-21. _queued —
  **post-v1**: S6 is about exponents, which parallelism does not change._ — engine.

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

- **Machine-readable error taxonomy** — **✅ shipped 2026-08-25**, and with it
  **S3, the last v1 criterion** (§1). `Error` is a struct — code, category,
  message, span, line/column position, suggestion — with `Display` composed from
  the fields. Two post-v1 follow-ons survive it, below.
  - **Spans on semantic and source errors — ✅ shipped 2026-08-24.** Every
    diagnostic carries a line and column; measured at 15 of 15 constructed
    families, and all five malformed reference pins moved to carry one. Stages
    after parsing record a span (`Error::at_span`) and `api.rs` resolves it once
    (§17, 2026-08-24). Spans stop at the file edge — see the new item below.
  - **A stable code vocabulary — ✅ shipped 2026-08-25.** 38 codes over 153
    emission sites, derived by census; the code is a required constructor
    argument, so the category is derived from it and a site cannot omit one.
    `Warning::code` too, in one flat namespace. Property **C16** and a pinned
    code set. — [`notes/error-codes.md`](notes/error-codes.md), §17 2026-08-25.
  - **`internal-error` renders under the wrong category** — a *malformed IR* is
    raised through the semantic stage, so it prints as `semantic error
    [internal-error]` when nothing about the program's semantics is at fault and
    the reader it addresses is us. Wants a fifth `ErrorKind`, which is normative
    §12 text and touches §14's exit vocabulary. _queued — **post-v1** (the code
    already tells a consumer what it needs)._ — §12/§14, `error.rs`.
  - **Per-file error attribution.** Spans are per-file byte offsets
    (`resolve.rs`), so a program that spliced in a module drops its spans rather
    than resolve them against the wrong text — correct, and a real loss for
    multi-file programs. `resolve.rs`'s `origins`/`files` side tables are the
    hook it already carries for this; module-resolution errors name their file in
    the message meanwhile. _queued — **post-v1** (single-file programs keep their
    positions)._ — §12/§13, `resolve.rs`.
  - **Suggestion coverage at the semantic stage** — 13 of 77 sites carry one, and
    none of the 36 source sites do. Guard-railed by §12's own hazard note: a
    suggestion that cannot be acted on costs a round, and one that is wrong on
    correct code is worse than none.

  The corpus was the test and it moved twice: all five malformed pins carry a
  position as of 2026-08-24 and a code as of 2026-08-25.
  _shipped — **v1** (S3, met)._ — §12, `../experiments/reference/malformed/`.
- **`--format json` scope** — a documented future *edge* feature (structured
  errors, provenance); the data path stays Datalog-native. _parked (low value) — **post-v1**._ — §14.
- **`serde` for the API** — decide as §14 stabilizes. _queued — **post-v1**._ — §14.

### Agent skill

- **`skill/recipes/source-analysis.md`** — the source-analysis recipe: extract
  with a real parser, import, ask; the five traps. Bundled by `cargo
  package-skill`, so `recipes/` is now a shipped part of the skill. ✅ 2026-07-27.
- **`ts-facts` — a TypeScript project as facts** — moved 2026-09-11 to its own
  project, `../code-analysis/` (renamed `code-facts`). ✅ 2026-09-10 here — §17.
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
  else here, which argues for early rather than first. _**shipped 2026-08-24** — **v1**: S1 has been measured once, which is what v1 asked of it._
  The harness is [`../experiments/`](../experiments/), its own top-level project.
  All seven domain packs are in, and the first full grid ran on 2026-08-24: a
  **null**, both arms at 41/48. §1 now names the harness and carries what the null
  does and does not say — chiefly that the engine arm reached for the engine in 9
  of 56 cells, so it is a null about *supplying* the engine rather than using it. — skill,
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
