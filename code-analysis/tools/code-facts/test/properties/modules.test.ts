// P2 — module-graph fidelity: the import, re-export and call-site facts of a
// generated project equal the graph it was generated from.
// P6 — determinism: the order a tsconfig lists its files in changes nothing.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { extract, snapshotDir, tempDir, writeProject } from "../helpers.ts";
import { arbProject, fnName, normalizeCalls, type ProjectModel, render, specifier } from "./modgen.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "25", 10);

function tsconfig(files: string[]): object {
  // noLib: these properties are about the project's own graph, and parsing the
  // standard library for every generated project would dominate the run.
  return { compilerOptions: { module: "nodenext", moduleResolution: "nodenext", strict: true, noEmit: true, noLib: true, types: [] }, files };
}

function sorted<T>(xs: T[]): T[] {
  return [...xs].sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
}

function expectedImports(model: ProjectModel): [string, string, string, string][] {
  const out: [string, string, string, string][] = [];
  for (const f of model.files) {
    for (const imp of f.imports) {
      const from = model.files[imp.from];
      if (from !== undefined) out.push([f.path, specifier(f.path, from.path), "static", from.path]);
    }
    for (const [k] of f.reexports) {
      const from = model.files[k];
      if (from !== undefined) out.push([f.path, specifier(f.path, from.path), "reexport", from.path]);
    }
  }
  return out;
}

function expectedNames(model: ProjectModel): [string, string, string, string][] {
  const out: [string, string, string, string][] = [];
  const idOf = (k: number, j: number) => `${model.files[k]?.path}#${fnName(k, j)}`;
  for (const f of model.files) {
    for (const imp of f.imports) {
      const from = model.files[imp.from];
      if (from === undefined) continue;
      if (imp.style === "namespace") out.push([f.path, `ns${imp.from}`, "*", `${from.path}#<module>`]);
      else if (imp.style === "named") for (const j of imp.names) out.push([f.path, fnName(imp.from, j), fnName(imp.from, j), idOf(imp.from, j)]);
      // Through the barrel, the name still resolves to where it is declared.
      else for (const k of imp.names) out.push([f.path, fnName(k, 0), fnName(k, 0), idOf(k, 0)]);
    }
    for (const [k, j] of f.reexports) out.push([f.path, fnName(k, j), fnName(k, j), idOf(k, j)]);
  }
  return out;
}

function expectedCalls(model: ProjectModel): [string, string, string][] {
  const out: [string, string, string][] = [];
  model.files.forEach((f, i) => {
    f.calls.forEach((calls, j) => {
      for (const [k, fn] of calls) out.push([`${f.path}#${fnName(i, j)}`, `${model.files[k]?.path}#${fnName(k, fn)}`, "static"]);
    });
  });
  return out;
}

test("P2: imports and imported names are exactly the generated module graph", () => {
  let crossFile = 0;
  let throughBarrel = 0;
  fc.assert(
    fc.property(arbProject, (raw) => {
      const model = normalizeCalls(raw);
      const dir = tempDir("p2");
      const files = render(model);
      writeProject(dir, files, tsconfig(Object.keys(files)));
      const { tables } = extract(dir, { layers: ["refs"] });
      const imports = tables.rows("imports").map((i) => [i.file, i.specifier, i.kind, i.target_file] as [string, string, string, string]);
      assert.deepEqual(sorted(imports), sorted(expectedImports(model)));
      const names = tables.rows("import_name").map((n) => [n.file, n.local, n.imported, n.target] as [string, string, string, string]);
      assert.deepEqual(sorted(names), sorted(expectedNames(model)));
      // Every call resolves to the declaration it names, through namespaces and barrels.
      const calls = tables.rows("call_site").map((c) => [c.caller, c.callee, c.dispatch] as [string, string, string]);
      assert.deepEqual(sorted(calls), sorted(expectedCalls(model)));
      crossFile += imports.length > 0 ? 1 : 0;
      throughBarrel += model.files.some((f) => f.imports.some((i) => i.style === "via")) ? 1 : 0;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  // Non-vacuity, read against the sentence: the runs carried cross-file edges,
  // and some of them resolved a name through a re-exporting barrel.
  assert.ok(crossFile >= RUNS / 2, `only ${crossFile}/${RUNS} runs had an import`);
  assert.ok(throughBarrel >= 1, "no run imported through a barrel");
});

test("P6: listing a project's files in another order changes no output byte", () => {
  let reordered = 0;
  fc.assert(
    fc.property(arbProject, fc.array(fc.double({ noNaN: true }), { minLength: 6, maxLength: 6 }), (raw, keys) => {
      const model = normalizeCalls(raw);
      const dir = tempDir("p6");
      const files = render(model);
      const order = Object.keys(files);
      const shuffled = order
        .map((f, i) => [keys[i] ?? 0, f] as const)
        .sort((a, b) => a[0] - b[0])
        .map(([, f]) => f);
      writeProject(dir, files, tsconfig(order));
      const out1 = path.join(dir, "out1");
      extract(dir, { out: out1, layers: ["refs", "flow", "dataflow", "quality"] });
      fs.writeFileSync(path.join(dir, "tsconfig.json"), JSON.stringify(tsconfig(shuffled)));
      const out2 = path.join(dir, "out2");
      extract(dir, { out: out2, layers: ["refs", "flow", "dataflow", "quality"] });
      assert.deepEqual(snapshotDir(out2), snapshotDir(out1));
      if (shuffled.join() !== order.join()) reordered++;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  assert.ok(reordered >= RUNS / 2, `only ${reordered}/${RUNS} runs actually reordered the files`);
});
