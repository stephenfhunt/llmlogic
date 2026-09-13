---
id: 004
title: afferent/instability at directory depth N silently omits every file above N — including the entry point
severity: doc
area: reference
spec: ["decisions.md 2026-09-12"]
found: 2026-09-12
resolution:
---

`lib/units.dl` states the rule: "A file sitting directly in a shallower
directory belongs to no component at a deeper N." That is correct and
deliberate. The *consequence* is not written anywhere, and it produces a
confident wrong answer of exactly the kind the playbook exists to prevent.

## Repro

`@grafana/ui`, `lib/coupling.dl` at directory depth 4:

```
afferent(4, "packages/grafana-ui/src/graveyard", 0).
```

Read plainly: *nothing in the package depends on the graveyard* — so it is safe
to delete. It is wrong. `packages/grafana-ui/src/index.ts` re-exports the
graveyard at **eight** points (lines 128, 400–404, 406–407; verified by reading).
`index.ts` sits directly in `src/`, at depth 3, so it belongs to no depth-4
component and its eight edges cross no component boundary.

**Entry points and barrels are exactly the files that sit shallow.** So any
`afferent` / `instability` / `distance` reading at depth N ≥ 3 in a `src/`-rooted
project is missing its entry point by construction — and the entry point is
usually the one importer whose existence decides the question being asked.

## Acceptance criteria

A line in `reference/typescript.md` §5's traps list, and one in `lib/units.dl`'s
header, saying which files a depth-N reading omits and that barrels are among
them. The heavier alternative — a `comp(G, C)` that also admits shallower files
as a pseudo-component — changes every coupling number and should not be taken
just to avoid documenting a rule that is otherwise right.

## Fallout

Bounds every coupling reading at a directory granularity. File-level
(`G = -1`) and package-level (`G = -2`) components are unaffected: every file has
one of each.

## Resolution

**Fixed 2026-09-13.** The documentation this file asked for landed: a sentence in
`units.dl`'s header and trap 11 in `reference/typescript.md` §5. The consequence
is also **queryable** now, rather than only written down. `coupling.dl` gains
`unplaced(G, F)` (files with no component at directory depth G) and
`unplaced_dependent(G, C, F)` (an unplaced file referencing component C), which is
exactly the edge `afferent(G, C, _)` does not count. SKILL.md's coupling row names
it. The diagnosis held.

**Not taken**, as the file advised: a pseudo-component for shallower files. It
changes every coupling number to avoid a rule that is right. `unplaced_dependent`
gives the reader the omitted edges without moving any metric.

*Test*: on the `basic` fixture at depth 2, `afferent(2, "src/util", 0)` stands
beside `unplaced_dependent(2, "src/util", "src/main.ts")`. Nothing is unplaced at
depth 1 or at `G = -1`. *Mutation*: `placed` ignores the depth → red.

## Resolution, amended 2026-09-13 — the diagnosis did not hold

The first fix was checked on the subject after it was committed, and it did not
find the case it was for. `unplaced_dependent(4, "…/graveyard", F)` was empty,
because **the graveyard's 0 was never about depth.** On the cached `@grafana/ui`
base, `src/index.ts` has eight `reexport` import edges into the graveyard and
**zero** `ref` edges. `coupling.dl` builds components from references. A re-export
is an import with no reference, so `index.ts` counts toward no afferent coupling
at any granularity: at `G = -1` too, where every file has a component. Two claims
above are therefore wrong: the root cause ("belongs to no component at depth 4"),
and "files and packages are unaffected".

The depth gap is real, just not what hid this barrel, so both mechanisms are now
one query. `unplaced_dependent` is replaced by `uncounted_dependent(G, C, F)`: F
imports C (any `dep` edge), and `afferent(G, C, _)` does not count F, because F is
unplaced at G or no symbol of F references C. On `@grafana/ui` it names `index.ts`
for the graveyard (2.0 s, 208 MB). 1,300 files there have an importer `afferent`
leaves out. No metric moved. Counting re-exports in `afferent` would change every
coupling number and was not taken.

*Test*: `basic` gives three rows, each explained in the test. *Mutations*: count
importers that do reference C → red; follow references instead of imports → red.
The lesson is the session's own: a fix shipped on a fixture before it was run on
the subject that filed the bug.
