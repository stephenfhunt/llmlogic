// The rule library over fixture facts, against expectations worked by hand
// from the fixture source.

import assert from "node:assert/strict";
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
