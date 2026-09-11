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

test("param rows: a method's bound self is not a parameter; an assigned lambda's are", () => {
  const params = (fn: string) => rows("param").filter((p) => p.fn === fn).map((p) => [p.index, p.name]);
  assert.deepEqual(params("shapes/circle.py#Circle.register"), [[0, "registry"]]);
  assert.deepEqual(params("shapes/base.py#Shape.unit"), [], "a staticmethod binds nothing, and has no parameters");
  assert.deepEqual(params("shapes/util.py#square"), [[0, "x"]]);
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

test("flow and quality: decisions, metrics, and what the authors suppressed or deferred", () => {
  const dir = tempDir("py-quality");
  writeFiles(dir, {
    "q.py": [
      "from typing import Any, cast",
      "",
      "async def fetch(x):",
      "    return x",
      "",
      "def work(a, b: int, *rest):  # noqa: C901",
      "    total = 0  # TODO: make it configurable",
      "    for i in range(b):",
      "        if a and i > 2:",
      "            total += i",
      "    try:",
      "        fetch(a)",
      "    except ValueError:",
      "        pass",
      "    except Exception as e:",
      "        raise RuntimeError('wrapped') from e",
      "    label = f'{total} items'",
      "    return cast(int, total), {'key': 1.5}, label  # type: ignore[return-value]",
      "",
      "class K:",
      "    def m(self, y):  # pragma: no cover",
      "        return [z for z in y if z]",
    ].join("\n"),
  });
  const r = extractPython(dir, { layers: ["refs", "flow", "quality"] });
  const rows = (rel: string) => r.tables.rows(rel);
  const fn = (id: string) => rows("fn").find((f) => f.id === id);
  // for, if, and, two except clauses
  assert.equal(fn("q.py#work")?.cyclomatic, 6);
  assert.equal(fn("q.py#work")?.params, 3);
  assert.equal(fn("q.py#K.m")?.params, 1, "a method's self is not counted");
  assert.equal(fn("q.py#K.m")?.cyclomatic, 3, "a comprehension's for and if are decisions");
  assert.equal(fn("q.py#<module>")?.kind, "module");
  assert.deepEqual(
    rows("lint_directive").map((d) => [d.line, d.tool, d.directive, d.rules]),
    [
      [6, "noqa", "ignore", "C901"],
      [18, "mypy", "ignore", "return-value"],
      [21, "coverage", "no cover", null],
    ],
  );
  assert.deepEqual(rows("comment_marker").map((m) => [m.line, m.kind, m.text]), [[7, "todo", "make it configurable"]]);
  assert.deepEqual(rows("catch_site").map((c) => [c.line, c.binds, c.empty, c.rethrows]), [
    [13, false, true, false],
    [15, true, false, true],
  ]);
  assert.deepEqual(rows("throw_site").map((t) => [t.fn, t.type]), [["q.py#work", "lib#RuntimeError"]]);
  assert.deepEqual(rows("assertion").map((a) => [a.kind, a.to_type]), [["cast", "int"]]);
  assert.equal(rows("floating_promise").length, 1, "fetch(a)'s coroutine is discarded");
  const lits = rows("literal").map((l) => [l.kind, l.value]);
  assert.ok(lits.some(([k, v]) => k === "string" && v === "wrapped"));
  assert.ok(lits.some(([k, v]) => k === "template" && v === "{total} items"));
  assert.ok(lits.some(([k, v]) => k === "number" && v === "1.5"));
  assert.ok(!lits.some(([, v]) => v === "key"), "a dict key is a name, not a value");
  assert.ok(!lits.some(([, v]) => v === "int"), "a cast's type is not a value");
  const implicit = rows("any_site").filter((a) => a.kind === "implicit_param").map((a) => a.fn);
  // `a` and `*rest` share a line, so their rows are one fact
  assert.deepEqual(implicit.sort(), ["q.py#K.m", "q.py#fetch", "q.py#work"]);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("lambdas: in a decorator's arguments, nested in a lambda, each resolves its own parameters", () => {
  const dir = tempDir("py-lambdas");
  writeFiles(dir, {
    "l.py": [
      "def deco(key):",
      "    return lambda f: f",
      "",
      "@deco(key=lambda e: e.name)",
      "def g():",
      "    return 1",
      "",
      "adder = lambda x: lambda y: x + y",
      "",
      "def outer(flag):",
      "    if flag:",
      "        def inner(items):",
      "            return [z for z in items]",
      "        return inner",
    ].join("\n"),
  });
  const r = extractPython(dir, { layers: ["refs", "flow"] });
  assert.deepEqual(r.tables.rows("unresolved_ref"), []);
  const ids = r.tables.rows("symbol").map((s) => s.id);
  assert.ok(ids.includes("l.py#<lambda@4:11>.e"), "the decorator's lambda is declared, with its parameter");
  assert.ok(ids.includes("l.py#adder.<lambda@8:19>.y"), "a lambda in a lambda nests in it");
  assert.deepEqual(r.tables.rows("captures").map((c) => [c.fn, c.var]), [["l.py#adder.<lambda@8:19>", "l.py#adder.x"]]);
  assert.ok(ids.includes("l.py#outer.inner.z"));
  assert.ok(!ids.includes("l.py#outer.z"), "a comprehension in a nested def binds there, not in the enclosing function");
  fs.rmSync(dir, { recursive: true, force: true });
});
