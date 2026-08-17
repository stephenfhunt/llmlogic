# The query answer shape — survey, boundary, and the reopened question

Long-form backing for the **2026-08-03** §17 decision (the substituted-atom form
widens to one positive atom plus non-binding literals). §17 holds the decision;
this file holds the survey, the measured boundary, and the alternatives that were
rejected.

**Reopened 2026-08-16** (user call, §17). It was the implementation session's
brief; it is now the *design* session's, and the widening is not to be built until
the question below is answered. Nothing measured here is retracted — what changed
is that the question got wider than the widening. The axes are at the end.

## The boundary

Measured 2026-08-03 against a freshly built `trunk` binary, over

```
declare person(name: string, age: int).
person("alice", 34).  person("bob", 17).  person("carol", 29).
banned("carol").
```

| `-q` argument | today | under the decision |
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

## Why variable-set equality, and not the current check

`answer_lines` (`src/api.rs`) tests that **every atom argument is a constant or a
projected variable**. That is one direction. It is sufficient today only because
the guard above it requires the body to be a single literal, so the atom is the
only binder and the reverse inclusion holds for free.

Widening removes that guarantee: a body may now carry an aggregate that binds a
variable the atom does not mention (row 5). Emitting the atom would silently drop
that column — an answer with a missing field, which is worse than an ugly name.
So the widened guard needs both directions, i.e. the atom's variable set **equals**
the projection.

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

## What this does to the existence-check item

The queued ROADMAP item (**Surface uniformity & the agent edge**, §5/§14) narrows
rather than closes. Row 3 of the table is a ground single-atom-plus-filter query
that currently has no answer and gains one. What remains is the genuine
*multi-atom* check — `?- p("a"), q("b").` — which still has two positive atoms,
still binds nothing, and still has nowhere to put a yes/no, with §5's ban on
0-arity atoms removing the workaround.

## The reopened question — five axes

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

**2. If it synthesizes, is `answer/N` the right name?** It collides with every
other query's answer, so piping two runs merges unrelated relations under one head
— and the survey above already established that *an inferred name is the wrong name
for composition either way*. tsdl's `answer` is arity-1 boolean **only**, which
gives it exactly one meaning. A narrower name is a cheaper name.

**3. The projection hazard is unfixed in both engines.** Their §16 re-emits a
substituted atom exactly as we do, so `?- p(X, "a").` prints `p` facts over a
subset of `p`'s rows with nothing marking it as one. §14 documents this here; their
spec does not mention it. **Nobody has solved it**, and a session reading their
design must not take it for the version without the hazard. The verified
demonstration is in *Rejected: keep `answer/N` as a projection marker* above.

**4. Yes/no.** `?- p("a"), q("b").` prints nothing and exits 0 whether or not it
holds — the *silently lost question*, which `EXPERIMENTS.md` names as the failure
mode that matters most, and §5's ban on 0-arity atoms removes the workaround. tsdl
emits `answer(true)` / `answer(false)` for a body with **no answer variables**,
arity one, always boolean, with no second sigil: which form a query gets is read
off its body. **This axis has no real opposition** and could be settled ahead of
1–3.

**5. A CLI has an option a library does not, and neither engine has considered
it.** `answer/N`, `unnamed-answer` and define-and-select are all *language*
answers. The name could instead be given at the **invocation** — `-q 'conflict:
p(X), q(X)'`, or `--as conflict` — which keeps the language total, keeps output
composable under a name the author chose, and costs no round. It is a strictly
smaller ask than define-and-select (no rule head, no body repetition) and it lands
where the asymmetry actually is: tsdl is embedded in a host that has its own
vocabulary for naming things, and we are a program invoked from a shell.

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

**No recommendation is recorded, deliberately.** One would turn a question the user
asked to have thought through into a decision with a default.

## Implementation brief

*Frozen 2026-08-16 pending the question above — accurate for the widening as
decided, and not to be executed until the shape is settled.*

- Site is `answer_lines` in `src/api.rs`; the shape decision is entirely
  syntactic, over `ir::Query`. No evaluator, lowering or IR change.
- The guard becomes: exactly one `BodyLiteralKind::Atom` that is not negated;
  every other literal binds nothing (comparisons, negated atoms, `is [not]
  absent`); and the atom's variable set equals the projection set.
- Aggregates bind, so a body carrying one falls to `answer/N` unless the
  aggregate's result variable is somehow inside the atom — which the equality
  test handles without a special case.
- `testing.md` C8 carries the property, written `#[ignore]`d and failing first.
  Its generator must be built backwards from a fact the EDB contains: the
  cautionary precedent is `a_computed_query_argument_answers_like_its_value`,
  whose first rewrite-based version **passed unfixed** because the shapes that
  can differ almost never co-occur over arbitrary programs.
- Sweep on landing: §14's answer-shape list, `skill/SKILL.md`, and
  `docs/agent-skill.md` each restate the rule for their own audience.
