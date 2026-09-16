// A `module-info.java`, on test/fixtures/java-module: what the module declares
// becomes `module_directive` rows, and the types a `uses` or `provides` names
// resolve as references — which is the one place a service implementation that
// no code constructs becomes visible. The module and package names around them
// are not references, and used to be reported as unresolved.

import assert from "node:assert/strict";
import { test } from "node:test";
import { extractJava, fixture, javaAvailable, tempDir } from "./helpers.ts";

const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";
const mod = NO_JAVA === false ? extractJava(fixture("java-module"), { out: tempDir("java-module"), layers: ["refs"] }) : undefined;
const rows = (rel: string) => mod?.tables.rows(rel) ?? [];

test("every directive, a row — and a row per target where one names several", { skip: NO_JAVA }, () => {
  assert.deepEqual(
    rows("module_directive").map((d) => [d.directive, d.path, d.module, d.target, d.modifier]),
    [
      ["module", null, "com.example.core", null, "open"],
      ["requires", "java.logging", "com.example.core", null, null],
      ["requires", "java.sql", "com.example.core", null, "transitive"],
      ["requires", "java.desktop", "com.example.core", null, "static"],
      ["exports", "com.example.api", "com.example.core", null, null],
      ["exports", "com.example.impl", "com.example.core", "java.logging", null],
      ["exports", "com.example.impl", "com.example.core", "java.sql", null],
      ["uses", "com.example.api.Spi", "com.example.core", null, null],
      ["provides", "com.example.api.Spi", "com.example.core", "com.example.impl.Impl", null],
    ],
  );
});

test("the service and its implementation are references; module and package names are not", { skip: NO_JAVA }, () => {
  const fromModule = rows("ref").filter((r) => String(r.file) === "src/module-info.java");
  assert.deepEqual(
    fromModule.map((r) => [r.to, r.kind]),
    [
      ["src/com/example/api/Spi.java#Spi", "type"],
      ["src/com/example/api/Spi.java#Spi", "type"],
      ["src/com/example/impl/Impl.java#Impl", "type"],
    ],
  );
  // `com`, `java` and the rest of a module name resolve to nothing, and are no
  // one's reference — reporting them as unresolved was noise.
  assert.deepEqual(rows("unresolved_ref"), []);
});
