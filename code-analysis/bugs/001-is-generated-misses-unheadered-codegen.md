---
id: 001
title: file.is_generated misses generated code whose generator writes no header
severity: wrong-answer
area: extractor
spec: ["decisions.md 2026-09-12"]
found: 2026-09-12
resolution:
---

`file.is_generated` is a header heuristic: the first lines are matched against
`@generated` / `auto-generated` / `DO NOT EDIT`. Plenty of code generators write
no such header, and the files they produce are exactly the ones that dominate a
cohesion or complexity ranking — they are machine-shaped, enormous, and nobody
should read them.

## Repro

Grafana's frontend, `lib/cohesion.dl`. The top of `module_lcom4` is entirely
RTK-Query codegen, none of it flagged:

```
366  packages/grafana-api-clients/src/clients/rtkq/legacy/endpoints.gen.ts
 98  packages/grafana-runtime/src/internal/openFeature/openfeature.gen.ts
 69  packages/grafana-api-clients/src/clients/rtkq/provisioning/v0alpha1/endpoints.gen.ts
 64  …/notifications.alerting/v1beta1/endpoints.gen.ts
```

`head -1 endpoints.gen.ts` is `import { api } from './baseAPI';`. Measured over
the repository: **170** `*.gen.ts` files, **29** with no recognisable header —
and those 29 are the ones at the top of the ranking. The first real finding
(`alerting/unified/Analytics.ts`, 43) is below four of them.

So every "worst cohesion" list from a repository with a code generator is noise
until the reader filters by filename, and the reader has no way to do that: the
query language has no string operations (see the ROADMAP item on path
conventions), so the filter has to be built outside the engine.

## Root cause

`src/layers/structure.ts`, `GENERATED` is matched against `head` — the file's
first lines only. There is no filename component.

## Acceptance criteria

A fixture with a `*.gen.ts` whose content carries no marker, asserting
`file(is_generated: true)`. The obvious fix is to add a filename test
(`*.gen.*`, `*.generated.*`, `*_pb.*`, `*.pb.go`-style suffixes) alongside the
header test — and to say in `reference/typescript.md` that `is_generated` is a
heuristic over *both*, since a project with its own convention will still miss.

**The general property this is one instance of**: every `file` classification
column (`is_test`, `is_decl`, `is_generated`) is a heuristic, and the reference
documents none of them as such. `is_test`'s heuristic is at least named in trap
6; the other two are not.

## Fallout

Nothing recorded rests on it. `notes/code-facts.md` § Dogfooding — Grafana cites
the measurement.
