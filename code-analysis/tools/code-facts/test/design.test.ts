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
    'module_lcom4("test/format.check.ts", 1).',
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
    'worst_coupling("test/format.check.ts", "src/format.ts", data).',
  ]);
  assert.deepEqual(ask("coupling_kinds.dl", 'module_coupling("src/client.ts", "src/config.ts", K)'), [
    'module_coupling("src/client.ts", "src/config.ts", content).',
    'module_coupling("src/client.ts", "src/config.ts", control).',
    'module_coupling("src/client.ts", "src/config.ts", data).',
    'module_coupling("src/client.ts", "src/config.ts", stamp).',
  ]);
});

test("package hygiene: package.json against what the files import", { skip }, () => {
  // node:fs is a builtin, left-pad is declared (with its @types), the rest are wrong one way each.
  assert.deepEqual(ask("packages.dl", "undeclared(P, D, F)"), ['undeclared("design-fixture", "chalk", "src/format.ts").']);
  assert.deepEqual(ask("packages.dl", "unused(P, D, K)"), ['unused("design-fixture", "unused-lib", prod).']);
  assert.deepEqual(ask("packages.dl", "dev_in_production(P, D, F)"), ['dev_in_production("design-fixture", "dev-only", "src/format.ts").']);
  assert.deepEqual(ask("packages.dl", "only_in_tests(P, D)"), ['only_in_tests("design-fixture", "only-in-tests").']);
  // imported only with `import type`: erased, so it belongs in devDependencies
  assert.deepEqual(ask("packages.dl", "types_only(P, D)"), ['types_only("design-fixture", "types-only-dep").']);
});

test("the extractor names builtins and what an @types package types", () => {
  const { tables } = extract(fixture("design"), { layers: [] });
  const fs = tables.rows("imports").find((r) => r.specifier === "node:fs");
  assert.deepEqual([fs?.builtin, fs?.target_package], [true, "node:fs"]);
  assert.equal(tables.rows("imports").find((r) => r.specifier === "chalk")?.builtin, false);
  assert.deepEqual(
    tables.rows("package_dep").filter((d) => d.types_for !== null).map((d) => [d.dep, d.types_for]),
    [["@types/left-pad", "left-pad"]],
  );
});

// A monorepo: workspace siblings resolve to files inside the root, so
// `imports.target_package` is absent on every one of them. Found dogfooding
// Grafana, where `unused` named four `@grafana/*` packages that 540 import
// statements use (`../../notes/code-facts.md` § Dogfooding — Grafana).
test("package hygiene in a monorepo: a workspace sibling is a dependency", { skip }, () => {
  const mono = tempDir("monorepo");
  extract(fixture("monorepo"), { out: mono, layers: ["refs"] });
  const askm = (query: string): string[] => {
    const r = datalog(path.join(mono, "lib", "packages.dl"), [query]);
    assert.ok(r.code === 0 || r.code === 1, `${query}: exit ${r.code}\n${r.stderr}`);
    return r.stdout.split("\n").filter((l) => l !== "");
  };

  // Every edge is file-to-file: `imported` sees none of them.
  assert.deepEqual(askm('i(P, D) :- imported(P, D, _), P = "@fix/app"'), []);
  assert.deepEqual(askm("imported_workspace(P, D, F)"), [
    'imported_workspace("@fix/app", "@fix/lib", "packages/app/src/index.ts").',
    'imported_workspace("@fix/app", "@fix/other", "packages/app/src/index.ts").',
    // a relative import into a file of the root package is an edge too
    'imported_workspace("@fix/app", "monorepo-fixture", "packages/app/src/index.ts").',
  ]);

  // The point of the fix: `@fix/lib` is declared by `@fix/app` and really used,
  // so it must not be reported unused. Only the root's `left-pad` is.
  assert.deepEqual(askm("unused(P, D, K)"), ['unused("monorepo-fixture", "left-pad", prod).']);

  // And the deliberate silence: `@fix/app` imports `@fix/other` without
  // declaring it, but `imports` cannot say whether a specifier was written bare
  // or as a path, so no `undeclared` is claimed. If this ever starts reporting,
  // the rule has begun guessing.
  assert.deepEqual(askm("undeclared(P, D, F)"), []);
});

// A `package.json` with no `name` — the `{"type": "module"}` marker — is not a
// package: npm cannot install it and nothing can declare a dependency on it.
// Naming it after its directory made an import into `scripts/tool/` read as an
// undeclared dependency on a package called `scripts/tool`.
test("an unnamed package.json is a module-system marker, not a package", () => {
  const { tables } = extract(fixture("monorepo"), { layers: [] });
  assert.deepEqual(tables.rows("package").map((p) => p.name).sort(), [
    "@fix/app",
    "@fix/lib",
    "@fix/other",
    "monorepo-fixture",
  ]);
  // Its files belong to the nearest *named* package instead.
  assert.equal(tables.rows("file").find((f) => f.path === "scripts/tool/name.ts")?.package, "monorepo-fixture");
});
