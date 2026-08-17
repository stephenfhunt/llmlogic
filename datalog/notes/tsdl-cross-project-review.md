# What `tsdl` has to teach this engine — a cross-project review

Long-form backing for the **2026-08-16** §17 entries. §17 holds the decisions;
this file holds the survey, the evidence behind each one, and — the part §17 has
no room for — everything **declined**, with its reason.

**Snapshot, not a live reference.** Read against `~/code/tsdl` as of 2026-08-16.
Every claim below carries a file-and-section pointer so a later session can check
it rather than trust it; that project moves daily, and a claim here may already be
one of its own amended decisions.

## What tsdl is, and why it is not an outside project

An embeddable Datalog engine in TypeScript, started 2026-07-31. Its `README.md`
names this project as its prior art — *"its surface syntax, diagnostic discipline
and body scheduler are adopted here; its eager-materialization source layer,
string-rendered results and always-on derivation store are the things this
inverts"* — and its `testing.md` states that its four testing rules are *"earned
from the defect record of `~/code/llmlogic/datalog`"*, citing our `bugs/` files
by count.

So it is this project's design run forward in a different runtime, with a written
rationale for every divergence. That makes it unusually good evidence and a bad
authority: its forcing constraints are a browser tab, a host-supplied fact base
and zero dependencies, and none of the three is ours.

| | this engine | tsdl |
|---|---|---|
| runtime | Rust CLI + library | TypeScript, browser and Node, zero deps |
| facts in | `import` (CSV/JSONL/Parquet/URL, DuckDB) | `FactSource` only — no filesystem, no import |
| evaluation | semi-naive; naive is a test oracle | naive ships; semi-naive deferred |
| performance | a goal — profiling is the next item | an explicit non-goal |
| provenance | all derivations recorded in the fixpoint, always on | boolean semiring by default; proofs extracted backwards |
| decisions | `spec.md` §17 | `decisions.md`, a separate file |
| status markers | `TBD`/`Draft`/`Stable` | none — a status marker is an audit failure |
| backlog | `ROADMAP.md` + `bugs/` | neither, deliberately |

## Adopted — see §17, 2026-08-16

Three entries. What follows is the evidence each one rests on.

### The truncation contract

Their `spec.md` §15 splits termination in two: the same static rule we chose
2026-07-25, *plus* an `EvaluationLimits` budget over rounds, derived facts, wall
clock and source calls. **We took the reporting half and declined the budget** —
the decision and its rejected alternatives are in §17.

The argument that moved us is theirs and is runtime-independent:

> **Truncation costs soundness and not only completeness.** The route is the
> query. A budget stops evaluation where it stands, so no *rule* ever reads a
> truncated relation — but a query is solved against the model as it was left, and
> `not p(X)` over an incomplete `p` **succeeds**. The row it produces is not a
> missing answer but a wrong one.

The asymmetry underneath it is what makes the contract implementable: the store
only grows, so what it holds when a run is cut short is the fixpoint **minus
rows** — missing facts, never false ones. Hence their split, which we adopt: the
whole-model surface survives truncation and the *answers* do not.

Their `verdict` field (`'ran' | 'rejected' | 'failed' | 'stopped' | 'dropped'`,
`spec.md` §§16, 18) generalizes this past termination to any short relation, with
the rule we took verbatim:

> what a dropped row costs the run depends on **how the program reads the
> relation**: projected, the answers come back one row short and say so; folded or
> negated, the run withholds everything, because a `sum` over a relation missing a
> row returns a number nothing supports and no layer downstream can tell.

That is §9's skip-but-report case decided on a principle rather than a warning.
**What does not transfer is the shape.** `verdict` is a field on a return value; a
CLI has an exit code and two streams, and a shell pipeline reading stdout cannot
see a stderr warning at all. Ours is a stdout discipline, and designing it is the
implementation item.

Two smaller observations, kept because they are cheap to lose:

- A budget is read *between rounds*, so it cannot fire while a run is suspended on
  a source. We have no budget and no suspension, but the shape recurs wherever a
  check is placed at a loop boundary.
- A run that gave up one round before convergence is marked exactly like one that
  gave up halfway: *"the engine cannot tell the two apart without the round it was
  denied."* An honest report cannot claim to know how close it got.

**They ship with cost-accumulating transitive closure refused unconditionally**
(`spec.md` §15, and their agent guide calls it "the case you will hit"). That is
evidence the escape hatch is not needed to *ship*. It is not an answer to our open
question about whether the acyclic case deserves one.

### The provenance query surface

Their `spec.md` §13 and four `decisions.md` entries dated 2026-08-08. Both tiers
are built; ours has neither. The claims we adopt:

- **One union, whichever form asked** — `proof` | `underivable` | `unknown`. The
  sigil is a **cost hint**, not a selector, *"because a form that could only answer
  one way would make a reader know the answer before asking"*: `?why` over a fact
  that turns out not to hold, and `?whynot` over one that does, are both ordinary
  questions with ordinary answers. **Rejected there:** a single `?explain` (the two
  are genuinely different work, so the sigil says where effort goes), and two
  separate result shapes.
- **`unknown` is the arm that keeps the other two honest.** Over a store that was
  cut short, *not derivable* and *not derived yet* are the same silence. Its reason
  is the run's verdict minus `'ran'` — an explanation is solved after the fixpoint
  exactly as an answer is, so whatever withholds one withholds the other. This is
  the truncation contract and the provenance surface meeting, and it is why the two
  §17 entries are dated together.
- **A near-miss is a rule, not a binding, and that is the entire bound.** One entry
  per rule whose head unifies with the goal, carrying the longest prefix any binding
  satisfies and the first binding that reaches it — so a body over `parent("alice",
  Y)` reports one near-miss and not one per child. Nothing truncates, no budget is
  spent, and the bound is the program's own rule count.
- **How far a body got is a fact about the schedule.** The trace is printed in body
  order and re-solved through the *same* solver the fixpoint uses, because *"an
  extractor that decided literal order for itself would be a second evaluator"*.
  We have the same scheduler, so this transfers unchanged — and it means the literal
  reported as blocked is the first one the run would really have failed.
- **A repair is a step, not a promise.** The literals past the block were never
  evaluated, so adding the fact a repair names advances that rule's prefix and need
  not derive the goal. Three cases name no fact: a blocked *derived* predicate,
  whose repair is the next question to ask (`repair: ask ?whynot ancestor("bob",
  "dan").`); a pattern with a slot nothing bound; and a refuted negation, which
  names the row that refuted it, since there is no retraction in the language.
- **A goal names one fact**; a variable in it is its own diagnostic pointing at
  `?-`, since a claim about *this* row has nothing to bind a goal standing for many.
- **An explanation rides in `%` comments** in the one text stream. *"A proof tree is
  not a fact and could join a fact stream on no other terms."* Strip the comments
  and what is left is byte-for-byte what the same program without its goals prints —
  which is a property, and their guard. The structured value stays the normative
  artifact; the text is a renderer over it.

**This resolves our "provenance as facts" ROADMAP item in the negative**, and the
reason is better than "we didn't get to it": a proof tree is not a fact, so
emitting it as ground derivation-edge facts either invents relations the program
never declared or flattens a tree into rows that no longer compose. The comment
channel keeps Datalog-in / Datalog-out intact and costs the closure property
nothing. Their `spec.md` §16 has the rendering; `docs/agent-guide.md`'s last
section has a worked `?whynot` we can read as a target.

Related and **not** adopted, because it is theirs to have and not ours: they
factor the lineage legend so a base fact cited by twelve rows prints once, and
group the shared suffix of a fold's citations — 400 base facts over 200 rows
falling from 241 KB to 15 KB, *"quadratic to linear"* (`spec.md` §16). We have no
lineage annotation to print, so this is a note for whenever Tier 1 lands.

### The three names

Our ROADMAP has carried *"rename the provenance absence pattern"* since the
2026-07-25 review, with §4 and §11 each holding a footnote apologising for the
collision. They settled it (`spec.md` §13):

> the missing-data value is `absent`; the premise justifying a negated literal is
> a **no-match pattern**; the explanation of a missing answer is a **failure
> trace**. Three names, because one word doing two jobs needs a footnote at every
> use.

Taken verbatim. Divergent vocabulary between two engines that put the same kind of
guide in front of the same kind of reader has a cost of its own.

## Adopted — spec hygiene and testing

### Their spec carries no status markers, and no decisions

Four of our *Spec hygiene* ROADMAP items are one problem, and they have run the
experiment on the fix:

| our item | their answer |
|---|---|
| `Stable` never reached; `Draft` spans unwritten and shipped | **no status markers.** `*Draft*`, `*Ratified*` and a deferral note are *audit failures* in their spec (`AGENTS.md`) |
| §17 is more than half of `spec.md` | **decisions live in `decisions.md`**, a separate file. Measured 2026-08-16: our §§1–16 are 1 390 lines and §17 is 1 538; their spec is 2 869 lines of language definition holding no decisions at all, with 3 763 more in `decisions.md` |
| implementation audit trail in normative text | their spec names no source file in normative prose |
| deferrals scattered; stale forward pointers | **every section ends with *Not covered*** — one home per deferral, so a session proposing to add it finds the prior decision first |

Two conventions that come with it:

- **"No example means no feature."** Every worked example ends with `` Test: `name` ``
  and their audit fails if one does not; an example reaches the tour only once its
  test passes, so *"a block nobody wired up fails rather than quietly never
  running"*. Our §16 is already the canonical corpus at every level of the pyramid
  — nothing enforces the link.
- **An executed example proves a claim about the channels it compares, and nothing
  about a channel it is silent on.** Their harness prints errors *instead of*
  answers and warnings *above* them, because comparing only the channel an example
  happens to use is how their guide came to show two blocks omitting a warning the
  engine was really emitting — and, earlier, how every guide example passed while
  the prose's comment spelling was wrong, because no example had a comment in it
  (`testing.md`, *The layers*).

**Not adopted:** moving §17 to its own file is a separate session with
`notes/decisions-log-restructure.md` already written for it; bundling a
~1 300-line split into this one would bury three decisions inside a
reorganization.

### Testing rules 2 and 3, and two refinements

Their four rules are ours, harvested from our defect record and stated back more
sharply. We had written 1 and 4; 2 and 3 we practised without stating, so nothing
audited them.

- **Rule 2 — every generator carries a non-vacuity guard**, and the guard is
  checked against **the property's sentence** — not the generator's breadth, not
  the assertion's reach. *"It is the claim, which is the only thing a guard can be
  checked against."* They found five violations in two months, in two shapes: a
  guard certifying the **data** where the claim is about what the **run** carried,
  and a guard certifying breadth **no property reads**. Auditing one means reading
  it against its property's sentence; nothing mechanical substitutes.
- **Rule 3 — mutation-verify, and write the mutation on the catalog line in the
  same sitting.** *"A mutation nobody recorded is a check that happened once."*
  They keep it a manual habit rather than a mutation-testing tool, on the evidence
  that their two informative runs produced *the mutation was aimed wrong, a sibling
  property reddened* and *this one hangs, because a fixpoint whose progress signal
  never quiets has no last round* — neither a state a runner can assign.
- **Independent vs differential oracles**, as a rule about how to *write* one: an
  oracle that calls the engine's own function agrees with a wrong engine forever.
  Not hypothetical there — a safety oracle and a schedule replay both called
  `effectsOf`, and a mutation dropping a negated atom's reads went undetected
  because both sides asked the same broken function. Their stratification oracle
  additionally refuses to consult its *generator's* intent, on the same reasoning.
  We reached this from the other side in `bugs/006`; they have it as a construction
  rule, and our open **E6** replay hole is the same species.
- **A rejection claim wants a biconditional property.** Their cross-type-comparison
  property is *"rejected exactly when the two sides differ"*, because a one-sided
  property also passes against a checker that refuses **every** comparison. That is
  `bugs/006` restated as a property shape, and it generalizes to every "the checker
  rejects X" claim we make.

## Staged, not decided — the answer shape

See §17's open question and `notes/query-answer-shape.md`, which is that
question's home. In brief: they emit `answer(true)`/`answer(false)` for a body
with no answer variables — arity one, always boolean, the only synthesized name in
the language — and make everything else an **`unnamed-answer`** error handing back
the rule that would name it, on the argument that *nothing is ever printed under an
invented relation name*, which raises the closure property from "output re-lexes"
to "output re-parses and typechecks against the program that produced it".

Two things worth carrying across before that question is answered:

- **The projection hazard is unfixed in both projects.** Their §16 re-emits an atom
  with bindings substituted exactly as ours does, so `?- p(X, "a").` prints `p`
  facts over a subset of `p`'s rows. Our §14 documents the hazard; theirs does not
  mention it. Nobody has solved it, and their design should not be read as the
  version without it.
- Their `EXPERIMENTS.md` task A found a **bare conjunctive query is the first
  reflex**, and `unnamed-answer` **never fired** — the guide's "define a rule and
  query its head" line prevented it. So the error's *prevention* is measured and its
  *recovery* is not.

## Findings recorded, not acted on

### `count distinct`, and what their experiment learned about the diagnostic

They spelled `count distinct`, citing our own 36-where-20-was-meant measurement as
the reason. The transferable part is what their agent experiment then found about
the warning beside it, and it generalizes past aggregation to every `suggestion`
field we emit:

- Their `invisible-witness` warning fired **on both correct programs and was
  silent on both wrong ones** — perfectly inverted. Half that inversion is now
  closed by asking the rows whether the unmentioned field changed the answer; half
  stays open on purpose, because a rule deriving one head tuple from several bodies
  is a set built deliberately as often as it is a mistake.
- **Its suggested fix was destructive and it led with it.** `sum distinct` on a
  *correct* program folds the projected durations and collapses two equal ones,
  halving the answer. *"A diagnostic that fires on correct code and recommends
  breaking it is worse than no diagnostic."*
- **Models act on a suggestion literally.** One subject took the *null* suggestion
  — naming the field `_`, which silences the warning without moving the number —
  as a corrective one and stated in prose that the double-counting was fixed.
  Another hit a message whose suggestion was *"the cast table is in the
  specification, §3"*: a document the reader of that guide does not have and never
  will.

Ours is `Errors & API edges` (§12), where the open pieces are a stable code
vocabulary and spans on semantic errors. This adds a third: a suggestion is
executable text, and one that cannot be acted on costs a round.

### Their experiment is a measuring instrument; ours is a checklist

The largest qualitative gap, and the highest-leverage thing in this document that
we have not taken. Our `EXPERIMENTS.md` is honest about being *"a checklist to
eyeball in a session"*. Theirs (`EXPERIMENTS.md`, `tools/experiment/`) is
controlled:

- **A fixture in a domain the guide never uses** — *"a subject handed a
  familiar-shaped table copies the guide's program instead of writing one."* Ours
  reuses `tests/programs/`.
- **The prompt is assembled by a tool** whose relation catalogue is *derived from
  the fixture's schemas*, so a renamed field cannot silently turn the experiment
  into a measurement of something else.
- **The subject sees the prompt and nothing else**, and its **first program is
  recorded before any feedback**, because that is the one that reads the guide
  rather than the diagnostics.
- **Every task is run at two model strengths.** *"A guide that only the strongest
  available model can follow is a guide that fails in production, and the strongest
  model is also the least informative subject: it routes around gaps instead of
  falling into them."*
- **A guide line is measured by cutting it** from one cell's assembled prompt,
  leaving engine, fixture and question alone. They ablated a four-line date-range
  paragraph across ten subjects: all ten wrote a correct range comparison either
  way, and what the block moved was a *spelling* the type system makes
  unfalsifiable. The paragraph was aimed at a reflex nobody has.
- **Reference programs are pinned byte-exact** and run on every check, so the
  corpus cannot rot — and their 2026-08-16 entry records the limit of that: *"a
  corpus of correct programs cannot pin what a tool does when a run goes wrong."*

What it produced that reasoning would not:

- **The failure that matters is silent and survived three rounds of feedback.** A
  subject wrote an intermediate rule projecting away the work-order id; a relation
  is a set, so seven closed orders became three rows and the `sum` was a third of
  the truth. Given the output, an error, and the output again, it fixed two other
  things and **never touched the deduplication, because nothing in an answer
  distinguishes a total from a collapsed row.** Reproduced on a second task at the
  same strength split. *This is our own `dead(F)`-returns-six-used-functions
  finding under control* — and their `decisions.md` cites ours as its motivation.
- **Recursion needs no teaching**: their guide shows no recursive program and both
  strengths got it first try. **Disjunction likewise** — neither subject reached
  for `;` at all, both writing two rules over one head. That is evidence on our own
  open question about extending `;` to queries and aggregate goals: the demand is
  not there, so the case for uniformity is consistency, not need.

**What to re-derive rather than copy.** Our subject is a Claude Code agent with a
CLI, a filesystem and a real repo, not a text-in/text-out subject behind a `dl.ts`.
The *controls* transfer — unfamiliar domain, first-program-before-feedback, two
strengths, derived catalogue, pinned references. The harness shape does not. And
our most valuable finding to date has no analogue in their setup: the three
questions the model answered with `grep` because the engine could not express
them, because their subject has no `grep`. **A skill that cannot say something
loses the question silently, and the model does not announce the switch** — that
is ours to keep, and a redesign adopting their controls must not lose it.

Note what this gates: our ROADMAP parks *other agent-exposure forms* on the
grounds that *"how well a model actually drives it is what should decide whether a
second form is worth building"* — a decision waiting on an experiment we have not
run rigorously.

### Temporal types

They ship `date`, `timestamp` and `duration` as first-class scalars with a typed
arithmetic table (`spec.md` §3); we have `symbol`/`string`/`int`/`float`/`bool`
and nothing temporal. **We need this more than they do:** §13 imports CSV, JSONL
and Parquet from real files, where date columns are ubiquitous and currently land
as strings or ints with no arithmetic.

Their experiment is worth reading before designing it. A subject asked for "days"
wrote `(C - O) as number`, which was **not** a wrong cast — it yielded
*milliseconds*, so the answer came back as `172800000` where `2` was meant, with
no error and a number plausible enough to ship. Their first fix was a paragraph in
the guide stating the unit. That paragraph is gone: `duration as number` was
**retired** in favour of `duration / duration → number`, so `(C - O) / @P1D` says
days and the divisor is where a program names its unit. *"A finding closed by
deleting the construct rather than documenting it."*

### Small items they shipped that we have queued

- **Parenthesized expressions** — added at the start, as one of *"llmlogic's
  documented regrets"*. No new information; a nudge on priority.
- **One uniform body grammar** for rules, queries and aggregate goals — our "three
  different body grammars" item, closed by fiat. Their disjunction finding above is
  the evidence about how much it matters.
- **`declare` defines a predicate.** Our item stands; their resolution is **not
  portable**. They cut `Signature` from `FactSource`, which made a
  declared-and-underived predicate the *normal* shape of a host-backed relation, so
  the warning would have fired on the working case; the typo it was meant to catch
  is now caught from the other side, by an error the moment anything reads an
  undeclared relation by name. We have imports, so our shape differs — but their
  `decisions.md` entry is worth reading before we decide.

## Declined, with reasons

Recorded because an unwritten decline gets re-proposed, and because each of these
looks like an improvement from inside their document.

1. **`FactSource`-only ingest; no import statement, no filesystem.** They removed
   what their entry calls *"llmlogic's single largest subsystem"* because a browser
   has no filesystem and the host supplies facts by design. §13's imports are a
   shipped pillar and the thing the CLI is *for*; their own entry concedes ours is
   *"genuinely convenient for a CLI"*.
2. **Deleting `ROADMAP.md` and `bugs/`.** They ship without either, deliberately.
   Our defect record is the most productive artifact in this repo — it is what
   taught *them* their four testing rules, by their own attribution — and
   location-is-status has closed six defects. The *length caps* remain the right
   control and already exist.
3. **Naive-first evaluation.** They ship naive so *"the shipping evaluator is the
   obviously-correct one"*, with semi-naive deferred over an annotated-delta hazard
   (under annotations, rediscovering a tuple is not a no-op — the delta must be over
   annotation *change*, not tuple presence). We ship semi-naive with naive as a
   test-only oracle, which is right for a Rust engine where throughput is on the
   table. Their own 2026-08-04 profile note concedes the exponent is still O(n⁴)
   for a chain of n, and that what they fixed was the constant.
4. **Performance as an explicit non-goal.** Correctly opposite. Worth reading their
   phrasing anyway, because the *ingest* carve-out is one we already make in
   practice and have never written down: *"typing, coercion, precision guards and
   the pushdown contract are correctness the host cannot work around."*
5. **Zero-dependency / ES-library-only build discipline**, and the `make check` /
   `knip` / discipline-audit toolchain around it. Environment-specific. The *idea*
   of a mechanical audit is worth having — it is what makes "no status markers" and
   `` Test: `name` `` enforceable rather than aspirational — but it belongs on our
   stack, and CI is already a ROADMAP item.
6. **One `number` type.** They rejected our strict int/float split explicitly:
   *"defensible for a data-analysis language, but it makes mixed arithmetic a type
   error an agent must debug."* Their reason for one `number` is that JS has one,
   which does not transfer, and our position is argued at length in ROADMAP
   (identifier precision above 2⁵³, the import boundary as the only coercion site).
   No change. Recorded so the question is not reopened from scratch.
