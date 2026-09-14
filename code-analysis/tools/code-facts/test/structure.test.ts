import assert from "node:assert/strict";
import { test } from "node:test";
import { extract, fixture, tempDir, writeProject } from "./helpers.ts";

const basic = extract(fixture("basic"), { layers: [] });
const rows = (rel: string) => basic.tables.rows(rel);

test("every compiled file is a file row, with its package and flags", () => {
  assert.deepEqual(
    rows("file").map((f) => [f.path, f.dir, f.package, f.lang, f.is_test, f.is_decl]),
    [
      ["src/main.ts", "src", "basic-fixture", "ts", false, false],
      ["src/util/index.ts", "src/util", "basic-fixture", "ts", false, false],
      ["src/util/math.ts", "src/util", "basic-fixture", "ts", false, false],
    ],
  );
  const math = rows("file").find((f) => f.path === "src/util/math.ts");
  assert.equal(math?.loc, 17);
  assert.equal(math?.sloc, 13); // 17 lines, less 3 blank and the one-line doc comment
});

test("directories form a tree and every file has an ancestor at each depth", () => {
  assert.deepEqual(
    rows("dir").map((d) => [d.path, d.parent, d.name, d.depth]),
    [
      [".", null, ".", 0],
      ["src", ".", "src", 1],
      ["src/util", "src", "util", 2],
    ],
  );
  assert.deepEqual(
    rows("file_ancestor")
      .filter((a) => a.file === "src/util/math.ts")
      .map((a) => [a.dir, a.depth]),
    [
      ["src/util", 2],
      ["src", 1],
      [".", 0],
    ],
  );
});

test("package.json is read for the package and its dependencies", () => {
  assert.deepEqual(rows("package"), [{ name: "basic-fixture", dir: ".", version: "1.0.0", private: true }]);
  assert.deepEqual(rows("package_dep"), [{ package: "basic-fixture", dep: "left-pad", kind: "prod", range: "^1.3.0", types_for: null, scope: null }]);
});

test("imports: every module edge, its kind, and where it resolves", () => {
  assert.deepEqual(
    rows("imports").map((i) => [i.file, i.line, i.specifier, i.kind, i.target_file, i.target_package, i.resolved]),
    [
      ["src/main.ts", 1, "./util/math.js", "static", "src/util/math.ts", null, true],
      ["src/main.ts", 2, "./util/index.js", "static", "src/util/index.ts", null, true],
      ["src/main.ts", 3, "node:path", "static", null, "node:path", true],
      ["src/main.ts", 17, "./util/math.js", "dynamic", "src/util/math.ts", null, true],
      ["src/util/index.ts", 1, "./math.js", "reexport", "src/util/math.ts", null, true],
      ["src/util/index.ts", 2, "./math.js", "reexport_all", "src/util/math.ts", null, true],
      ["src/util/index.ts", 3, "./math.js", "reexport", "src/util/math.ts", null, true],
    ],
  );
});

test("an imported name resolves through a re-exporting barrel to its declaration", () => {
  const plus = rows("import_name").find((n) => n.local === "plus");
  assert.deepEqual(plus, { file: "src/main.ts", line: 2, local: "plus", imported: "add", target: "src/util/math.ts#add", type_only: false });
  const shape = rows("import_name").find((n) => n.file === "src/main.ts" && n.local === "Shape");
  assert.equal(shape?.type_only, true);
  assert.equal(shape?.target, "src/util/math.ts#Shape");
  assert.equal(rows("import_name").find((n) => n.local === "Square")?.target, "src/util/math.ts#Square");
});

test("exports: a barrel's names are re-exports of the declaring file's symbols", () => {
  const barrel = rows("exports").filter((e) => e.file === "src/util/index.ts");
  assert.deepEqual(
    barrel.map((e) => [e.name, e.symbol, e.kind, e.is_type]),
    [
      ["add", "src/util/math.ts#add", "reexport", false],
      ["double", "src/util/math.ts#double", "reexport", false],
      ["Shape", "src/util/math.ts#Shape", "reexport", true],
    ],
  );
  const def = rows("exports").find((e) => e.file === "src/util/math.ts" && e.name === "default");
  assert.equal(def?.symbol, "src/util/math.ts#Square");
});

test("symbol ids: containers, adopted arrows, locals, parameter properties", () => {
  const byId = new Map(rows("symbol").map((s) => [s.id, s]));
  assert.equal(byId.get("src/util/math.ts#double")?.kind, "function", "a const-bound arrow is a function");
  assert.equal(byId.get("src/util/math.ts#double.x")?.kind, "parameter");
  assert.equal(byId.get("src/util/math.ts#double.x")?.parent, "src/util/math.ts#double");
  assert.equal(byId.get("src/main.ts#total.sum")?.kind, "local");
  assert.equal(byId.get("src/main.ts#sq")?.kind, "variable");
  assert.equal(byId.get("src/main.ts#later")?.is_async, true);
  const side = byId.get("src/util/math.ts#Square.side");
  assert.equal(side?.kind, "property", "a parameter property is also a class property");
  assert.equal(side?.visibility, "private");
  assert.equal(side?.is_readonly, true);
  assert.equal(byId.get("src/util/math.ts#Square.constructor.side")?.kind, "parameter");
  assert.equal(byId.get("src/util/math.ts#Square")?.exported, true, "the default export");
  assert.equal(byId.get("src/util/math.ts#add")?.exported, true);
  assert.equal(byId.get("src/main.ts#sq")?.exported, false);
  assert.equal(byId.get("src/util/math.ts#Shape.area")?.parent, "src/util/math.ts#Shape");
  for (const s of rows("symbol")) assert.equal(s.origin, "project", `${s.id} is external but nothing references it`);
});

test("params are positional and point at the parameter symbols", () => {
  assert.deepEqual(
    rows("param").filter((p) => p.fn === "src/util/math.ts#add").map((p) => [p.index, p.symbol, p.name]),
    [
      [0, "src/util/math.ts#add.a", "a"],
      [1, "src/util/math.ts#add.b", "b"],
    ],
  );
});

test("doc rows cover API-level declarations only", () => {
  const docs = new Map(rows("doc").map((d) => [d.symbol, d]));
  assert.deepEqual(docs.get("src/util/math.ts#add"), { symbol: "src/util/math.ts#add", has_doc: true, lines: 1 });
  assert.equal(docs.get("src/util/math.ts#double")?.has_doc, false);
  assert.equal(docs.has("src/main.ts#total.sum"), false, "locals are not documented API");
  assert.equal(docs.has("src/util/math.ts#add.a"), false, "nor are parameters");
});

// An import of something that is not a module — a stylesheet, a bundler-prefixed
// specifier — resolves through a wildcard `declare module` and names no file and
// no package. `resolved: true` with both targets absent is a fact base
// contradicting itself, which is what `checks.dl` used to (rightly) report on 42
// files of VS Code; `target_ambient` is the third kind of target.
const assets = extract(fixture("assets"), { layers: [] });

test("an asset import resolves to an ambient module pattern, not a file or a package", () => {
  assert.deepEqual(
    assets.tables
      .rows("imports")
      .map((i) => [i.specifier, i.kind, i.target_file, i.target_package, i.target_ambient, i.resolved]),
    [
      ["./widget.css", "side_effect", null, null, "*.css", true],
      ["bundler!./widget.css", "side_effect", null, null, "bundler!*", true],
      ["./help.js", "static", "src/help.ts", null, null, true],
    ],
  );
});

test("an ambient target is not a module edge: nothing depends on the stylesheet", () => {
  // The point of keeping it out of `target_file`: a file that imports a
  // stylesheet must not gain an import edge, or every such file would look
  // coupled to whatever `.d.ts` declared the wildcard.
  const edges = assets.tables.rows("imports").filter((i) => i.target_file !== null);
  assert.deepEqual(
    edges.map((i) => [i.file, i.target_file]),
    [["src/widget.ts", "src/help.ts"]],
  );
});

// A `.json` a module imports is one of the program's files under
// `resolveJsonModule` — but only when something `import`s it. A CommonJS
// `require` of the same file resolves on disk and leaves the program alone, so
// `imports.target_file` named a file that had no `file` row. VS Code's
// `product.json` is the real case, and `checks.dl` reported the contradiction.
const jsonmod = extract(fixture("jsonmod"), { layers: [] });

test("a json module is data, and is a file row however it was imported", () => {
  const rows = jsonmod.tables.rows("file").map((f) => [f.path, f.lang]);
  assert.deepEqual(rows.sort(), [
    ["src/cjs.ts", "ts"],
    // Reached by `import`: in the program, and no longer called TypeScript.
    ["src/data.json", "json"],
    ["src/esm.ts", "ts"],
    // Reached only by `require`: synthesised, because a resolved target that is
    // not a file row is a fact base contradicting itself.
    ["src/settings.json", "json"],
  ]);
});

test("both json spellings resolve to a target that is a known file", () => {
  const known = new Set(jsonmod.tables.rows("file").map((f) => f.path));
  const targets = jsonmod.tables
    .rows("imports")
    .filter((i) => typeof i.target_file === "string")
    .map((i) => i.target_file);
  assert.deepEqual(targets.sort(), ["src/data.json", "src/settings.json"]);
  for (const target of targets) assert.ok(known.has(target), `${target} has no file row`);
});

test("is_generated reads a header or a generator's file name (bugs/001)", () => {
  const dir = tempDir("generated");
  writeProject(dir, {
    // RTK Query's codegen writes no header; its name is the only mark.
    "src/endpoints.gen.ts": "import { base } from './base.js';\nexport const api = base;\n",
    "src/__generated__/query.ts": "export const q = 1;\n",
    "src/schema_pb.ts": "export const m = 1;\n",
    "src/stamped.ts": "// @generated by a tool\nexport const s = 1;\n",
    // Names that only look like it.
    "src/base.ts": "export const base = 1;\n",
    "src/codegen.ts": "export const c = 1;\n",
    "src/generator.ts": "export const g = 1;\n",
  });
  const r = extract(dir, { layers: [] });
  assert.deepEqual(
    r.tables.rows("file").map((f) => [f.path, f.is_generated]),
    [
      ["src/__generated__/query.ts", true],
      ["src/base.ts", false],
      ["src/codegen.ts", false],
      ["src/endpoints.gen.ts", true],
      ["src/generator.ts", false],
      ["src/schema_pb.ts", true],
      ["src/stamped.ts", true],
    ],
  );
});

test("an unresolved bare specifier is a guess, never a target_package (bugs/002)", () => {
  const dir = tempDir("unresolved");
  writeProject(dir, {
    // A webpack alias, and a declared package that is not installed.
    "src/index.ts": "import 'vendor/css/font_awesome.css';\nimport pad from 'left-pad';\nimport { x } from '@scope/lib/sub';\nexport const y = [pad, x];\n",
  });
  const r = extract(dir, { layers: [] });
  assert.deepEqual(
    r.tables.rows("imports").map((i) => [i.specifier, i.target_package, i.unresolved_package, i.resolved]),
    [
      ["vendor/css/font_awesome.css", null, "vendor", false],
      ["left-pad", null, "left-pad", false],
      ["@scope/lib/sub", null, "@scope/lib", false],
    ],
  );
  // A resolved specifier carries no guess.
  assert.ok(rows("imports").every((i) => i.unresolved_package === null));
});

test("a class static block is a function with no parameters, beside every kind that has them (bugs/006)", () => {
  const dir = tempDir("static-block");
  writeProject(dir, {
    "src/kinds.ts": [
      "export function decl(a: number, b = 2, ...rest: number[]): number { return a + b + rest.length; }",
      "export class K {",
      "  static count = 0;",
      "  static {",
      "    K.count = decl(1);",
      "  }",
      "  constructor(public x: number, y?: string) {}",
      "  method(m: string): string { return m; }",
      "  get g(): number { return this.x; }",
      "  set g(v: number) { this.x = v; }",
      "}",
      "export const fe = function (e: number): number { return e; };",
      "export const af = (a1: string): string => a1;",
      "export interface Sig {",
      "  ms(s: number): void;",
      "  (c: string): void;",
      "  new (n: boolean): Sig;",
      "}",
    ].join("\n"),
  });
  const r = extract(dir, { layers: ["refs", "flow", "dataflow", "quality"] });
  const block = "src/kinds.ts#K.<static@4:3>";
  assert.ok(r.tables.rows("fn").some((f) => f.id === block), "the static block is a function");
  assert.ok(r.tables.rows("flow_node").some((n) => n.fn === block), "and has a control-flow graph");
  assert.deepEqual(
    r.tables.rows("param").map((p) => [p.fn, p.index, p.name, p.optional, p.rest, p.has_default]),
    [
      ["src/kinds.ts#decl", 0, "a", false, false, false],
      ["src/kinds.ts#decl", 1, "b", true, false, true],
      ["src/kinds.ts#decl", 2, "rest", false, true, false],
      ["src/kinds.ts#K.constructor", 0, "x", false, false, false],
      ["src/kinds.ts#K.constructor", 1, "y", true, false, false],
      ["src/kinds.ts#K.method", 0, "m", false, false, false],
      ["src/kinds.ts#K.g@10", 0, "v", false, false, false], // the setter; the getter holds the bare name
      ["src/kinds.ts#fe", 0, "e", false, false, false],
      ["src/kinds.ts#af", 0, "a1", false, false, false],
      ["src/kinds.ts#Sig.ms", 0, "s", false, false, false],
      ["src/kinds.ts#Sig.<call@16:3>", 0, "c", false, false, false],
      ["src/kinds.ts#Sig.<new@17:3>", 0, "n", false, false, false],
    ],
  );
});
