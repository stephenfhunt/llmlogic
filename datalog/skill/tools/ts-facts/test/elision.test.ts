// imports.runtime: which import statements survive into the emitted JavaScript.
// TypeScript elides an import whose bindings are only used as types even when it
// is not written `import type`; this fixture pins the cases syntax cannot see.

import assert from "node:assert/strict";
import * as path from "node:path";
import { test } from "node:test";
import { datalog, engineAvailable, extract, fixture, tempDir } from "./helpers.ts";

const rows = (dir: string) =>
  extract(fixture(dir), { layers: [] })
    .tables.rows("imports")
    .map((r) => [r.file, r.line, r.kind, r.runtime]);

test("an import used only in types is elided, with or without `import type`", () => {
  assert.deepEqual(rows("elision"), [
    ["src/barrel.ts", 1, "reexport", false], // re-exports an interface: elided
    ["src/barrel.ts", 2, "reexport", true], // re-exports a value
    ["src/barrel.ts", 3, "reexport_all", true],
    ["src/declared.ts", 1, "type_only", false],
    ["src/declared.ts", 2, "side_effect", true],
    ["src/typesonly.ts", 1, "static", false], // Circle and Shape only annotate: elided
    ["src/value.ts", 1, "static", true], // PI is a value
    ["src/view.tsx", 1, "static", true], // h is the JSX factory: no identifier uses it
  ]);
});

test("under verbatimModuleSyntax an import survives unless it says `type`", () => {
  assert.deepEqual(rows("elision/vms"), [
    ["src/uses.ts", 1, "static", true], // Circle only annotates, but is written as a value import
    ["src/uses.ts", 2, "static", true], // `import { type Shape }` is emitted as `import {}`
    ["src/uses.ts", 3, "type_only", false],
  ]);
});

test("runtime_dep follows what the emitter keeps", { skip: !engineAvailable() }, () => {
  const out = tempDir("elision");
  extract(fixture("elision"), { out, layers: [] });
  const r = datalog(path.join(out, "lib", "modgraph.dl"), ["runtime_dep(A, B)"]);
  assert.deepEqual(r.stdout.trim().split("\n"), [
    'runtime_dep("src/barrel.ts", "src/h.ts").',
    'runtime_dep("src/barrel.ts", "src/shapes.ts").',
    'runtime_dep("src/declared.ts", "src/h.ts").',
    'runtime_dep("src/value.ts", "src/shapes.ts").',
    'runtime_dep("src/view.tsx", "src/h.ts").',
  ]);
});
