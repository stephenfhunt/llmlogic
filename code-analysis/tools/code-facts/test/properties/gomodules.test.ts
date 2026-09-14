// P2-go — module-graph fidelity for Go: the import facts of a generated module
// equal the graph it was generated from.
// P6-go — determinism: the order a go.work lists its modules in changes nothing.
//
// The model is modgen's; only the rendering is Go. A directory is a package, so
// a reference to another file of the same directory needs no import — it is an
// `implicit` row — and one import names every file of the package the importer
// references. Go forbids import cycles, so references between packages keep one
// direction only (PACKAGES' order); a barrel's re-exports become references to
// what it re-exported.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { extractGo, goAvailable, snapshotDir, tempDir, writeFiles } from "../helpers.ts";
import { arbProject, normalizeCalls, type ProjectModel } from "./modgen.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "25", 10);
// 40 for P2-go: its rarest guard, an import naming two files of its package,
// fires in 73 of 400 runs, so 25 would miss it about one suite in 150 and 40
// about one in 3,000. Imports across packages (276/400) and within one (204/400)
// are guarded well below their rates.
const P2_RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "40", 10);
const NO_GO = goAvailable() ? false : "needs the `go` command";

const MODULE = "example.com/p2";
const PACKAGES = ["src", "src/a", "src/b", "src/a/deep"];

const goPath = (tsPath: string) => tsPath.replace(/\.ts$/, ".go");
const dirOf = (p: string) => p.slice(0, p.lastIndexOf("/"));
const goName = (k: number, j: number) => `F${k}_${j}`;
const alias = (dir: string) => `d${PACKAGES.indexOf(dir)}`;

interface Rendered {
  files: Record<string, string>;
  imports: string[][];
  names: string[][];
  crossRows: number;
  implicitRows: number;
  multiFileImports: number;
}

function sorted<T>(xs: T[]): T[] {
  return [...xs].sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
}

function render(model: ProjectModel): Rendered {
  const out: Rendered = { files: { "go.mod": `module ${MODULE}\n\ngo 1.26\n` }, imports: [], names: [], crossRows: 0, implicitRows: 0, multiFileImports: 0 };
  const paths = model.files.map((f) => goPath(f.path));
  const rank = (k: number) => PACKAGES.indexOf(dirOf(paths[k] ?? ""));
  model.files.forEach((f, i) => {
    const own = dirOf(paths[i] ?? "");
    // Every name the file references, in source order: each is used at package
    // level (`var _ = …`), since Go rejects an unused import.
    const refs: [number, number][] = [];
    for (const imp of f.imports) {
      const pairs: [number, number][] = imp.style === "via" ? imp.names.map((k) => [k, 0]) : imp.names.map((j) => [imp.from, j]);
      for (const [k, j] of pairs) {
        if (k === i || rank(k) < rank(i) || refs.some(([a, b]) => a === k && b === j)) continue;
        if ((model.files[k]?.fnCount ?? 0) <= j) continue;
        refs.push([k, j]);
      }
    }
    const ref = (k: number, j: number) => (dirOf(paths[k] ?? "") === own ? goName(k, j) : `${alias(dirOf(paths[k] ?? ""))}.${goName(k, j)}`);
    const importedDirs = [...new Set(refs.map(([k]) => dirOf(paths[k] ?? "")).filter((d) => d !== own))].sort(
      (a, b) => PACKAGES.indexOf(a) - PACKAGES.indexOf(b),
    );
    const lines = [`package ${path.posix.basename(own)}`, ""];
    if (importedDirs.length > 0) {
      lines.push("import (", ...importedDirs.map((d) => `\t${alias(d)} "${MODULE}/${d}"`), ")", "");
    }
    for (const [k, j] of refs) lines.push(`var _ = ${ref(k, j)}`);
    for (let j = 0; j < f.fnCount; j++) {
      const body = (f.calls[j] ?? []).filter(([k, fn]) => k === i || refs.some(([a, b]) => a === k && b === fn)).map(([k, fn]) => `\t${ref(k, fn)}()`);
      lines.push("", `func ${goName(i, j)}() {`, ...body, "}");
    }
    out.files[paths[i] ?? ""] = `${lines.join("\n")}\n`;

    const firstUse = new Map<number, string>();
    for (const [k, j] of refs) {
      const d = dirOf(paths[k] ?? "");
      if (d === own) {
        if (!firstUse.has(k)) firstUse.set(k, goName(k, j));
        continue;
      }
      const row = [paths[i] ?? "", `${MODULE}/${d}`, "static", paths[k] ?? ""];
      if (!out.imports.some((r) => JSON.stringify(r) === JSON.stringify(row))) {
        out.imports.push(row);
        out.crossRows++;
      }
    }
    for (const [k, name] of firstUse) {
      out.imports.push([paths[i] ?? "", name, "implicit", paths[k] ?? ""]);
      out.implicitRows++;
    }
    for (const d of importedDirs) {
      out.names.push([paths[i] ?? "", alias(d), "*", `${d}#<package>`]);
      const files = new Set(refs.filter(([k]) => dirOf(paths[k] ?? "") === d).map(([k]) => k));
      if (files.size > 1) out.multiFileImports++;
    }
  });
  return out;
}

test("P2-go: imports, implicit rows and imported names are exactly the generated module graph", { skip: NO_GO }, () => {
  let crossRuns = 0;
  let implicitRuns = 0;
  let multiFileRuns = 0;
  fc.assert(
    fc.property(arbProject, (raw) => {
      const r = render(normalizeCalls(raw));
      const dir = tempDir("p2-go");
      writeFiles(dir, r.files);
      const { tables } = extractGo(dir, { layers: [] });
      const imports = tables.rows("imports").map((i) => [i.file, i.specifier, i.kind, i.target_file] as string[]);
      assert.deepEqual(sorted(imports), sorted(r.imports));
      const names = tables.rows("import_name").map((n) => [n.file, n.local, n.imported, n.target] as string[]);
      assert.deepEqual(sorted(names), sorted(r.names));
      crossRuns += r.crossRows > 0 ? 1 : 0;
      implicitRuns += r.implicitRows > 0 ? 1 : 0;
      multiFileRuns += r.multiFileImports > 0 ? 1 : 0;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: P2_RUNS },
  );
  // Non-vacuity, read against the sentence: imports between packages, implicit
  // references within one, and an import naming several files of its package.
  assert.ok(crossRuns >= P2_RUNS / 4, `only ${crossRuns}/${P2_RUNS} runs imported across packages`);
  assert.ok(implicitRuns >= P2_RUNS / 8, `only ${implicitRuns}/${P2_RUNS} runs referenced within a package`);
  assert.ok(multiFileRuns >= 1, "no run imported a package through two of its files");
});

test("P6-go: the order a go.work lists its modules in changes no output byte", { skip: NO_GO }, () => {
  let reordered = 0;
  fc.assert(
    fc.property(arbProject, fc.array(fc.double({ noNaN: true }), { minLength: 4, maxLength: 4 }), (raw, keys) => {
      const r = render(normalizeCalls(raw));
      const dir = tempDir("p6-go");
      const files: Record<string, string> = {};
      for (const [p, text] of Object.entries(r.files)) files[`app/${p}`] = text;
      // Three more modules, each with two `init`s: ids that collide, whose
      // suffixes must land on the same declaration whatever the module order.
      for (let n = 0; n < 3; n++) {
        files[`lib${n}/go.mod`] = `module example.com/lib${n}\n\ngo 1.26\n`;
        files[`lib${n}/x.go`] = `package lib${n}\n\nfunc init() {}\n\nfunc init() {}\n\ntype T struct{ A int }\n`;
      }
      const modules = ["./app", "./lib0", "./lib1", "./lib2"];
      const shuffled = modules
        .map((m, i) => [keys[i] ?? 0, m] as const)
        .sort((a, b) => a[0] - b[0])
        .map(([, m]) => m);
      const work = (order: string[]) => `go 1.26\n\nuse (\n${order.map((m) => `\t${m}`).join("\n")}\n)\n`;
      writeFiles(dir, { ...files, "go.work": work(modules) });
      const out1 = path.join(dir, "out1");
      extractGo(dir, { out: out1, layers: [] });
      fs.writeFileSync(path.join(dir, "go.work"), work(shuffled));
      const out2 = path.join(dir, "out2");
      extractGo(dir, { out: out2, layers: [] });
      assert.deepEqual(snapshotDir(out2), snapshotDir(out1));
      if (shuffled.join() !== modules.join()) reordered++;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  assert.ok(reordered >= RUNS / 2, `only ${reordered}/${RUNS} runs actually reordered the modules`);
});
