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

## Shipped milestones

The evaluation-first roadmap (decided 2026-07-10; rationale in §17). One line
each; detail in §17 and `docs/worklog.md`.

1. **AST + IR** — surface AST + positional core IR + lowering. ✅ 2026-07-19
2. **Core evaluator** — stratified semi-naive fixpoint with provenance recording. ✅ 2026-07-19
3. **Named-argument lowering** — named literals desugar to positional IR. ✅ 2026-07-20
4. **Stratified negation** — Ullman relaxation numbering + anti-join eval. ✅ 2026-07-20
5. **Lexer + parser** — hand-rolled, zero-dep; canonical printer; `parse→lower→typecheck→eval`. ✅ 2026-07-22
6. **Agent CLI** — one-shot `-q` queries; the Claude Code skill. ✅ 2026-07-23
7. **§13 imports** — data (CSV/JSONL/Parquet/URL via DuckDB) + module imports. ✅ 2026-07-23
8. **First-class absent value** — two-valued `absent`; `is [not] absent`; uniform null→absent imports (type-neutral). ✅ 2026-07-24
9. **§9 aggregation** — the canonical five (`count`/`sum`/`min`/`max`/`avg`), set-builder `op { Expr | Goal }` syntax, implicit grouping, skip-but-report via provenance, stratified (recursion rejected). ✅ 2026-07-24

## Open backlog

> **Open defects live in [`bugs/`](bugs/)** — currently `003` (three normative
> errors in `spec.md`) and `004` (§6 asserts a finiteness that arithmetic
> falsified), both out of the 2026-07-25 spec review and the design session that
> followed it (§17). `004` is blocked on "Termination & value-creating recursion"
> below, which leaves **`003` the cheapest to close**: one sitting's work, and no
> code change — the implementation is right and the spec is wrong.
>
> Three are resolved in `bugs/resolved/`: `001` (a compound argument in a negated
> atom silently misread as a wildcard), **fixed 2026-07-25** by negation item 2
> below; `002` (`-q` rejected a disjunctive rule), **fixed 2026-07-26**; and
> `005` (a ground query with a computed argument printed nothing), **fixed
> 2026-07-27** by folding a ground compound argument in a query rather than
> hoisting it (§17 — the bug file's plumb-the-IR sketch was rejected for
> colliding with A15).
>
> `cargo test -- --ignored` should report exactly **two** known failures, both
> absent × negation; a third means something regressed.

### Negation (§7)

Item 2 landed first, against the original sequencing. The reason is recorded in
§17 and in `bugs/001`'s resolution: the interaction item 1 owns is reachable
through an ordinary positive binding already, so item 2 added spellings that
reach an already-broken cell rather than creating a new one — and item 2 was
also the fix for a live soundness defect.

1. **`absent` × negation — a soundness bug.** `q(X) :- p(X), not p(X).` derives
   `q(absent)`: P ∧ ¬P, in an engine that advertises consistency checking. A
   variable bound to `absent` is both matched and unmatchable — a fresh slot
   binds to a stored absent, but every later use applies the semantic rule, which
   absent fails. Checked against SQLite: the companion symptom (`p(X), p(X)`
   selecting less than `p(X)`) is **not** an anomaly — SQL does the same, as the
   price of `NULL ≠ NULL`, which is the FK-blowup protection we want. Only the
   negation cell differs, and the leading fix is to make the anti-join a
   *structural membership test*, on the grounds that a negated atom binds nothing
   and so is not a join at all. Three directions, the SQL comparison, and the one
   behaviour change to decide (rows with an absent key drop out of "things with
   no …") are in §17. Acceptance criterion: two `#[ignore]`d tests
   (`a_fact_never_satisfies_its_own_negation`,
   `repeating_a_body_literal_does_not_change_the_answer`) already assert the
   sound behaviour. **Needs a design session — do not patch ahead of it**; every
   direction moves §4's structural/semantic split. _designing._ — §4/§7/§11.

2. **Negated atoms in the dependency schedule.** ✅ **2026-07-25.** A negated
   atom now reads the argument variables something else in the body binds, so
   `not q(X+1)`, `Y = X+1, not q(Y)` and `not q(Y), Y = X+1` are one conjunction
   with one answer; negations the positives already ground keep their early phase
   so anti-joins still prune first. Safety relaxed from "bound *positively*" to
   "bound by the body" in §7/§10. The rule had been stated in three places
   (scheduler phases, lowering's check, the engine's hand-built-IR contract) and
   the naive oracle hard-coded the old order — all four moved together. Closed
   `bugs/001`. Detail in §17; property coverage is testing.md **C7** + B1.
   — §7/§8/§10.

### Expressions (§5/§8)

Two findings from the 2026-07-25 spec review. They touch the same production —
`primary = [ "-" ] number | aggregate | term` admits neither grouping nor calls —
and are otherwise unrelated. **Deliberately not bundled:** the first is a small
task, the second an open design question, and pairing them would make the fix wait
on the question.

- **Parenthesized expressions — just add them.** `V = (A + B) * C` is a parse
  error, so any non-trivial expression must be hand-decomposed into `=`-chains.
  Verified 2026-07-25 that this is an implementation gap, not a design decision:
  **§17 has no entry on it** (the "deliberately flat" claim lives only in commit
  a7d1ff1's message, and the Phase D entry that ratified precedence, signed
  literals, and inline arithmetic never mentions grouping). Nothing semantic
  moves — `ast::ExprKind::Binary` and `ir::Expr::Binary` are already general
  trees, so lowering, typecheck, engine, safety, and provenance are untouched —
  and there is no ambiguity, since an atom must start with an identifier, so a
  body literal beginning with `(` can only be a comparison. Two edits:
  - `src/parser.rs:619-628` — the rejection block is *longer* than the
    `primary → "(" expr ")"` production that replaces it.
  - `src/print.rs:169` — the real work. `print_expr` documents its dependence on
    the current state ("No parentheses are emitted (the v1 grammar has
    none)… flat printing re-parses to the same tree"), which parens falsify.
    Printing becomes precedence-aware: parenthesize a looser-binding child, plus
    the right child of a left-associative operator at equal precedence so
    `A - (B - C)` survives. D2/D3 are already the right properties — they are
    scoped to parse-reachable ASTs, so widening `arb_ast_program` is what makes
    them exercise the new path.

  _queued (small; good company for `bugs/001-003`)._ — §5/§14.

- **Implement the `as` cast** — `Expr as type`, the conversion form (**design
  ratified 2026-07-25**, §17; written into §4/§5/§8). Postfix, binds tighter than
  `*` `/`, chains left-to-right; result type is the named type unconditionally;
  `absent as T` is `absent`; a lossy `i64 → f64` widening above 2⁵³ is an error,
  mirroring §13's import rule. Touches: the parser (a `cast` level between `mul`
  and `primary` — `as` is already `TokenKind::As`, and `type` is an existing
  production, so no lexer change), `ast::ExprKind::Cast`, `ir::Expr::Cast`, a
  `typecheck` arm (`set_type(result, T)` without unioning the operand — §9's `Avg`
  arm at `typecheck.rs:317` is the precedent), an `eval_expr` arm
  (`engine/mod.rs:960`) plus the naive oracle, and a `print_expr` arm. **One
  decision deferred to implementation time:** whether a failed conversion
  (`"abc" as int`) errors or yields `absent` — see the §17 open question, which
  states the trade-off. _queued._ — §4/§5/§8.

  *Supersedes the former "scalar-function call form" item.* User-defined scalar
  functions were **declined** 2026-07-25 (a rule already is one; §17), and what
  remains open is only *builtin* scalars with no relational spelling (`abs`,
  `length`, `lower`, `substr`), deferred until a consumer needs them — §17's open
  questions. If those ever land they must solve the `ident (` atom-vs-call
  ambiguity that ruled out `float(A)`; scan-ahead is the candidate.

**Strict numerics stay; conversion is the motivating case for the call form**
(user call, 2026-07-25). Verified: `int` and `float` never meet — `V = A + B`
across an int and a float column is a type error, `1 / 3` is `0`, and the only
conversion escape hatch is the import boundary (an explicit `as t(a: float)`
schema coerces cells; typed Parquet/DB sources carry their own types, so the
inference problem is really CSV-only). Two reasons not to fix that by widening
`int + float → float` implicitly:

- **This is a data-analysis language, not a general-purpose one.** When source
  data carries both an int and a float column there is usually a reason — a count
  vs. a measurement, an identifier vs. an amount — and that distinction is part of
  the data model the engine should respect rather than dissolve.
- **It reintroduces the hazard the import path just closed.** Widening a large
  `i64` to `f64` loses precision above 2⁵³, which §13 made a structured error
  2026-07-25 precisely because large integers in real data are usually
  identifiers. `id + 0.0` would silently round in expressions what `import` now
  refuses.

So the fix is an *explicit* `float(X)`, which keeps the widening visible in the
source text: `float(1) / float(3)` asks for a ratio, `1 / 3` stays honest integer
division. (SQL parity is the one real argument the other way. Note `avg`'s
`int → float` result type already shows the language will widen where a *declared
result type* says so — as distinct from coercing operands silently.) Open
sub-question: whether a rule this strict wants a diagnostic that names the
conversion, since today's message only reports a type clash.

### Termination & value-creating recursion (§6/§10) — a design session

**The language does not terminate, and has not since milestone 4.** This is
accepted today and runs forever — no output, no partial results, no cap:

```datalog
nat(0).
nat(N) :- nat(M), N = M + 1.
```

`N` is bound by the `=`-assignment, which §10 accepts as a binder; `nat` depends on
itself positively, which stratification allows. There is no iteration cap,
fact-count cap, or wall-clock budget anywhere in the engine. Pure Datalog's
guarantee rests on a finite Herbrand universe — no way to synthesise values absent
from the input — and §8 arithmetic ended that. On the surface that runs
LLM-generated programs this is a live denial-of-service vector, and it is why
`bugs/004` (§6 still asserts the finiteness) is blocked on this item.

**Direction chosen (user call, 2026-07-25): a static semantic error, not runtime
fuel.** Fuel was considered and rejected as hacky — a budget is not a guarantee,
and the point of this property is that it should be a theorem.

**Rule sketch** — precise enough to start from, not settled:

> Reject a program in which an **arithmetic-computed** value flows to the head of a
> **positively recursive** predicate: a rule whose head contains a variable bound
> by an `=`-assignment over arithmetic rather than by a positive body atom, where
> the head predicate participates in a positive cycle of the dependency graph.

**Where it lives.** `stratify` (`src/lower.rs`) already builds the graph with the
edge kinds needed — `Dep::Positive` is a distinct variant (`lower.rs:1139`) and
`strict()` separates it from `Negated`/`Aggregated`. One correction to the natural
assumption: `dependency_path` walks *all* edges regardless of kind, since it exists
to report a cycle after stratification diverges, so positive-cycle detection needs a
kind-filtered variant — a small addition, not a verbatim reuse.
`stratification_error` is the precedent for naming a concrete cycle in the message.

**The sketch discriminates correctly on the cases that matter** (verified
2026-07-25):

| program | behaviour | verdict |
|---|---|---|
| `succ(M,N) :- nat(M), N = M+1.` + `nat(N) :- succ(M,N).` | **hangs** | rejected — the assignment-bound head var sits on a rule in the positive cycle `succ → nat → succ` |
| `gen(M,N) :- base(M), N = M+1.` + `nat(N) :- nat(M), gen(M,N).` | terminates | accepted — `gen` is outside every positive cycle, and `nat`'s recursive rule binds `N` from a positive atom |

Informal soundness argument to make rigorous or refute: unbounded growth requires
arithmetic value creation, and if every value-creating assignment lies outside all
positive cycles then its predicate's extent is bounded by strictly lower strata and
is finite. Range restriction already helps — a pure generator
(`succ(M,N) :- N = M+1.`) is rejected today because `M` is unbound, so value
creation cannot occur without a positive atom supplying an input.

**Cases to confirm:** `next_year(X,N) :- age(X,A), N = A+1.` accepted (not
recursive); `Y = X as float` in a recursive rule accepted (casts map a finite set to
a finite set with no accumulation, §8); aggregates and negation already covered by
stratification, so the new rule need only cover arithmetic.

**The main open question — and the real cost.**
`path_cost(X,Z,C) :- path_cost(X,Y,C1), edge(Y,Z,C2), C = C1 + C2.` is **rejected**.
Cost-accumulating transitive closure is a legitimate, common idiom and directly
relevant to source analysis (call depth, cost propagation). The rejection is correct
in general — a cyclic graph makes it diverge — but it also blocks the acyclic case
users legitimately want. Whether that needs an escape hatch (an explicit bound, an
opt-in annotation, or "hoist it out of the recursion") is what makes this a design
session rather than a patch.

The session must also **write §10's missing Termination section**, and then either
ratify §2's "predictable evaluation" pillar in a form the implementation satisfies
or soften it — including deciding what it promises about merely *slow* programs,
which a static rule does not address at all. _designing._ — §2/§6/§8/§10.

### Aggregation follow-ons (§9)

- **Count-distinct, and the `_`-in-a-goal trap.** `count { S | e(_, S) }` returns
  the edge count, not the distinct-`S` count (verified: 3 where 2 is wanted),
  because a wildcard inside a goal is a witness dimension — the one place `_` does
  not mean "don't care" (§9). §9 documents both halves honestly and offers
  "project into a helper relation first", but every ranking and "how many" in the
  source-analysis use case that motivated §9 (§17, 2026-07-23) wants a distinct
  count, and the natural spelling silently returns the wrong number. Decide
  whether v1 gets `count_distinct` / a `distinct` modifier, or whether the trap is
  documented harder. _queued._ — §9.
- **Statistical reducers** — `median`/`stddev`/`variance`/`percentile`. The
  aggregate node reserves a parameter slot for `percentile(p)`; each is a reducer
  registration + a typecheck arm, no evaluator restructure. _queued._ — §9.
- **Collection-valued reducers** — `collect`/`string_agg`. Blocked on a
  first-class collection value (the value model is flat, §4); its own design
  session. _queued (blocked)._ — §9/§4.
- **Recursive/monotonic aggregation** — v1 rejects recursion through an aggregate
  via stratification; how far to take a fixpoint semantics (Zaniolo et al.,
  `references.md`) is open. _parked (research)._ — §9.

### Surface uniformity & the agent edge (§5/§14)

From the 2026-07-25 spec review. Each is a documented v1 limit rather than a
defect, but all three land on the agent surface, which is the one consumer least
able to work around them.

- **Three different body grammars.** Rule bodies are DNF; queries and aggregate
  goals are conjunction-only (all three verified). So a disjunctive filter can be
  expressed only by detouring through the rule form — which `bugs/002` made
  unusable via `-q` until 2026-07-26, and the detour is still the only route.
  Decide whether `;` extends to queries and goals, or whether the asymmetry is
  deliberate and gets stated as such in §5 (it currently reads as an aside:
  "Queries stay conjunctive"). _queued._ — §5.
- **An existence check has no answer.** `?- p("a"), q("b").` prints nothing and
  exits 0 whether or not it holds (verified both ways, still true 2026-07-27).
  §14 files this as a closure gap, but the sharper framing is that the engine
  cannot answer a yes/no question — and §5's ban on 0-arity atoms removes the
  obvious workaround. A single *ground atom* is distinguishable (output vs. no
  output), but a conjunction is not. This is now all that is left of that gap:
  `bugs/005` was the case where an ordinary single-atom query fell into it by
  accident, closed 2026-07-27. _queued._ — §5/§14.
- **`declare` does not count as defining a predicate.** `declare banned(name:
  string).` plus `not banned(X)` still warns "referenced but never defined"
  (verified). Declaring a schema is the user stating a relation exists and may be
  empty, and there is no way to suppress the warning. Either `declare` should
  define, or there should be a way to say "intentionally empty". _queued._ — §10/§12.

### Provenance surface (§11)

- **Provenance query syntax** — `?why <fact>` is provisional across CLI + API;
  also decide the proof-tree JSON encoding. Now also the surface §9's
  skip-but-report rule was designed around: an interim stderr warning covers the
  aggregate skip count (2026-07-25), but `?why` is what makes the rest of the
  recorded derivation readable. _queued._ — §11/§14.
- **Provenance as facts** — emit `?why` output as ground derivation-edge facts
  so provenance itself pipes back in (the Datalog-in/out closure). _queued._ — §11/§14.
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
  _queued._ — §11/§15.
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

### Performance (now unblocked)

Deliberately sequenced **after** aggregation and the absent value (both shipped
2026-07-24): tune a feature-complete surface rather than re-profiling as core
semantics change (both added evaluation paths that would move the hotspots). Now
the highest-signal next item.

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

### Spec hygiene & §6 (the document itself)

From the 2026-07-25 style review. The *false assertions* are `bugs/003`; what
follows is staleness, unwritten sections, and structure — real, but a different
kind of work.

- **§6 was never extended — the largest substantive gap.** Its own note still
  reads "Still to fill in: extension to aggregation (§9); semantics of
  comparison/arithmetic literals (§8)", both long shipped. So there is no
  model-theoretic account of aggregation, and none of `absent`. For a spec whose
  pitch is a logic engine an LLM can trust over its own reasoning, declarative
  semantics stopping at positive programs is the hole that matters most. Best done
  **with** the absent × negation session (negation item 1): "what does `p(X), not
  p(X)` mean" is a §6 question wearing an implementation costume. _queued (with
  negation item 1)._ — §6.
- **Ratify §1 and §2.** Both are still `TBD` — the doc opens on "*To fill in:
  concrete goals, non-goals, target users, success criteria*" and "candidate
  principles to ratify", while those pillars have driven every decision for three
  weeks and §2 is cited as settled authority at least four times. _queued._ — §1/§2.
- **Fix the status vocabulary.** Declared `TBD → Draft → Stable`; actual usage adds
  `Ratified` (§9, §13) and `living` (§17), and **nothing has ever reached
  `Stable`** despite §4/§5/§7/§8/§9/§13 being implemented, tested, and dogfooded.
  So the top rung is dead and "Draft" now spans "unwritten" (§6, §10) and "shipped
  and hardened" (§5, §8). _queued._ — all.
- **Clear the stale forward pointers.** All verified shipped: §4 "lands with
  roadmap step 7", §7 "lands with roadmap step 4", §15 "negation joins the loop at
  roadmap step 4", §6 "arrives with §7/§9", §13 "implementation is roadmap step
  7". §16.7 still explains a workaround in the present tense ("Until then, named
  access to a schema-less import is a structured error") that dissolved when step 7
  landed, and §16's preamble calls §16.4 "provisional" while §16.4's own footer
  says ratified. _queued._ — §4/§6/§7/§13/§15/§16.
- **Move the implementation audit trail out of normative text.** Status lines and
  body prose name Rust modules and dates ("validated by the hand-rolled lexer
  (`src/lexer.rs`), 2026-07-22"). Valuable traceability at the wrong altitude — a
  language spec should not need to know the engine has a file called `lower.rs`. A
  per-section footer or one traceability table keeps it without diluting the spec.
  _queued._ — all.
- **Normalize §17's chronology.** Reverse-chronological for 07-25/24/23/22, then it
  jumps to 07-03 and runs *forward* through 07-21, so a reader cannot tell which
  end is current. _queued._ — §17.
- **Restructure the §17 decisions log** — 48% of `spec.md`, growing 36× faster
  than the body, driven by 9 oversized entries rather than by decision count.
  Topic-keyed rewrite proposed; measurements, what is already decided, and the
  open questions are in
  [`notes/decisions-log-restructure.md`](notes/decisions-log-restructure.md).
  Pairs with `bugs/003` and the stale-pointer items below.
  _queued (user call; own session)._ — §17.
- **Rename the provenance "absence pattern".** §4 and §11 each carry a footnote
  apologizing for the collision with the `absent` *value* ("the two share a word,
  not a concept"). Two standing disclaimers is the signal to rename the provenance
  one — `why-not pattern`, or `no-match record`. Cheap, and it removes a permanent
  snag for the LLM readers who are the target audience. Touches
  `provenance::AbsentPattern` and `Premise::Absent`. _queued._ — §4/§7/§11.

### Errors & API edges (§12/§14)

- **Machine-readable error taxonomy** — §12 is now Draft: `Error` is a struct
  (category, message, span, line/column position, suggestion) with `Display`
  composed from the fields, and lexer/parser diagnostics carry real positions
  (2026-07-25). Two pieces remain: a stable **code** vocabulary so an agent can
  branch on `unsafe-aggregate` without matching prose, and **spans on semantic
  errors** — lowering reports many from points where the responsible span is not
  threaded, so choosing one per diagnostic is a design pass. _queued (partially
  shipped)._ — §12.
- **`--format json` scope** — a documented future *edge* feature (structured
  errors, provenance); the data path stays Datalog-native. _parked (low value)._ — §14.
- **`serde` for the API** — decide as §14 stabilizes. _queued._ — §14.

### Agent skill

- **`skill/recipes/source-analysis.md`** — a source-analysis recipe, now
  unblocked by §13 imports. _queued._ — skill.
- **"Big external fact base" demo** — the motivating import demo; unblocked by
  §13 (the USDA dogfood is a first pass). _queued._ — skill.
- **Other agent-exposure forms** — a Claude API agent-loop harness, and an MCP
  server. Both are deliberately waiting on a trigger: the skill (2026-07-23) is
  the first experiment, and how well a model actually drives it is what should
  decide whether a second form is worth building. _parked (awaiting the skill
  experiment)._ — skill.
