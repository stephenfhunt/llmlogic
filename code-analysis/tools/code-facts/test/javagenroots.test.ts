// Two ways a Maven reactor hides code from a reader that only looks at poms, on
// test/fixtures/java-maven-gen:
//
//   - a plugin writes generated sources either into a directory of its own
//     (`target/generated-sources/wsdl/…`) or straight into
//     `target/generated-sources` itself, and the root it added is invisible —
//     the model is read before any plugin runs. Each file says where its root is,
//     by the package it declares;
//   - a module depends on a sibling's *test* classes (`<type>test-jar</type>`).
//     The jar is no use, since the sibling is in the reactor and read from
//     source, so its test sources go on the source path.
//
// Both show up the same way: names that do not resolve. The third is the
// opposite — a file the build's own excludes leave out, which is in no relation
// at all unless `excluded_file` says so.

import assert from "node:assert/strict";
import { test } from "node:test";
import { extractJava, fixture, javaAvailable, mavenAvailable, tempDir, withMavenRepo } from "./helpers.ts";

const NO_MAVEN = !javaAvailable() ? "needs a JDK" : !mavenAvailable() ? "needs Maven" : false;
const gen =
  NO_MAVEN === false
    ? withMavenRepo(() => extractJava(fixture("java-maven-gen"), { out: tempDir("java-maven-gen"), layers: ["refs", "quality"] }))
    : undefined;
const rows = (rel: string) => gen?.tables.rows(rel) ?? [];

test("every name resolves: nothing unresolved, and javac reports no error", { skip: NO_MAVEN }, () => {
  assert.deepEqual(rows("unresolved_ref"), []);
  assert.deepEqual(
    rows("diagnostic").filter((d) => d.category === "error").map((d) => [d.file, d.line, d.message]),
    [],
  );
});

test("generated sources are read from the root their package declares, under either layout", { skip: NO_MAVEN }, () => {
  // `Flat` sits at the top of `target/generated-sources`; `Nested` under a
  // directory of the plugin's own. The licence header above `Flat` names a
  // package too, and is not one.
  assert.deepEqual(
    rows("file").filter((f) => String(f.path).includes("generated-sources")).map((f) => [f.path, f.namespace, f.is_generated]),
    [
      ["lib/target/generated-sources/com/example/lib/gen/Flat.java", "com.example.lib.gen", true],
      ["lib/target/generated-sources/wsdl/com/example/lib/wire/Nested.java", "com.example.lib.wire", true],
    ],
  );
  // A sibling module reaches both, which is what a root being right means.
  const from = "app/src/main/java/com/example/app/App.java#App.total";
  const calls = rows("ref").filter((r) => r.from === from && r.kind === "call").map((r) => r.to);
  assert.deepEqual(calls.sort(), [
    "ext:java.lang#String.length",
    "lib/src/main/java/com/example/lib/Lib.java#Lib.name",
    "lib/target/generated-sources/com/example/lib/gen/Flat.java#Flat.one",
    "lib/target/generated-sources/wsdl/com/example/lib/wire/Nested.java#Nested.two",
  ]);
});

test("a sibling's test classes are read from its test sources, not from a jar", { skip: NO_MAVEN }, () => {
  assert.deepEqual(
    rows("extends").filter((e) => String(e.child).includes("AppTest")),
    [
      {
        child: "app/src/test/java/com/example/app/AppTest.java#AppTest",
        parent: "lib/src/test/java/com/example/lib/LibTestSupport.java#LibTestSupport",
      },
    ],
  );
  // And the inherited member it calls resolves to that declaration.
  assert.ok(
    rows("ref").some((r) => r.to === "lib/src/test/java/com/example/lib/LibTestSupport.java#LibTestSupport.fixture"),
    "the inherited method does not resolve",
  );
});

test("a file the build's own excludes leave out is an excluded_file and nothing else", { skip: NO_MAVEN }, () => {
  assert.deepEqual(rows("excluded_file"), [
    { path: "app/src/main/java/com/example/app/Legacy.java", reason: "build_excluded", detail: "**/Legacy*.java" },
  ]);
  // Nothing else describes it: no file row, no symbol, no reference.
  for (const rel of ["file", "symbol", "ref"]) {
    assert.deepEqual(rows(rel).filter((r) => String(r.path ?? r.file ?? "").includes("Legacy.java")), [], `${rel} still describes it`);
  }
});
