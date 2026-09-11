import assert from "node:assert/strict";
import { test } from "node:test";
import { extract, fixture } from "./helpers.ts";

const flow = extract(fixture("flow"), { layers: ["refs", "flow"] });
const rows = (rel: string) => flow.tables.rows(rel);
const fnRow = (name: string) => rows("fn").find((f) => f.id === `src/flow.ts#${name}`);
const nodesOf = (name: string) => rows("flow_node").filter((n) => n.fn === `src/flow.ts#${name}`);
const edgesFrom = (id: unknown) => rows("flow_edge").filter((e) => e.from === id);

test("cyclomatic complexity counts ESLint's decision points", () => {
  const cyc = Object.fromEntries(rows("fn").map((f) => [f.id, f.cyclomatic]));
  assert.deepEqual(cyc, {
    "src/flow.ts#<module>": 1,
    "src/flow.ts#branches": 3, // if, else-if
    "src/flow.ts#loops": 8, // for, if, ??, for-of, if, while, do
    "src/flow.ts#sw": 4, // three cases
    "src/flow.ts#guarded": 2, // catch
    "src/flow.ts#logic": 6, // ?. ?? && ?: and a default parameter
    "src/flow.ts#later": 1,
    "src/flow.ts#gen": 1,
    "src/flow.ts#outer": 1,
    "src/flow.ts#outer.<arrow@68:10>": 1,
    "src/flow.ts#dead": 1,
    "src/flow.ts#stores": 2,
  });
  assert.deepEqual(
    rows("decision").filter((d) => d.fn === "src/flow.ts#logic").map((d) => d.kind).sort(),
    ["and", "conditional", "default_value", "nullish", "optional_chain"],
  );
});

test("cognitive complexity and nesting follow SonarSource's rules", () => {
  assert.equal(fnRow("branches")?.cognitive, 3); // if, else if, else
  assert.equal(fnRow("branches")?.max_nesting, 1); // an else-if is not deeper
  assert.equal(fnRow("loops")?.cognitive, 9); // for 1, nested if 2, ?? 1, for-of 1, nested if 2, while 1, do 1
  assert.equal(fnRow("loops")?.max_nesting, 2);
  assert.equal(fnRow("logic")?.cognitive, 3);
});

test("counts: statements, returns, awaits, yields", () => {
  assert.deepEqual([fnRow("sw")?.returns, fnRow("later")?.awaits, fnRow("gen")?.yields], [3, 1, 1]);
  assert.equal(fnRow("sw")?.params, 1);
  assert.equal(fnRow("<module>")?.statements, 0, "the module's only statements are function declarations");
});

test("a finally is entered from the return and the exception, and re-issues both", () => {
  const nodes = nodesOf("guarded");
  const kind = (k: string) => nodes.filter((n) => n.kind === k).map((n) => n.id);
  const [fin] = kind("finally");
  const [cat] = kind("catch");
  const [exit] = kind("exit");
  const [throwExit] = kind("throw_exit");
  // The try body's call may throw into the catch.
  const tryCall = nodes.find((n) => n.kind === "stmt" && n.line === 44);
  assert.ok(edgesFrom(tryCall?.id).some((e) => e.to === cat && e.kind === "throw"));
  // Both returns go through the finally.
  for (const r of kind("return")) assert.ok(edgesFrom(r).some((e) => e.to === fin && e.kind === "return"));
  // The finally's own call continues to the exit (a return was pending) and to
  // throw_exit (an exception from the catch block was).
  const finCall = nodes.find((n) => n.kind === "stmt" && n.line === 49);
  assert.deepEqual(
    edgesFrom(finCall?.id).map((e) => [e.to, e.kind]).sort(),
    [[exit, "return"], [throwExit, "throw"]].sort(),
  );
});

test("a switch tests its cases in order and falls through empty ones", () => {
  const nodes = nodesOf("sw");
  const tests = nodes.filter((n) => n.kind === "case_test").map((n) => n.id as number);
  assert.equal(tests.length, 3);
  // case 0 → return "zero"; its on_false → case 1; case 1 falls into case 2's body.
  const [t0, t1, t2] = tests;
  assert.ok(edgesFrom(t0).some((e) => e.to === t1 && e.kind === "on_false"));
  assert.ok(edgesFrom(t1).some((e) => e.to === t2 && e.kind === "on_false"));
  const small = nodes.find((n) => n.kind === "return" && n.line === 36)?.id;
  assert.ok(edgesFrom(t1).some((e) => e.to === small && e.kind === "case"), "an empty case falls through");
  const big = nodes.find((n) => n.kind === "return" && n.line === 38)?.id;
  assert.ok(edgesFrom(t2).some((e) => e.to === big && e.kind === "default"));
});

test("def and use: a declaration without a value defines nothing; branches define", () => {
  const ids = new Set(nodesOf("stores").map((n) => n.id));
  const line = new Map(nodesOf("stores").map((n) => [n.id, n.line]));
  const defs = rows("def").filter((d) => ids.has(d.node) && d.var === "src/flow.ts#stores.b").map((d) => line.get(d.node));
  assert.deepEqual(defs, [81, 82]);
  const aDefs = rows("def").filter((d) => ids.has(d.node) && d.var === "src/flow.ts#stores.a").map((d) => line.get(d.node));
  assert.deepEqual(aDefs, [78, 79]);
  const params = rows("def").filter((d) => d.var === "src/flow.ts#stores.flag");
  assert.equal(line.get(params[0]?.node), 77, "a parameter is defined at the entry");
});

test("closures: creation site, captured variable, suspension points", () => {
  const arrow = "src/flow.ts#outer.<arrow@68:10>";
  assert.deepEqual(rows("captures"), [{ fn: arrow, var: "src/flow.ts#outer.n" }]);
  const outerIds = new Set(nodesOf("outer").map((n) => n.id));
  assert.ok(rows("closure").some((c) => c.fn === arrow && outerIds.has(c.node)));
  const laterIds = new Set(nodesOf("later").map((n) => n.id));
  assert.equal(rows("await_at").filter((a) => laterIds.has(a.node)).length, 1);
  assert.equal(rows("yield_at").length, 1);
});

test("call_at ties each call site to the function that executes it", () => {
  const sites = rows("call_site").filter((c) => c.caller === "src/flow.ts#guarded").map((c) => c.id);
  const ids = new Set(nodesOf("guarded").map((n) => n.id));
  for (const s of sites) assert.ok(rows("call_at").some((a) => a.call_site === s && ids.has(a.node)), `call site ${s}`);
});
