// The Java frontend's refs layer, on test/fixtures/java-refs: plain sources with
// an abstract class, an interface with a default method, an anonymous class,
// lambdas, method references, a throws clause, and a file javac cannot resolve.

import assert from "node:assert/strict";
import { test } from "node:test";
import { extractJava, fixture, javaAvailable, tempDir } from "./helpers.ts";

const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";
const refs = NO_JAVA === false ? extractJava(fixture("java-refs"), { out: tempDir("java-refs"), layers: ["refs"] }) : undefined;
const rows = (rel: string) => refs?.tables.rows(rel) ?? [];

const USE = "app/Use.java#Use";
const BASE = "shapes/Base.java#Base";
const CIRCLE = "shapes/Circle.java#Circle";
const SHAPE = "shapes/Shape.java#Shape";
const ANON = `${USE}.make.<class@26:30>`;

function site(caller: string, name: string) {
  const s = rows("call_site").find((c) => c.caller === caller && c.callee_name === name);
  assert.ok(s !== undefined, `no call from ${caller} to ${name}`);
  return s;
}

test("calls: static and constructor calls are static; an instance method is virtual, at the declaration javac resolved", { skip: NO_JAVA }, () => {
  assert.deepEqual([site(`${USE}.make`, "unit").callee, site(`${USE}.make`, "unit").dispatch], [`${BASE}.unit`, "static"]);
  assert.deepEqual([site(`${USE}.total`, "area").callee, site(`${USE}.total`, "area").dispatch], [`${SHAPE}.area`, "virtual"]);
  // An inherited default method, called on `this` implicitly.
  assert.deepEqual([site(`${BASE}.describe`, "name").callee, site(`${BASE}.describe`, "name").dispatch], [`${SHAPE}.name`, "virtual"]);
  assert.deepEqual([site(`${USE}.total`, "forEach").callee, site(`${USE}.total`, "forEach").dispatch], ["ext:java.lang#Iterable.forEach", "virtual"]);
  // A lambda's calls are the lambda's.
  assert.equal(site(`${USE}.total.<lambda@17:19>`, "area").callee, `${SHAPE}.area`);
  const ids = rows("call_site").map((c) => c.id);
  assert.equal(new Set(ids).size, ids.length);
});

test("new names the constructor, or the class when it declares none; super(…) and this(…) the constructors they run", { skip: NO_JAVA }, () => {
  assert.deepEqual([site(`${USE}.make`, "Circle").callee, site(`${USE}.make`, "Circle").kind, site(`${USE}.make`, "Circle").args], [`${CIRCLE}.constructor`, "new", 1]);
  assert.equal(site(`${USE}.make`, "Plain").callee, "shapes/Plain.java#Plain");
  assert.equal(site(`${USE}.make`, "Shape").callee, ANON);
  assert.deepEqual(
    rows("call_site").filter((c) => c.callee_name === "super" || c.callee_name === "this").map((c) => [c.caller, c.callee, c.kind, c.dispatch]),
    [
      [`${CIRCLE}.constructor`, BASE, "super", "static"],
      [`${CIRCLE}.constructor@11`, `${CIRCLE}.constructor`, "call", "static"],
    ],
  );
});

test("what the compiler writes is no one's: a default constructor makes no call site and has no type", { skip: NO_JAVA }, () => {
  assert.ok(!rows("call_site").some((c) => [USE, BASE, ANON, "shapes/Plain.java#Plain"].includes(String(c.caller))));
  assert.ok(!rows("symbol_type").some((t) => t.symbol === ANON));
});

test("hierarchy: the supertypes written, and each method's overridden members in its direct supertypes, Object's included", { skip: NO_JAVA }, () => {
  assert.deepEqual(rows("extends").map((e) => [e.child, e.parent]), [[CIRCLE, BASE]]);
  assert.deepEqual(rows("implements").map((i) => [i.class, i.interface, i.pointer]), [
    [ANON, SHAPE, null],
    [BASE, SHAPE, null],
  ]);
  assert.deepEqual(rows("overrides").map((o) => [o.member, o.base]), [
    [`${ANON}.area`, `${SHAPE}.area`],
    [`${BASE}.area`, `${SHAPE}.area`],
    [`${BASE}.toString`, "ext:java.lang#Object.toString"],
    [`${CIRCLE}.area`, `${BASE}.area`],
  ]);
});

test("ref kinds: read, write, readwrite, call, new, type, extends, implements, decorator, value", { skip: NO_JAVA }, () => {
  const kindsTo = (from: string, to: string) => rows("ref").filter((r) => r.from === from && r.to === to).map((r) => r.kind);
  assert.deepEqual(kindsTo(`${CIRCLE}.constructor`, `${CIRCLE}.radius`), ["write"]);
  assert.deepEqual(kindsTo(`${CIRCLE}.grow`, `${CIRCLE}.radius`), ["readwrite"]);
  assert.deepEqual(kindsTo(`${CIRCLE}.area`, `${CIRCLE}.radius`), ["read"]);
  assert.deepEqual(kindsTo(CIRCLE, BASE), ["extends"]);
  assert.deepEqual(kindsTo(BASE, SHAPE), ["implements"]);
  assert.deepEqual(kindsTo(`${CIRCLE}.area`, "ext:java.lang#Override"), ["decorator"]);
  // `Circle::new` is a value: the constructor handed to a Supplier.
  assert.deepEqual(kindsTo(`${USE}.make`, `${CIRCLE}.constructor@11`), ["value"]);
  assert.ok(kindsTo(`${USE}.make`, CIRCLE).includes("new") && kindsTo(`${USE}.make`, CIRCLE).includes("type"));
  const symbols = new Map(rows("symbol").map((s) => [s.id, s]));
  for (const r of rows("ref")) {
    const kind = symbols.get(r.to)?.kind;
    assert.ok(kind !== undefined && !["local", "parameter", "type_parameter"].includes(String(kind)), `${r.to} is a ${kind}`);
  }
});

test("member access: its mode, and via_this through `this.`, `super.` or an unqualified instance member", { skip: NO_JAVA }, () => {
  assert.deepEqual(
    rows("member_access").filter((m) => m.member === `${CIRCLE}.radius`).map((m) => [m.fn, m.mode, m.via_this]),
    [
      [`${USE}.make`, "write", false],
      [`${CIRCLE}.constructor`, "write", true],
      [`${CIRCLE}.area`, "read", true],
      [`${CIRCLE}.grow`, "readwrite", true],
    ],
  );
  const access = (fn: string, member: string) => rows("member_access").find((m) => m.fn === fn && m.member === member);
  // An inherited field, unqualified.
  assert.deepEqual([access(`${CIRCLE}.grow`, `${BASE}.scale`)?.owner, access(`${CIRCLE}.grow`, `${BASE}.scale`)?.via_this], [BASE, true]);
  // A static method, through its class.
  assert.deepEqual([access(`${USE}.make`, `${BASE}.unit`)?.mode, access(`${USE}.make`, `${BASE}.unit`)?.via_this], ["call", false]);
});

test("type_ref records where a type is written", { skip: NO_JAVA }, () => {
  const pos = (from: string, to: string) => [...new Set(rows("type_ref").filter((r) => r.from === from && r.to === to).map((r) => r.position))];
  assert.deepEqual(pos(`${USE}.shapes`, SHAPE), ["property"]);
  assert.deepEqual(pos(`${USE}.total`, SHAPE), ["param", "variable"]);
  assert.deepEqual(pos(`${USE}.total.<lambda@17:19>`, SHAPE), ["param"]);
  assert.deepEqual(pos(`${USE}.make`, BASE), ["return", "other"]);
  assert.deepEqual(pos(`${USE}.make`, CIRCLE), ["variable", "other", "assertion"]);
});

test("throws_decl: what a method declares it throws, checked or not", { skip: NO_JAVA }, () => {
  assert.deepEqual(rows("throws_decl").map((t) => [t.fn, t.type]), [
    [`${USE}.make`, "ext:java.io#IOException"],
    [`${USE}.make`, "ext:java.lang#IllegalStateException"],
  ]);
});

test("symbol_type: javac's type; a method's is its signature, and a functional interface's value is a function", { skip: NO_JAVA }, () => {
  const ty = new Map(rows("symbol_type").map((t) => [t.symbol, t]));
  assert.deepEqual([ty.get(`${CIRCLE}.radius`)?.text, ty.get(`${CIRCLE}.radius`)?.is_function], ["double", false]);
  assert.deepEqual([ty.get(`${USE}.total`)?.text, ty.get(`${USE}.total`)?.is_function], ["(java.util.List<shapes.Shape>)double", true]);
  assert.deepEqual([ty.get(`${USE}.shapes`)?.text, ty.get(`${USE}.shapes`)?.is_function], ["java.util.List<shapes.Shape>", false]);
  assert.deepEqual([ty.get(`${USE}.make.fresh`)?.text, ty.get(`${USE}.make.fresh`)?.is_function], ["java.util.function.Supplier<shapes.Circle>", true]);
});

test("unresolved_ref: names javac could not resolve, but not members of what it could not type", { skip: NO_JAVA }, () => {
  assert.deepEqual(rows("unresolved_ref").map((u) => [u.from, u.name, u.kind, u.line]), [
    ["app/Broken.java#Broken.missing", "Missing", "type", 4],
    ["app/Broken.java#Broken.run", "undefined", "call", 8],
  ]);
  assert.deepEqual(
    rows("call_site").filter((c) => c.caller === "app/Broken.java#Broken.run").map((c) => [c.callee_name, c.callee, c.dispatch]),
    [
      ["go", null, "unresolved"],
      ["undefined", null, "unresolved"],
    ],
  );
});
