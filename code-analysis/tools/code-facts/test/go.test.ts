// The Go frontend's structure layer, on test/fixtures/go-basic and on small
// modules written per test.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import { datalog, engineAvailable, extractGo, fixture, goAvailable, tempDir, writeFiles } from "./helpers.ts";

const NO_GO = goAvailable() ? false : "needs the `go` command";
const out = tempDir("go-basic");
const go = NO_GO === false ? extractGo(fixture("go-basic"), { out, layers: [] }) : undefined;
const rows = (rel: string) => go?.tables.rows(rel) ?? [];

function symbol(id: string) {
  const s = rows("symbol").find((r) => r.id === id);
  assert.ok(s !== undefined, `no symbol ${id}`);
  return s;
}

test("files are Go; tests are `_test.go`; the namespace is the import path, `_test` for an external test package", { skip: NO_GO }, () => {
  assert.deepEqual(
    rows("file").map((f) => [f.path, f.lang, f.is_test, f.package, f.namespace]),
    [
      ["base.go", "go", false, "example.com/shapes", "example.com/shapes"],
      ["cmd/app/main.go", "go", false, "example.com/shapes", "example.com/shapes/cmd/app"],
      ["example_test.go", "go", true, "example.com/shapes", "example.com/shapes_test"],
      ["internal/geom/geom.go", "go", false, "example.com/shapes", "example.com/shapes/internal/geom"],
      ["shapes.go", "go", false, "example.com/shapes", "example.com/shapes"],
      ["shapes_test.go", "go", true, "example.com/shapes", "example.com/shapes"],
      ["square.go", "go", false, "example.com/shapes", "example.com/shapes"],
    ],
  );
  assert.deepEqual(rows("project"), [{ id: "go.mod", dir: ".", files: 7, strict: false, module: null, target: null }]);
  assert.match(String(rows("extraction")[0]?.go_version), /^1\.\d+/);
});

test("a file a build constraint leaves out is an excluded_file, and nothing else", { skip: NO_GO }, () => {
  assert.deepEqual(rows("excluded_file"), [{ path: "windows.go", reason: "build_constraint", detail: "//go:build windows" }]);
  assert.ok(!rows("file").some((f) => f.path === "windows.go"));
});

test("imports: a row per file of an in-root package the importer references, and implicit rows within a package", { skip: NO_GO }, () => {
  assert.deepEqual(
    rows("imports").map((i) => [i.file, i.line, i.specifier, i.kind, i.target_file, i.target_dir, i.target_package, i.builtin]),
    [
      ["base.go", 13, "Circle", "implicit", "shapes.go", ".", null, false],
      ["cmd/app/main.go", 4, "embed", "side_effect", null, null, "embed", true],
      ["cmd/app/main.go", 5, "fmt", "static", null, null, "fmt", true],
      // Circle and its Area are in shapes.go, Square's Area in square.go.
      ["cmd/app/main.go", 7, "example.com/shapes", "static", "shapes.go", ".", null, false],
      ["cmd/app/main.go", 7, "example.com/shapes", "static", "square.go", ".", null, false],
      ["cmd/app/main.go", 8, "example.com/shapes/internal/geom", "static", "internal/geom/geom.go", "internal/geom", null, false],
      ["example_test.go", 4, "fmt", "static", null, null, "fmt", true],
      ["example_test.go", 6, "example.com/shapes", "static", "shapes.go", ".", null, false],
      ["internal/geom/geom.go", 3, "math", "static", null, null, "math", true],
      ["shapes.go", 4, "math", "static", null, null, "math", true],
      ["shapes_test.go", 3, "testing", "static", null, null, "testing", true],
      ["shapes_test.go", 6, "Circle", "implicit", "shapes.go", ".", null, false],
      ["square.go", 5, "base", "implicit", "base.go", ".", null, false],
      ["square.go", 13, "Unit", "implicit", "shapes.go", ".", null, false],
    ],
  );
  assert.ok(rows("imports").every((i) => i.runtime === true && i.resolved === true));
});

test("an import binds a package: its name, or its alias, to the package's symbol", { skip: NO_GO }, () => {
  assert.deepEqual(
    rows("import_name").filter((n) => n.file === "cmd/app/main.go").map((n) => [n.local, n.imported, n.target]),
    [
      ["fmt", "*", "ext:fmt#<package>"],
      ["shapes", "*", ".#<package>"],
      ["g", "*", "internal/geom#<package>"],
    ],
  );
  assert.deepEqual([symbol(".#<package>").kind, symbol(".#<package>").name], ["namespace", "shapes"]);
  assert.deepEqual([symbol(".#<package_test>").name, symbol(".#<package_test>").file], ["shapes_test", "example_test.go"]);
  assert.deepEqual([symbol("ext:fmt#<package>").origin, symbol("ext:fmt#<package>").package], ["external", "fmt"]);
});

test("types map onto the shared kinds, with the Go construct in `form`", { skip: NO_GO }, () => {
  const kf = (id: string) => [symbol(id).kind, symbol(id).form];
  assert.deepEqual(kf("shapes.go#Circle"), ["class", "struct"]);
  assert.deepEqual(kf("base.go#Celsius"), ["class", "defined_type"]);
  assert.deepEqual(kf("base.go#Round"), ["type_alias", null]);
  assert.deepEqual(kf("shapes.go#Shape"), ["interface", null]);
  assert.deepEqual(kf("square.go#Square.base"), ["property", "embedded"]);
  assert.deepEqual(kf("shapes.go#Map.T"), ["type_parameter", null]);
  assert.deepEqual(kf("shapes.go#Unit"), ["variable", null]);
  assert.equal(symbol("shapes.go#Unit").is_readonly, true);
  // `var double = func(…)` is `double`, a function.
  assert.deepEqual(kf("square.go#double"), ["function", null]);
  assert.ok(!rows("symbol").some((s) => String(s.id).startsWith("square.go#double.<function")));
});

test("a method hangs off its receiver's type, wherever each is declared; members carry Go's visibility", { skip: NO_GO }, () => {
  const m = symbol("square.go#Square.Area");
  assert.deepEqual([m.kind, m.parent, m.file, m.line, m.visibility], ["method", "square.go#Square", "square.go", 12, "public"]);
  assert.deepEqual([symbol("base.go#base.name").visibility, symbol("base.go#Namer.format").visibility], ["package", "package"]);
  assert.deepEqual([symbol("shapes.go#Shape.Area").is_abstract, symbol("shapes.go#Circle.Area").is_abstract], [true, false]);
  // An anonymous struct's fields belong to the field that holds it.
  assert.equal(symbol("square.go#Square.meta.label").parent, "square.go#Square.meta");
  assert.deepEqual([symbol("shapes.go#Register").exported, symbol("shapes.go#registry").exported, symbol("shapes_test.go#TestArea").exported], [true, false, true]);
});

test("param: the receiver is no parameter, a variadic one is rest, an unnamed one has a symbol of its own", { skip: NO_GO }, () => {
  const params = (fn: string) => rows("param").filter((p) => p.fn === fn).map((p) => [p.index, p.name, p.symbol, p.rest]);
  assert.deepEqual(params("shapes.go#Circle.Area"), []);
  assert.equal(symbol("shapes.go#Circle.Area.c").kind, "parameter");
  assert.deepEqual(params("shapes.go#Sum"), [[0, "xs", "shapes.go#Sum.xs", true]]);
  assert.deepEqual(params("base.go#Namer.format"), [
    [0, "<unnamed>", "base.go#Namer.format.<param@0>", false],
    [1, "<unnamed>", "base.go#Namer.format.<param@1>", false],
  ]);
  assert.deepEqual(params("square.go#double"), [[0, "x", "square.go#double.x", false]]);
});

test("doc comments, and a `Deprecated:` paragraph as the deprecated tag", { skip: NO_GO }, () => {
  const doc = (id: string) => rows("doc").find((d) => d.symbol === id);
  assert.deepEqual([doc("shapes.go#Register")?.has_doc, doc("shapes.go#Register")?.lines], [true, 3]);
  assert.deepEqual([doc("shapes.go#<module>")?.has_doc, doc("base.go#base")?.has_doc], [true, false]);
  assert.deepEqual(rows("jsdoc_tag"), [{ symbol: "shapes.go#Register", tag: "deprecated", text: "keep a map of your own." }]);
});

test("checks.dl finds nothing wrong with the Go facts", { skip: NO_GO || !engineAvailable() }, () => {
  const r = datalog(path.join(out, "lib", "checks.dl"));
  assert.equal(r.code, 1, `expected a clean run, got exit ${r.code}:\n${r.stdout}${r.stderr}`);
});

test("go.mod dependencies; dot, `_` and unresolvable imports", { skip: NO_GO }, () => {
  const dir = tempDir("go-edge");
  writeFiles(dir, {
    "go.mod": [
      "module example.com/edge",
      "",
      "go 1.26",
      "",
      "require (",
      "\texample.com/missing v1.2.3",
      "\tgolang.org/x/mod v0.41.0 // indirect",
      ")",
      "",
      "tool golang.org/x/tools/cmd/stringer",
      "",
    ].join("\n"),
    "a/a.go": 'package a\n\nimport (\n\t. "example.com/edge/b"\n\t_ "example.com/edge/c"\n\t"example.com/missing/pkg"\n)\n\nfunc F() int { return B() + pkg.X }\n',
    "b/b.go": "package b\n\nfunc B() int { return 1 }\n",
    "c/c.go": "package c\n\nfunc init() {}\n",
  });
  const { tables } = extractGo(dir, { layers: [] });
  assert.deepEqual(
    tables.rows("package_dep").map((d) => [d.dep, d.kind, d.range, d.scope]),
    [
      ["example.com/missing", "prod", "v1.2.3", "require"],
      ["golang.org/x/mod", "optional", "v0.41.0", "indirect"],
      ["golang.org/x/tools/cmd/stringer", "dev", "", "tool"],
    ],
  );
  assert.deepEqual(
    tables.rows("imports").map((i) => [i.line, i.specifier, i.kind, i.target_file, i.target_dir, i.resolved]),
    [
      [4, "example.com/edge/b", "dot", "b/b.go", "b", true],
      // A `_` import references nothing, and still runs the package's init.
      [5, "example.com/edge/c", "side_effect", null, "c", true],
      [6, "example.com/missing/pkg", "static", null, null, false],
    ],
  );
  assert.deepEqual(
    tables.rows("import_name").map((n) => [n.local, n.target]),
    [
      [".", "b#<package>"],
      ["pkg", null],
    ],
  );
  fs.rmSync(dir, { recursive: true, force: true });
});

const NO_CGO = NO_GO || spawnSync("go", ["env", "CGO_ENABLED"], { encoding: "utf8" }).stdout.trim() !== "1" ? "needs cgo" : false;

test("a cgo file is the file as written: its `import \"C\"`, and none of cgo's generated declarations", { skip: NO_CGO }, () => {
  const dir = tempDir("go-cgo");
  writeFiles(dir, {
    "go.mod": "module example.com/cg\n\ngo 1.26\n",
    "cg.go": 'package cg\n\n// #include <stdlib.h>\nimport "C"\n\nfunc G() { C.free(nil) }\n',
  });
  const { tables } = extractGo(dir, { layers: [] });
  assert.deepEqual(tables.rows("file").map((f) => [f.path, f.loc]), [["cg.go", 6]]);
  assert.deepEqual(
    tables.rows("imports").map((i) => [i.line, i.specifier, i.kind, i.target_package, i.builtin, i.resolved]),
    [[4, "C", "cgo", "C", true, true]],
  );
  assert.deepEqual(
    tables.rows("symbol").filter((s) => s.origin === "project").map((s) => s.id),
    ["cg.go#<module>", ".#<package>", "cg.go#G"],
  );
  fs.rmSync(dir, { recursive: true, force: true });
});
