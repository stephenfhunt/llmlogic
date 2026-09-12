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
