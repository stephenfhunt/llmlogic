---
id: 003
title: the dead-export recipe reports namespace re-exports and lazy imports as dead
severity: doc
area: reference
spec: ["decisions.md 2026-09-12"]
found: 2026-09-12
resolution:
---

`reference/typescript.md` §4 spells out the dead-export query and warns about one
case: "A public API re-exported from an entry point counts only as a re-export,
so name your entry points." Naming them is not enough. Two idioms that are
standard TypeScript, not exotic, defeat the recipe as written.

## Repro

**(a) Namespace re-export.** `packages/grafana-ui/src/index.ts`:

```ts
import * as commonOptionsBuilder from './options/builder';   // :14
export { styleMixins, commonOptionsBuilder };                 // :477
```

`exports` on `index.ts` carries the namespace *object*, not its members, so every
member came back dead — `addAxisConfig`, `addLegendOptions`, `addTooltipOptions`,
`addTextSizeOptions`, `addHideFrom`, `cardChrome`, `focusCss`, `listItem`,
`listItemSelected`. They are called from
`public/app/plugins/panel/{bargauge,candlestick,barchart}/module.tsx`
(verified by reading). **12 false positives from one barrel idiom.**

The fix is five lines and needs a column the reference never mentions — that a
namespace import is marked by the *string* `"*"` in `import_name.imported`:

```datalog
ns_reexported(F) :- entry(E), import_name(file: E, imported: "*", target: T), symbol(id: T, file: F).
entry_export(S)  :- ns_reexported(F), exports(file: F, symbol: S).
```

**(b) Lazy dynamic import.** `ReactMonacoDiffEditorLazy.tsx:21` does
`import('./ReactMonacoDiffEditor')` and line 43 reads
`dependency.ReactMonacoDiffEditor` — a property read off a resolved module
namespace object, which is not a `ref`. Traps 2–4 cover a callback handed to a
library and a structural implementation CHA cannot see; they do not cover a
*module* reached this way. A `React.lazy` codebase produces this everywhere.

## What it cost

On `@grafana/ui`, after filtering stories and applying both fixes by hand: **8
non-type dead exports, 8 of them false positives, 0 true.** The recipe's headline
question answered nothing correct on a real published library.

## Acceptance criteria

§4's `dead_export` block carries the `ns_reexported` rule, and the traps list
gains the module-namespace case beside traps 2–4. A fixture with a namespace
re-export and a lazily imported module, asserting neither is dead.

Two smaller false-positive classes seen in the same run and worth a sentence,
because they are *not* dead code and should not share the bucket: same-file
compound-component assignment (`VizLayout.Legend = VizLayoutLegend`) and
same-file subclassing of an exported abstract class.
