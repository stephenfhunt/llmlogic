// The Java frontend's flow layer, on test/fixtures/java-flow: every statement form
// once — both switch forms, a switch expression, labelled jumps, try-with-resources
// with two catches and a finally, synchronized, and a lambda capturing locals.

import assert from "node:assert/strict";
import { test } from "node:test";
import { extractJava, fixture, javaAvailable, tempDir } from "./helpers.ts";

const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";
const flow = NO_JAVA === false ? extractJava(fixture("java-flow"), { out: tempDir("java-flow"), layers: ["refs", "flow"] }) : undefined;
const rows = (rel: string) => flow?.tables.rows(rel) ?? [];
const S = "flow/Shapes.java#Shapes";

/** A function's edges as `kind@line > kind@line : edge`, sorted. */
function edges(fn: string): string[] {
  const nodes = new Map(rows("flow_node").filter((n) => n.fn === fn).map((n) => [n.id, `${n.kind}@${n.line}`]));
  return rows("flow_edge")
    .filter((e) => nodes.has(e.from))
    .map((e) => `${nodes.get(e.from)} > ${nodes.get(e.to)} : ${e.kind}`)
    .sort();
}

test("fn: a row per method and lambda, with its metrics", { skip: NO_JAVA }, () => {
  assert.deepEqual(
    rows("fn").map((f) => [f.id, f.kind, f.params, f.statements, f.max_nesting, f.cyclomatic, f.cognitive, f.returns, f.throws]),
    [
      [`${S}.classify`, "method", 1, 14, 1, 9, 8, 3, 0],
      [`${S}.name`, "method", 1, 14, 1, 5, 2, 2, 0],
      [`${S}.read`, "method", 1, 22, 3, 7, 13, 1, 1],
      [`${S}.later`, "method", 1, 2, 0, 1, 0, 1, 0],
      [`${S}.later.<lambda@77:12>`, "lambda", 0, 0, 0, 1, 0, 0, 0],
    ],
  );
});

test("decisions: if, the loops, && and ||, ?:, each case and each catch", { skip: NO_JAVA }, () => {
  assert.deepEqual(
    rows("decision").map((d) => [String(d.fn).slice(S.length + 1), d.kind, d.line]),
    [
      ["classify", "if", 12],
      ["classify", "if", 14],
      ["classify", "for", 18],
      ["classify", "while", 21],
      ["classify", "do", 24],
      ["classify", "and", 14],
      ["classify", "or", 21],
      ["classify", "conditional", 27],
      ["name", "case", 32],
      ["name", "case", 34],
      ["name", "case", 40],
      ["name", "case", 41],
      ["read", "for_of", 53],
      ["read", "for_of", 54],
      ["read", "if", 55],
      ["read", "if", 56],
      ["read", "catch", 62],
      ["read", "catch", 64],
    ],
  );
});

test("switch: a colon case falls through, an arrow case leaves, a switch expression's yield goes to the statement it feeds", { skip: NO_JAVA }, () => {
  assert.deepEqual(edges(`${S}.name`), [
    "break@37 > switch@39 : break",
    // yield "large"
    "break@43 > stmt@39 : break",
    "case_test@32 > case_test@34 : on_false",
    "case_test@32 > stmt@33 : case",
    "case_test@34 > break@37 : default",
    "case_test@34 > return@35 : case",
    "case_test@40 > case_test@41 : on_false",
    "case_test@40 > stmt@40 : case",
    "case_test@41 > stmt@42 : case",
    "case_test@41 > stmt@45 : default",
    "entry@30 > switch@31 : next",
    "return@35 > exit@48 : return",
    "return@47 > exit@48 : return",
    "stmt@33 > return@35 : next",
    "stmt@39 > return@47 : next",
    "stmt@40 > stmt@39 : next",
    "stmt@42 > break@43 : next",
    "stmt@45 > stmt@39 : next",
    "switch@31 > case_test@32 : next",
    "switch@39 > case_test@40 : next",
  ]);
});

test("labelled jumps, try-with-resources, catches tested in order, finally, synchronized", { skip: NO_JAVA }, () => {
  assert.deepEqual(edges(`${S}.read`), [
    "break@56 > stmt@60 : break",
    // the second catch does not match: the exception goes on, through finally
    "catch@62 > catch@64 : on_false",
    "catch@62 > stmt@63 : on_true",
    "catch@64 > finally@66 : on_false",
    "catch@64 > throw@65 : on_true",
    "cond@55 > cond@56 : on_false",
    "cond@55 > continue@55 : on_true",
    "cond@56 > break@56 : on_true",
    "cond@56 > stmt@57 : on_false",
    "continue@55 > loop_head@53 : continue",
    "entry@50 > stmt@51 : next",
    // the resources close, then the catches, then the finally
    "finally@60 > catch@62 : throw",
    "finally@60 > finally@66 : next",
    "finally@66 > stmt@67 : next",
    // the monitor is released on every way out of the block
    "finally@69 > return@72 : next",
    "finally@69 > throw_exit@73 : throw",
    "loop_head@53 > stmt@54 : on_true",
    "loop_head@53 > stmt@60 : on_false",
    "loop_head@54 > cond@55 : on_true",
    "loop_head@54 > loop_head@53 : on_false",
    "return@72 > exit@73 : return",
    "stmt@51 > stmt@53 : next",
    "stmt@53 > loop_head@53 : next",
    "stmt@54 > loop_head@54 : next",
    "stmt@57 > loop_head@54 : back",
    "stmt@60 > catch@62 : throw",
    "stmt@60 > stmt@61 : next",
    "stmt@61 > finally@60 : next",
    "stmt@61 > finally@60 : throw",
    "stmt@63 > finally@66 : next",
    "stmt@63 > finally@66 : throw",
    "stmt@67 > stmt@69 : next",
    "stmt@67 > throw_exit@73 : throw",
    "stmt@69 > stmt@70 : next",
    "stmt@70 > finally@69 : next",
    "stmt@70 > finally@69 : throw",
    "throw@65 > finally@66 : throw",
  ]);
});

test("def and use of locals and parameters; a lambda captures, and is created where it is written", { skip: NO_JAVA }, () => {
  const nodeLine = new Map(rows("flow_node").map((n) => [n.id, `${n.kind}@${n.line}`]));
  const at = (rel: string, v: string) =>
    rows(rel)
      .filter((r) => r.var === `${S}.${v}`)
      .map((r) => nodeLine.get(r.node))
      .sort();
  assert.deepEqual(at("def", "classify.i"), ["stmt@18", "stmt@18"]);
  assert.deepEqual(at("use", "classify.i"), ["loop_head@18", "stmt@18", "stmt@19"]);
  assert.deepEqual(at("def", "read.line"), ["loop_head@53"]);
  assert.deepEqual(at("def", "read.r"), ["stmt@60"]);
  // Two catch parameters named `e` are two variables.
  assert.deepEqual([at("def", "read.e"), at("def", "read.e@64"), at("use", "read.e@64")], [["catch@62"], ["catch@64"], ["throw@65"]]);
  assert.deepEqual(rows("captures").map((c) => [c.fn, c.var]), [
    [`${S}.later.<lambda@77:12>`, `${S}.later.base`],
    [`${S}.later.<lambda@77:12>`, `${S}.later.offset`],
  ]);
  assert.deepEqual(rows("closure").map((c) => [nodeLine.get(c.node), c.fn]), [["return@77", `${S}.later.<lambda@77:12>`]]);
  const sites = new Map(rows("call_site").map((c) => [c.id, c.callee_name]));
  assert.deepEqual(rows("call_at").map((c) => [nodeLine.get(c.node), sites.get(c.call_site)]).sort(), [
    ["stmt@54", "toCharArray"],
    ["stmt@60", "StringReader"],
    ["stmt@61", "read"],
  ]);
});
