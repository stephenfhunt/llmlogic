// The rule library over fixture facts, against expectations worked by hand
// from the fixture source.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import { datalog, engineAvailable, extract, fixture, tempDir } from "./helpers.ts";

const skip = !engineAvailable();

function outOf(name: string): string {
  const out = tempDir(`lib-${name}`);
  extract(fixture(name), { out, layers: ["refs", "flow", "dataflow", "quality"] });
  return out;
}

/** Answers to one query, as printed lines. */
function ask(out: string, lib: string, query: string): string[] {
  const r = datalog(path.join(out, "lib", lib), [query]);
  assert.ok(r.code === 0 || r.code === 1, `${lib} ${query}: exit ${r.code}\n${r.stderr}`);
  return r.stdout.split("\n").filter((l) => l !== "");
}

test("callgraph: a virtual call reaches every override of its declared target", { skip }, () => {
  const out = outOf("refs");
  assert.deepEqual(ask(out, "callgraph.dl", 'handler_calls(B) :- call_edge("src/use.ts#Registry.handler", B)'), [
    'handler_calls("src/shapes.ts#Base.area").',
    'handler_calls("src/shapes.ts#Circle.area").',
    'handler_calls("src/shapes.ts#Shape.area").',
  ]);
  assert.deepEqual(ask(out, "callgraph.dl", 'declared(B) :- call_edge_declared("src/use.ts#Registry.handler", B)'), [
    'declared("src/shapes.ts#Shape.area").',
  ]);
  // main's callback calls Shape.area; lexically, main does too.
  assert.ok(ask(out, "callgraph.dl", 'm(B) :- call_edge_lexical("src/use.ts#main", B)').includes('m("src/shapes.ts#Circle.area").'));
  assert.ok(!ask(out, "callgraph.dl", 'm(B) :- call_edge("src/use.ts#main", B)').includes('m("src/shapes.ts#Circle.area").'));
});

test("callreach: reachability and (the absence of) recursion", { skip }, () => {
  const out = outOf("refs");
  const fromCreate = ask(out, "callreach.dl", 'r(B) :- reaches("src/shapes.ts#Base.create", B)');
  assert.deepEqual(fromCreate, ['r("src/shapes.ts#Base").', 'r("src/shapes.ts#Circle.constructor").']);
  assert.deepEqual(ask(out, "callreach.dl", "recursive(F)"), []);
});

test("modgraph: file edges by kind, roll-up to directories, external packages", { skip }, () => {
  const out = outOf("basic");
  assert.deepEqual(ask(out, "modgraph.dl", "file_dep(A, B, K)"), [
    'file_dep("src/main.ts", "src/util/index.ts", static).',
    'file_dep("src/main.ts", "src/util/math.ts", dynamic).',
    'file_dep("src/main.ts", "src/util/math.ts", static).',
    'file_dep("src/util/index.ts", "src/util/math.ts", reexport).',
    'file_dep("src/util/index.ts", "src/util/math.ts", reexport_all).',
  ]);
  assert.deepEqual(ask(out, "modgraph.dl", "in_cycle(F)"), []);
  // At depth 1 everything is in `src`, so no edge crosses a unit boundary; at
  // depth 2 only src/util/* have a unit at all.
  assert.deepEqual(ask(out, "modgraph.dl", "unit_dep(1, A, B)"), []);
  assert.deepEqual(ask(out, "modgraph.dl", "external_dep(F, P)"), ['external_dep("src/main.ts", "node:path").']);
});

test("coupling: Martin's metrics per file, CBO per type", { skip }, () => {
  const out = outOf("refs");
  // use.ts depends on shapes.ts and nothing depends on it; view.tsx is isolated.
  assert.deepEqual(ask(out, "coupling.dl", "instability(-1, C, I)"), [
    'instability(-1, "src/shapes.ts", 0.0).',
    'instability(-1, "src/use.ts", 1.0).',
  ]);
  // shapes.ts: Shape and abstract Base over Shape, Base, Circle, Plain.
  assert.ok(ask(out, "coupling.dl", "abstractness(-1, C, A)").includes('abstractness(-1, "src/shapes.ts", 0.5).'));
  assert.deepEqual(ask(out, "coupling.dl", "distance(-1, C, D)"), [
    'distance(-1, "src/shapes.ts", 0.5).',
    'distance(-1, "src/use.ts", 0.0).',
  ]);
  assert.deepEqual(ask(out, "coupling.dl", "sdp_violation(G, A, B)"), []);
  // Base ↔ Shape (implements), Base ↔ Circle (new / extends); Shape ↔ Registry (types).
  const cbo = ask(out, "coupling.dl", "cbo(T, N)");
  assert.ok(cbo.includes('cbo("src/shapes.ts#Base", 2).'));
  assert.ok(cbo.includes('cbo("src/shapes.ts#Shape", 2).'));
  assert.ok(cbo.includes('cbo("src/shapes.ts#Circle", 1).'));
  assert.ok(cbo.includes('cbo("src/use.ts#Registry", 1).'));
});

test("cohesion: LCOM4, TCC, LCOM-HS and relational cohesion", { skip }, () => {
  const out = outOf("refs");
  // Base: describe calls this.area and reads the getter this.name — one component.
  // Registry: add touches this.items, handler touches nothing — two.
  assert.deepEqual(ask(out, "cohesion.dl", "lcom4(C, N)"), [
    'lcom4("src/shapes.ts#Base", 1).',
    'lcom4("src/shapes.ts#Circle", 1).',
    'lcom4("src/shapes.ts#Plain", 0).',
    'lcom4("src/use.ts#Registry", 2).',
  ]);
  assert.deepEqual(ask(out, "cohesion.dl", "tcc(C, T)"), [
    'tcc("src/shapes.ts#Base", 0.0).',
    'tcc("src/shapes.ts#Circle", 1.0).',
    'tcc("src/use.ts#Registry", 0.0).',
  ]);
  // Circle: both methods touch its one field → 0; Registry: one of two → 1.
  assert.deepEqual(ask(out, "cohesion.dl", "lcom_hs(C, L)"), [
    'lcom_hs("src/shapes.ts#Circle", 0.0).',
    'lcom_hs("src/use.ts#Registry", 1.0).',
  ]);
  // shapes.ts: 4 types, 3 internal type references → (3 + 1) / 4.
  assert.ok(ask(out, "cohesion.dl", "relational_cohesion(-1, U, H)").includes('relational_cohesion(-1, "src/shapes.ts", 1.0).'));
});

test("flow: dead code, dead stores, def-use chains, loops", { skip }, () => {
  const out = outOf("flow");
  assert.deepEqual(ask(out, "flow.dl", "u(F, L) :- unreachable(F, N, L)"), ['u("src/flow.ts#dead", 74).']);
  // branches: `let x = 0` is overwritten on every path; stores: `a = 1` likewise;
  // guarded: the catch binding is never read.
  assert.deepEqual(ask(out, "flow.dl", "d(V, L) :- dead_store(D, V, L)"), [
    'd("src/flow.ts#branches.x", 2).',
    'd("src/flow.ts#guarded.e", 46).',
    'd("src/flow.ts#stores.a", 78).',
  ]);
  assert.deepEqual(ask(out, "flow.dl", "undefined_use(N, V)"), []);
  // `return b` in stores is reached by both branch assignments.
  assert.deepEqual(ask(out, "flow.dl", 'r(L) :- def_use(D, N, "src/flow.ts#stores.b"), flow_node(id: D, line: L)'), ["r(81).", "r(82)."]);
  assert.deepEqual(ask(out, "dominators.dl", 'h(L) :- loop_header("src/flow.ts#loops", H), flow_node(id: H, line: L)'), [
    "h(15).",
    "h(19).",
    "h(23).",
    "h(24).",
  ]);
});

test("metrics: DIT, NOC, WMC, RFC, fan-in and fan-out", { skip }, () => {
  const out = outOf("refs");
  assert.ok(ask(out, "metrics.dl", "dit(C, N)").includes('dit("src/shapes.ts#Circle", 1).'));
  assert.ok(ask(out, "metrics.dl", "dit(C, N)").includes('dit("src/shapes.ts#Base", 0).'));
  assert.ok(ask(out, "metrics.dl", "noc(C, N)").includes('noc("src/shapes.ts#Base", 1).'));
  // Circle: constructor, area and grow, each cyclomatic 1.
  assert.ok(ask(out, "metrics.dl", "wmc(C, W)").includes('wmc("src/shapes.ts#Circle", 3).'));
  // …plus Base (the super call's target) in its response set.
  assert.ok(ask(out, "metrics.dl", "rfc(C, N)").includes('rfc("src/shapes.ts#Circle", 4).'));
  assert.ok(ask(out, "metrics.dl", "fan_in(F, N)").includes('fan_in("src/use.ts#total", 1).'));
});

test("pointsto: an indirect call resolves to the callback passed in", { skip }, () => {
  const out = outOf("refs");
  // total(shapes, each) calls each(s); main passes an arrow — the only target.
  assert.deepEqual(ask(out, "pointsto.dl", 'e(F) :- call_edge_pt("src/use.ts#total", F)'), ['e("src/use.ts#main.<arrow@32:14>").']);
  assert.deepEqual(ask(out, "pointsto.dl", "unresolved_call(CS)"), []);
  // The Circle made in main reaches Registry.add's parameter through r.add(c).
  assert.deepEqual(
    ask(out, "pointsto.dl", 't(T) :- pts("src/use.ts#Registry.add.s", O), alloc(site: O, type: T)'),
    ['t("src/shapes.ts#Circle").'],
  );
});

test("taint: a value followed from a source through calls and fields to a sink", { skip }, () => {
  const out = outOf("refs");
  const q = path.join(out, "taint-q.dl");
  // Source: the radius main passes to new Circle(2)'s argument slot is not a
  // variable, so seed from the Circle object itself: `c` in main.
  fs.writeFileSync(
    q,
    [
      'import "lib/taint.dl".',
      'source("src/use.ts#main.c").',
      'sink(V) :- formal(fn: "src/use.ts#Registry.add", index: 0, var: V).',
      "",
    ].join("\n"),
  );
  const r = datalog(q, ["tainted_sink(V)"]);
  assert.equal(r.code, 0, r.stderr);
  assert.equal(r.stdout.trim(), 'tainted_sink("src/use.ts#Registry.add.s").');
});
