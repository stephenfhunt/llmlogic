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
