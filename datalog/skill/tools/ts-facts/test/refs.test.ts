import assert from "node:assert/strict";
import { test } from "node:test";
import * as fs from "node:fs";
import { extract, fixture, tempDir, writeProject } from "./helpers.ts";

const refs = extract(fixture("refs"), { layers: ["refs"] });
const rows = (rel: string) => refs.tables.rows(rel);

function site(callerSuffix: string, calleeName: string) {
  const s = rows("call_site").find((c) => (c.caller as string).endsWith(callerSuffix) && c.callee_name === calleeName);
  assert.ok(s !== undefined, `no call from …${callerSuffix} to ${calleeName}`);
  return s;
}

test("a call to a free function is static, to the function's own id", () => {
  const s = site("#main", "total");
  assert.equal(s.callee, "src/use.ts#total");
  assert.equal(s.dispatch, "static");
  assert.equal(s.args, 2);
});

test("a method called through an interface or abstract class is virtual, at the declared member", () => {
  assert.deepEqual([site("Registry.handler", "area").callee, site("Registry.handler", "area").dispatch], ["src/shapes.ts#Shape.area", "virtual"]);
  assert.deepEqual([site("Base.describe", "area").callee, site("Base.describe", "area").dispatch], ["src/shapes.ts#Base.area", "virtual"]);
  assert.equal(site("#main", "describe").dispatch, "virtual");
});

test("a static method is static; a class-property arrow is virtual (a subclass can replace it)", () => {
  assert.deepEqual([site("#main", "create").callee, site("#main", "create").dispatch], ["src/shapes.ts#Base.create", "static"]);
  assert.deepEqual([site("#main", "handler").callee, site("#main", "handler").dispatch], ["src/use.ts#Registry.handler", "virtual"]);
});

test("calling a function-typed parameter is indirect, and names the parameter", () => {
  const s = site("#total", "each");
  assert.equal(s.dispatch, "indirect");
  assert.equal(s.callee, "src/use.ts#total.each");
});

test("new targets the constructor, or the class when the constructor is implicit; super() too", () => {
  assert.equal(site("#main", "Circle").callee, "src/shapes.ts#Circle.constructor");
  assert.equal(site("#main", "Circle").kind, "new");
  assert.equal(site("#main", "Plain").callee, "src/shapes.ts#Plain");
  const sup = rows("call_site").find((c) => c.kind === "super");
  assert.deepEqual([sup?.caller, sup?.callee, sup?.dispatch], ["src/shapes.ts#Circle.constructor", "src/shapes.ts#Base", "static"]);
});

test("a callback's calls belong to the callback, named after its position", () => {
  const s = rows("call_site").find((c) => c.caller === "src/use.ts#main.<arrow@32:14>");
  assert.equal(s?.callee, "src/shapes.ts#Shape.area");
});

test("decorators and JSX elements are call sites", () => {
  const dec = rows("call_site").find((c) => c.kind === "decorator");
  assert.deepEqual([dec?.caller, dec?.callee, dec?.dispatch], ["src/use.ts#Registry.add", "src/use.ts#logged", "static"]);
  const jsx = rows("call_site").filter((c) => c.kind === "jsx").map((c) => [c.caller, c.callee, c.dispatch]);
  assert.deepEqual(jsx, [
    ["src/view.tsx#Label", "src/view.tsx#JSX.IntrinsicElements.div", "static"],
    ["src/view.tsx#Page", "src/view.tsx#Label", "static"],
  ]);
});

test("library calls resolve into the lib id-space", () => {
  assert.equal(site("Registry.add", "push").callee, "lib#Array.push");
  const push = rows("symbol").find((s) => s.id === "lib#Array.push");
  assert.deepEqual([push?.origin, push?.kind, push?.parent], ["lib", "method", "lib#Array"]);
});

test("inheritance: extends, implements, and each member's overrides", () => {
  assert.deepEqual(rows("extends"), [{ child: "src/shapes.ts#Circle", parent: "src/shapes.ts#Base" }]);
  assert.deepEqual(rows("implements"), [{ class: "src/shapes.ts#Base", interface: "src/shapes.ts#Shape" }]);
  assert.deepEqual(
    rows("overrides").map((o) => [o.member, o.base]),
    [
      ["src/shapes.ts#Base.area", "src/shapes.ts#Shape.area"],
      ["src/shapes.ts#Base.name", "src/shapes.ts#Shape.name"],
      ["src/shapes.ts#Circle.area", "src/shapes.ts#Base.area"],
    ],
  );
});

test("ref kinds: read, write, readwrite, value, type, extends, implements, decorator", () => {
  const kindsTo = (from: string, to: string) =>
    rows("ref")
      .filter((r) => r.from === from && r.to === to)
      .map((r) => r.kind);
  assert.deepEqual(kindsTo("src/shapes.ts#Circle.constructor", "src/shapes.ts#Circle.radius"), ["write"]);
  assert.deepEqual(kindsTo("src/shapes.ts#Circle.grow", "src/shapes.ts#Circle.radius"), ["readwrite"]);
  assert.deepEqual(kindsTo("src/shapes.ts#Circle.area", "src/shapes.ts#Circle.radius"), ["read"]);
  assert.deepEqual(kindsTo("src/shapes.ts#Circle", "src/shapes.ts#Base"), ["extends"]);
  assert.deepEqual(kindsTo("src/shapes.ts#Base", "src/shapes.ts#Shape"), ["implements"]);
  assert.deepEqual(kindsTo("src/use.ts#Registry.add", "src/use.ts#logged"), ["decorator"]);
  assert.deepEqual(kindsTo("src/use.ts#main", "src/shapes.ts#Base"), ["type", "value"]);
  for (const r of rows("ref")) {
    const target = rows("symbol").find((s) => s.id === r.to);
    assert.ok(target !== undefined && !["local", "parameter", "type_parameter"].includes(target.kind as string), `${r.to} is a local`);
  }
});

test("member access records mode and whether it went through this", () => {
  const acc = rows("member_access").filter((m) => m.member === "src/shapes.ts#Circle.radius");
  assert.deepEqual(
    acc.map((m) => [m.fn, m.mode, m.via_this]),
    [
      ["src/shapes.ts#Circle.constructor", "write", true],
      ["src/shapes.ts#Circle.area", "read", true],
      ["src/shapes.ts#Circle.grow", "readwrite", true],
    ],
  );
  const grow = rows("member_access").find((m) => m.member === "src/shapes.ts#Circle.grow");
  assert.deepEqual([grow?.fn, grow?.mode, grow?.via_this, grow?.owner], ["src/use.ts#main", "call", false, "src/shapes.ts#Circle"]);
});

test("type_ref records the position a type is written in", () => {
  const pos = (from: string, to: string) =>
    rows("type_ref")
      .filter((r) => r.from === from && r.to === to)
      .map((r) => r.position);
  assert.deepEqual(pos("src/use.ts#total", "src/shapes.ts#Shape"), ["param"]);
  assert.deepEqual(pos("src/shapes.ts#Base.create", "src/shapes.ts#Base"), ["return"]);
  assert.deepEqual(pos("src/use.ts#Registry.items", "src/shapes.ts#Shape"), ["property"]);
  assert.deepEqual(pos("src/use.ts#main", "src/shapes.ts#Base"), ["variable"]);
});

test("symbol_type records the checker's type", () => {
  const ty = new Map(rows("symbol_type").map((s) => [s.symbol, s]));
  assert.equal(ty.get("src/use.ts#total")?.text, "(shapes: Shape[], each: (s: Shape) => number) => number");
  assert.equal(ty.get("src/use.ts#total")?.is_function, true);
  assert.equal(ty.get("src/use.ts#total.each")?.is_function, true);
  assert.equal(ty.get("src/shapes.ts#Circle.radius")?.text, "number");
});

test("nothing in the fixture is unresolved", () => {
  assert.deepEqual(rows("unresolved_ref"), []);
});

test("a library's function-typed value is named as the target; a project's stays indirect", () => {
  const { tables } = extract(fixture("../fixtures-external/app"), { layers: ["refs"] });
  const sites = tables.rows("call_site").map((c) => [c.callee_name, c.dispatch, c.callee]);
  assert.deepEqual(sites.sort(), [
    ["expect", "static", "ext:../vendor/api.d.ts#expect"],
    ["expect", "static", "ext:../vendor/api.d.ts#expect"],
    ["it", "static", "ext:../vendor/api.d.ts#it"],
    ["run", "indirect", "src/app.ts#check.run"],
    ["run", "indirect", "src/app.ts#check.run"],
    ["toBe", "virtual", "ext:../vendor/api.d.ts#Assertion.toBe"],
    ["toEqual", "virtual", "ext:../vendor/api.d.ts#Assertion.toEqual"],
  ]);
});

test("member access reaches object type aliases and inline object types, not only classes", () => {
  const dir = tempDir("members");
  writeProject(dir, {
    "src/m.ts": [
      "export type Config = { host: string; port: number; debug: boolean };",
      "export interface Named { name: string }",
      "export function connect(c: Config, n: Named, o: { retries: number }): string {",
      "  return `${c.host}:${c.port} ${n.name} ${o.retries}`;",
      "}",
      "",
    ].join("\n"),
  });
  const { tables } = extract(dir, { layers: ["refs"] });
  assert.deepEqual(
    tables.rows("member_access").map((m) => [m.member, m.owner, m.mode]),
    [
      ["src/m.ts#Config.host", "src/m.ts#Config", "read"],
      ["src/m.ts#Config.port", "src/m.ts#Config", "read"],
      ["src/m.ts#Named.name", "src/m.ts#Named", "read"],
      ["src/m.ts#connect.<type@3:49>.retries", "src/m.ts#connect.<type@3:49>", "read"],
    ],
  );
  fs.rmSync(dir, { recursive: true, force: true });
});
