import assert from "node:assert/strict";
import { test } from "node:test";
import { extract, fixture } from "./helpers.ts";

const q = extract(fixture("quality"), { layers: ["refs", "quality"] });
const rows = (rel: string) => q.tables.rows(rel);
const pick = (rel: string, cols: string[]) => rows(rel).map((r) => cols.map((c) => r[c]));

test("compiler diagnostics, with what @ts-expect-error suppressed left out", () => {
  assert.deepEqual(pick("diagnostic", ["file", "line", "code", "category"]), [["src/q.ts", 45, 2322, "error"]]);
  assert.deepEqual(pick("ts_directive", ["line", "kind"]), [[39, "ts_expect_error"]]);
});

test("suppressions and markers, from line and block comments alike", () => {
  assert.deepEqual(pick("lint_directive", ["line", "tool", "directive", "rules"]), [[16, "eslint", "disable-next-line", "no-console"]]);
  assert.deepEqual(pick("comment_marker", ["line", "kind", "text"]), [
    [1, "todo", "split this file"],
    [7, "fixme", "the retry count"],
    [8, "hack", "around the flaky API"],
  ]);
});

test("where any enters: written, and returned by a call", () => {
  assert.deepEqual(pick("any_site", ["fn", "line", "kind"]), [
    ["src/q.ts#parse", 5, "explicit"],
    ["src/q.ts#loose", 34, "explicit"],
    ["src/q.ts#loose", 35, "call_result"],
  ]);
});

test("assertions by form, and whether they launder an any", () => {
  assert.deepEqual(pick("assertion", ["line", "kind", "to_type", "from_any"]), [
    [36, "cast", "number", true],
    [37, "non_null", null, false],
    [38, "as_const", "const", false],
  ]);
});

test("literals are values only — not keys, specifiers or types — and keep their owner", () => {
  const lits = pick("literal", ["fn", "kind", "value"]);
  assert.ok(lits.some(([f, k, v]) => f === "src/q.ts#URL_PREFIX" && k === "string" && v === "https://example.com/"));
  assert.ok(lits.some(([, k, v]) => k === "regexp" && v === "/a\\/\\/b/"));
  assert.ok(lits.some(([f, k, v]) => f === "src/q.ts#careful" && k === "template" && v === "missing ${id}"));
  assert.ok(!lits.some(([, , v]) => v === "mode"), "an object key is not a literal value");
});

test("a discarded promise is floating; a voided one is not", () => {
  assert.deepEqual(pick("floating_promise", ["fn", "line"]), [["src/q.ts#fetchAll", 14]]);
  const site = rows("floating_promise")[0]?.call_site;
  assert.equal(rows("call_site").find((c) => c.id === site)?.callee_name, "load");
});

test("throw and catch sites", () => {
  assert.deepEqual(pick("throw_site", ["fn", "type"]), [["src/q.ts#careful", "src/q.ts#NotFound"]]);
  assert.deepEqual(pick("catch_site", ["fn", "binds", "empty", "rethrows"]), [
    ["src/q.ts#careful", true, false, true],
    ["src/q.ts#swallow", false, true, false],
  ]);
});
