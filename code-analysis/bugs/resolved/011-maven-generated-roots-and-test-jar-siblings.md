---
id: 011
title: A Maven reactor's generated source roots and its test-jar siblings are both missed
severity: wrong-output
area: extractor
spec: [2026-09-14]
found: 2026-09-16
resolution: fixed 2026-09-16 — a generated root is read from the package its files declare; a sibling depended on for its test classes is read from its test sources
---

Extracting OpenRefine gave **2,942 javac errors and 2,184 unresolved references**
over 320 files. Every layer downstream was wrong there: a name javac cannot
resolve is no `ref`, no `call_site` callee, no `extends`, no points-to target.
Two independent causes, both in the project model.

## Repro

A Maven reactor where a plugin adds `target/generated-sources` itself as a source
root, and one module depends on another's `<type>test-jar</type>`:
`test/fixtures/java-maven-gen`. Before the fix, `App` cannot see `Flat`, and
`AppTest` cannot see `LibTestSupport`.

## Cause

**Generated roots.** The model is read before any plugin runs, so a root
`build-helper-maven-plugin` adds is invisible. `generatedRoots` guessed it back by
adding *every directory under* `target/generated-sources`, which is right only for
a plugin that writes into a directory of its own. A build that writes straight
into `generated-sources` got `generated-sources/com` as its root, so its files
were extracted but no package resolved against them.

**Test-jar siblings.** The extension drops a dependency on a module in the same
reactor, since that module is read from source rather than from a jar that may not
be built — and it records the sibling so its *main* sources go on the source path.
A `<type>test-jar</type>` dependency was dropped by the same filter, and nothing
put the sibling's *test* sources anywhere. So a module reusing another's test
support saw neither the jar nor the sources.

## Fix

A generated root is the directory a file's own `package` declaration implies —
the language's rule, not a naming convention — read past comments so a licence
header naming a package counts for nothing. A file whose path does not end in its
package is left out rather than guessed at.

The model carries `testModules` beside `modules`: siblings depended on for their
test classes, whose test roots go on the source path. The classifier `tests` is
what tells them apart.

On OpenRefine both together take 2,942 errors and 2,184 unresolved references to
**zero**, and add 3,302 `ref`, 111 `extends` and 84 `overrides` rows.
