---
id: 006
title: A class static block crashes the structure layer
severity: crash
area: extractor
spec: []
found: 2026-09-14
resolution: fixed 2026-09-14 — parameter extraction's predicate excludes static blocks, which keep their function and flow facts
---

Reported as GitHub issue [#1](https://github.com/stephenfhunt/llmlogic/issues/1).
`code-facts` aborted extraction on any TypeScript class with a static
initialization block:

```
TypeError: Cannot read properties of undefined (reading 'forEach')
    at visit (src/layers/structure.ts:473:23)
```

## Repro

```ts
class Example {
  static {
    initialize();
  }
}
```

## Cause

`isFunctionLike` (`src/context.ts`) includes `ClassStaticBlockDeclaration`
deliberately: a static block has a body, so flow and dataflow treat it as a
function. `structure.ts`'s `hasBodyOrSignature` reused it for parameter
extraction under the type guard `node is ts.SignatureDeclaration`, which is false
for a static block. The caller then read `node.parameters`, which a static block
does not have.

Every other `.parameters` read (`flow.ts`, `dataflow.ts`) already excluded static
blocks. Nothing in the tests contained one, so no fixture or generator reached this.

## Resolution

The predicate is now `hasParameters`: it returns false for a static block, so its
type guard holds. A static block keeps its `fn` and `flow_node` rows and emits no
`param` rows. `test/structure.test.ts` extracts every function-like kind through
all the source layers and checks their `param` rows.

Not done: P1's generator emits no classes, so static blocks' CFGs have no
execution oracle.
