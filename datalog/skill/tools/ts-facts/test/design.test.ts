// The module-level, coupling-kind and package libraries over the design
// fixture, against expectations worked by hand from its source.

import assert from "node:assert/strict";
import * as path from "node:path";
import { test } from "node:test";
import { datalog, engineAvailable, extract, fixture, tempDir } from "./helpers.ts";

const skip = !engineAvailable();
let out: string | undefined;
function facts(): string {
  if (out === undefined) {
    out = tempDir("design");
    extract(fixture("design"), { out, layers: ["refs", "flow", "dataflow", "quality"] });
  }
  return out;
}
function ask(lib: string, query: string): string[] {
  const r = datalog(path.join(facts(), "lib", lib), [query]);
  assert.ok(r.code === 0 || r.code === 1, `${lib} ${query}: exit ${r.code}\n${r.stderr}`);
  return r.stdout.split("\n").filter((l) => l !== "");
}

test("module cohesion: a file whose exports share nothing is several modules", { skip }, () => {
  assert.deepEqual(ask("cohesion.dl", "module_lcom4(F, N)"), [
    'module_lcom4("src/client.ts", 1).',
    // connect and address share the Config type; add, render, state, Vault stand alone
    'module_lcom4("src/config.ts", 5).',
    'module_lcom4("src/format.ts", 1).',
    // the cache (remember, recall) and the clock (now, later); the interface is not an operation
    'module_lcom4("src/mixed.ts", 2).',
    'module_lcom4("src/server.ts", 2).',
    'module_lcom4("src/tight.ts", 1).',
    'module_lcom4("test/format.test.ts", 1).',
  ]);
  assert.deepEqual(ask("cohesion.dl", 'module_component("src/mixed.ts", R, E)'), [
    'module_component("src/mixed.ts", "src/mixed.ts#later", "src/mixed.ts#later").',
    'module_component("src/mixed.ts", "src/mixed.ts#later", "src/mixed.ts#now").',
    'module_component("src/mixed.ts", "src/mixed.ts#recall", "src/mixed.ts#recall").',
    'module_component("src/mixed.ts", "src/mixed.ts#recall", "src/mixed.ts#remember").',
  ]);
});

test("coupling kinds: each of Myers' six, found where the fixture put it", { skip }, () => {
  // client.run reads Vault's private field by bracket access.
  assert.deepEqual(ask("coupling_kinds.dl", "content_access(A, M, C)"), [
    'content_access("src/client.ts#run", "src/config.ts#Vault.secret", "src/config.ts#Vault").',
  ]);
  // client.run writes state.hits, server.report reads it; tight.ts's `total` is shared only inside its file.
  assert.deepEqual(ask("coupling_kinds.dl", "common_state(A, B, V)"), [
    'common_state("src/client.ts#run", "src/server.ts#report", "src/config.ts#state").',
  ]);
  assert.deepEqual(ask("coupling_kinds.dl", "shared_literal(A, B, V)"), [
    'shared_literal("src/client.ts", "src/server.ts", "application/json").',
  ]);
  assert.deepEqual(ask("coupling_kinds.dl", "control_param(F, P)"), [
    'control_param("src/config.ts#render", "src/config.ts#render.verbose").',
  ]);
  // connect reads host of Config's four fields; address reads all four, so it is not stamp.
  assert.deepEqual(ask("coupling_kinds.dl", "stamp_param(F, T, U, N)"), [
    'stamp_param("src/config.ts#connect", "src/config.ts#Config", 1, 4).',
  ]);
  assert.deepEqual(ask("coupling_kinds.dl", "worst_coupling(A, B, K)"), [
    'worst_coupling("src/client.ts", "src/config.ts", content).',
    'worst_coupling("src/client.ts", "src/server.ts", common).',
    'worst_coupling("test/format.test.ts", "src/format.ts", data).',
  ]);
  assert.deepEqual(ask("coupling_kinds.dl", 'module_coupling("src/client.ts", "src/config.ts", K)'), [
    'module_coupling("src/client.ts", "src/config.ts", content).',
    'module_coupling("src/client.ts", "src/config.ts", control).',
    'module_coupling("src/client.ts", "src/config.ts", data).',
    'module_coupling("src/client.ts", "src/config.ts", stamp).',
  ]);
});
