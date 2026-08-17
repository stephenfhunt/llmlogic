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

> **Open defects live in [`bugs/`](bugs/)** — currently only `004` (§6 asserts a
> finiteness that arithmetic falsified), out of the 2026-07-25 spec review and the
> design session that followed it (§17). It is **partly unblocked as of
> 2026-08-16**: three of its five acceptance criteria are now answerable, since §6,
> §10 and §15 state what they do not cover and §15 says a budget's absence is a
> decision. What it still waits on is the Termination *rule* being written and
> built. The remaining design sessions are **Termination** and **§6's extension**,
> the latter now two unknowns lighter — the anti-join settled `p(X), not p(X)` on
> 2026-07-29, and the truncation contract settled what an incomplete model is worth
> on 2026-08-16.
>
> Five are resolved in `bugs/resolved/`: `001` (a compound argument in a negated
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

### Expressions (§5/§8)

One finding from the 2026-07-25 spec review remains. Its sibling —
**parenthesized expressions** — shipped ✅ 2026-08-16: `primary → "(" expr ")"`,
with precedence-aware printing, an arbitrarily-shaped expression generator, and
`grouping_survives_the_print_round_trip` as the concrete acceptance partner. The
two were **deliberately not bundled**, one being a small task and the other an
open design question; that is why the fix did not wait.

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

### The value model (§4)

- **Temporal types — `date`, `timestamp`, `duration`.** The value model has five
  primitives and nothing temporal, so a date column out of §13's CSV/JSONL/Parquet
  lands as a string or an int and there is no arithmetic over it. This engine needs
  them *more* than a semantic-layer engine does, because it imports real files where
  date columns are ubiquitous. **Read the sibling engine's finding before designing
  it** (`notes/tsdl-cross-project-review.md`): a subject asked for "days" got
  `172800000` with no error, because the cast it wrote was not wrong — it yielded
  milliseconds — and they closed it by *deleting* the cast in favour of
  `duration / duration → number`, so the divisor is where a program names its unit.
  A unit that has to be documented is the design being wrong. _queued (own
  session)._ — §4/§8/§13.

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
and the point of this property is that it should be a theorem. **Reaffirmed
2026-08-16** against a sibling engine that ships both: a budget's forcing case is a
hung browser tab, and a CLI has `^C`, so a slow program stays slow. The exposure
that argument does *not* cover is a hosted surface (MCP, API harness) — both parked.

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
or soften it. What it no longer owes: an answer about *slow* programs, which
2026-08-16 settled as "a slow program stays slow", and an account of what an
incomplete model is worth, which is the item below. _designing._ — §2/§6/§8/§10.

### The truncation contract (§9/§13/§15) — decided, not built

**An incomplete model does not answer, and the whole-model surface survives it**
(§17, 2026-08-16). Decided, with three live instances that report through a stderr
warning today: §9's skipped aggregate rows, §13's unrepresentable import cells, and
an external signal. The discriminating rule is how the program reads the short
relation — projected, one row short and say so; folded or negated, withhold.

The design question the decision leaves open is **the CLI shape**: a shell pipeline
reads stdout and cannot see a stderr warning at all, so "withhold" has to mean an
exit code and a stdout discipline, not a return field. Sequence it with §11's
`?why`, whose `unknown` arm is the same distinction reached from the other side.
_queued — decided, not built._ — §15, `notes/tsdl-cross-project-review.md`.

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
  _queued._ — §9/§13.
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

Items landing on the agent surface, the one consumer least able to work around
them. Except where noted these are documented v1 limits rather than defects.

- **The answer shape — what a query prints, and under what relation name.**
  **Reopened 2026-08-16** (user call): the 2026-08-03 widening stands but is **not
  to be built** while the shape it widens is under review. Absorbs two items that
  were separate and are one question — the widening itself (`?- p("a"), 1 < 2.`
  answering nothing where `?- p("a").` answers), and the multi-atom existence check
  (`?- p("a"), q("b").` printing nothing and exiting 0 either way, which is the
  *silently lost question* `EXPERIMENTS.md` names as the failure mode that matters).
  Five separable axes, a fifth option neither engine has considered, and the
  evidence are in §17's open question. _designing._ — §5/§14,
  `notes/query-answer-shape.md`.
- **Three different body grammars.** Rule bodies are DNF; queries and aggregate
  goals are conjunction-only (all three verified). So a disjunctive filter can be
  expressed only by detouring through the rule form — which `bugs/002` made
  unusable via `-q` until 2026-07-26, and the detour is still the only route.
  Decide whether `;` extends to queries and goals, or whether the asymmetry is
  deliberate and gets stated as such in §5 (it currently reads as an aside:
  "Queries stay conjunctive"). One datum on how much it matters: in a sibling
  engine's experiment **neither subject reached for `;` at all**, both writing two
  rules over one head, so the case for uniformity is consistency rather than demand
  (`notes/tsdl-cross-project-review.md`). _queued._ — §5.
- **No string operations.** No prefix, split or concat, so reducing `lower::tests`
  to `lower` is unwritable and the fact producer must do it (2026-07-27). Closed:
  string construction fails §5's own termination test and would widen the hole the
  Termination session has not yet closed. _rejected — §17, 2026-07-27._ — §8/§10.
  (Ordered comparison over strings was the separate, opposite call: widened,
  `bugs/006`, closed 2026-07-27.)
- **`declare` does not count as defining a predicate.** `declare banned(name:
  string).` plus `not banned(X)` still warns "referenced but never defined"
  (verified). Declaring a schema is the user stating a relation exists and may be
  empty, and there is no way to suppress the warning. Either `declare` should
  define, or there should be a way to say "intentionally empty". _queued._ — §10/§12.

### Provenance surface (§11)

- **Provenance query syntax — designed 2026-08-16, not built.** One union of
  `proof` / `underivable` / `unknown`; the sigil is a cost hint; a near-miss is a
  *rule*, not a binding; a repair is a step, not a promise; the trace re-solves
  through the scheduler the fixpoint uses. §17 has the decision. What remains open
  is the rendering, the JSON encoding, and sequencing against the truncation
  contract, whose distinction `unknown` is. _queued — decided, not built._ — §11/§14.
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
  _queued._ — §11/§15.
- **Semiring provenance under negation** — tropical cheapest-proof selection and
  the algebra behind it; sketch in `notes/semiring-provenance.md`. **`?whynot` with
  minimal repairs is no longer part of this item**: it was designed 2026-08-16
  without a semiring, a near-miss being a *rule* rather than a binding, which is
  what bounds it. What stays parked is the algebra. _parked (research)._ — §11.
- **Does the derivation store earn its cost?** A sibling engine extracts a proof
  *backwards* from the retained model — no recorder in the fixpoint, no re-run — so
  a run nobody questions pays one integer per row. It is not a free swap: we record
  *all* derivations (§17, 2026-07-19) and backwards extraction yields one. But
  `notes/performance-baseline.md` names this recorder as its top hypothesis for the
  35× cliff and has never measured it, so **the profiling item below now has a
  concrete architecture to profile against.** Sequence after the profile, never
  before. _queued (after profiling)._ — §11/engine.

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

- **Profile the engine** — never profiled; first measurements and the leads they
  suggest are in [`notes/performance-baseline.md`](notes/performance-baseline.md)
  (import is 0.32 s and not the problem; ~15k derived tuples/s; a 35× cliff from
  bottom-up materialisation). Stand up a repeatable benchmark and a profile
  *before* optimizing anything. _queued (after feature-complete)._ — engine.
- **Parallelism** — assess how much of semi-naive evaluation and joins can go
  parallel (independent rules within a stratum, partitioned/hash joins) while
  preserving the deterministic canonical output and full provenance recording,
  which are load-bearing guarantees. Scope follows from the profile. _queued
  (after profiling)._ — engine.

### Spec hygiene & §6 (the document itself)

From the 2026-07-25 style review. The *false assertions* were `bugs/003`, closed
2026-07-27; what follows is staleness, unwritten sections, and structure — real,
but a different kind of work.

- **§6 was never extended — the largest substantive gap.** Its own note still
  reads "Still to fill in: extension to aggregation (§9); semantics of
  comparison/arithmetic literals (§8)", both long shipped. So there is no
  model-theoretic account of aggregation, and none of `absent`. For a spec whose
  pitch is a logic engine an LLM can trust over its own reasoning, declarative
  semantics stopping at positive programs is the hole that matters most. **Its own
  session, and now the best-prepared one:** the anti-join decision settled what
  `p(X), not p(X)` *means* on 2026-07-29 and §4 states the four match sites, so §6 has a
  ratified semantics to describe rather than one to decide. Its remaining unknowns
  are aggregation and the finiteness claim `bugs/004` owns (blocked on
  Termination). _queued._ — §6.
- **Ratify §1 and §2.** §1's goals, non-goals, target users and success criteria
  have never been written, and §2's principles are still candidates — one of which,
  "predictable evaluation", the implementation does not satisfy. Both now say so in
  their own *Not covered* footers instead of behind a status marker. _queued._ — §1/§2.
- **Rename the provenance "absence pattern" → `no-match pattern`**, with the
  explanation of a missing answer being a **failure trace** (§17, 2026-08-16 — three
  names, adopted verbatim from a sibling engine). Counted: **31 sites over 8 files**,
  nothing frozen, so the sweep is total. _queued — decided, not built._ — §4/§7/§11.
- **Name a test per §16 example.** No example names the test that runs it, and the
  expected output is prose in a comment rather than a fence compared byte-for-byte —
  so a block nobody wired up cannot fail. The rule going forward is **no example
  means no feature**; retrofitting the eight is the item. Add a diagnostic example
  while doing it: none of them exercises an error, so nothing checks what the engine
  prints when a program is wrong. _queued._ — §16, `testing.md`.
- **Normalize §17's chronology.** Reverse-chronological for 08-16 back to 07-25,
  then it jumps to 07-03 and runs *forward* through 07-21, so a reader cannot tell
  which end is current. _queued._ — §17.
- **Restructure the §17 decisions log** — **53% of `spec.md`** as of 2026-08-16
  (1 538 lines against §§1–16's 1 390; it was 48% on 2026-07-26), growing far faster
  than the body, driven by oversized entries rather than by decision count.
  Topic-keyed rewrite proposed; measurements, what is already decided, and the
  open questions are in
  [`notes/decisions-log-restructure.md`](notes/decisions-log-restructure.md).
  **Moving it to its own file** is the option a sibling engine took and this should
  weigh — their spec holds no decisions at all. _queued (user call; own session)._ — §17.

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
  shipped)._ — §12.
- **`--format json` scope** — a documented future *edge* feature (structured
  errors, provenance); the data path stays Datalog-native. _parked (low value)._ — §14.
- **`serde` for the API** — decide as §14 stabilizes. _queued._ — §14.

### Agent skill

- **`skill/recipes/source-analysis.md`** — the source-analysis recipe: extract
  with a real parser, import, ask; the five traps. Bundled by `cargo
  package-skill`, so `recipes/` is now a shipped part of the skill. ✅ 2026-07-27.
- **"Big external fact base" demo** — the motivating import demo; unblocked by
  §13 (the USDA dogfood is a first pass). _queued._ — skill.
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
  something loses the question silently. _queued._ — skill, `EXPERIMENTS.md`.
- **Other agent-exposure forms** — a Claude API agent-loop harness, and an MCP
  server. Both are deliberately waiting on a trigger: the skill (2026-07-23) is
  the first experiment, and how well a model actually drives it is what should
  decide whether a second form is worth building — **which is a decision gated on
  the item above**, not on elapsed time. A hosted surface is also the one place the
  no-budget termination call does not cover (see Termination). _parked (awaiting the
  skill experiment)._ — skill.
