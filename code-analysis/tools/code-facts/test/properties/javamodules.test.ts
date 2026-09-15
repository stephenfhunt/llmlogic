// P2-java — module-graph fidelity for Java: the import facts of generated
// sources equal the graph they were generated from.
// P6-java — determinism: the order a Maven reactor lists its modules in changes nothing.
//
// The model is modgen's; only the rendering is Java. A directory is a package
// (`src/a` is `src.a`) and a file one class `M{k}` of static methods. A class of
// the file's own package needs no import — an `implicit` row at the first
// reference — and across packages the model's import styles become Java's: a
// named import of one function is a static import, a named import of several or
// a namespace import an on-demand import of the package (as is every other
// reference into a package the file imports with `*`), and an import through a
// barrel a single-type import of the class the barrel re-exported. Java allows
// cycles, so references go both ways.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { extractJava, javaAvailable, mavenAvailable, snapshotDir, tempDir, withMavenRepo, writeFiles } from "../helpers.ts";
import { arbProject, normalizeCalls, type ProjectModel } from "./modgen.ts";

// 40 for P2-java: its rarest guard, an on-demand import reaching two files of its
// package, fires in 40 of 200 runs, so 25 would miss it about one suite in 250
// and 40 about one in 7,000. A single-type import (46/200) is guarded at one run;
// static imports (109/200), on-demand imports (123/200) and references within a
// package (99/200) well below their rates.
const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "40", 10);
// Each P6-java case is two Maven extractions of four modules, seconds apiece.
const P6_RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "4", 10);
const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";
const NO_MAVEN = NO_JAVA !== false ? NO_JAVA : mavenAvailable() ? false : "needs Maven (`mvn`)";

const javaPath = (tsPath: string) => tsPath.replace(/m(\d+)\.ts$/, "M$1.java");
const dirOf = (p: string) => p.slice(0, p.lastIndexOf("/"));
const pkgOf = (dir: string) => dir.replaceAll("/", ".");
const cls = (k: number) => `M${k}`;
const fn = (k: number, j: number) => `f${k}_${j}`;

type How = "static" | "demand" | "type" | "local";

interface Rendered {
  files: Record<string, string>;
  imports: string[][];
  names: string[][];
  staticRows: number;
  typeRows: number;
  demandRows: number;
  implicitRows: number;
  multiFileDemands: number;
}

function sorted<T>(xs: T[]): T[] {
  return [...xs].sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
}

function render(model: ProjectModel): Rendered {
  const out: Rendered = { files: {}, imports: [], names: [], staticRows: 0, typeRows: 0, demandRows: 0, implicitRows: 0, multiFileDemands: 0 };
  const paths = model.files.map((f) => javaPath(f.path));
  model.files.forEach((f, i) => {
    const own = dirOf(paths[i] ?? "");
    const refs: [number, number, How][] = [];
    for (const imp of f.imports) {
      const pairs: [number, number][] = imp.style === "via" ? imp.names.map((k) => [k, 0]) : imp.names.map((j) => [imp.from, j]);
      for (const [k, j] of pairs) {
        if (k === i || (model.files[k]?.fnCount ?? 0) <= j) continue;
        const several = imp.style === "named" && imp.names.length > 1;
        const how: How = dirOf(paths[k] ?? "") === own ? "local" : imp.style === "namespace" || several ? "demand" : imp.style === "named" ? "static" : "type";
        if (!refs.some(([a, b, c]) => a === k && b === j && c === how)) refs.push([k, j, how]);
      }
    }
    const demanded = new Set(refs.filter(([, , h]) => h === "demand").map(([k]) => dirOf(paths[k] ?? "")));
    for (const r of refs) if (r[2] === "static" && demanded.has(dirOf(paths[r[0]] ?? ""))) r[2] = "demand";
    // A class imported by name is found through that import, never through its package's `*`.
    const typed = new Set(refs.filter(([, , how]) => how === "type").map(([k]) => k));
    for (const r of refs) if (r[2] === "demand" && typed.has(r[0])) r[2] = "type";

    const pkg = (k: number) => pkgOf(dirOf(paths[k] ?? ""));
    const statics = sorted([...new Set(refs.filter(([, , h]) => h === "static").map(([k, j]) => `${k}:${j}`))]).map((s) => s.split(":").map(Number) as [number, number]);
    const types = [...new Set(refs.filter(([, , h]) => h === "type").map(([k]) => k))].sort((a, b) => a - b);
    const demandDirs = [...new Set(refs.filter(([, , h]) => h === "demand").map(([k]) => dirOf(paths[k] ?? "")))].sort();
    const locals = [...new Set(refs.filter(([, , h]) => h === "local").map(([k]) => k))];

    const lines = [`package ${pkgOf(own)};`, ""];
    for (const [k, j] of statics) lines.push(`import static ${pkg(k)}.${cls(k)}.${fn(k, j)};`);
    for (const k of types) lines.push(`import ${pkg(k)}.${cls(k)};`);
    for (const d of demandDirs) lines.push(`import ${pkgOf(d)}.*;`);
    lines.push("", `public class ${cls(i)} {`, "  static void uses() {");
    for (const [k, j, how] of refs) lines.push(how === "static" ? `    ${fn(k, j)}();` : `    ${cls(k)}.${fn(k, j)}();`);
    lines.push("  }");
    for (let j = 0; j < f.fnCount; j++) lines.push("", `  public static void ${fn(i, j)}() {}`);
    lines.push("}");
    out.files[paths[i] ?? ""] = `${lines.join("\n")}\n`;

    const file = paths[i] ?? "";
    for (const [k, j] of statics) {
      out.imports.push([file, `${pkg(k)}.${cls(k)}.${fn(k, j)}`, "static_import", paths[k] ?? ""]);
      out.names.push([file, fn(k, j), fn(k, j), `${paths[k]}#${cls(k)}.${fn(k, j)}`]);
      out.staticRows++;
    }
    for (const k of types) {
      out.imports.push([file, `${pkg(k)}.${cls(k)}`, "static", paths[k] ?? ""]);
      out.names.push([file, cls(k), cls(k), `${paths[k]}#${cls(k)}`]);
      out.typeRows++;
    }
    for (const d of demandDirs) {
      const reached = [...new Set(refs.filter(([k, , h]) => h === "demand" && dirOf(paths[k] ?? "") === d).map(([k]) => k))];
      for (const k of reached) out.imports.push([file, `${pkgOf(d)}.*`, "on_demand", paths[k] ?? ""]);
      out.names.push([file, "*", "*", `${d}#<package>`]);
      out.demandRows += reached.length;
      if (reached.length > 1) out.multiFileDemands++;
    }
    for (const k of locals) {
      out.imports.push([file, cls(k), "implicit", paths[k] ?? ""]);
      out.implicitRows++;
    }
  });
  return out;
}

test("P2-java: imports, implicit rows and imported names are exactly the generated module graph", { skip: NO_JAVA }, () => {
  let staticRuns = 0;
  let typeRuns = 0;
  let demandRuns = 0;
  let implicitRuns = 0;
  let multiFileRuns = 0;
  fc.assert(
    fc.property(arbProject, (raw) => {
      const r = render(normalizeCalls(raw));
      const dir = tempDir("p2-java");
      writeFiles(dir, r.files);
      const { tables } = extractJava(dir, { layers: [] });
      const imports = tables.rows("imports").map((i) => [i.file, i.specifier, i.kind, i.target_file] as string[]);
      assert.deepEqual(sorted(imports), sorted(r.imports));
      const names = tables.rows("import_name").map((n) => [n.file, n.local, n.imported, n.target] as string[]);
      assert.deepEqual(sorted(names), sorted(r.names));
      staticRuns += r.staticRows > 0 ? 1 : 0;
      typeRuns += r.typeRows > 0 ? 1 : 0;
      demandRuns += r.demandRows > 0 ? 1 : 0;
      implicitRuns += r.implicitRows > 0 ? 1 : 0;
      multiFileRuns += r.multiFileDemands > 0 ? 1 : 0;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  if (process.env.CODE_FACTS_GUARDS !== undefined) console.log(`P2-java guards over ${RUNS} runs: static ${staticRuns}, type ${typeRuns}, on-demand ${demandRuns}, implicit ${implicitRuns}, on-demand to two files ${multiFileRuns}`);
  // Non-vacuity, read against the sentence: each import form, references within a
  // package, and an on-demand import reaching two files.
  assert.ok(staticRuns >= RUNS / 8, `only ${staticRuns}/${RUNS} runs had a static import`);
  assert.ok(typeRuns >= 1, `only ${typeRuns}/${RUNS} runs had a single-type import`);
  assert.ok(demandRuns >= RUNS / 8, `only ${demandRuns}/${RUNS} runs had an on-demand import`);
  assert.ok(implicitRuns >= RUNS / 8, `only ${implicitRuns}/${RUNS} runs referenced within a package`);
  assert.ok(multiFileRuns >= 1, "no run reached two files through one on-demand import");
});

const POM = (body: string) => `<?xml version="1.0" encoding="UTF-8"?>\n<project xmlns="http://maven.apache.org/POM/4.0.0">\n  <modelVersion>4.0.0</modelVersion>\n${body}\n</project>\n`;

test("P6-java: the order a Maven reactor lists its modules in changes no output byte", { skip: NO_MAVEN }, () => {
  const repo = tempDir("m2-p6");
  let reordered = 0;
  fc.assert(
    fc.property(arbProject, fc.array(fc.double({ noNaN: true }), { minLength: 4, maxLength: 4 }), (raw, keys) => {
      const r = render(normalizeCalls(raw));
      const dir = tempDir("p6-java");
      const files: Record<string, string> = {};
      for (const [p, text] of Object.entries(r.files)) files[`app/src/main/java/${p}`] = text;
      // Three more modules declaring one class name in one package, with overloads
      // whose ids collide: the suffixes and the package's symbol must land on the
      // same declarations whatever the module order.
      for (let n = 0; n < 3; n++) {
        files[`lib${n}/src/main/java/x/T.java`] = "package x;\n\npublic class T {\n  static {}\n\n  void f() {}\n\n  void f(int a) {}\n}\n";
      }
      const modules = ["app", "lib0", "lib1", "lib2"];
      for (const m of modules) {
        files[`${m}/pom.xml`] = POM(`  <parent><groupId>g</groupId><artifactId>parent</artifactId><version>1</version></parent>\n  <artifactId>${m}</artifactId>`);
      }
      const shuffled = modules
        .map((m, i) => [keys[i] ?? 0, m] as const)
        .sort((a, b) => a[0] - b[0])
        .map(([, m]) => m);
      const parent = (order: string[]) =>
        POM(`  <groupId>g</groupId>\n  <artifactId>parent</artifactId>\n  <version>1</version>\n  <packaging>pom</packaging>\n  <modules>${order.map((m) => `<module>${m}</module>`).join("")}</modules>`);
      writeFiles(dir, { ...files, "pom.xml": parent(modules) });
      const out1 = path.join(dir, "out1");
      withMavenRepo(() => extractJava(dir, { out: out1, layers: [] }), repo);
      fs.writeFileSync(path.join(dir, "pom.xml"), parent(shuffled));
      const out2 = path.join(dir, "out2");
      withMavenRepo(() => extractJava(dir, { out: out2, layers: [] }), repo);
      assert.deepEqual(snapshotDir(out2), snapshotDir(out1));
      if (shuffled.join() !== modules.join()) reordered++;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: P6_RUNS },
  );
  assert.ok(reordered >= 1, "no run reordered the reactor");
});
