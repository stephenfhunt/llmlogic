---
id: 009
title: Allocation-site ids collide when two frontends with a dataflow layer run together
severity: wrong-output
area: extractor
spec: [2026-09-14]
found: 2026-09-15
resolution: fixed 2026-09-15 — `__counters__` carries `alloc_site`, chained through every frontend as call-site and flow-node ids already were
---

A repository holding both a tsconfig and a Go module, extracted in one run with
the dataflow layer, gives two different variables the same `alloc.site`.
`lib/checks.dl`'s `alloc.site is not a key` reports it, and every library reading
`pts` merges two unrelated objects.

## Repro

```sh
node src/main.ts test/fixtures/basic/tsconfig.json test/fixtures/go-basic \
  --root test/fixtures -o /tmp/mixed --no-git
grep -o '"site":[0-9]*' /tmp/mixed/facts/alloc.jsonl | sort | uniq -d | head
```

## Cause

Ids that must be unique across frontends chain through each frontend's closing
`__counters__` row, which carried `call_site` and `flow_node` only. Allocation
sites were never in it: `Context.nextAllocSite` (TypeScript) and the Go
extractor's `allocSites` each counted from 1 of their own.

It went unnoticed because no test extracted two dataflow languages at once —
Python has no dataflow layer, and Java had none yet, so in practice only one
emitter ever ran.

## Fix

`__counters__` carries `alloc_site`. The TypeScript pass, which runs first,
reports its next free id; each frontend takes `--first-alloc-site` and passes
the value on. `test/counters.test.ts` extracts `basic` and `go-basic` together
and asserts `alloc.site`, `flow_node.id` and `call_site.id` are each unique.
