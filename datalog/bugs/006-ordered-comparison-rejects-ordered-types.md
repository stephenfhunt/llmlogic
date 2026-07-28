---
id: 006
title: "`<` rejects strings/symbols/bools, which §8 orders and which `min`/`max` already compare"
severity: usability
area: typecheck
spec: ["§4", "§8", "§9"]
found: 2026-07-27
resolution:
---

§8 says ordered comparisons work on every primitive: *"Ordered comparisons use the
operand type's natural order (ints/floats numerically, **strings/symbols
lexicographically**, `false < true`)."* The typechecker requires int or float and
rejects the rest.

The same file contradicts itself about the same section. `src/typecheck.rs:15`
(module doc): *"builtin operands (§8): arithmetic and ordered comparisons require
int or float."* `src/typecheck.rs:322` (the `min`/`max` arm): *"may be any single
ordered type (every primitive is ordered, §8) — so no numeric constraint."* Both
cite §8; only the second matches what §8 says.

## Repro

```
p("a").
p("b").
p("c").
```

```sh
datalog p.dl -q 'M = min { X | p(X) }' -q 'M = max { X | p(X) }'
# answer("a").
# answer("c").          ← strings ordered lexicographically, as §8 specifies

datalog p.dl -q 'p(X), p(Y), X < Y'
# semantic error: type error: variable `X` in query 0 has type string but is used
# in arithmetic or an ordered comparison, which requires int or float
```

Same relation, same three values, same order. One construct uses it; the other
refuses. `=` and `!=` on strings are accepted, so `<` is the only gap.

## Root cause

Three lines in `src/typecheck.rs`, in the `Compare` arm (line 280):

```rust
self.union(l, r);                                       // 279 — same-type rule
if matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge) {
    self.numeric.push(l);                               // 281 — the defect
}
```

`finish` (line 360) then rejects any slot in `self.numeric` whose resolved type
fails `is_numeric` (line 69). The constraint is deliberate rather than an
oversight — `ordered_comparison_on_non_numeric_is_a_type_error`
(`src/typecheck.rs:589`) pins it with `X > "a"` — so whichever way this goes, one
of §8's text or that test is wrong today.

**The evaluator needs no change; it already implements §8 as written.**
`apply_compare` (`src/engine/mod.rs:946`) checks operand types at runtime and
then evaluates `lhs < rhs` on `Value` directly, and `Value` derives `Ord`
(`src/ir.rs:189`) over `Absent < Symbol < String < Int < Float < Bool` — §4's
canonical order, the same one `min`/`max` fold with and the printer sorts by. The
naive oracle does the identical thing (`src/engine/naive.rs:213`, `:228`). So
both evaluators would already agree on `"a" < "b"` if the program reached them.
The typechecker is the only gate.

Two things the fix must not disturb, both in the same arm:

- **`union(l, r)` on line 279 is what makes cross-type a type error.** It is
  separate from the numeric constraint, so deleting lines 280–282 leaves
  `1 < "a"` rejected, as §8 requires.
- **The `absent` early-`continue` on line 273** already exempts a bare `absent`
  operand from every type rule (a comparison with `absent` is two-valued false,
  not an error). Widening does not touch it.

## What it cost

Found by using the engine on real data, not by reading the spec. Change-coupling
analysis needs the standard co-occurrence idiom — canonicalise the unordered pair
`(A, B)` as `A < B` so each pair is counted once. With string keys that is
unwritable, so the extractor had to be changed to emit a numeric `file_id`
alongside every filename purely to give the query something to order by. A
language limit pushed work back into the fact producer, which is the layer a
Datalog user is least able to change when the facts come from someone else.

## Acceptance criteria

**Direction chosen 2026-07-27 (owner, in session): widen the typechecker.** The
reasoning was end-user usefulness, and it is also the side that needs no spec
change — §8 already says this works. Recorded here as the intended direction, not
as a ratified §17 decision; the session that implements it owns the entry.

So: delete lines 280–282 of `src/typecheck.rs` (the `matches!` block), keeping
`union(l, r)`. Then

- **Invert the pinning test.** `ordered_comparison_on_non_numeric_is_a_type_error`
  (`src/typecheck.rs:589`) becomes an acceptance test. Its current name is a
  statement of the defect.
- **Reword `src/typecheck.rs:15`**, the module doc that says ordered comparisons
  "require int or float". After the fix only arithmetic does.
- **Cover the four widened types and the boundary**: `"a" < "b"` true, `"b" < "a"`
  false, symbols ordered lexicographically, `false < true`, and `1 < "a"` *still*
  a cross-type error. Also `absent < "a"` still false, not an error.

**The existing differential property will not cover this.**
`b1_comparison_programs_agree` (`src/engine/mod.rs:2626`) looks like the natural
guard, but its generator `arb_comparison_program` (`src/testgen.rs:506`) builds
facts from `(0u8..3, -2i64..=2)` — integers only, so it cannot reach a string or
bool comparison. Extending that generator is part of the work, not an optional
extra — and extending it is the whole job, since a property whose generator
cannot reach the case is green for the wrong reason. `testing.md` C8 records this
exact failure happening once already (a spelling-equivalence property that
"passed unfixed" because arbitrary programs almost never satisfied its
precondition); the fix there was a purpose-built generator, and the same applies
here.

The general property this is one instance of: **a value order that one construct
honours and another rejects is a defect in whichever one is out of step.** As a
property — for every pair of same-typed constants `a`, `b`, `min { X | p(X) }`
over `{a, b}` agrees with whichever of `a < b` / `b < a` succeeds. It cannot be
written today because the `<` half does not typecheck, which is the bug; it
becomes writable the moment the check widens, and it is the guard that keeps §4's
order single-homed. Write it in the same sitting — `bugs/README.md` is explicit
that `002` and `003` each identified their property and neither filed it, which
is how `001` stayed reachable by a second spelling.

**Out of scope, and now settled: string *operations*.** Rejected outright (§17,
2026-07-27) — not deferred, so nothing here waits on them. The line is that a
comparison *filters* and constructs no value, while `concat` would construct
unboundedly many, failing the termination test §5 already applies to casts. This
matters to the fix only as reassurance: widening `<` adds no reachable value and
so touches none of the open termination work.

## Fallout

- **§17, 2026-07-22 (strict types, no coercion)** — the decision is unaffected in
  substance (no coercion is still right), but its scope needs the amendment: it is
  what §8's operand constraints were derived from, and this file is evidence they
  were derived one step too far.
- **ROADMAP *Surface uniformity & the agent edge*** — a fourth instance of the
  same genre as the three already listed. Each is small; the pattern is that they
  all land on the agent surface.
