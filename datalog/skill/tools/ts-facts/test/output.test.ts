// P7 — schema: the writer validates every row against schema.ts, and the engine
// imports every table the run writes, empty ones included.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import { RELATIONS } from "../src/schema.ts";
import { Tables } from "../src/writer.ts";
import { datalog, engineAvailable, extract, fixture, tempDir } from "./helpers.ts";

test("the writer rejects a row that does not match its relation", () => {
  const t = new Tables();
  assert.throws(() => t.add("file", { path: "a.ts" }), /not nullable/);
  assert.throws(() => t.add("dir", { path: ".", parent: null, name: ".", depth: 0, extra: 1 }), /no column `extra`/);
  assert.throws(() => t.add("dir", { path: ".", parent: null, name: ".", depth: 1.5 }), /integer/);
  assert.throws(() => t.add("flow_edge", { from: 1, to: 2, kind: "sideways" }), /one of/);
  assert.throws(() => t.add("nope", {}), /no relation/);
});

test("a string with a lone surrogate is written as well-formed Unicode", () => {
  const t = new Tables();
  t.add("comment_marker", { file: "a.ts", line: 1, kind: "todo", text: "bad \uD800 half" });
  assert.equal(t.rows("comment_marker")[0]?.text, "bad \uFFFD half");
});

test("no symbol value in the schema is a reserved word or unwritable bare", () => {
  const reserved = new Set(["import", "as", "declare", "not", "true", "false", "absent", "is"]);
  for (const r of RELATIONS) {
    for (const c of r.columns) {
      for (const v of c.values ?? []) {
        assert.match(v, /^[a-z][A-Za-z0-9_]*$/, `${r.name}.${c.name}: ${v}`);
        assert.ok(!reserved.has(v), `${r.name}.${c.name}: ${v} is reserved`);
      }
    }
    assert.ok(!reserved.has(r.name), `relation ${r.name} is a reserved word`);
  }
});

test("every written table imports into the engine, and the counts agree", { skip: !engineAvailable() }, () => {
  const out = tempDir("p7");
  const result = extract(fixture("basic"), { out });
  for (const r of RELATIONS.filter((x) => result.layers.has(x.layer))) {
    assert.ok(fs.existsSync(path.join(out, "facts", `${r.name}.jsonl`)), `${r.name}.jsonl written`);
  }
  const all = datalog(path.join(out, "schema", "all.dl"));
  assert.equal(all.code, 0, all.stderr);
  // The engine's own count of a table equals relation_rows' claim for it. Every
  // column is a witness dimension of the count, so it counts whole rows.
  const queries: string[] = [];
  const expected: string[] = [];
  for (const r of result.tables.rows("relation_rows")) {
    const rel = RELATIONS.find((x) => x.name === r.relation);
    if (rel === undefined || rel.name === "relation_rows") continue;
    const vars = rel.columns.map((_, i) => `C${i}`).join(", ");
    queries.push(`rows_${rel.name}(N) :- N = count { C0 | ${rel.name}(${vars}) }`);
    expected.push(`rows_${rel.name}(${r.rows}).`);
  }
  const q = datalog(path.join(out, "schema", "all.dl"), queries);
  assert.equal(q.code, 0, q.stderr);
  const got = q.stdout.split("\n").filter((l) => l.startsWith("rows_"));
  assert.deepEqual(got, expected);
});

test("a layer switched off leaves its schema file as a comment", () => {
  const out = tempDir("layers");
  extract(fixture("basic"), { out, layers: [] });
  const flow = fs.readFileSync(path.join(out, "schema", "flow.dl"), "utf8");
  assert.match(flow, /NOT EXTRACTED/);
  assert.doesNotMatch(flow, /^import/m);
  assert.ok(!fs.existsSync(path.join(out, "facts", "flow_node.jsonl")));
  assert.doesNotMatch(fs.readFileSync(path.join(out, "schema", "all.dl"), "utf8"), /flow\.dl/);
});
