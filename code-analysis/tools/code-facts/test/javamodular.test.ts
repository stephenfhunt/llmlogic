// A Maven project that *is* a module, on test/fixtures/java-modular: its
// dependencies belong on the module path, not the class path. On the class path
// they land in the unnamed module, which a named module cannot read, so every
// `requires` fails with "module not found" and nothing a dependency holds
// resolves — the opposite of too much being visible.
//
// With the module path, javac enforces the boundary both ways: a package the
// dependency exports resolves, one it does not is not visible, and neither is a
// JDK module this one does not require.

import assert from "node:assert/strict";
import { test } from "node:test";
import { extractJava, fixture, javaAvailable, mavenAvailable, tempDir, withMavenRepo } from "./helpers.ts";

const NO_MAVEN = !javaAvailable() ? "needs a JDK" : !mavenAvailable() ? "needs Maven" : false;
const mod =
  NO_MAVEN === false
    ? withMavenRepo(() => extractJava(fixture("java-modular"), { out: tempDir("java-modular"), layers: ["refs", "quality"] }))
    : undefined;
const rows = (rel: string) => mod?.tables.rows(rel) ?? [];

test("a `requires` resolves: the dependency is a module, not a jar in the unnamed one", { skip: NO_MAVEN }, () => {
  assert.deepEqual(
    rows("module_directive").map((d) => [d.directive, d.path, d.module]),
    [
      ["module", null, "com.example.app"],
      ["requires", "com.acme.modular", "com.example.app"],
      ["requires", "java.logging", "com.example.app"],
      ["exports", "com.example.app", "com.example.app"],
    ],
  );
  // No "module not found", which is what the class path produced for every one.
  assert.deepEqual(
    rows("diagnostic").filter((d) => String(d.key).includes("module.not.found")),
    [],
  );
});

test("what an exported package publishes resolves, through the module path", { skip: NO_MAVEN }, () => {
  const from = "src/main/java/com/example/app/App.java#App.fromExportedPackage";
  // The call and the type it is on, both to the dependency's exported package.
  assert.deepEqual(
    rows("ref").filter((r) => r.from === from).map((r) => [r.to, r.kind]),
    [["ext:com.acme.modular#Exported.value", "call"], ["ext:com.acme.modular#Exported", "type"]],
  );
  assert.equal(rows("symbol").find((s) => s.id === "ext:com.acme.modular#Exported")?.package, "com.acme:modular");
});

test("the boundary hides what it should: a non-exported package, and a module not required", { skip: NO_MAVEN }, () => {
  const notVisible = rows("diagnostic").filter((d) => String(d.message).includes("not visible")).map((d) => d.line);
  assert.equal(notVisible.length, 2, `expected both names to be hidden, got ${JSON.stringify(rows("diagnostic"))}`);
  // Neither resolves to a symbol, so no library counts them as a dependency.
  assert.deepEqual(rows("ref").filter((r) => String(r.to).includes("Hidden") || String(r.to).includes("DocumentBuilder")), []);
  // `DocumentBuilderFactory` twice: the return type, and the call on it.
  assert.deepEqual(
    rows("unresolved_ref").map((r) => r.name).sort(),
    ["DocumentBuilderFactory", "DocumentBuilderFactory", "Hidden"],
  );
});
