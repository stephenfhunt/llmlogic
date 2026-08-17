# The query answer shape — survey and boundary

Long-form backing for the **2026-08-03** §17 decision (the substituted-atom form
widens to one positive atom plus non-binding literals). §17 holds the decision;
this file holds the survey, the measured boundary, and the alternatives that were
rejected. It is the implementation session's brief.

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

## Implementation brief

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
