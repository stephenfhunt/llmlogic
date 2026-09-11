import assert from "node:assert/strict";
import { test } from "node:test";
import { extract, fixture } from "./helpers.ts";

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
  assert.deepEqual(rows("package_dep"), [{ package: "basic-fixture", dep: "left-pad", kind: "prod", range: "^1.3.0" }]);
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
