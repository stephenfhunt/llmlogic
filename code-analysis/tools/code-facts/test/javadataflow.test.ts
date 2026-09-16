// The Java frontend's dataflow layer, on test/fixtures/java-dataflow: fields
// read off `this` and off the class, arrays, records built and read back,
// lambdas and method references as values, type and record patterns, and the
// call through a functional interface that points-to resolves to the lambda.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import { datalog, engineAvailable, extractJava, fixture, javaAvailable, tempDir } from "./helpers.ts";

const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";
const out = tempDir("java-dataflow");
const df = NO_JAVA === false ? extractJava(fixture("java-dataflow"), { out, layers: ["refs", "flow", "dataflow"] }) : undefined;
const rows = (rel: string) => df?.tables.rows(rel) ?? [];
const B = "df/Box.java#Box";
const S = "df/Shapes.java#Shapes";
const LEAF = "df/Leaf.java#Leaf";

/** A relation's rows with the fixture's id prefixes stripped, for readable assertions. */
function short(v: unknown): unknown {
  return typeof v === "string" ? v.replace(`${B}.`, "Box.").replace(`${S}.`, "Shapes.").replace(`${LEAF}.`, "Leaf.") : v;
}

test("alloc: an instance per `new`, an array per array, a cell per class holding statics, a function per lambda and method reference", { skip: NO_JAVA }, () => {
  assert.deepEqual(
    rows("alloc").map((a) => [short(a.var), a.kind, short(a.type), short(a.fn_target), a.file, a.line]),
    [
      [B, "cell", null, null, "df/Box.java", 6],
      ["Box.SHARED$t1", "instance", B, null, "df/Box.java", 7],
      ["Box.SHARED$t2", "instance", LEAF, null, "df/Box.java", 7],
      ["Box.slots$t1", "array", null, null, "df/Box.java", 9],
      ["Box.viaLocal$t1", "instance", LEAF, null, "df/Box.java", 23],
      ["Box.viaLocal$t2", "instance", B, null, "df/Box.java", 24],
      ["Box.throughLambda$t1", "function", null, "Box.throughLambda.<lambda@35:34>", "df/Box.java", 35],
      ["Box.throughLambda.<lambda@35:34>$t1", "instance", LEAF, null, "df/Box.java", 35],
      ["Box.reference$t1", "function", null, "Box.identity", "df/Box.java", 40],
      ["Box.arrayOf$t1", "array", null, null, "df/Box.java", 48],
      ["Shapes.Kind", "cell", null, null, "df/Shapes.java", 9],
      ["Shapes.Kind.ONE$t1", "instance", "Shapes.Kind", null, "df/Shapes.java", 10],
      ["Shapes.Kind.TWO$t1", "instance", "Shapes.Kind", null, "df/Shapes.java", 11],
      ["Shapes.build$t1", "instance", "Shapes.Pair", null, "df/Shapes.java", 19],
      ["Shapes.records$t1", "instance", "Shapes.Pair", null, "df/Shapes.java", 24],
      ["Shapes.records$t2", "instance", LEAF, null, "df/Shapes.java", 24],
      ["Shapes.records$t3", "instance", LEAF, null, "df/Shapes.java", 24],
      ["Shapes.caught$t1", "function", null, "Shapes.caught.<lambda@54:28>", "df/Shapes.java", 54],
      ["Shapes.caught$t2", "instance", "ext:java.lang#IllegalStateException", null, "df/Shapes.java", 56],
    ],
  );
  // Every site is its own: the id-space is one across frontends.
  assert.equal(new Set(rows("alloc").map((a) => a.site)).size, rows("alloc").length);
});

test("var: a named variable is its symbol id, `this` belongs to its class, and a class holding statics is a variable", { skip: NO_JAVA }, () => {
  const kinds: Record<string, number> = {};
  for (const v of rows("var")) kinds[String(v.kind)] = (kinds[String(v.kind)] ?? 0) + 1;
  assert.deepEqual(kinds, { module: 2, temp: 48, this: 2, param: 20, local: 19, ret: 18, thrown: 1, catch: 1 });

  // `this` is the class's, which is what points-to binds a receiver to.
  assert.deepEqual(
    rows("var").filter((v) => v.kind === "this"),
    [{ id: `${B}$this`, fn: B, kind: "this" }, { id: `${LEAF}$this`, fn: LEAF, kind: "this" }],
  );
  // A class is a variable only where its static state lives.
  assert.deepEqual(rows("var").filter((v) => v.kind === "module").map((v) => v.id), [B, `${S}.Kind`]);
  // A local declared in a lambda belongs to the lambda, not the method around it.
  assert.deepEqual(rows("var").find((v) => v.id === `${B}.roundTrip.out`), { id: `${B}.roundTrip.out`, fn: `${B}.roundTrip`, kind: "local" });
});

test("fields: a simple name loads from `this`, a static from its class, an array element from `[]`", { skip: NO_JAVA }, () => {
  const box = rows("load").filter((l) => String(l.fn).startsWith(`${B}.`)).map((l) => [short(l.to), short(l.base), l.field]);
  assert.deepEqual(box, [
    ["Box.roundTrip$t1", `${B}$this`, "held"],
    ["Box.roundTrip$t2", `${B}$this`, "slots"],
    ["Box.roundTrip$t3", `${B}$this`, "slots"],
    ["Box.roundTrip$t4", "Box.roundTrip$t3", "[]"],
    ["Box.viaStatic$t1", B, "SHARED"],
    ["Box.viaStatic$t2", "Box.viaStatic$t1", "held"],
    ["Box.viaStatic$t3", B, "SHARED"],
    ["Box.arrayOf$t2", "Box.arrayOf.both", "[]"],
    ["Box.arrayOf$t3", "Box.arrayOf.both", "[]"],
  ]);
  const stores = rows("store").filter((s) => String(s.fn).startsWith(`${B}.`)).map((s) => [short(s.base), s.field, short(s.from)]);
  assert.deepEqual(stores, [
    [B, "SHARED", "Box.SHARED$t1"],
    [`${B}$this`, "slots", "Box.slots$t1"],
    [`${B}$this`, "held", "Box.constructor.initial"],
    [`${B}$this`, "held", "Box.roundTrip.in"],
    ["Box.roundTrip$t2", "[]", "Box.roundTrip.out"],
    ["Box.viaStatic$t3", "held", "Box.viaStatic.s"],
    ["Box.arrayOf$t1", "[]", "Box.arrayOf.a"],
    ["Box.arrayOf$t1", "[]", "Box.arrayOf.b"],
    [`${B}$this`, "held", "Box.arrayOf.one"],
  ]);
});

test("records: a canonical constructor the compiler wrote stores each component, and its accessors load them back", { skip: NO_JAVA }, () => {
  assert.deepEqual(
    rows("store").filter((s) => String(s.base).includes("build$t1") || String(s.base).includes("records$t1")).map((s) => [short(s.base), s.field, short(s.from)]),
    [
      ["Shapes.build$t1", "left", "Shapes.build.a"],
      ["Shapes.build$t1", "right", "Shapes.build.b"],
      ["Shapes.records$t1", "left", "Shapes.records$t2"],
      ["Shapes.records$t1", "right", "Shapes.records$t3"],
    ],
  );
  assert.deepEqual(
    rows("load").filter((l) => l.field === "left" || l.field === "right").map((l) => [short(l.to), short(l.base), l.field]),
    [
      ["Shapes.component$t1", "Shapes.component.p", "left"],
      ["Shapes.build$t2", "Shapes.build.p", "right"],
      ["Shapes.recordPattern$t3", "Shapes.recordPattern$t2", "left"],
      ["Shapes.recordPattern$t4", "Shapes.recordPattern$t2", "right"],
    ],
  );
});

test("patterns: a type pattern binds by copy, a record pattern by a load per component", { skip: NO_JAVA }, () => {
  assert.deepEqual(
    rows("assign").filter((a) => String(a.to).startsWith(`${S}.`) && String(a.to).endsWith(".leaf")).map((a) => [short(a.to), short(a.from), a.kind]),
    [
      ["Shapes.typePattern.leaf", "Shapes.typePattern.v", "copy"],
      ["Shapes.recordPattern.leaf", "Shapes.recordPattern.v", "copy"],
    ],
  );
  // `case Nested(Pair(Object l, Object r), Object tail)` walks in.
  assert.deepEqual(
    rows("load").filter((l) => String(l.fn) === `${S}.recordPattern`).map((l) => [short(l.base), l.field]),
    [["Shapes.recordPattern.v", "inner"], ["Shapes.recordPattern$t2", "left"], ["Shapes.recordPattern$t2", "right"], ["Shapes.recordPattern.v", "tail"]],
  );
});

test("calls: a receiver per instance call, `callee_var` only where a functional interface's own method runs", { skip: NO_JAVA }, () => {
  assert.deepEqual(rows("callee_var").map((c) => short(c.var)), ["Box.throughLambda.wrap"]);
  // A constructor takes the object it constructs as its receiver, so `this_var` binds it.
  assert.deepEqual(rows("this_var").map((t) => [short(t.fn), short(t.var)]), [
    ["Box.constructor", `${B}$this`],
    ["Box.roundTrip", `${B}$this`],
    ["Box.viaLocal", `${B}$this`],
    ["Box.throughLambda", `${B}$this`],
    ["Box.reference", `${B}$this`],
    ["Box.arrayOf", `${B}$this`],
    ["Leaf.constructor", `${LEAF}$this`],
    ["Leaf.tag@11", `${LEAF}$this`],
  ]);
  // Arguments and results join at the call site the refs layer numbered.
  const sites = new Set(rows("call_site").map((c) => c.id));
  for (const rel of ["actual", "actual_ret", "receiver", "callee_var"]) {
    for (const r of rows(rel)) assert.ok(sites.has(r.call_site), `${rel} names call site ${r.call_site}, which does not exist`);
  }
});

test("points-to resolves a call through a functional interface, a record's components, and a static field", { skip: NO_JAVA || !engineAvailable() ? "needs a JDK and the datalog engine" : false }, () => {
  const q = path.join(out, "pt.dl");
  fs.writeFileSync(q, 'import "lib/pointsto.dl".\n');
  const r = datalog(q, [
    `lambda(O) :- pts("${B}.throughLambda$t2", O)`,
    `lambdaTarget(F) :- target(CS, F), callee_var(call_site: CS)`,
    `record(O) :- pts("${S}.records$ret", O)`,
    `statics(O) :- pts("${B}.viaStatic$t2", O)`,
  ]);
  assert.ok(r.code === 0 || r.code === 1, r.stderr);
  const site = (held: string): number => {
    const a = rows("alloc").find((x) => x.var === held);
    assert.ok(a !== undefined, `nothing is allocated into ${held}`);
    return a.site as number;
  };
  // The lambda's own `new Leaf(…)` comes back out of `wrap.apply(seed)`.
  assert.match(r.stdout, new RegExp(`^lambda\\(${site(`${B}.throughLambda.<lambda@35:34>$t1`)}\\)\\.$`, "m"));
  assert.match(r.stdout, new RegExp(`^lambdaTarget\\("${B}\\.throughLambda\\.<lambda@35:34>"\\)\\.$`, "m"));
  // `component(p)` returns `p.left()`: the first `new Leaf(…)` of `records`.
  assert.match(r.stdout, new RegExp(`^record\\(${site(`${S}.records$t2`)}\\)\\.$`, "m"));
  // `SHARED.held` reaches the object the static field's initializer put there.
  assert.match(r.stdout, new RegExp(`^statics\\(${site(`${B}.SHARED$t2`)}\\)\\.$`, "m"));
});
