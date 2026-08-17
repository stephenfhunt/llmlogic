# The query answer shape — survey, boundary, and how the axes resolved

Long-form backing for the §17 decisions of **2026-08-03** (the substituted-atom
form widens) and **2026-08-17** (the shape as it now stands, plus the named query).
§17 holds the decisions; this file holds the survey, the measured boundary, the
alternatives that were rejected, and the axis-by-axis outcome.

**Reopened 2026-08-16, settled 2026-08-17.** Nothing measured here was retracted;
what changed is that the question got wider than the widening, and then answered
in a shape none of the five axes had on its own. The outcomes are at the end.

## The boundary

Measured 2026-08-03 against a freshly built `trunk` binary, over

```
declare person(name: string, age: int).
person("alice", 34).  person("bob", 17).  person("carol", 29).
banned("carol").
```

| `-q` argument | before 2026-08-17 | now (verified against the binary) |
|---|---|---|
| `person(name: N, age: A), A >= 18` | `answer("alice", 34).` … | `person("alice", 34).` … |
| `person(name: N, age: A), not banned(N)` | `answer("alice", 34).` … | `person("alice", 34).` … |
| `person("bob", 17), 1 < 2` | *nothing*, exit 0 | `person("bob", 17).` |
| `person("bob", 17)` | `person("bob", 17).` | unchanged |
| `person(name: N, age: A), C = count { X \| banned(X) }` | `answer("alice", 34, 1).` … | unchanged — `C` is outside the atom |
| `person(N, _)` | `answer("alice").` … | unchanged — projection ≠ atom variables |
| `parent(X, Y), parent(Y, Z)` | `answer("alice", "bob", "carol").` … | unchanged — no unique name |

Row 3 is the finding that moved the decision: adding a trivially-true filter to a
query that answers makes it answer nothing. It is `bugs/005`'s shape reached by a
different route, and it is why this was not filed as a cosmetic complaint about
the name `answer`.

**Four rows the table did not have**, measured 2026-08-17 while checking the
premises, all of which printed nothing whether or not they held:

| `-q` argument | before | now |
|---|---|---|
| `person("bob", 17), banned("carol")` | *nothing* either way | `banned("carol").` `person("bob", 17).` |
| `1 < 2` | *nothing* either way | `holds(true).` |
| `not banned("bob")` | *nothing* either way | `holds(true).` |
| `person(_, _)` | *nothing* either way | `holds(true).` |

So the lost question was **three shapes wide**, not one: the multi-atom check the
axes named, plus a bare comparison, a negation-only body, and an atom whose only
variable is a wildcard. The last is the one nobody had written down, and it is the
most likely of the four to be typed by hand.

## Why variable-set equality, and not the check it replaced

`answer_lines` (`src/api.rs`) used to test that **every atom argument is a constant
or a projected variable**. That is one direction, and it was sufficient only while
the guard above it required the body to be a single literal, so the atom was the
only binder and the reverse inclusion held for free.

Widening removes that guarantee: a body may carry an aggregate that binds a
variable the atom does not mention (row 5). Emitting the atom would silently drop
that column — an answer with a missing field, which is worse than an ugly name.
So the guard needs both directions, i.e. the atoms' variable set **equals** the
projection.

**Confirmed by mutation, not by argument** (2026-08-17): restoring the
one-directional test is mutation M1 of C8's property, and it fails on exactly row
5's shape. That is also why the property's generator had to be given a fourth
case — a body binding a variable no atom mentions — since without it every
generated case passes under either guard.

## Rejected: adopt Soufflé's shape

Soufflé was the comparison asked for. It gives two answers, and they point in
opposite directions.

**Batch mode has no `?-` at all.** Goals are simulated by `.output` directives:
you write a relation, mark it for output, and Soufflé writes `<relation>.csv` as
tab-separated tuples — the relation name is the *filename*, not part of a row.
So Soufflé's answer to "what is the output of a filtered join called?" is *the
user names it*, and there is no synthetic name because there are no anonymous
queries. That answer already exists here as define-and-select:

```sh
datalog people.dl -q 'adult(N) :- person(name: N, age: A), A >= 18.'
# adult("alice").
# adult("carol").
```

**Interactive `explain` mode** does answer comma-bodies — `query edge(1, x),
path(x, y)` — but prints Prolog-style bindings, `x = 2, y = 3 ;`, advancing with
`;` and stopping with `.`. That forfeits the D1 closure outright: bindings are
not valid input.

So neither is adoptable. What the survey *did* settle is the division of labour
now stated in §14: **inference is for reading one query's output, naming is for
composing it.** Inference cannot help composition, because an inferred name is
the wrong name for composition either way — `person` collides with the source
relation it was projected from, and `answer` collides with every other query.
Only a name the author chose collides with nothing.

Sources: <https://souffle-lang.github.io/directives>,
<https://souffle-lang.github.io/provenance>.

## Rejected: keep `answer/N` as a projection marker

The most serious argument for the status quo is that `answer/N` *signals*
something — "this is a query result, not a relation dump" — and that widening
removes the signal from more queries.

It does not survive contact with the current behaviour. `answer/N` marks
**unnameable**, not **projection**: the single-atom form has printed a real
predicate's name over a subset of its rows since 2026-07-22, with no marker at
all. Verified:

```sh
datalog family.dl -q 'ancestor("alice", W)' | datalog - -q 'ancestor(X, Y), X != "alice"'
# (nothing — stage two sees an `ancestor` holding only alice's rows)
```

So the hazard is real, undocumented before this session, and *pre-existing*. The
widening enlarges its blast radius without changing its character: a filtered
query's rows are all true tuples of the named predicate, exactly as a projection's
are. The resolution was to document it in §14 — where it now has a paragraph —
rather than to buy a partial marker by keeping more queries anonymous.

## What this did to the existence-check item

The queued ROADMAP item (**Surface uniformity & the agent edge**, §5/§14) narrowed
first and then closed. Row 3 of the table is a ground single-atom-plus-filter query
that had no answer and gained one from the widening. The genuine *multi-atom* check
— `?- p("a"), q("b").` — survived that, having two positive atoms, binding nothing,
and with §5's ban on 0-arity atoms removing the workaround; **it closed 2026-08-17
in two pieces**, since a ground conjunction prints its atoms and everything else
with no answer variables prints `holds(true)`.

## The reopened question — five axes

*Settled 2026-08-17; each axis's outcome is recorded under it, and the section
that follows draws the conclusions the axes did not individually contain.*

Added 2026-08-16. **They are separable, and answering them as one is the trap.**
Where a second engine (`~/code/tsdl`, see `notes/tsdl-cross-project-review.md`)
took the other branch, it is named; it is evidence and not an authority.

**1. Does an unnameable query error, or synthesize a name?** The real
disagreement; everything else is downstream. tsdl errors — `unnamed-answer`,
handing back the rule that would name it — on the argument that *nothing is ever
printed under an invented relation name*, which raises the closure property from
"output re-lexes" to "output **re-parses and typechecks** against the program that
produced it". We synthesize. The cost of erroring is a round trip on a question the
user has already asked clearly; the cost of synthesizing is below.

> **Answered: synthesize.** The measured support for erroring covers its
> *prevention* and never its recovery, and axis 5 protects composition more
> cheaply than a refusal does. Requiring a name would also make the language
> *partial* — well-formed queries with no answer — where naming as an addition
> keeps it total. Axis 5 is nonetheless the **prerequisite** for ever revisiting
> this: with it, recovery is "prepend a word" instead of "rewrite it as a rule".

**2. If it synthesizes, is `answer/N` the right name?** It collides with every
other query's answer, so piping two runs merges unrelated relations under one head
— and the survey above already established that *an inferred name is the wrong name
for composition either way*. tsdl's `answer` is arity-1 boolean **only**, which
gives it exactly one meaning. A narrower name is a cheaper name.

> **Answered: `answer/N` stays, and the truth value gets `holds/1`.** Renaming
> addresses none of the three hazards, which are all about *composition*; and the
> axis had the dependency backwards. `-q 'flag(_, X), truthy(X)'` prints
> `answer(true).` today — a 1-column bool projection — so adopting tsdl's boolean
> `answer` would make two meanings byte-identical. They do not have that collision
> because their `answer` is boolean-only; we would have been creating it.
>
> A **warning on reading `answer` facts** was proposed as the substantive fix and
> withdrawn the same session: the hazard is a two-run merge, indistinguishable
> inside a single run from legitimate single-pipe composition, so it would fire on
> correct programs and be silent on the case that matters — the inverted
> `invisible-witness` warning, reached from the other side.

**3. The projection hazard is unfixed in both engines.** Their §16 re-emits a
substituted atom exactly as we do, so `?- p(X, "a").` prints `p` facts over a
subset of `p`'s rows with nothing marking it as one. §14 documents this here; their
spec does not mention it. **Nobody has solved it**, and a session reading their
design must not take it for the version without the hazard. The verified
demonstration is in *Rejected: keep `answer/N` as a projection marker* above.

> **Answered: unfixed for unnamed queries, and axis 5 is its fix.** Not by
> choosing between the two engines' answers but by noticing that a *named* query
> has no hazard at all — named output wears no source relation's name, so nothing
> is narrowed. This is the axis that turned axis 5 from ergonomics into the reason
> to build it.

**4. Yes/no.** `?- p("a"), q("b").` prints nothing and exits 0 whether or not it
holds — the *silently lost question*, which `EXPERIMENTS.md` names as the failure
mode that matters most, and §5's ban on 0-arity atoms removes the workaround. tsdl
emits `answer(true)` / `answer(false)` for a body with **no answer variables**,
arity one, always boolean, with no second sigil: which form a query gets is read
off its body. **This axis has no real opposition** and could be settled ahead of
1–3.

> **Answered: `holds(true).` on a yes, nothing on a no.** Settling it first was
> load-bearing, not procedural — it is what exposed axis 2's collision. Two
> departures from tsdl's shape: the name, per axis 2; and **no `holds(false)`**,
> because silence already means no for `?- p("a").` and has since 2026-07-22, so
> adding a false arm would have made one rule out of two spellings of the same
> answer. The measurement above also widened the axis: three shapes were losing
> the question, not the one it names.

**5. A CLI has an option a library does not, and neither engine has considered
it.** `answer/N`, `unnamed-answer` and define-and-select are all *language*
answers. The name could instead be given at the **invocation** — `-q 'conflict:
p(X), q(X)'`, or `--as conflict` — which keeps the language total, keeps output
composable under a name the author chose, and costs no round. It is a strictly
smaller ask than define-and-select (no rule head, no body repetition) and it lands
where the asymmetry actually is: tsdl is embedded in a host that has its own
vocabulary for naming things, and we are a program invoked from a shell.

> **Answered: taken — and it is a *language* form, not an invocation option.** The
> axis's own framing was the part that was wrong. A name given at the invocation
> cannot reach the printer without either surviving lowering or having `api.rs`
> recompute the projection, and it should not be `-q`-only anyway, because a file
> program carries the same projection hazard. What the axis got right is that this
> is the cheapest of the naming answers: it is exact sugar for a rule whose head is
> the projection, so it adds a spelling and no semantics. Brief at the end of this
> file; not built in the settling session.

### Evidence to weigh, and what it does not show

- tsdl's `EXPERIMENTS.md` task A: **a bare conjunctive query is the first reflex**,
  and `unnamed-answer` **never fired** — the guide's "define a rule and query its
  head" line prevented it. So the error's *prevention* is measured; its *recovery*
  is not, and axis 1 turns partly on recovery.
- The same runs found models act on a diagnostic's suggestion **literally**, which
  is what an `unnamed-answer` handing back a pasteable rule depends on — and, from
  the other side, why a suggestion that cannot be acted on costs a round.
- Against that, our own `EXPERIMENTS.md`: a skill that cannot say something **loses
  the question silently, and the model does not announce the switch**. An error is
  loud. `answer/N` and axis 4's silence are not.

### What the axes did not individually contain

Recorded 2026-08-17, because three of the four findings that decided this came from
the *relations between* axes rather than from any one of them — which is the
concrete payoff of the instruction not to answer them as one.

- **Axis 4 settled axis 2.** `answer(true)` collides with a 1-column bool
  projection, and that is only visible once the yes/no form and the synthesized
  name are held apart. Settling the "no real opposition" axis first was therefore
  not a warm-up.
- **Axis 3 settled axis 5.** Naming stopped being an ergonomic extra once it turned
  out to be the only known fix for the projection hazard — which nobody had
  connected, in either project.
- **Axis 5 settled axis 1**, but only for now: it is what makes erroring
  *affordable* later, and simultaneously what makes it unnecessary today.
- **The measurement widened axis 4** beyond what any axis said: three shapes were
  losing the question, and `?- p(_).` — the likeliest of them to be typed by hand —
  appears in neither project's discussion of this.

The one axis that answered on its own evidence is **axis 1**, and the asymmetry
recorded above is why: prevention was measured, recovery never was.

*Superseding the note that stood here from 2026-08-16 — "no recommendation is
recorded, deliberately" — which held until the question was put.*

## What shipped, 2026-08-17

The brief frozen 2026-08-16 was executed with two additions the axes forced. Site
was `answer_lines` (`src/api.rs`) and nothing else: no evaluator, lowering or IR
change, the whole decision being syntactic over `ir::Query`.

- **The guard** is exactly one non-negated `BodyLiteralKind::Atom` **or** any
  number of them with no variables at all, and the atoms' variable set equal to
  the projection. The "binds nothing" test on the other literals turned out
  unnecessary: set equality already covers it, because a literal that binds a
  *named* variable puts it in the projection, and one that binds an unnamed slot
  binds nothing observable. An aggregate used only as a filter therefore keeps the
  substituted form, which is right and was not foreseen.
- **Facts sort by name then value**, since a ground conjunction emits more than one
  relation. Verified body-order independent.
- **`holds/1`** where there is nothing to substitute, no false arm.
- **`testing.md` C8** carries `c8_an_answer_shape_neither_drops_nor_collapses_a_row`
  over `arb_answer_shape_case`, whose oracle filters the generator's own fact list.
  Four mutations recorded in the commit; M1 is the one-directional guard.
- **Four expectations changed**, each the widening working, and two spellings
  converged: `?- V = 1 + 1, p("a", V).` and `?- p(X, 1 + 1).` now print the same
  bytes, so `bugs/005`'s acceptance 3 moved to an assignment-bound variable
  *outside* every atom, which still discriminates.
- **Swept**: §14's rule and footer, §16.10 (new, with its named test),
  `skill/SKILL.md`, `docs/agent-skill.md`.

## The named query — brief for its own session

Decided 2026-08-17 (§17), deliberately not built. `?- conflict: p(X), q(X).` is a
**language** form, exact sugar for a rule whose head is the projection:

```datalog
?- adult: person(name: N, age: A), A >= 18.
% ≡  adult(N, A) :- person(name: N, age: A), A >= 18.
%    ?- adult(N, A).
```

- **Not `-q`-only sugar.** The name must survive lowering to reach the printer, and
  recomputing the projection in `api.rs` would be a second classifier of the kind
  §13's lexer move exists to prevent — where it diverged from lowering's notion of
  *bound*, the user would get a range-restriction error instead of an answer. It
  also belongs in the grammar because a file program carries the same projection
  hazard a `-q` does.
- **Arity is the projection's length**, i.e. exactly the columns `answer/N` prints:
  positional, `_` still not a named variable, aggregate goal-locals still
  unprojected. An empty projection yields `name(true).`, §5's 0-arity ban being why
  the yes carries an argument.
- **Always range-safe**, which is worth stating because it removes a whole error
  class from the design: the projection *is* the set of variables the body binds, so
  every synthesized head variable is bound by construction.
- **The name forces one relation**, which is the point — it is the fix for the
  projection hazard, and `?- adult: …` must print `adult` facts rather than the
  narrowed `person` facts the unnamed form prints.
- **One guard**: reject a name the program already defines or declares. Predicates
  intern by **name alone** (`src/ir.rs:411`), so a same-arity collision silently
  extends that relation and a differing arity produces an arity error whose message
  would not explain itself.
- Sites: `ast::Query` (`src/ast.rs:151`) gains one `Option<Ident>`; a parser arm;
  `print_query` (`src/print.rs:109`) for the round trip; `ir::Query`; §5's grammar;
  a §16 example. Naming stays **optional** — requiring it would make the language
  partial — but it is the prerequisite for ever revisiting axis 1's strictness.

## What shipped, 2026-08-17 — the named query

The brief above, executed the same day, with one of its sites turning out not to be
one. §17 has the decision; §16.11 and
`tests/system.rs::named_query_program_publishes_its_own_relation` are the example.

- **`ir::Query` gained nothing, and neither did `api.rs`.** The desugaring happens
  in `lower_query`: it synthesizes an `ir::Rule` over the body it has just lowered
  and leaves the one-atom query `name(<projection>)` behind, reusing the same
  `VarScope` — no re-lowering, no second scope. That query is then a single atom
  whose variables equal the projection, which is precisely the substituted-atom case
  the shape rule settled earlier the same day, so **the answer-shape function needed
  no third arm**. The empty projection falls out too: the head is the ground
  `name(true)`, printed by that same path.
- **The brief's "must survive lowering to reach the printer" understated its own
  conclusion.** Once lowering desugars to a rule, nothing needs to reach the
  printer at all. Carrying an `Option<Ident>` into the IR and branching in
  `answer_lines` would have been a *different, weaker* feature — it prints the same
  bytes, but the relation would not exist, so `eligible(N) :- adult(N, _).` could
  not read it and provenance would have nothing to explain.
- **The guard's third case had to be decided here**: reject a name the program
  **defines or declares**, allow one it merely **references**. Rejecting a
  referenced-only name would make the sugar inexact, since defining it is exactly
  what the equivalent hand-written rule does. This needed a `defined` set in
  lowering's pass 1, `by_name` alone conflating the two.
- **C8 carries `c8_a_named_query_matches_its_desugared_rule`**, whose oracle *is*
  the desugaring, written as program text. Sound only because the generator writes
  down the projection it emitted — reading the variable order back out of lowering
  would have made the oracle circular. Four mutations killed (`testing.md`).
- **One limit worth stating to consumers, and now stated**: a named query publishes
  *every* variable its body binds, so it is not a drop-in for a rule that projects
  fewer columns — `adult: person(name: N, age: A), A >= 18` is `adult/2` where
  `adult(N) :- …` chose `adult/1`. In `skill/SKILL.md` and `docs/agent-skill.md`.
- **Swept**: §5's grammar, §14's shape rule and hazard paragraphs and its footer,
  §16.10's closing line (its missing half is now §16.11), §16.11 with its named
  test, `testing.md` C8, the two agent-facing restatements.
