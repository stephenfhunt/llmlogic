---
id: 005
title: A ground query with a computed argument prints nothing, where the folded spelling prints the fact
severity: usability
area: api
spec: ["§5", "§14"]
found: 2026-07-26
resolution:
---

`?- p("a", 2).` prints the fact; `?- p("a", 1 + 1).` prints nothing and exits 0.
Same question, two spellings, two answers — and the empty one is
indistinguishable from "no such fact".

Found by `a15_inline_and_hoisted_arguments_agree` (testing.md C8) on its first
generated run, which is the third instance of this class after `001` and `002`.

## Repro

```datalog
p("a", 2).
?- p("a", 2).       % p("a", 2).
?- p("a", 1 + 1).   % (nothing), exit 0
```

Verified against the built binary. Not specific to arithmetic in the second
column, and not specific to `+` — any compound argument in an otherwise ground
query does it.

## Root cause

Not a bug in lowering or evaluation: the query *matches*, and both spellings
lower to the same join. It is §14's output rule meeting §5's hoisting.

`src/api.rs` decides output shape by body shape (§14):

- a **single positive atom** whose variable positions are all named → re-emit
  that atom with bindings substituted (this is the ground case that prints);
- any **other** body → synthesize `answer/N` over the query's *named* variables;
- a body with **no named variables** that is not a substitutable single atom →
  no fact-shaped output at all. §14 already flags this as "the one shape the
  closure does not cover yet".

A compound argument hoists to `V = 1 + 1, p("a", V)` with `V` anonymous
(`src/lower.rs`, `ArgMode::Hoist`). So the query is no longer a single atom, and
it has no named variables — it lands in the third case. The gap §14 documented as
a corner is reachable from a query that looks completely ordinary.

The hand-hoisted spelling `?- V = 1 + 1, p("a", V).` prints `answer(2).`, a third
distinct output for the same question, because `V` is named and so is an answer
variable. That difference is *correct* — the user named something and asked to
see it — and is not part of this defect.

## Fix sketch

The substitutable-single-atom test should be made against the query's **atom**,
not its literal count: a body that is one positive atom plus only the
`=`-assignments lowering generated for that atom's arguments is still, to the
user, a single-atom query. Lowering knows which assignments it generated; the
information is lost by the time `api.rs` inspects the body.

Cheaper alternative, if that plumbing is unwanted: treat a body with no named
variables as an **existence check** and print the ground atom when it holds,
which is what §14's first case already does for the folded spelling.

Either way §14's prose needs to stop describing the shape rule in terms the
surface no longer determines — the same defect `bugs/002` found in §14's `-q`
description.

## Acceptance criteria

- `?- p("a", 1 + 1).` and `?- p("a", 2).` produce identical output.
- A property: for a ground query, folding an argument by hand does not change the
  output. This is the §14 half of C8's spelling-equivalence group, and the reason
  `hoist_atom_args` currently skips queries — the rewrite cannot assert that
  spellings agree there while this is open.
- `?- V = 1 + 1, p("a", V).` still prints `answer(2).` — naming a value is a
  request to see it, and that difference is intended.

## Fallout

- **`testgen::hoist_atom_args` excludes queries** (2026-07-26) and says why.
  Once this is fixed, the exclusion should be revisited: the *projection*
  difference for an explicitly named variable is real and must stay, but the
  ground case should agree.
- **§14's "one shape the closure does not cover yet"** is more reachable than
  that phrasing suggests, and should say so.
- No engine or lowering change is implied; this is an output-shape defect.
