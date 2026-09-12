---
id: 005
title: hidden_coupling reports a successful decoupling refactor as its strongest signal
severity: doc
area: lib
spec: ["decisions.md 2026-09-12"]
found: 2026-09-12
resolution:
---

`cochange.dl`'s `hidden_coupling(A, B, N)` is "A and B change together N times
with no static link in either direction". It joins **today's** import and
reference graph against the **whole history window**. So a pair that used to be
linked, and was deliberately unlinked, scores maximally: the co-changes are all
still there and the link is gone.

That is the *opposite* of the finding it reads as. A refactor that succeeded
looks identical to coupling nobody noticed.

## Repro

`@grafana/ui`, 20,000-commit window:

```
hidden_coupling("…/TableNG/TableNG.tsx", "…/TableNG/hooks.ts", 31).
```

The strongest signal in the package, and `TableNG.tsx` genuinely does not import
`hooks.ts` today (12 imports, lines 1–12, verified). But
`git log -S"from './hooks'" --follow` shows the import removed by `329912d65ee`
(2026-07-07) and `47c21dab9b7` (2026-08-04) — the refactor that split
`TableFlat`/`TableNested` out — and the co-change commits run 2025-06 → 2026-08,
almost all of them before the split.

The same caveat applies more weakly to `TableNG.tsx ⇄ utils.ts` (44).

A pair created *after* such a refactor is unaffected and is a true finding:
`TableFlat.tsx ⇄ TableNested.tsx` (12 of 12 commits, both files five weeks old)
is real hidden coupling, confirmed by reading both files.

## Acceptance criteria

A paragraph in `cochange.dl`'s header and in `SKILL.md`'s `hidden_coupling` row:
the relation compares a graph at one instant to a history over an interval, a
removed dependency is indistinguishable from one that never existed, and
`--git-since` (or `first_change`) is the instrument — a pair whose
`first_change` postdates the window's start cannot be this artifact.

Deriving it in the library is the alternative and is not obviously right: the
extractor would have to record when an import edge disappeared, which is a
history of the *graph*, not of the files, and nothing else needs it.

## Fallout

Bounds every `hidden_coupling` reading over a long window — which is the default
(`--git-max-commits 20000`). It does not affect `cochange`, `confidence`,
`revisions` or `churn`, none of which join against the static graph.
