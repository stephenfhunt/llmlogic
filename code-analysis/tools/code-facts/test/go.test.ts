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

test("entry_point: main, and the functions go test runs", { skip: NO_GO }, () => {
  assert.deepEqual(
    rows("entry_point").map((e) => [e.symbol, e.kind]),
    [
      ["cmd/app/main.go#main", "main"],
      ["example_test.go#ExampleSum", "example"],
      ["shapes_test.go#TestArea", "test"],
    ],
  );
});

test("Go declarations: go test's naming rules, struct tags, typed constants, receivers, go.mod directives, generic types", { skip: NO_GO }, () => {
  const dir = tempDir("go-decls");
  writeFiles(dir, {
    "go.mod": [
      "module example.com/e",
      "",
      "go 1.26",
      "",
      "toolchain go1.26.1",
      "",
      "godebug default=go1.21",
      "",
      "require example.com/dep v1.0.0",
      "",
      "replace example.com/dep v1.0.0 => example.com/fork v1.0.1",
      "",
      "replace example.com/old => ../old",
      "",
      "exclude example.com/dep v0.9.0",
      "",
      "retract (\n\tv0.1.0\n\t[v0.2.0, v0.2.5]\n)",
      "",
    ].join("\n"),
    "e.go": [
      "package e",
      "",
      'import "fmt"',
      "",
      "type Color int",
      "",
      "const (\n\tRed Color = iota\n\tGreen\n\tBlue\n)",
      "",
      "const Answer Color = 42",
      "",
      "const Plain = 7",
      "",
      "type Point struct {\n\tX    int `json:\"x\" db:\"px\"`\n\tY, W int `json:\"y,omitempty\"`\n\tZ    int `weird`\n\tName string\n}",
      "",
      "func (p *Point) Move() {}",
      "",
      "func (p Point) Norm() int { return 0 }",
      "",
      "type Box[T any] struct{ v T }",
      "",
      "func (b *Box[T]) String() string { return fmt.Sprint(b.v) }",
      "",
      "var _ fmt.Stringer = (*Box[int])(nil)",
      "",
      "func init() {}",
      "",
      "func init() {}",
      "",
    ].join("\n"),
    "e_test.go": [
      "package e",
      "",
      'import "testing"',
      "",
      "func TestMain(m *testing.M) { m.Run() }",
      "func TestX(t *testing.T)     {}",
      "func Testlower(t *testing.T) {}",
      "func Test_under(t *testing.T) {}",
      "func BenchmarkX(b *testing.B) {}",
      "func FuzzX(f *testing.F)     {}",
      "func Example()               {}",
      "func ExamplePoint_Move()     {}",
      "func Examplebad()            {}",
      "func TestWrong(x int)        {}",
      "",
    ].join("\n"),
  });
  const { tables } = extractGo(dir, { layers: ["refs"] });
  // `Testlower` and `Examplebad` break the naming rule; `TestWrong` the signature.
  assert.deepEqual(
    tables.rows("entry_point").map((e) => [e.symbol, e.kind]),
    [
      ["e.go#init", "init"],
      ["e.go#init@36", "init"],
      ["e_test.go#TestMain", "test_main"],
      ["e_test.go#TestX", "test"],
      ["e_test.go#Test_under", "test"],
      ["e_test.go#BenchmarkX", "benchmark"],
      ["e_test.go#FuzzX", "fuzz"],
      ["e_test.go#Example", "example"],
      ["e_test.go#ExamplePoint_Move", "example"],
    ],
  );
  assert.deepEqual(
    tables.rows("field_tag").map((t) => [t.field, t.key, t.value, t.text]),
    [
      ["e.go#Point.X", "json", "x", 'json:"x" db:"px"'],
      ["e.go#Point.X", "db", "px", 'json:"x" db:"px"'],
      ["e.go#Point.Y", "json", "y,omitempty", 'json:"y,omitempty"'],
      ["e.go#Point.W", "json", "y,omitempty", 'json:"y,omitempty"'],
      ["e.go#Point.Z", null, null, "weird"],
    ],
  );
  // `Plain` is untyped: no named type, no row.
  assert.deepEqual(
    tables.rows("typed_const").map((c) => [c.symbol, c.type, c.value, c.iota]),
    [
      ["e.go#Red", "e.go#Color", "0", true],
      ["e.go#Green", "e.go#Color", "1", true],
      ["e.go#Blue", "e.go#Color", "2", true],
      ["e.go#Answer", "e.go#Color", "42", false],
    ],
  );
  const form = (id: string) => tables.rows("symbol").find((s) => s.id === id)?.form;
  assert.deepEqual([form("e.go#Point.Move"), form("e.go#Point.Norm"), form("e.go#Box.String")], ["pointer_receiver", "value_receiver", "pointer_receiver"]);
  assert.deepEqual(
    tables.rows("module_directive").map((d) => [d.directive, d.path, d.version, d.replacement, d.replacement_version]),
    [
      ["go", null, "1.26", null, null],
      ["toolchain", null, "go1.26.1", null, null],
      ["replace", "example.com/dep", "v1.0.0", "example.com/fork", "v1.0.1"],
      ["replace", "example.com/old", null, "../old", null],
      ["exclude", "example.com/dep", "v0.9.0", null, null],
      ["retract", null, "v0.1.0", null, null],
      ["retract", null, "[v0.2.0, v0.2.5]", null, null],
      ["godebug", "default", "go1.21", null, null],
    ],
  );
  // A generic type implements an interface as its own receiver `Box[T]` sees it.
  assert.deepEqual(tables.rows("implements").map((r) => [r.class, r.interface, r.pointer]), [["e.go#Box", "ext:fmt#Stringer", true]]);
  assert.deepEqual(tables.rows("overrides").map((r) => [r.member, r.base]), [["e.go#Box.String", "ext:fmt#Stringer.String"]]);
  fs.rmSync(dir, { recursive: true, force: true });
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

test("call_site: unsafe's builtins are its package's; a function value computed in place names no callee", { skip: NO_GO }, () => {
  const dir = tempDir("go-calls");
  writeFiles(dir, {
    "go.mod": "module example.com/c\n\ngo 1.26\n",
    "c.go":
      'package c\n\nimport "unsafe"\n\ntype H func() int\n\nfunc wrap(n int) func(int) int { return func(m int) int { return n + m } }\n\nfunc F(hs []H) int {\n\tsize := int(unsafe.Sizeof(uintptr(0)))\n\treturn wrap(size)(1) + hs[0]()\n}\n',
  });
  const { tables } = extractGo(dir, { layers: ["refs", "dataflow"] });
  const sites = tables.rows("call_site");
  assert.deepEqual(
    sites.map((c) => [c.line, c.col, c.callee, c.callee_name, c.dispatch]),
    [
      [10, 14, "ext:unsafe#Sizeof", "Sizeof", "static"],
      [11, 9, null, null, "unresolved"],
      [11, 9, "c.go#wrap", "wrap", "static"],
      [11, 25, null, null, "unresolved"],
    ],
  );
  // Points-to follows each through the value it calls.
  const viaVar = new Set(tables.rows("callee_var").map((v) => v.call_site));
  assert.deepEqual(sites.filter((c) => c.dispatch === "unresolved").map((c) => viaVar.has(c.id)), [true, true]);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("implements and overrides: the interfaces each type's method set satisfies, and the methods that satisfy them", { skip: NO_GO }, () => {
  assert.deepEqual(
    rows("implements").map((r) => [r.class, r.interface, r.pointer]),
    [
      ["base.go#base", "lib#error", true],
      ["shapes.go#Circle", "shapes.go#Shape", false],
      // Area has a pointer receiver, and Error is promoted from base: both only through *Square.
      ["square.go#Square", "shapes.go#Shape", true],
      ["square.go#Square", "lib#error", true],
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
  // Reader's Read has a pointer receiver; Temp's String a value receiver.
  assert.deepEqual(tables.rows("implements").map((r) => r.pointer), [true, false]);
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

const FLOW_SOURCE = `package f

import (
	"errors"
	"fmt"
)

func Loops(xs []int, ch chan int) (total int) {
	for i := 0; i < len(xs); i++ {
		if xs[i] < 0 {
			continue
		}
		total += xs[i]
	}
outer:
	for _, x := range xs {
		for {
			if x > 10 {
				break outer
			}
			break
		}
	}
	for v := range ch {
		total += v
	}
	return
}

func Switches(v any, n int) string {
	switch t := v.(type) {
	case int:
		return fmt.Sprint(t + n)
	case string, []byte:
		_ = t
	}
	switch {
	case n > 0 && n < 10:
		n++
		fallthrough
	case n == 0:
		n--
	default:
		goto done
	}
done:
	return "x"
}

func Selects(a, b chan int) int {
	select {
	case v := <-a:
		return v
	case b <- 1:
	default:
	}
	go func() { a <- 2 }()
	return 0
}

func Defers() (err error) {
	defer func() {
		if r := recover(); r != nil {
			err = errors.New("recovered")
		}
	}()
	x := 0
	f := func() { x++ }
	f()
	if x > 1 {
		panic("too big")
	}
	return nil
}

var initial, other = compute(), 2

func compute() int { return 1 }

func Dead() int {
	y := 1
	y = 2
	return y
}
`;

test("flow: statement-level graphs with defer, goto, fallthrough and select; def/use, captures, metrics", { skip: NO_GO }, () => {
  const dir = tempDir("go-flow");
  const out = path.join(dir, "out");
  writeFiles(dir, { "go.mod": "module example.com/f\n\ngo 1.26\n", "f.go": FLOW_SOURCE });
  const { tables } = extractGo(dir, { out, layers: ["refs", "flow"] });
  const nodes = new Map(tables.rows("flow_node").map((n) => [n.id as number, n]));
  const at = (id: unknown) => {
    const n = nodes.get(id as number);
    return `${n?.kind}@${n?.line}`;
  };
  const edges = (fn: string) =>
    tables
      .rows("flow_edge")
      .filter((e) => nodes.get(e.from as number)?.fn === `f.go#${fn}`)
      .map((e) => `${at(e.from)} -${e.kind}-> ${at(e.to)}`)
      .sort();
  assert.deepEqual(
    edges("Loops"),
    [
      "entry@8 -next-> stmt@9",
      "stmt@9 -next-> loop_head@9",
      "loop_head@9 -on_true-> cond@10",
      "cond@10 -on_true-> continue@11",
      // `continue` runs the post statement; the post statement loops back.
      "continue@11 -continue-> stmt@9",
      "cond@10 -on_false-> stmt@13",
      "stmt@13 -next-> stmt@9",
      "stmt@9 -back-> loop_head@9",
      "loop_head@9 -on_false-> stmt@16",
      "stmt@16 -next-> loop_head@16",
      "loop_head@16 -on_true-> loop_head@17",
      "loop_head@17 -on_true-> cond@18",
      "cond@18 -on_true-> break@19",
      "cond@18 -on_false-> break@21",
      // The inner `break` ends the inner loop, and the outer body with it; `break outer` ends both.
      "break@21 -break-> loop_head@16",
      "loop_head@16 -on_false-> stmt@24",
      "break@19 -break-> stmt@24",
      "stmt@24 -next-> loop_head@24",
      "loop_head@24 -on_true-> stmt@25",
      "stmt@25 -back-> loop_head@24",
      "loop_head@24 -on_false-> return@27",
      "return@27 -return-> exit@28",
    ].sort(),
  );
  assert.deepEqual(
    edges("Switches"),
    [
      "entry@30 -next-> switch@31",
      "switch@31 -next-> case_test@32",
      "case_test@32 -on_false-> case_test@34",
      "case_test@32 -case-> return@33",
      "return@33 -return-> exit@48",
      "case_test@34 -case-> stmt@35",
      "case_test@34 -on_false-> switch@37",
      "stmt@35 -next-> switch@37",
      "switch@37 -next-> case_test@38",
      "case_test@38 -on_false-> case_test@41",
      "case_test@38 -case-> stmt@39",
      "stmt@39 -next-> fallthrough@40",
      "fallthrough@40 -fallthrough-> stmt@42",
      "case_test@41 -case-> stmt@42",
      "case_test@41 -default-> goto@44",
      // The label a goto targets is a node.
      "goto@44 -goto-> stmt@46",
      "stmt@42 -next-> stmt@46",
      "stmt@46 -next-> return@47",
      "return@47 -return-> exit@48",
    ].sort(),
  );
  assert.deepEqual(
    edges("Selects"),
    [
      "entry@50 -next-> select@51",
      "select@51 -next-> case_test@52",
      "case_test@52 -on_false-> case_test@54",
      "case_test@52 -case-> return@53",
      "return@53 -return-> exit@59",
      "case_test@54 -case-> stmt@57",
      "case_test@54 -default-> stmt@57",
      "stmt@57 -next-> return@58",
      "return@58 -return-> exit@59",
    ].sort(),
  );
  assert.deepEqual(
    edges("Defers"),
    [
      "entry@61 -next-> stmt@62",
      "stmt@62 -next-> stmt@67",
      "stmt@67 -next-> stmt@68",
      "stmt@68 -next-> stmt@69",
      "stmt@69 -next-> cond@70",
      "cond@70 -on_true-> throw@71",
      "cond@70 -on_false-> return@73",
      // In a function that defers, any node may panic into the deferred calls.
      "stmt@62 -throw-> finally@74",
      "stmt@67 -throw-> finally@74",
      "stmt@68 -throw-> finally@74",
      "stmt@69 -throw-> finally@74",
      "cond@70 -throw-> finally@74",
      "throw@71 -throw-> finally@74",
      "return@73 -throw-> finally@74",
      "return@73 -return-> finally@74",
      // The deferred literal recovers, so a panic may end in a normal return.
      "finally@74 -return-> exit@74",
      "finally@74 -throw-> throw_exit@74",
    ].sort(),
  );

  const fn = (id: string) => tables.rows("fn").find((f) => f.id === `f.go#${id}`);
  const loops = fn("Loops");
  assert.deepEqual(
    [loops?.kind, loops?.cyclomatic, loops?.cognitive, loops?.statements, loops?.max_nesting, loops?.params, loops?.returns],
    ["function", 6, 11, 15, 3, 2, 1],
  );
  assert.deepEqual([fn("Defers")?.throws, fn("Switches")?.cognitive, fn("Defers.f")?.kind], [1, 4, "function_expression"]);

  const rowsAt = (rel: string, key: string) =>
    tables.rows(rel).map((r) => `${at(r.node)}:${String(r[key]).replace("f.go#", "")}`).sort();
  assert.ok(rowsAt("def", "var").includes("switch@31:Switches.t"), "a type switch's variable is defined at the switch");
  assert.ok(rowsAt("def", "var").includes("case_test@52:Selects.v"), "a receive in a select case defines at the case");
  assert.ok(rowsAt("use", "var").includes("return@27:Loops.total"), "a bare return reads the named results");
  assert.ok(rowsAt("def", "var").includes("stmt@76:initial"), "a package-level variable is defined in its file's <module>");
  const sites = new Map(tables.rows("call_site").map((c) => [c.id, c.callee]));
  const deferred = tables.rows("call_at").filter((c) => nodes.get(c.node as number)?.kind === "finally").map((c) => sites.get(c.call_site));
  assert.deepEqual(deferred, ["f.go#Defers.<function@62:8>"]);
  assert.deepEqual(
    tables.rows("captures").map((c) => [c.fn, c.var]),
    [
      ["f.go#Selects.<function@57:5>", "f.go#Selects.a"],
      ["f.go#Defers.<function@62:8>", "f.go#Defers.err"],
      ["f.go#Defers.f", "f.go#Defers.x"],
    ],
  );
  assert.deepEqual(
    tables.rows("concurrency_site").map((c) => `${at(c.node)}:${c.kind}`),
    ["loop_head@24:chan_recv", "select@51:select", "case_test@52:chan_recv", "case_test@54:chan_send", "stmt@57:go", "stmt@57:chan_send", "stmt@62:defer"],
  );

  if (engineAvailable()) {
    const r = datalog(path.join(out, "lib", "flow.dl"), ["dead_store(D, V, L)", "undefined_use(N, V)", "unreachable(F, N, L)"]);
    assert.equal(r.code, 0, r.stderr);
    const answers = r.stdout.split("\n").filter((l) => /^(dead_store|undefined_use|unreachable)\(/.test(l));
    assert.equal(answers.length, 1, r.stdout);
    assert.match(answers[0] ?? "", /^dead_store\(\d+, "f\.go#Dead\.y", 81\)\.$/);
    const checks = datalog(path.join(out, "lib", "checks.dl"));
    assert.equal(checks.code, 1, `expected a clean run, got exit ${checks.code}:\n${checks.stdout}${checks.stderr}`);
  }
  fs.rmSync(dir, { recursive: true, force: true });
});

test("dataflow: closures capture through cells, channels, recover, package variables, and points-to over them", { skip: NO_GO }, () => {
  const dir = tempDir("go-dataflow");
  const out = path.join(dir, "out");
  writeFiles(dir, { "go.mod": "module example.com/f\n\ngo 1.26\n", "f.go": FLOW_SOURCE });
  const { tables } = extractGo(dir, { out, layers: ["refs", "flow", "dataflow"] });
  const allocOf = new Map(tables.rows("alloc").map((a) => [a.var as string, a]));
  // Package-level variables are cells; every project function is a function value.
  assert.deepEqual([allocOf.get("f.go#initial")?.kind, allocOf.get("f.go#other")?.kind], ["cell", "cell"]);
  assert.deepEqual([allocOf.get("f.go#compute")?.kind, allocOf.get("f.go#compute")?.fn_target], ["function", "f.go#compute"]);
  // `f := func() { x++ }` captures x: the closure object's `free0` is x's cell,
  // which the literal loads through its own `this`.
  const closure = tables.rows("alloc").find((a) => a.kind === "function" && a.fn_target === "f.go#Defers.f" && a.var !== "f.go#Defers.f");
  assert.ok(closure !== undefined, "no closure allocation for Defers.f");
  const captured = tables.rows("store").find((s) => s.base === closure.var && s.field === "free0");
  assert.equal(allocOf.get(String(captured?.from))?.kind, "cell");
  assert.deepEqual(
    tables.rows("this_var").filter((t) => t.fn === "f.go#Defers.f").map((t) => t.var),
    ["f.go#Defers.f$this"],
  );
  assert.ok(tables.rows("load").some((l) => l.fn === "f.go#Defers.f" && l.base === "f.go#Defers.f$this" && l.field === "free0"));
  // recover() reads what panicked; a receive loads a channel's elements.
  assert.ok(tables.rows("assign").some((a) => a.fn === "f.go#Defers.<function@62:8>" && a.from === "$thrown"));
  assert.ok(tables.rows("load").some((l) => l.fn === "f.go#Selects" && l.field === "<chan>"));
  // A named result a deferred literal writes is read back into the return slot.
  assert.ok(tables.rows("formal_ret").some((r) => r.fn === "f.go#Defers"));

  if (engineAvailable()) {
    const r = datalog(path.join(out, "lib", "pointsto.dl"), ['c(O) :- pts("f.go#Defers.f$free0", O)', `x(O) :- pts("${captured?.from}", O)`]);
    assert.equal(r.code, 0, r.stderr);
    const site = (rel: string) => r.stdout.split("\n").filter((l) => l.startsWith(`${rel}(`)).map((l) => l.slice(rel.length));
    assert.deepEqual(site("c"), site("x"));
    assert.equal(site("c").length, 1);
    const checks = datalog(path.join(out, "lib", "checks.dl"));
    assert.equal(checks.code, 1, `expected a clean run, got exit ${checks.code}:\n${checks.stdout}${checks.stderr}`);
  }
  fs.rmSync(dir, { recursive: true, force: true });
});

const QUALITY_SOURCE = `package q

import (
	_ "embed"
	"encoding/json"
	"fmt"
	"os"
)

//go:generate stringer -type=Mode

//go:embed data.txt
var data string

type Mode int

// TODO: remove this
func Load(path string) (map[string]any, error) {
	f, err := os.Open(path) //nolint:gosec // trusted
	if err != nil {
		return nil, err
	}
	defer f.Close()
	var out map[string]any
	_ = json.NewDecoder(f).Decode(&out) //lint:ignore SA9003 fine
	fmt.Println("loaded", 3.5) // #nosec G104
	return out, nil
}

func Guard(v interface{}) (n int) {
	defer func() {
		if r := recover(); r != nil {
			panic(fmt.Errorf("again: %v", r))
		}
	}()
	defer func() { recover() }()
	s := v.(string)
	switch t := v.(type) {
	case int:
		n = t
	}
	if s == "" {
		panic(errBad)
	}
	var arr [4]int
	_ = arr
	go os.Remove("x")
	return len(s)
}

var errBad = fmt.Errorf("bad")

func Generic[T any](x T) any { return x }

type Tagged struct {
	A int \`json:"a"\`
}

func broken() { undefined() }
`;

test("quality: diagnostics, suppressions, directives, dropped errors, any, assertions, literals, panic and recover", { skip: NO_GO }, () => {
  const dir = tempDir("go-quality");
  writeFiles(dir, { "go.mod": "module example.com/q\n\ngo 1.26\n", "data.txt": "hello\n", "q.go": QUALITY_SOURCE });
  const { tables } = extractGo(dir, { layers: ["refs", "quality"] });
  const cols = (rel: string, ...names: string[]) => tables.rows(rel).map((r) => names.map((n) => r[n]));
  // The go command's echo of the failed compile is the type error again: one row.
  assert.deepEqual(cols("diagnostic", "file", "line", "code", "message"), [["q.go", 59, 0, "undefined: undefined"]]);
  assert.deepEqual(cols("lint_directive", "line", "tool", "directive", "rules"), [
    [19, "golangci", "nolint", "gosec"],
    [25, "staticcheck", "ignore", "SA9003"],
    [26, "gosec", "nosec", "G104"],
  ]);
  assert.deepEqual(cols("compiler_directive", "line", "name", "text"), [
    [10, "generate", "stringer -type=Mode"],
    [12, "embed", "data.txt"],
  ]);
  assert.deepEqual(cols("comment_marker", "line", "kind", "text"), [[17, "todo", "remove this"]]);
  const callee = new Map(tables.rows("call_site").map((c) => [c.id, c.callee_name]));
  assert.deepEqual(
    tables.rows("ignored_error").map((r) => [r.fn, callee.get(r.call_site), r.how]),
    [
      ["q.go#Load", "Close", "deferred"],
      ["q.go#Load", "Decode", "blank"],
      ["q.go#Load", "Println", "discarded"],
      ["q.go#Guard", "Remove", "go"],
    ],
  );
  // `any` as a type parameter's constraint says nothing about a value; recover() returns one.
  assert.deepEqual(cols("any_site", "fn", "line", "kind"), [
    ["q.go#Load", 18, "explicit"],
    ["q.go#Load", 24, "explicit"],
    ["q.go#Guard", 30, "explicit"],
    ["q.go#Guard.<function@31:8>", 32, "call_result"],
    ["q.go#Guard.<function@36:8>", 36, "call_result"],
    ["q.go#Generic", 53, "explicit"],
  ]);
  assert.deepEqual(cols("assertion", "line", "kind", "to_type", "from_any", "to_any"), [
    [37, "type_assert", "string", true, false],
    [38, "type_switch", null, true, false],
  ]);
  // Not literals: import paths, the array length, the struct tag.
  assert.deepEqual(cols("literal", "fn", "kind", "value"), [
    ["q.go#Load", "string", "loaded"],
    ["q.go#Load", "number", "3.5"],
    ["q.go#Guard.<function@31:8>", "string", "again: %v"],
    ["q.go#Guard", "string", ""],
    ["q.go#Guard", "string", "x"],
    ["q.go#errBad", "string", "bad"],
  ]);
  assert.deepEqual(cols("throw_site", "fn", "line", "type"), [
    ["q.go#Guard.<function@31:8>", 33, "lib#error"],
    ["q.go#Guard", 43, "lib#error"],
  ]);
  assert.deepEqual(cols("catch_site", "fn", "binds", "empty", "rethrows"), [
    ["q.go#Guard.<function@31:8>", true, false, true],
    ["q.go#Guard.<function@36:8>", false, true, false],
  ]);
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
