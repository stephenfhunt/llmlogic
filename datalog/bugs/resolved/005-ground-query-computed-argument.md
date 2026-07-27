---
id: 005
title: A ground query with a computed argument prints nothing, where the folded spelling prints the fact
severity: usability
area: api
spec: ["§5", "§14"]
found: 2026-07-26
resolution: fixed 2026-07-27 — a query constant-folds a ground compound argument (`ArgMode::FoldGround`)
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

## Resolution

**fixed 2026-07-27** — `?- p("a", 1 + 1).` and `?- p("a", 2).` both answer
`p("a", 2).`, verified against the release binary. A query lowers a compound atom
argument with a new `ArgMode::FoldGround`: fold when the expression is ground,
hoist otherwise. `api.rs` did not change at all — the folded query is a single
atom again, so it reaches the substituted-atom case that already existed.

**The fix sketch was not taken.** It proposed plumbing hoist-origin into the IR
so `api.rs` could recognize lowering's own assignments; that would have cost a
new IR field and A15's claim that inline and hand-hoisted arguments lower to the
same program. The bug file's own "cheaper alternative" — an existence-check case
in `api.rs` — was also rejected, and it was not cheaper: `V` is unprojected, so
the row must be reconstructed before the atom can be printed. Rationale and what
each rejected option would have cost are in §17 (2026-07-27).

**It fixed one case more than the report described.** `?- p(X, 1 + 1).` — ground
argument, non-ground atom — printed the weaker `answer("a")` and now prints
`p("a", 2).`. Same defect, one variable short of ground; the report only covered
the fully ground spelling.

**Acceptance criterion 3 holds unchanged:** `?- V = 1 + 1, p("a", V).` still
prints `answer(2).`, so the *fallout* item stands — `testgen::hoist_atom_args`
keeps excluding queries, because the projection difference for an explicitly
named variable is real and was never part of this defect.

**The property took two attempts, and that is the durable finding.** Written
first as a rewrite over `arb_ast_program`, it passed *with the bug present*: two
spellings of a query that matches nothing both print nothing, and over arbitrary
generated programs a query that both computes and matches is vanishingly rare.
The syntactic non-vacuity guard was green the whole time — it counted ground
compound arguments in queries, which was never the binding constraint. The
targeted generator (`arb_ground_query_spellings`, which builds the expression
backwards from a value the EDB contains) failed on its first generated case and
shrank to `n("b", 2). ?- n(K, 0 + 2).`; that seed is recorded. The guard that
would have caught the vacuity asserts the generated queries *hold*
(`ground_query_spellings_generate_queries_that_hold`). Catalogued in testing.md
C8, which now carries both the property and this lesson.
