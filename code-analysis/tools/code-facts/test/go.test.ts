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
const go = NO_GO === false ? extractGo(fixture("go-basic"), { out, layers: ["refs"] }) : undefined;
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

test("ref: each resolved name from its enclosing declaration, and how it is used", { skip: NO_GO }, () => {
  const refs = (from: string) => rows("ref").filter((r) => r.from === from).map((r) => [r.to, r.kind, r.line]);
  assert.deepEqual(refs("cmd/app/main.go#main"), [
    ["shapes.go#Circle", "new", 12],
    ["shapes.go#Circle.R", "write", 12],
    ["square.go#Square", "type", 13],
    ["ext:fmt#Println", "call", 14],
    ["shapes.go#Circle.Area", "call", 14],
    ["square.go#Square.Area", "call", 14],
    ["internal/geom/geom.go#Hypot", "call", 14],
  ]);
  assert.deepEqual(refs("cmd/app/main.go#tally"), [
    ["square.go#Square", "type", 21],
    ["shapes.go#Circle", "type", 21],
    ["cmd/app/main.go#count", "readwrite", 22],
    ["cmd/app/main.go#count", "write", 23],
    ["lib#len", "call", 23],
    ["cmd/app/main.go#names", "read", 23],
    ["cmd/app/main.go#names", "write", 24],
    ["lib#append", "call", 24],
    ["cmd/app/main.go#names", "read", 24],
    ["cmd/app/main.go#describe", "call", 25],
    ["shapes.go#Circle.Area", "value", 25],
  ]);
  // Locals, parameters and type parameters are the flow layer's; basic types are written like keywords.
  assert.deepEqual(refs("shapes.go#Map"), [
    ["lib#make", "call", 35],
    ["lib#len", "call", 35],
    ["lib#append", "call", 37],
  ]);
  assert.deepEqual(refs("base.go#NamedShape"), [
    ["shapes.go#Shape", "extends", 22],
    ["base.go#Namer", "extends", 23],
  ]);
  // A field owns its type's reference; a package-level variable its initializer's.
  assert.deepEqual(refs("square.go#Square.base"), [["base.go#base", "type", 5]]);
  assert.deepEqual(refs("shapes.go#registry"), [["shapes.go#Shape", "type", 24]]);
});

test("call_site: functions and concrete methods are static, interface methods virtual, function values indirect", { skip: NO_GO }, () => {
  const sites = (caller: string, name: string) =>
    rows("call_site").filter((c) => c.caller === caller && c.callee_name === name).map((c) => [c.callee, c.dispatch, c.args]);
  assert.deepEqual(sites("cmd/app/main.go#main", "Area"), [
    ["shapes.go#Circle.Area", "static", 0],
    ["square.go#Square.Area", "static", 0],
  ]);
  assert.deepEqual(sites("cmd/app/main.go#describe", "Area"), [["shapes.go#Shape.Area", "virtual", 0]]);
  assert.deepEqual(sites("cmd/app/main.go#describe", "area"), [["cmd/app/main.go#describe.area", "indirect", 0]]);
  assert.deepEqual(sites("cmd/app/main.go#tally", "append"), [["lib#append", "static", 2]]);
  // A method promoted from an unexported embedded type is named where it is declared.
  assert.deepEqual(sites("shapes_test.go#TestArea", "Fatal"), [["ext:testing#common.Fatal", "static", 1]]);
  const ids = rows("call_site").map((c) => c.id);
  assert.deepEqual(ids, ids.map((_, i) => i + 1));
});

test("implements and overrides: the interfaces each type's method set satisfies, and the methods that satisfy them", { skip: NO_GO }, () => {
  assert.deepEqual(
    rows("implements").map((r) => [r.class, r.interface]),
    [
      ["base.go#base", "lib#error"],
      ["shapes.go#Circle", "shapes.go#Shape"],
      // Area has a pointer receiver, and Error is promoted from base: both through *Square.
      ["square.go#Square", "shapes.go#Shape"],
      ["square.go#Square", "lib#error"],
    ],
  );
  assert.deepEqual(
    rows("overrides").map((r) => [r.member, r.base]),
    [
      ["base.go#base.Error", "lib#error.Error"],
      ["shapes.go#Circle.Area", "shapes.go#Shape.Area"],
      ["square.go#Square.Area", "shapes.go#Shape.Area"],
    ],
  );
  assert.deepEqual(rows("extends").map((r) => [r.child, r.parent]), [
    ["base.go#NamedShape", "shapes.go#Shape"],
    ["base.go#NamedShape", "base.go#Namer"],
  ]);
  assert.deepEqual(rows("embeds"), [{ outer: "square.go#Square", inner: "base.go#base", pointer: false }]);
});

test("member_access: fields and methods of project types, via_this through the method's receiver", { skip: NO_GO }, () => {
  const access = (fn: string) => rows("member_access").filter((a) => a.fn === fn).map((a) => [a.member, a.owner, a.mode, a.via_this]);
  assert.deepEqual(access("base.go#base.Error"), [["base.go#base.Name", "base.go#base", "call", true]]);
  assert.deepEqual(access("cmd/app/main.go#main"), [
    ["shapes.go#Circle.R", "shapes.go#Circle", "write", false],
    ["shapes.go#Circle.Area", "shapes.go#Circle", "call", false],
    ["square.go#Square.Area", "square.go#Square", "call", false],
  ]);
  assert.deepEqual(access("cmd/app/main.go#tally"), [["shapes.go#Circle.Area", "shapes.go#Circle", "read", false]]);
  assert.deepEqual(access("cmd/app/main.go#describe"), [["shapes.go#Shape.Area", "shapes.go#Shape", "call", false]]);
});

test("type_ref positions, and each declaration's type as go/types prints it", { skip: NO_GO }, () => {
  const positions = rows("type_ref").map((r) => [r.from, r.to, r.position]);
  for (const expected of [
    ["cmd/app/main.go#tally", "square.go#Square", "param"],
    ["base.go#base.Err", "lib#error", "return"],
    ["base.go#Round", "shapes.go#Circle", "alias"],
    ["cmd/app/main.go#main", "square.go#Square", "variable"],
    ["square.go#Square.base", "base.go#base", "property"],
  ]) {
    assert.ok(positions.some((p) => JSON.stringify(p) === JSON.stringify(expected)), `no type_ref ${expected.join(" ")}`);
  }
  const type = (id: string) => {
    const t = rows("symbol_type").find((r) => r.symbol === id);
    return [t?.text, t?.is_function, t?.is_any];
  };
  assert.deepEqual(type("cmd/app/main.go#describe"), ["func(sh shapes.Shape, area func() float64) float64", true, false]);
  assert.deepEqual(type("square.go#Square.Area.s"), ["*Square", false, false]);
  // A type parameter constrained by `any` is not itself `any`.
  assert.deepEqual(type("shapes.go#Map.x"), ["T", false, false]);
  assert.equal(rows("unresolved_ref").length, 0);
});

test("refs outside the root: embedded interfaces, fields of outside structs, conversions, generics, function literals", { skip: NO_GO }, () => {
  const dir = tempDir("go-refs");
  writeFiles(dir, {
    "go.mod": "module example.com/r\n\ngo 1.26\n",
    "r.go": [
      "package r",
      "",
      'import (\n\t"fmt"\n\t"io"\n\t"net/http"\n)',
      "",
      "type Reader struct {\n\tio.Reader\n\tn int\n}",
      "",
      "func (r *Reader) Read(p []byte) (int, error) {\n\tr.n++\n\treturn r.Reader.Read(p)\n}",
      "",
      "type Temp float64",
      "",
      "func (t Temp) String() string { return fmt.Sprint(float64(t)) }",
      "",
      "var _ fmt.Stringer = Temp(0)",
      "",
      "type Number interface{ ~int | ~float64 }",
      "",
      "func Sum[T Number](xs ...T) (s T) {\n\tfor _, x := range xs {\n\t\ts += x\n\t}\n\treturn s\n}",
      "",
      "type Named interface{ Name() string }",
      "",
      "func Names[T Named](xs []T) []string {\n\tvar out []string\n\tfor _, x := range xs {\n\t\tout = append(out, x.Name())\n\t}\n\treturn out\n}",
      "",
      'func Serve() *http.Server {\n\tsrv := &http.Server{Addr: ":80"}\n\tsrv.Addr = ":81"\n\ttotal := Sum[int](1, 2)\n\t_ = total\n\tf := func() {}\n\tf()\n\tfunc() {}()\n\treturn srv\n}',
      "",
    ].join("\n"),
  });
  const { tables } = extractGo(dir, { layers: ["refs"] });
  const pairs = (rel: string, a: string, b: string) => tables.rows(rel).map((r) => [r[a], r[b]]);
  assert.deepEqual(pairs("implements", "class", "interface"), [
    ["r.go#Reader", "ext:io#Reader"],
    ["r.go#Temp", "ext:fmt#Stringer"],
  ]);
  // Reader's Read hides the embedded io.Reader's, and satisfies it.
  assert.deepEqual(pairs("overrides", "member", "base"), [
    ["r.go#Reader.Read", "ext:io#Reader.Read"],
    ["r.go#Temp.String", "ext:fmt#Stringer.String"],
  ]);
  assert.deepEqual(tables.rows("embeds"), [{ outer: "r.go#Reader", inner: "ext:io#Reader", pointer: false }]);
  assert.deepEqual(
    tables.rows("call_site").map((c) => [c.caller, c.callee, c.callee_name, c.dispatch]),
    [
      ["r.go#Reader.Read", "ext:io#Reader.Read", "Read", "virtual"],
      ["r.go#Temp.String", "ext:fmt#Sprint", "Sprint", "static"],
      ["r.go#Names", "lib#append", "append", "static"],
      // A method of a type parameter dispatches through its constraint.
      ["r.go#Names", "r.go#Named.Name", "Name", "virtual"],
      ["r.go#Serve", "r.go#Sum", "Sum", "static"],
      // `f := func() {…}` is that function; a literal called where it stands is itself.
      ["r.go#Serve", "r.go#Serve.f", "f", "static"],
      ["r.go#Serve", "r.go#Serve.<function@51:2>", null, "static"],
    ],
  );
  const ref = (from: string) => tables.rows("ref").filter((r) => r.from === from).map((r) => [r.to, r.kind]);
  assert.deepEqual(ref("r.go#Serve").slice(0, 4), [
    ["ext:net/http#Server", "type"],
    ["ext:net/http#Server", "new"],
    ["ext:net/http#Server.Addr", "write"],
    ["ext:net/http#Server.Addr", "write"],
  ]);
  const addr = tables.rows("symbol").find((s) => s.id === "ext:net/http#Server.Addr");
  assert.deepEqual([addr?.kind, addr?.parent, addr?.package], ["property", "ext:net/http#Server", "net/http"]);
  // `Temp(0)` is a conversion: a type in an assertion position, and no call.
  assert.deepEqual(
    tables.rows("type_ref").filter((r) => r.from === "r.go#<module>").map((r) => [r.to, r.position]),
    [
      ["ext:fmt#Stringer", "variable"],
      ["r.go#Temp", "assertion"],
    ],
  );
  fs.rmSync(dir, { recursive: true, force: true });
});

test("unresolved_ref: names the checker could not resolve, but not members of what it could not type", { skip: NO_GO }, () => {
  const dir = tempDir("go-unresolved");
  writeFiles(dir, {
    "go.mod": "module example.com/u\n\ngo 1.26\n",
    "u.go":
      'package u\n\nimport "example.com/missing/pkg"\n\ntype T struct {\n\tA Unknown\n}\n\nfunc F(t T) int {\n\tt.nope = 1\n\treturn undefinedFn() + pkg.X + missingVar +\n\t\tmissingVar.field\n}\n',
  });
  const { tables } = extractGo(dir, { layers: ["refs"] });
  // `field`, a member of the unresolved `missingVar`, is that gap again and has no row.
  assert.deepEqual(
    tables.rows("unresolved_ref").map((u) => [u.from, u.name, u.kind, u.line]),
    [
      ["u.go#T.A", "Unknown", "type", 6],
      ["u.go#F", "nope", "read", 10],
      ["u.go#F", "undefinedFn", "call", 11],
      ["u.go#F", "X", "read", 11],
      ["u.go#F", "missingVar", "read", 11],
      ["u.go#F", "missingVar", "read", 12],
    ],
  );
  assert.deepEqual(
    tables.rows("call_site").map((c) => [c.callee, c.callee_name, c.dispatch]),
    [[null, "undefinedFn", "unresolved"]],
  );
  fs.rmSync(dir, { recursive: true, force: true });
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
