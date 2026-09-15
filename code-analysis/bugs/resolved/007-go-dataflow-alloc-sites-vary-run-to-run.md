---
id: 007
title: Go dataflow's allocation sites for package-level variables vary run to run
severity: wrong-output
area: extractor
spec: [2026-09-14]
found: 2026-09-15
resolution: fixed 2026-09-15 — globals and functions are allocated in declaration order, a package and its test variant told apart by package id; P6-go widened to every layer
---

Extracting the same Go module twice gives different `alloc`, `store` and `var`
files. On `test/fixtures/go-basic`, six extractions of unchanged code differed
from the first in every run: two package-level map variables in `shapes.go`
swapped allocation sites 20 and 21, and `var` and `store` rows followed that
change of order.

## Repro

```sh
for i in 1 2 3; do node src/main.ts test/fixtures/go-basic -o /tmp/g$i --no-git --root test/fixtures/go-basic; done
cmp /tmp/g1/facts/alloc.jsonl /tmp/g2/facts/alloc.jsonl
```

## Cause

`emitDataflow` allocates a `cell` for every project package-level variable by
iterating `built` (a map of SSA packages) and each package's `Members` (a map),
so site ids and the order of the `var` rows follow Go's randomised map order.

It went unnoticed because P6-go extracted only the structure layer, and every
other determinism check compares two orderings of the *input* within one layer.
The fixture tests assert sets, and do not assert site ids.

## Fix

Collect the globals and sort them by file, then offset, then name, before
allocating. P6-go now extracts every layer, and its three extra modules declare
package-level variables.
