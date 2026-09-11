// P2-py — module-graph fidelity for Python: the import and call-site facts of a
// generated package tree equal the graph it was generated from.
//
// The model is modgen's; only the rendering is Python. Each import is spelled
// absolute or relative; a namespace import as `import a.b.m as ns`,
// `from a.b import m as ns`, or plain `import a.b.m` called through the dotted
// path; and a barrel re-exports through `__all__`. Some
// runs have each package's __init__ import its own submodules — sqlparse's
// shape, through which `from pkg import sub` has to still reach the submodule.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import { test } from "node:test";
import fc from "fast-check";
import { extractPython, tempDir, writeFiles } from "../helpers.ts";
import { arbProject, fnName, normalizeCalls, type ProjectModel } from "./modgen.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "25", 10);

const PACKAGES = ["src", "src/a", "src/b", "src/a/deep"];

const pyPath = (tsPath: string) => tsPath.replace(/\.ts$/, ".py");
const dirOf = (p: string) => p.slice(0, p.lastIndexOf("/"));
const moduleParts = (p: string) => p.replace(/\.py$/, "").split("/");

/** The spelling of `target` (dotted parts) from a module in package `fromPkg`, relative or absolute. */
function spell(fromPkg: string[], target: string[], relative: boolean): string {
  if (!relative) return target.join(".");
  let common = 0;
  while (common < fromPkg.length && common < target.length && fromPkg[common] === target[common]) common++;
  return ".".repeat(fromPkg.length - common + 1) + target.slice(common).join(".");
}

/** `from P import m`'s second specifier: the submodule's, spelled as P was. */
const subSpec = (pkgSpec: string, m: string) => (/^\.*$/.test(pkgSpec) ? pkgSpec + m : `${pkgSpec}.${m}`);

interface Rendered {
  files: Record<string, string>;
  imports: string[][];
  names: string[][];
  calls: string[][];
  relative: number;
  fromPackage: number;
  throughInit: number;
  dotted: number;
}

function render(model: ProjectModel, withInits: boolean): Rendered {
  const out: Rendered = { files: {}, imports: [], names: [], calls: [], relative: 0, fromPackage: 0, throughInit: 0, dotted: 0 };
  const initImports = (k: number) => withInits && k % 2 === 0; // does m{k}'s package __init__ import it?
  const paths = model.files.map((f) => pyPath(f.path));
  const idOf = (k: number, j: number) => `${paths[k]}#${fnName(k, j)}`;
  const inits: Record<string, string[]> = Object.fromEntries(PACKAGES.map((p) => [p, []]));
  if (withInits) {
    paths.forEach((p, k) => {
      if (!initImports(k)) return;
      const init = `${dirOf(p)}/__init__.py`;
      inits[dirOf(p)]?.push(`from . import m${k}`);
      out.imports.push([init, `.m${k}`, paths[k] ?? ""]);
      out.names.push([init, `m${k}`, `m${k}`, `${paths[k]}#<module>`]);
    });
  }
  for (const pkg of PACKAGES) out.files[`${pkg}/__init__.py`] = inits[pkg]?.map((l) => `${l}\n`).join("") ?? "";

  model.files.forEach((f, i) => {
    const me = paths[i] ?? "";
    const fromPkg = dirOf(me).split("/");
    const lines: string[] = [];
    const prefix = new Map<number, string>(); // how this file's calls reach a namespace-imported file
    for (const imp of f.imports) {
      const target = paths[imp.from];
      if (target === undefined) continue;
      const relative = (i + imp.from) % 2 === 0;
      let spelledRelative = relative;
      if (imp.style === "named" || imp.style === "via") {
        const spec = spell(fromPkg, moduleParts(target), relative);
        const fns = imp.style === "named" ? imp.names.map((j) => [imp.from, j] as const) : imp.names.map((k) => [k, 0] as const);
        lines.push(`from ${spec} import ${fns.map(([k, j]) => fnName(k, j)).join(", ")}`);
        out.imports.push([me, spec, target]);
        for (const [k, j] of fns) out.names.push([me, fnName(k, j), fnName(k, j), idOf(k, j)]);
      } else if ((i * 3 + imp.from) % 3 === 0) {
        const spec = moduleParts(target).join(".");
        spelledRelative = false; // `import` is always absolute
        lines.push(`import ${spec} as ns${imp.from}`);
        prefix.set(imp.from, `ns${imp.from}`);
        out.imports.push([me, spec, target]);
        out.names.push([me, `ns${imp.from}`, "*", `${target}#<module>`]);
      } else if ((i * 3 + imp.from) % 3 === 1) {
        // `import src.a.m0` binds `src`, the top package; calls go through the whole path
        const spec = moduleParts(target).join(".");
        spelledRelative = false;
        out.dotted++;
        lines.push(`import ${spec}`);
        prefix.set(imp.from, spec);
        out.imports.push([me, spec, target]);
        out.names.push([me, "src", "*", "src/__init__.py#<module>"]);
      } else {
        const pkgSpec = spell(fromPkg, dirOf(target).split("/"), relative);
        const m = `m${imp.from}`;
        out.fromPackage++;
        if (initImports(imp.from)) out.throughInit++;
        lines.push(`from ${pkgSpec} import ${m} as ns${imp.from}`);
        prefix.set(imp.from, `ns${imp.from}`);
        out.imports.push([me, pkgSpec, `${dirOf(target)}/__init__.py`]);
        out.imports.push([me, subSpec(pkgSpec, m), target]);
        out.names.push([me, `ns${imp.from}`, m, `${target}#<module>`]);
      }
      if (spelledRelative) out.relative++;
    }
    for (const [k, j] of f.reexports) {
      const target = paths[k];
      if (target === undefined) continue;
      const spec = spell(fromPkg, moduleParts(target), (i + k) % 2 === 0);
      lines.push(`from ${spec} import ${fnName(k, j)}`);
      out.imports.push([me, spec, target]);
      out.names.push([me, fnName(k, j), fnName(k, j), idOf(k, j)]);
    }
    if (f.reexports.length > 0) {
      lines.push(`__all__ = [${f.reexports.map(([k, j]) => JSON.stringify(fnName(k, j))).join(", ")}]`);
    } else {
      for (let j = 0; j < f.fnCount; j++) {
        const calls = f.calls[j] ?? [];
        const body = calls.map(([k, fn]) => {
          const via = k !== i ? prefix.get(k) : undefined;
          out.calls.push([`${me}#${fnName(i, j)}`, idOf(k, fn), "static"]);
          return via !== undefined ? `    ${via}.${fnName(k, fn)}()` : `    ${fnName(k, fn)}()`;
        });
        lines.push("", `def ${fnName(i, j)}():`, ...(body.length > 0 ? body : ["    pass"]));
      }
    }
    out.files[me] = `${lines.join("\n")}\n`;
  });
  return out;
}

const sorted = (xs: string[][]) => xs.map((x) => x.join(" ")).sort();

test("P2-py: imports, imported names and calls are exactly the generated module graph", () => {
  let crossFile = 0;
  let throughBarrel = 0;
  let relative = 0;
  let fromPackage = 0;
  let throughInit = 0;
  let dotted = 0;
  fc.assert(
    fc.property(arbProject, fc.boolean(), (raw, withInits) => {
      const model = normalizeCalls(raw);
      const r = render(model, withInits);
      const dir = tempDir("p2py");
      writeFiles(dir, r.files);
      const { tables } = extractPython(dir, { layers: ["refs"] });
      const imports = tables.rows("imports").map((i) => [i.file, i.specifier, i.target_file] as string[]);
      assert.deepEqual(sorted(imports), sorted(r.imports));
      assert.ok(tables.rows("imports").every((i) => i.kind === "static" && i.runtime === true && i.resolved === true));
      const names = tables.rows("import_name").map((n) => [n.file, n.local, n.imported, n.target] as string[]);
      assert.deepEqual(sorted(names), sorted(r.names));
      const calls = tables.rows("call_site").map((c) => [c.caller, c.callee, c.dispatch] as string[]);
      assert.deepEqual(sorted(calls), sorted(r.calls));
      assert.deepEqual(tables.rows("unresolved_ref"), []);
      crossFile += model.files.some((f) => f.imports.length > 0) ? 1 : 0;
      throughBarrel += model.files.some((f) => f.imports.some((i) => i.style === "via")) ? 1 : 0;
      relative += r.relative > 0 ? 1 : 0;
      fromPackage += r.fromPackage > 0 ? 1 : 0;
      throughInit += r.throughInit > 0 ? 1 : 0;
      dotted += r.dotted > 0 ? 1 : 0;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  // Non-vacuity, one guard per clause of the header: cross-file edges, a
  // barrel, a relative spelling, each namespace spelling, and `from pkg import
  // m` through an __init__ that imports m itself.
  assert.ok(crossFile >= RUNS / 2, `only ${crossFile}/${RUNS} runs had an import`);
  assert.ok(throughBarrel >= 1, "no run imported through a barrel");
  assert.ok(relative >= 1, "no run spelled an import relatively");
  assert.ok(fromPackage >= 1, "no run imported a submodule from its package");
  assert.ok(dotted >= 1, "no run imported a module plainly and called through its dotted path");
  assert.ok(throughInit >= 1, "no run imported a submodule through an __init__ that imports it");
});
