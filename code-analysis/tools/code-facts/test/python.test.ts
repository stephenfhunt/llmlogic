// The Python frontend's structure and refs layers, on test/fixtures/python-basic
// and on small projects written per test.

import assert from "node:assert/strict";
import { test } from "node:test";
import * as fs from "node:fs";
import { extractPython, fixture, tempDir, writeFiles } from "./helpers.ts";

const py = extractPython(fixture("python-basic"), { layers: ["refs"] });
const rows = (rel: string) => py.tables.rows(rel);

function site(caller: string, calleeName: string) {
  const s = rows("call_site").find((c) => c.caller === caller && c.callee_name === calleeName);
  assert.ok(s !== undefined, `no call from ${caller} to ${calleeName}`);
  return s;
}

function symbol(id: string) {
  const s = rows("symbol").find((r) => r.id === id);
  assert.ok(s !== undefined, `no symbol ${id}`);
  return s;
}

test("files are Python, tests are recognised by path, and the package comes from pyproject.toml", () => {
  const files = rows("file").map((f) => [f.path, f.lang, f.is_test, f.package]);
  assert.deepEqual(files, [
    ["app.py", "py", false, "shapes-demo"],
    ["shapes/__init__.py", "py", false, "shapes-demo"],
    ["shapes/base.py", "py", false, "shapes-demo"],
    ["shapes/circle.py", "py", false, "shapes-demo"],
    ["shapes/util.py", "py", false, "shapes-demo"],
    ["tests/test_circle.py", "py", true, "shapes-demo"],
  ]);
  const ex = rows("extraction")[0];
  assert.equal(ex?.targets, ".");
  assert.match(String(ex?.python_version), /^3\.\d+\.\d+$/);
});

test("package_dep: distribution names normalized, dependency groups are dev", () => {
  const deps = rows("package_dep").map((d) => [d.dep, d.kind, d.range]);
  assert.deepEqual(deps, [
    ["requests", "prod", "requests>=2"],
    ["pyyaml", "prod", "PyYAML"],
    ["pytest", "dev", "pytest"],
  ]);
});

test("an import under `if TYPE_CHECKING:` is type-only and not runtime", () => {
  const imp = rows("imports").find((i) => i.file === "shapes/circle.py" && i.specifier === ".util");
  assert.deepEqual([imp?.kind, imp?.runtime, imp?.target_file], ["type_only", false, "shapes/util.py"]);
  const name = rows("import_name").find((n) => n.file === "shapes/circle.py" && n.local === "Registry");
  assert.deepEqual([name?.target, name?.type_only], ["shapes/util.py#Registry", true]);
});

test("`from . import base` imports the package and its submodule; relative specifiers resolve", () => {
  const line5 = rows("imports")
    .filter((i) => i.file === "shapes/circle.py" && i.line === 5)
    .map((i) => [i.specifier, i.target_file]);
  assert.deepEqual(line5, [
    [".", "shapes/__init__.py"],
    [".base", "shapes/base.py"],
  ]);
  const base = rows("import_name").find((n) => n.file === "shapes/circle.py" && n.local === "base");
  assert.equal(base?.target, "shapes/base.py#<module>");
});

test("stdlib imports are builtin; third-party ones name their top-level package", () => {
  const imp = (file: string, spec: string) => rows("imports").find((i) => i.file === file && i.specifier === spec);
  assert.deepEqual([imp("shapes/base.py", "math")?.builtin, imp("shapes/base.py", "math")?.resolved], [true, true]);
  assert.deepEqual([imp("app.py", "yaml")?.builtin, imp("app.py", "yaml")?.target_package, imp("app.py", "yaml")?.resolved], [false, "yaml", false]);
  assert.equal(imp("shapes/util.py", "shapes.base")?.kind, "reexport_all");
});

test("an import through a package's re-export resolves to the defining module", () => {
  const circle = rows("import_name").find((n) => n.file === "app.py" && n.local === "Circle");
  assert.equal(circle?.target, "shapes/circle.py#Circle");
});

test("exports: `__all__` when present, re-exports named as such; otherwise public names", () => {
  const exp = (file: string) => rows("exports").filter((e) => e.file === file).map((e) => [e.name, e.symbol, e.kind]);
  assert.deepEqual(exp("shapes/__init__.py"), [
    ["Shape", "shapes/base.py#Shape", "reexport"],
    ["Circle", "shapes/circle.py#Circle", "reexport"],
  ]);
  assert.deepEqual(exp("shapes/util.py").map((e) => e[0]), ["counter", "Registry", "make_counter", "square"]);
  assert.deepEqual(exp("shapes/base.py").map((e) => e[0]), ["Shape"]); // `math`, imported, is not an export
});

test("symbol kinds: constructor, getter, static method, lambda-bound function, nested function", () => {
  assert.equal(symbol("shapes/base.py#Shape.__init__").kind, "constructor");
  assert.equal(symbol("shapes/circle.py#Circle.diameter").kind, "getter");
  assert.equal(symbol("shapes/base.py#Shape.unit").is_static, true);
  assert.equal(symbol("shapes/util.py#square").kind, "function");
  assert.equal(symbol("shapes/util.py#square.x").kind, "parameter");
  assert.equal(symbol("shapes/util.py#make_counter.bump").parent, "shapes/util.py#make_counter");
});

test("members: `self.x = …` declares a property; visibility follows the underscore convention", () => {
  assert.deepEqual([symbol("shapes/base.py#Shape.name").kind, symbol("shapes/base.py#Shape.name").parent], ["property", "shapes/base.py#Shape"]);
  assert.equal(symbol("shapes/base.py#Shape.sides").visibility, "public");
  assert.equal(symbol("shapes/base.py#Shape._check").visibility, "protected");
  assert.equal(symbol("shapes/base.py#Shape.__valid").visibility, "private");
  assert.equal(symbol("shapes/base.py#Shape.__init__").visibility, "public"); // a dunder is not name-mangled
});

test("`nonlocal` binds the enclosing function's local; `global` the module's variable", () => {
  const ids = rows("symbol").map((s) => s.id);
  assert.ok(!ids.includes("shapes/util.py#make_counter.bump.n"));
  assert.ok(!ids.includes("shapes/util.py#Registry.add.counter"));
  assert.ok(ids.includes("shapes/util.py#counter"));
});

test("dispatch: calling a class is `new` to the class; a method on an instance is virtual", () => {
  assert.deepEqual([site("app.py#main", "Circle").callee, site("app.py#main", "Circle").kind, site("app.py#main", "Circle").dispatch], [
    "shapes/circle.py#Circle",
    "new",
    "static",
  ]);
  // `c = Circle(2.0)` then `c.register(…)`: the local's class comes from its constructor call
  assert.deepEqual([site("app.py#main", "register").callee, site("app.py#main", "register").dispatch], ["shapes/circle.py#Circle.register", "virtual"]);
  assert.deepEqual([site("shapes/base.py#Shape.describe", "area").callee, site("shapes/base.py#Shape.describe", "area").dispatch], [
    "shapes/base.py#Shape.area",
    "virtual",
  ]);
});

test("dispatch: an annotated parameter's class resolves its method calls", () => {
  const s = site("shapes/circle.py#Circle.register", "add");
  assert.deepEqual([s.callee, s.dispatch], ["shapes/util.py#Registry.add", "virtual"]);
});

test("dispatch: super().__init__ is static to the base's constructor; a static method through a module path is static", () => {
  const sup = site("shapes/circle.py#Circle.__init__", "__init__");
  assert.deepEqual([sup.callee, sup.dispatch], ["shapes/base.py#Shape.__init__", "static"]);
  const unit = site("shapes/circle.py#Circle.area", "unit");
  assert.deepEqual([unit.callee, unit.dispatch], ["shapes/base.py#Shape.unit", "static"]);
});

test("dispatch: externals and builtins are named; an unknown receiver is unresolved but keeps its name", () => {
  assert.equal(site("app.py#main", "safe_load").callee, "ext:yaml#safe_load");
  assert.equal(site("app.py#main", "get").callee, "ext:requests#get");
  assert.equal(site("app.py#main", "len").callee, "lib#len");
  const append = site("shapes/util.py#Registry.add", "append");
  assert.deepEqual([append.callee, append.dispatch], [null, "unresolved"]);
});

test("extends and overrides come from resolved bases", () => {
  assert.deepEqual(rows("extends"), [{ child: "shapes/circle.py#Circle", parent: "shapes/base.py#Shape" }]);
  assert.deepEqual(rows("overrides"), [{ member: "shapes/circle.py#Circle.area", base: "shapes/base.py#Shape.area" }]);
});

test("member_access: through self, and through a typed receiver", () => {
  const acc = (fn: string) =>
    rows("member_access")
      .filter((a) => a.fn === fn)
      .map((a) => [a.member, a.mode, a.via_this]);
  assert.deepEqual(acc("shapes/base.py#Shape.describe"), [
    ["shapes/base.py#Shape.name", "read", true],
    ["shapes/base.py#Shape.area", "call", true],
  ]);
  assert.deepEqual(acc("shapes/circle.py#Circle.register"), [["shapes/util.py#Registry.add", "call", false]]);
});

test("type_ref and symbol_type come from annotations, as written", () => {
  const refs = rows("type_ref").map((r) => [r.from, r.to, r.position]);
  assert.ok(refs.some((r) => r[0] === "shapes/circle.py#Circle.register" && r[1] === "shapes/util.py#Registry" && r[2] === "param"));
  const t = rows("symbol_type").find((s) => s.symbol === "shapes/circle.py#Circle.register.registry");
  assert.equal(t?.text, "Registry");
  assert.ok(!rows("symbol_type").some((s) => s.symbol === "app.py#main.c"), "no annotation, no inferred type");
});

test("`from pkg import sub` inside pkg/__init__.py still lets other modules reach pkg.sub", () => {
  // sqlparse's shape: the package imports its own submodules, and modules import them back through it
  const dir = tempDir("py-selfimport");
  writeFiles(dir, {
    "pkg/__init__.py": "from pkg import tokens\n",
    "pkg/tokens.py": "Keyword = 1\ndef kind(t):\n    return t\n",
    "pkg/lexer.py": "from pkg import tokens as T\n\ndef lex():\n    return T.kind(T.Keyword)\n",
  });
  const r = extractPython(dir, { layers: ["refs"] });
  assert.deepEqual(r.tables.rows("unresolved_ref"), []);
  const kind = r.tables.rows("call_site").find((c) => c.caller === "pkg/lexer.py#lex");
  assert.deepEqual([kind?.callee, kind?.dispatch], ["pkg/tokens.py#kind", "static"]);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("a local rebound from itself (`x = x.next()`) resolves without recursing forever", () => {
  const dir = tempDir("py-selfref");
  writeFiles(dir, {
    "walk.py": "def walk(node):\n    cur = node\n    while cur:\n        cur = cur.next()\n    return cur\n",
  });
  const r = extractPython(dir, { layers: ["refs"] });
  const next = r.tables.rows("call_site").find((c) => c.callee_name === "next");
  assert.equal(next?.dispatch, "unresolved");
  fs.rmSync(dir, { recursive: true, force: true });
});
