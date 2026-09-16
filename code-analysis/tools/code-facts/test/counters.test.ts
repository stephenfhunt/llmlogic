// One id-space across frontends: call-site, flow-node and allocation-site ids
// chain through each frontend's closing `__counters__` row. Extracting two
// languages that both have a dataflow layer is what tells them apart — each
// frontend counts from 1 of its own otherwise, and `alloc.site` stops being a
// key.

import assert from "node:assert/strict";
import * as path from "node:path";
import { test } from "node:test";
import { run } from "../src/main.ts";
import { FIXED_TIME, FIXTURES, goAvailable, tempDir } from "./helpers.ts";
import type { Layer } from "../src/schema.ts";

const NO_GO = goAvailable() ? false : "needs the `go` command";

test("a TypeScript and a Go target in one run share the call-site, flow-node and allocation-site id-spaces", { skip: NO_GO }, () => {
  const { tables } = run({
    tsconfigs: [path.join(FIXTURES, "basic", "tsconfig.json")],
    go: [path.join(FIXTURES, "go-basic")],
    root: FIXTURES,
    out: tempDir("counters"),
    layers: new Set<Layer>(["meta", "structure", "refs", "flow", "dataflow"]),
    time: FIXED_TIME,
  });

  const unique = (rel: string, column: string): void => {
    const rows = tables.rows(rel);
    const ids = new Set(rows.map((r) => r[column]));
    assert.equal(ids.size, rows.length, `${rel}.${column} repeats across frontends`);
  };
  // Both frontends allocate, both number call sites and flow nodes.
  assert.ok(tables.rows("alloc").length > 0, "no allocations");
  unique("alloc", "site");
  unique("flow_node", "id");
  unique("call_site", "id");
});
