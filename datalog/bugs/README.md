# Defects

One file per defect: `NNN-short-slug.md`. This is where **things that are wrong**
live. `ROADMAP.md` holds things that are *missing* — features, design items,
deferred work. The split is "broken" vs "not built yet".

A defect is: the implementation contradicts the spec, the spec contradicts the
implementation, or either contradicts itself. A missing feature is not a defect,
even when its absence is painful.

## Location is the status

`bugs/[0-9]*.md` is **exactly the open set**. Resolving one is `git mv`:

```sh
git mv bugs/001-negated-atom-hoisted-argument.md bugs/resolved/
```

So there is no `status:` field and no index file — either would be a second copy
of what the directory already says, and stale cross-references are this project's
demonstrated failure mode (the 2026-07-25 spec review found five). Discovery is:

```sh
ls bugs/[0-9]*.md                       # what's open — the index
grep -h 'severity:' bugs/[0-9]*.md      # triage
grep -rl '§7' bugs/                     # what touches a spec section, open or not
```

(The numeric glob is what skips this README. Defect files are always `NNN-`.)

If `ls` ever stops being enough, *generate* an index from the frontmatter. Don't
write one by hand.

Resolved files stay on disk rather than being deleted, because agents grep the
working tree, not `git log` — a later session needs to find that something was
already tried without knowing to look for it. Delete only a genuine misfile (a
duplicate, or one opened in error before any work). Git remains the archive of
last resort either way.

**A file does not move until it says how it was resolved** (below). A resolved
file that records only the problem is worth almost nothing — it tells a later
session that something happened, not what the answer was. The resolution note is
what the archive is *for*.

## Frontmatter

```yaml
---
id: 001
title: one line, states the defect
severity: soundness   # soundness | wrong-answer | crash | usability | doc
area: lower           # lexer | parser | lower | typecheck | engine | api | import | spec
spec: ["§5", "§7", "§10"]
found: 2026-07-25
resolution:           # on resolving: fixed | wontfix | duplicate, the date, the commit
---
```

**Severity** — `soundness` (derives something false, or accepts an unsound
program), `wrong-answer` (silently returns the wrong rows), `crash` (panic or
non-zero exit where the program is valid), `usability` (correct but misleading —
bad error message, accepted in one spelling and rejected in another), `doc` (the
spec asserts something untrue).

## Body

Whatever the defect needs, but a fixable bug wants four things:

- **Repro** — a copy-pasteable program, what it prints, and what it should print.
- **Root cause** — `file:line`, and *why* the code is wrong, not just where.
- **Acceptance criteria** — the test that must pass. An `#[ignore]`d test
  asserting the sound behaviour is the strongest form (precedent: the two
  absent × negation tests). **If the defect is an instance of a general
  property, say so and file the property** — as a test, in the same sitting.
  `002` and `003` each identified theirs precisely ("the general property this
  bug is one instance of"; "the only durable fix for this class") and neither
  became one, which is how `001` was still reachable by a second spelling.
  `006` is the rule working: its property was unwritable *until* the fix landed,
  and writing it in the same sitting is what exposed that the fix's own
  acceptance test had to call `typecheck`, not just widen a differential.
- **Fallout** — other decisions this invalidates. A defect that falsifies a
  recorded §17 decision must say so, and §17 gets the amendment; one that
  undercuts a ROADMAP item's rationale must name the item.

Reference the file from the commit that resolves it (`bugs/001`), and from
`spec.md` §17 when the fix changes a design decision rather than just the code.

## Resolving: append a `## Resolution`

Before `git mv`, add a `## Resolution` section — dated, with the commit. **Append;
never rewrite the original diagnosis.** A first theory that turned out wrong is
useful signal about where the code misleads, and the same reason §17 amends
entries in place rather than editing them silently.

It should answer:

- **What changed**, and *which* candidate fix was taken when the file listed
  several — plus why the others were dropped. The rejected option is the part a
  later session cannot recover from the diff.
- **Whether the diagnosis held.** If the root cause was not what the file says,
  say so here rather than correcting the text above.
- **What it cost elsewhere** — behaviour that changed, tests or properties added,
  §17 entries amended, ROADMAP items whose rationale moved.
- **For `wontfix`** — why living with it is acceptable, and what would reopen it.
  This is the highest-value note of all: without it the bug gets re-filed.

Keep it short. Two paragraphs beats a narrative; the commit carries the detail.

