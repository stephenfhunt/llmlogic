// The Java frontend's structure layer, on test/fixtures/java-maven (a reactor
// resolving jars from a local repository), java-gradle (through its wrapper) and
// java-plain (a directory with no build), and on small builds written per test.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import type { Result } from "../src/main.ts";
import { extractJava, fixture, gradleAvailable, javaAvailable, mavenAvailable, mavenRepo, tempDir, withMavenRepo, writeFiles } from "./helpers.ts";

const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";
const NO_MAVEN = NO_JAVA !== false ? NO_JAVA : mavenAvailable() ? false : "needs Maven (`mvn`)";
const NO_GRADLE = NO_JAVA !== false ? NO_JAVA : gradleAvailable(fixture("java-gradle")) ? false : "the Gradle wrapper cannot run here (it downloads Gradle on first use)";

const maven = NO_MAVEN === false ? withMavenRepo(() => extractJava(fixture("java-maven"), { out: tempDir("java-maven"), layers: [] })) : undefined;
const rows = (rel: string, r: Result | undefined = maven) => r?.tables.rows(rel) ?? [];
const C = "core/src/main/java/com/example/core";
const APP = "app/src/main/java/com/example/app";
const TEST = "core/src/test/java/com/example/core";

function symbol(id: string, r: Result | undefined = maven) {
  const s = rows("symbol", r).find((x) => x.id === id);
  assert.ok(s !== undefined, `no symbol ${id}`);
  return s;
}

test("files are Java; the namespace is the package, the package the Maven module, tests the test source set", { skip: NO_MAVEN }, () => {
  assert.deepEqual(
    rows("file").map((f) => [f.path, f.lang, f.package, f.is_test, f.namespace]),
    [
      [`${APP}/Main.java`, "java", "com.example:app", false, "com.example.app"],
      [`${C}/Circle.java`, "java", "com.example:core", false, "com.example.core"],
      [`${C}/Kind.java`, "java", "com.example:core", false, "com.example.core"],
      [`${C}/Registry.java`, "java", "com.example:core", false, "com.example.core"],
      [`${C}/Shape.java`, "java", "com.example:core", false, "com.example.core"],
      [`${C}/Tag.java`, "java", "com.example:core", false, "com.example.core"],
      [`${C}/geom/Geom.java`, "java", "com.example:core", false, "com.example.core.geom"],
      [`${C}/geom/Point.java`, "java", "com.example:core", false, "com.example.core.geom"],
      [`${TEST}/CircleTest.java`, "java", "com.example:core", true, "com.example.core"],
    ],
  );
  assert.match(String(rows("extraction")[0]?.java_version), /^\d+/);
});

test("a Kotlin file under a source root is an excluded_file, and nothing else", { skip: NO_MAVEN }, () => {
  assert.deepEqual(rows("excluded_file"), [{ path: `${C}/Legacy.kt`, reason: "other_language", detail: null }]);
});

test("projects are the reactor's modules with sources; packages every module, with the build's dependencies", { skip: NO_MAVEN }, () => {
  assert.deepEqual(rows("project"), [
    { id: "app/pom.xml", dir: "app", files: 1, strict: false, module: null, target: "17" },
    { id: "core/pom.xml", dir: "core", files: 8, strict: false, module: null, target: "17" },
  ]);
  assert.deepEqual(rows("package").map((p) => [p.name, p.dir, p.version]), [
    ["com.example:app", "app", "1.0"],
    ["com.example:core", "core", "1.0"],
    ["com.example:shapes-parent", ".", "1.0"],
  ]);
  assert.deepEqual(rows("package_dep").map((d) => [d.package, d.dep, d.kind, d.range, d.scope]), [
    ["com.example:app", "com.example:core", "prod", "1.0", "compile"],
    ["com.example:core", "com.acme:units", "prod", "1.0", "compile"],
    ["com.example:core", "com.acme:testkit", "dev", "1.0", "test"],
  ]);
});

test("imports: a row per file an import statement reaches, and an implicit row per file reached without one", { skip: NO_MAVEN }, () => {
  assert.deepEqual(
    rows("imports").map((i) => [i.file, i.line, i.specifier, i.kind, i.runtime, i.target_file, i.target_dir, i.target_package, i.builtin, i.resolved]),
    [
      [`${APP}/Main.java`, 3, "com.example.core.Kind.*", "static_import", true, `${C}/Kind.java`, C, null, false, true],
      [`${APP}/Main.java`, 4, "com.example.core.geom.Geom.sq", "static_import", true, `${C}/geom/Geom.java`, `${C}/geom`, null, false, true],
      [`${APP}/Main.java`, 6, "com.example.core.Circle", "static", true, `${C}/Circle.java`, C, null, false, true],
      // Imported and never used: the class needs nothing of it.
      [`${APP}/Main.java`, 7, "com.example.core.Tag", "static", false, `${C}/Tag.java`, C, null, false, true],
      // Geom is reached only for an inlined constant; Point is used.
      [`${APP}/Main.java`, 8, "com.example.core.geom.*", "on_demand", false, `${C}/geom/Geom.java`, `${C}/geom`, null, false, true],
      [`${APP}/Main.java`, 8, "com.example.core.geom.*", "on_demand", true, `${C}/geom/Point.java`, `${C}/geom`, null, false, true],
      [`${APP}/Main.java`, 9, "java.util.List", "static", true, null, null, "java.base", true, true],
      // A fully qualified name needs no import.
      [`${APP}/Main.java`, 16, "Registry", "implicit", true, `${C}/Registry.java`, C, null, false, true],
      [`${C}/Circle.java`, 3, "com.acme.units.Units", "static", false, null, null, "com.acme:units", false, true],
      [`${C}/Circle.java`, 6, "Tag", "implicit", true, `${C}/Tag.java`, C, null, false, true],
      [`${C}/Circle.java`, 7, "Shape", "implicit", true, `${C}/Shape.java`, C, null, false, true],
      [`${C}/Registry.java`, 3, "java.util.ArrayList", "static", true, null, null, "java.base", true, true],
      [`${C}/Registry.java`, 4, "java.util.List", "static", true, null, null, "java.base", true, true],
      [`${C}/Registry.java`, 8, "Shape", "implicit", true, `${C}/Shape.java`, C, null, false, true],
      [`${C}/Registry.java`, 36, "Circle", "implicit", true, `${C}/Circle.java`, C, null, false, true],
      [`${C}/Registry.java`, 41, "Kind", "implicit", true, `${C}/Kind.java`, C, null, false, true],
      // Only a Javadoc link.
      [`${C}/Shape.java`, 10, "Kind", "implicit", false, `${C}/Kind.java`, C, null, false, true],
      [`${C}/Tag.java`, 3, "java.lang.annotation.Retention", "static", true, null, null, "java.base", true, true],
      [`${C}/Tag.java`, 4, "java.lang.annotation.RetentionPolicy", "static", true, null, null, "java.base", true, true],
      [`${TEST}/CircleTest.java`, 3, "org.junit.jupiter.api.Test", "static", true, null, null, "com.acme:testkit", false, true],
      [`${TEST}/CircleTest.java`, 8, "Circle", "implicit", true, `${C}/Circle.java`, C, null, false, true],
    ],
  );
});

test("an import binds its simple name, or `*`, to what it names; a static import to each member of that name", { skip: NO_MAVEN }, () => {
  assert.deepEqual(
    rows("import_name").filter((n) => n.file === `${APP}/Main.java`).map((n) => [n.line, n.local, n.imported, n.target]),
    [
      [3, "*", "*", `${C}/Kind.java#Kind`],
      [4, "sq", "sq", `${C}/geom/Geom.java#Geom.sq`],
      [6, "Circle", "Circle", `${C}/Circle.java#Circle`],
      [7, "Tag", "Tag", `${C}/Tag.java#Tag`],
      [8, "*", "*", `${C}/geom#<package>`],
      [9, "List", "List", "ext:java.util#List"],
    ],
  );
  assert.deepEqual([symbol(`${C}/geom#<package>`).kind, symbol(`${C}/geom#<package>`).name, symbol(`${C}/geom#<package>`).file], ["namespace", "com.example.core.geom", `${C}/geom/Geom.java`]);
  assert.deepEqual([symbol("ext:com.acme.units#Units").origin, symbol("ext:com.acme.units#Units").package], ["external", "com.acme:units"]);
  assert.deepEqual([symbol("ext:java.util#List").kind, symbol("ext:java.util#List").package, symbol("ext:java.util#List").is_abstract], ["interface", "java.base", false]);
});

test("types map onto the shared kinds, with the Java construct in `form`", { skip: NO_MAVEN }, () => {
  const kf = (id: string) => [symbol(id).kind, symbol(id).form];
  assert.deepEqual(kf(`${C}/Circle.java#Circle`), ["class", "record"]);
  assert.deepEqual(kf(`${C}/Circle.java#Circle.r`), ["property", "record_component"]);
  assert.deepEqual(kf(`${C}/Kind.java#Kind`), ["class", "enum"]);
  assert.deepEqual(kf(`${C}/Kind.java#Kind.ROUND`), ["enum_member", null]);
  assert.deepEqual(kf(`${C}/Kind.java#Kind.COUNT`), ["property", null]);
  assert.deepEqual(kf(`${C}/Tag.java#Tag`), ["interface", "annotation"]);
  assert.deepEqual(kf(`${C}/Shape.java#Shape`), ["interface", null]);
  assert.deepEqual(kf(`${C}/Registry.java#Registry.<static@10:3>`), ["static_block", "static_init"]);
  assert.deepEqual(kf(`${C}/Registry.java#Registry.<instance@16:3>`), ["static_block", "instance_init"]);
  assert.deepEqual(kf(`${C}/Registry.java#Registry.constructor`), ["constructor", null]);
  assert.deepEqual(kf(`${APP}/Main.java#Main.main.circles`), ["local", null]);
  assert.deepEqual(kf(`${C}/Registry.java#Registry.add.shapes`), ["parameter", null]);
});

test("members carry Java's visibility, the modifiers written and the ones implied", { skip: NO_MAVEN }, () => {
  const flags = (id: string) => {
    const s = symbol(id);
    return [s.visibility, s.exported, s.is_static, s.is_abstract, s.is_readonly, s.line, s.parent];
  };
  // An interface's members are public, and abstract unless they have a body.
  assert.deepEqual(flags(`${C}/Shape.java#Shape.area`), ["public", true, false, true, false, 5, `${C}/Shape.java#Shape`]);
  assert.deepEqual(flags(`${C}/Shape.java#Shape.name`), ["public", true, false, false, false, 13, `${C}/Shape.java#Shape`]);
  assert.deepEqual(flags(`${C}/Registry.java#Registry.count`), ["private", false, false, false, false, 14, `${C}/Registry.java#Registry`]);
  assert.deepEqual(flags(`${C}/Registry.java#Registry.constructor`), ["protected", true, false, false, false, 20, `${C}/Registry.java#Registry`]);
  // A nested class is package-private here, and not static unless it says so.
  assert.deepEqual(flags(`${C}/Registry.java#Registry.Entry`), ["package", false, true, false, false, 40, `${C}/Registry.java#Registry`]);
  // An enum's constants are public static final.
  assert.deepEqual(flags(`${C}/Kind.java#Kind.ROUND`), ["public", true, true, false, true, 4, `${C}/Kind.java#Kind`]);
  // A declaration's line is its own, past its annotations.
  assert.deepEqual(flags(`${C}/Circle.java#Circle`), ["public", true, false, false, false, 7, `${C}/Circle.java#<module>`]);
  assert.deepEqual(flags(`${C}/geom/Geom.java#Geom.TAU`), ["public", true, true, false, true, 5, `${C}/geom/Geom.java#Geom`]);
  assert.deepEqual(flags(`${TEST}/CircleTest.java#CircleTest`), ["package", false, false, false, false, 5, `${TEST}/CircleTest.java#<module>`]);
});

test("param: a varargs parameter is rest", { skip: NO_MAVEN }, () => {
  const params = (fn: string) => rows("param").filter((p) => p.fn === fn).map((p) => [p.index, p.name, p.symbol, p.rest]);
  assert.deepEqual(params(`${C}/Registry.java#Registry.add`), [[0, "shapes", `${C}/Registry.java#Registry.add.shapes`, true]]);
  assert.deepEqual(params(`${APP}/Main.java#Main.main`), [[0, "args", `${APP}/Main.java#Main.main.args`, false]]);
});

test("Javadoc: its length and its block tags; `@Deprecated` is the deprecated tag when the comment has none", { skip: NO_MAVEN }, () => {
  const doc = (id: string) => {
    const d = rows("doc").find((r) => r.symbol === id);
    return [d?.has_doc, d?.lines];
  };
  assert.deepEqual(doc(`${C}/Registry.java#Registry.add`), [true, 6]);
  assert.deepEqual(doc(`${C}/Shape.java#Shape`), [true, 1]);
  assert.deepEqual(doc(`${C}/Kind.java#Kind`), [false, 0]);
  assert.ok(!rows("doc").some((d) => d.symbol === `${APP}/Main.java#Main.main.circles`));
  assert.deepEqual(rows("jsdoc_tag").map((t) => [t.symbol, t.tag, t.text]), [
    [`${C}/Registry.java#Registry.add`, "param", "shapes the shapes to add"],
    [`${C}/Registry.java#Registry.add`, "return", "how many are registered"],
    [`${C}/Shape.java#Shape.name`, "deprecated", "use {@link Kind} instead."],
    [`${C}/geom/Geom.java#Geom.TAU`, "deprecated", null],
  ]);
});

test("entry points: main, static initializers, and methods a test framework runs", { skip: NO_MAVEN }, () => {
  assert.deepEqual(rows("entry_point").map((e) => [e.symbol, e.kind]), [
    [`${APP}/Main.java#Main.main`, "main"],
    [`${C}/Registry.java#Registry.<static@10:3>`, "init"],
    [`${TEST}/CircleTest.java#CircleTest.area`, "test"],
  ]);
});

test("exports are a file's public top-level type; decorators its annotations, resolved, with their arguments", { skip: NO_MAVEN }, () => {
  assert.deepEqual(rows("exports").map((e) => [e.file, e.name, e.symbol, e.kind, e.is_type]).slice(0, 2), [
    [`${APP}/Main.java`, "Main", `${APP}/Main.java#Main`, "local", true],
    [`${C}/Circle.java`, "Circle", `${C}/Circle.java#Circle`, "local", true],
  ]);
  assert.equal(rows("exports").length, 8);
  assert.deepEqual(rows("decorator").map((d) => [d.target, d.decorator, d.name, d.line, d.text]), [
    [`${C}/Circle.java#Circle`, `${C}/Tag.java#Tag`, "Tag", 6, '"round"'],
    [`${C}/Circle.java#Circle.area`, "ext:java.lang#Override", "Override", 8, null],
    [`${C}/Shape.java#Shape.name`, "ext:java.lang#Deprecated", "Deprecated", 12, null],
    [`${C}/Tag.java#Tag`, "ext:java.lang.annotation#Retention", "Retention", 6, "RetentionPolicy.RUNTIME"],
    [`${C}/geom/Geom.java#Geom.TAU`, "ext:java.lang#Deprecated", "Deprecated", 4, null],
    [`${TEST}/CircleTest.java#CircleTest.area`, "ext:org.junit.jupiter.api#Test", "Test", 6, null],
  ]);
});

test("Gradle: projects and their dependencies come from the build, through its wrapper", { skip: NO_GRADLE }, () => {
  const g = extractJava(fixture("java-gradle"), { out: tempDir("java-gradle"), layers: [] });
  assert.deepEqual(rows("project", g), [
    { id: "app/build.gradle", dir: "app", files: 1, strict: false, module: null, target: "17" },
    { id: "lib/build.gradle", dir: "lib", files: 2, strict: false, module: null, target: "17" },
  ]);
  assert.deepEqual(rows("package", g).map((p) => [p.name, p.dir]), [
    [":app", "app"],
    [":lib", "lib"],
  ]);
  assert.deepEqual(rows("package_dep", g).map((d) => [d.package, d.dep, d.kind, d.scope]), [[":app", ":lib", "prod", "implementation"]]);
  assert.deepEqual(rows("file", g).map((f) => [f.path, f.package, f.is_test]), [
    ["app/src/main/java/com/example/app/App.java", ":app", false],
    ["lib/src/main/java/com/example/lib/Greeter.java", ":lib", false],
    ["lib/src/test/java/com/example/lib/GreeterTest.java", ":lib", true],
  ]);
  assert.deepEqual(rows("imports", g).map((i) => [i.file, i.line, i.specifier, i.kind, i.target_file]), [
    ["app/src/main/java/com/example/app/App.java", 3, "com.example.lib.Greeter", "static", "lib/src/main/java/com/example/lib/Greeter.java"],
    // A test reads its own module's main sources.
    ["lib/src/test/java/com/example/lib/GreeterTest.java", 5, "Greeter", "implicit", "lib/src/main/java/com/example/lib/Greeter.java"],
  ]);
});

test("a directory with no build is one project of plain sources, the unnamed package included", { skip: NO_JAVA }, () => {
  const p = extractJava(fixture("java-plain"), { out: tempDir("java-plain"), layers: [] });
  assert.deepEqual(rows("project", p), [{ id: ".", dir: ".", files: 2, strict: false, module: null, target: null }]);
  assert.deepEqual(rows("file", p).map((f) => [f.path, f.package, f.namespace]), [
    ["Hello.java", null, ""],
    ["util/Strings.java", null, "util"],
  ]);
  assert.deepEqual(rows("imports", p).map((i) => [i.file, i.specifier, i.kind, i.target_file, i.target_dir]), [["Hello.java", "util.Strings", "static", "util/Strings.java", "util"]]);
  assert.deepEqual(rows("package", p), []);
});

const POM = (body: string) => `<?xml version="1.0" encoding="UTF-8"?>\n<project xmlns="http://maven.apache.org/POM/4.0.0">\n  <modelVersion>4.0.0</modelVersion>\n${body}\n</project>\n`;

test("a dependency the build cannot resolve leaves its names unresolved, and the rest is read", { skip: NO_MAVEN }, () => {
  const dir = tempDir("java-unresolved");
  writeFiles(dir, {
    "pom.xml": POM(
      "  <groupId>g</groupId>\n  <artifactId>broken</artifactId>\n  <version>1</version>\n  <dependencies>\n    <dependency><groupId>com.acme</groupId><artifactId>missing</artifactId><version>1.0</version></dependency>\n  </dependencies>",
    ),
    "src/main/java/p/A.java": "package p;\n\nimport com.acme.missing.Thing;\n\npublic class A {\n  Thing t;\n  B b;\n}\n",
    "src/main/java/p/B.java": "package p;\n\npublic class B {}\n",
  });
  const r = withMavenRepo(() => extractJava(dir, { out: path.join(dir, "out"), layers: [] }), tempDir("m2-empty"));
  assert.deepEqual(rows("imports", r).map((i) => [i.file, i.specifier, i.kind, i.resolved, i.target_file, i.target_package]), [
    ["src/main/java/p/A.java", "com.acme.missing.Thing", "static", false, null, null],
    ["src/main/java/p/A.java", "B", "implicit", true, "src/main/java/p/B.java", null],
  ]);
  assert.deepEqual(rows("package_dep", r).map((d) => [d.package, d.dep, d.kind]), [["g:broken", "com.acme:missing", "prod"]]);
});

test("when Maven cannot read the reactor, its modules come from the poms, with conventional roots", { skip: NO_MAVEN }, () => {
  const dir = tempDir("java-fallback");
  writeFiles(dir, {
    "pom.xml": POM(
      "  <parent><groupId>com.acme</groupId><artifactId>nowhere</artifactId><version>1</version></parent>\n  <artifactId>root</artifactId>\n  <packaging>pom</packaging>\n  <modules><module>m</module></modules>",
    ),
    "m/pom.xml": POM("  <parent><groupId>com.acme</groupId><artifactId>root</artifactId><version>1</version></parent>\n  <artifactId>m</artifactId>"),
    "m/src/main/java/p/B.java": "package p;\n\npublic class B {}\n",
    "m/src/test/java/p/BTest.java": "package p;\n\nclass BTest {\n  B b;\n}\n",
  });
  const r = withMavenRepo(() => extractJava(dir, { out: path.join(dir, "out"), layers: [] }), tempDir("m2-empty"));
  assert.deepEqual(rows("file", r).map((f) => [f.path, f.package, f.is_test]), [
    ["m/src/main/java/p/B.java", "com.acme:m", false],
    ["m/src/test/java/p/BTest.java", "com.acme:m", true],
  ]);
  assert.deepEqual(rows("project", r).map((p) => p.id), ["m/pom.xml"]);
  assert.deepEqual(rows("imports", r).map((i) => [i.file, i.kind, i.target_file]), [["m/src/test/java/p/BTest.java", "implicit", "m/src/main/java/p/B.java"]]);
});

test("a lambda and the parameter it starts with are two declarations", { skip: NO_JAVA }, () => {
  const dir = tempDir("java-lambda");
  writeFiles(dir, { "p/L.java": "package p;\n\nimport java.util.function.Function;\n\nclass L {\n  Function<String, Integer> f = s -> s.length();\n}\n" });
  const r = extractJava(dir, { out: path.join(dir, "out"), layers: [] });
  assert.deepEqual(rows("symbol", r).filter((s) => String(s.id).includes("<lambda")).map((s) => [s.id, s.kind]), [
    ["p/L.java#L.f.<lambda@6:33>", "function"],
    ["p/L.java#L.f.<lambda@6:33>.s", "parameter"],
  ]);
  assert.deepEqual(rows("param", r).map((p) => [p.fn, p.symbol]), [["p/L.java#L.f.<lambda@6:33>", "p/L.java#L.f.<lambda@6:33>.s"]]);
});

test("what javac knows, not what is written: implied modifiers, local classes, record components, composed and unresolved test annotations", { skip: NO_JAVA }, () => {
  const dir = tempDir("java-declared");
  writeFiles(dir, {
    "p/Api.java": "package p;\n\npublic interface Api {\n  int LIMIT = 3;\n\n  void run();\n\n  class Nested {}\n\n  enum Mode { ON, OFF }\n}\n",
    "p/Holder.java":
      "package p;\n\npublic class Holder {\n  Runnable task = new Runnable() {\n    /** Below local scope. */\n    public void run() {}\n  };\n\n  public record Pair(int left, int right) {}\n}\n",
    "p/Tests.java":
      "package p;\n\nimport java.lang.annotation.Retention;\nimport java.lang.annotation.RetentionPolicy;\nimport org.junit.platform.commons.annotation.Testable;\n\n" +
      "class Tests {\n  @Retention(RetentionPolicy.RUNTIME)\n  @Testable\n  @interface Check {}\n\n  @Check\n  void composed() {}\n\n  @org.junit.Test\n  void unresolved() {}\n\n  void helper(String... names) {}\n}\n",
    "org/junit/platform/commons/annotation/Testable.java":
      "package org.junit.platform.commons.annotation;\n\nimport java.lang.annotation.Retention;\nimport java.lang.annotation.RetentionPolicy;\n\n@Retention(RetentionPolicy.RUNTIME)\npublic @interface Testable {}\n",
  });
  const r = extractJava(dir, { out: path.join(dir, "out"), layers: [] });
  const flags = (id: string) => {
    const s = symbol(id, r);
    return [s.kind, s.form, s.visibility, s.exported, s.is_static, s.is_abstract, s.is_readonly];
  };
  // An interface's constants are public static final, its methods public abstract, its member types public static.
  assert.deepEqual(flags("p/Api.java#Api.LIMIT"), ["property", null, "public", true, true, false, true]);
  assert.deepEqual(flags("p/Api.java#Api.run"), ["method", null, "public", true, false, true, false]);
  assert.deepEqual(flags("p/Api.java#Api.Nested"), ["class", null, "public", true, true, false, false]);
  assert.deepEqual(flags("p/Api.java#Api.Mode"), ["class", "enum", "public", true, true, false, false]);
  assert.deepEqual(flags("p/Api.java#Api.Mode.ON"), ["enum_member", null, "public", true, true, false, true]);
  // An anonymous class in a field's initializer is below local scope: no visibility, no doc rows.
  const anon = rows("symbol", r).find((s) => String(s.id).startsWith("p/Holder.java#Holder.task.<class@"));
  assert.deepEqual([anon?.visibility, anon?.exported], [null, false]);
  assert.ok(!rows("doc", r).some((d) => String(d.symbol).startsWith("p/Holder.java#Holder.task.")));
  // A record component's field is private; the component is its public accessor.
  assert.deepEqual(flags("p/Holder.java#Holder.Pair.left"), ["property", "record_component", "public", true, false, false, true]);
  // A JUnit 5 composed annotation makes a test; an annotation type that did not resolve is judged by its name.
  assert.deepEqual(rows("entry_point", r).map((e) => [e.symbol, e.kind]), [
    ["p/Tests.java#Tests.composed", "test"],
    ["p/Tests.java#Tests.unresolved", "test"],
  ]);
  assert.deepEqual(rows("param", r).filter((p) => p.fn === "p/Tests.java#Tests.helper").map((p) => [p.name, p.rest]), [["names", true]]);
});

const WIDGET = {
  "src/main/java/p/Widget.java": "package p;\n\nimport com.acme.gen.Factory;\n\n@Factory\npublic class Widget {\n  public Widget() {}\n}\n",
  "src/main/java/p/Use.java": "package p;\n\npublic class Use {\n  Widget make() {\n    return WidgetFactory.create();\n  }\n}\n",
};

test("annotation processors run as the Maven build configures them; what they generate is extracted, as generated", { skip: NO_MAVEN }, () => {
  const dir = tempDir("java-processors");
  writeFiles(dir, {
    "pom.xml": POM(
      "  <groupId>g</groupId>\n  <artifactId>app</artifactId>\n  <version>1</version>\n  <properties><maven.compiler.release>17</maven.compiler.release></properties>\n" +
        "  <dependencies>\n    <dependency><groupId>com.acme</groupId><artifactId>gen</artifactId><version>1.0</version><scope>provided</scope></dependency>\n  </dependencies>\n" +
        "  <build><plugins><plugin>\n    <groupId>org.apache.maven.plugins</groupId><artifactId>maven-compiler-plugin</artifactId>\n" +
        "    <configuration><annotationProcessorPaths><path><groupId>com.acme</groupId><artifactId>gen</artifactId><version>1.0</version></path></annotationProcessorPaths></configuration>\n" +
        "  </plugin></plugins></build>",
    ),
    ...WIDGET,
    // An earlier build's output that the processors no longer write.
    "target/generated-sources/annotations/p/Stale.java": "package p;\n\nclass Stale {}\n",
  });
  const stale = path.join(dir, "target/generated-sources/annotations/p/Stale.java");
  fs.utimesSync(stale, new Date(2020, 0, 1), new Date(2020, 0, 1));
  const r = withMavenRepo(() => extractJava(dir, { out: path.join(dir, "out"), layers: ["quality"] }));
  // Processors run twice, and the second pass finds what the first wrote: that is no diagnostic of the code's.
  assert.deepEqual(rows("diagnostic", r), []);
  assert.deepEqual(rows("file", r).map((f) => [f.path, f.is_generated]), [
    ["src/main/java/p/Use.java", false],
    ["src/main/java/p/Widget.java", false],
    // Written where the build writes it.
    ["target/generated-sources/annotations/p/WidgetFactory.java", true],
  ]);
  assert.ok(fs.existsSync(path.join(dir, "target/generated-sources/annotations/p/WidgetFactory.java")));
  assert.deepEqual(rows("imports", r).map((i) => [i.file, i.kind, i.target_file, i.target_package]), [
    ["src/main/java/p/Use.java", "implicit", "src/main/java/p/Widget.java", null],
    ["src/main/java/p/Use.java", "implicit", "target/generated-sources/annotations/p/WidgetFactory.java", null],
    ["src/main/java/p/Widget.java", "static", null, "com.acme:gen"],
    ["target/generated-sources/annotations/p/WidgetFactory.java", "implicit", "src/main/java/p/Widget.java", null],
  ]);
});

test("Gradle's annotation processor path runs too, into the build's generated sources directory", { skip: NO_GRADLE }, () => {
  const dir = tempDir("java-gradle-processors");
  const gen = path.join(mavenRepo(), "com/acme/gen/1.0/gen-1.0.jar");
  for (const f of ["gradlew", "gradle/wrapper/gradle-wrapper.jar", "gradle/wrapper/gradle-wrapper.properties"]) {
    fs.mkdirSync(path.dirname(path.join(dir, f)), { recursive: true });
    fs.copyFileSync(path.join(fixture("java-gradle"), f), path.join(dir, f));
  }
  fs.chmodSync(path.join(dir, "gradlew"), 0o755);
  writeFiles(dir, {
    "settings.gradle": "rootProject.name = 'procs'\n",
    "build.gradle": `plugins {\n    id 'java'\n}\n\ndependencies {\n    compileOnly files('${gen}')\n    annotationProcessor files('${gen}')\n}\n`,
    ...WIDGET,
  });
  const r = extractJava(dir, { out: path.join(dir, "out"), layers: ["quality"] });
  assert.deepEqual(rows("diagnostic", r), []);
  assert.deepEqual(rows("file", r).map((f) => [f.path, f.is_generated]), [
    ["build/generated/sources/annotationProcessor/java/main/p/WidgetFactory.java", true],
    ["src/main/java/p/Use.java", false],
    ["src/main/java/p/Widget.java", false],
  ]);
  assert.deepEqual(rows("imports", r).filter((i) => i.kind === "implicit").map((i) => [i.file, i.target_file]), [
    ["build/generated/sources/annotationProcessor/java/main/p/WidgetFactory.java", "src/main/java/p/Widget.java"],
    ["src/main/java/p/Use.java", "src/main/java/p/Widget.java"],
    ["src/main/java/p/Use.java", "build/generated/sources/annotationProcessor/java/main/p/WidgetFactory.java"],
  ]);
});

test("diagnostics are what javac reports with the build's lint options, by javac's key, once each", { skip: NO_MAVEN }, () => {
  const dir = tempDir("java-diagnostics");
  writeFiles(dir, {
    "pom.xml": POM(
      "  <groupId>g</groupId>\n  <artifactId>lint</artifactId>\n  <version>1</version>\n" +
        "  <build><plugins><plugin>\n    <groupId>org.apache.maven.plugins</groupId><artifactId>maven-compiler-plugin</artifactId>\n" +
        "    <configuration><compilerArgs><arg>-Xlint:deprecation</arg></compilerArgs></configuration>\n  </plugin></plugins></build>",
    ),
    "src/main/java/p/Old.java": "package p;\n\npublic class Old {\n  @Deprecated\n  public static void run() {}\n}\n",
    "src/main/java/p/Use.java": "package p;\n\nclass Use {\n  void go() {\n    new java.util.Date(2020, 1, 1);\n    missing();\n  }\n}\n",
    // Reads main from its source path, and reports nothing of main's again.
    "src/test/java/p/UseTest.java": "package p;\n\nclass UseTest {\n  Use use;\n}\n",
  });
  const r = withMavenRepo(() => extractJava(dir, { out: path.join(dir, "out"), layers: ["quality"] }), tempDir("m2-empty"));
  // In the order javac reports them: deprecation after attribution.
  assert.deepEqual(rows("diagnostic", r).map((d) => [d.file, d.line, d.code, d.category, d.key]), [
    ["src/main/java/p/Use.java", 6, 0, "error", "compiler.err.cant.resolve.location.args"],
    ["src/main/java/p/Use.java", 5, 0, "warning", "compiler.warn.has.been.deprecated"],
  ]);
});

test("sources are read in the encoding the build names", { skip: NO_MAVEN }, () => {
  const dir = tempDir("java-encoding");
  writeFiles(dir, {
    "pom.xml": POM("  <groupId>g</groupId>\n  <artifactId>enc</artifactId>\n  <version>1</version>\n  <properties><project.build.sourceEncoding>ISO-8859-1</project.build.sourceEncoding></properties>"),
  });
  fs.mkdirSync(path.join(dir, "src/main/java/p"), { recursive: true });
  fs.writeFileSync(path.join(dir, "src/main/java/p/Menu.java"), Buffer.from("package p;\n\npublic class Menu {\n  int caf\u00e9;\n}\n", "latin1"));
  const r = withMavenRepo(() => extractJava(dir, { out: path.join(dir, "out"), layers: [] }), tempDir("m2-empty"));
  assert.ok(rows("symbol", r).some((s) => s.id === "src/main/java/p/Menu.java#Menu.caf\u00e9"), "the field's name, decoded as the build decodes it");
});

test("sources a build plugin generated are read where the last build left them, marked generated", { skip: NO_MAVEN }, () => {
  const dir = tempDir("java-codegen");
  writeFiles(dir, {
    "pom.xml": POM("  <groupId>g</groupId>\n  <artifactId>client</artifactId>\n  <version>1</version>"),
    "src/main/java/p/Client.java": "package p;\n\npublic class Client {\n  Proto proto;\n}\n",
    "target/generated-sources/proto/p/Proto.java": "package p;\n\npublic class Proto {}\n",
  });
  const r = withMavenRepo(() => extractJava(dir, { out: path.join(dir, "out"), layers: [] }), tempDir("m2-empty"));
  assert.deepEqual(rows("file", r).map((f) => [f.path, f.is_generated]), [
    ["src/main/java/p/Client.java", false],
    ["target/generated-sources/proto/p/Proto.java", true],
  ]);
  assert.deepEqual(rows("imports", r).map((i) => [i.file, i.kind, i.target_file]), [["src/main/java/p/Client.java", "implicit", "target/generated-sources/proto/p/Proto.java"]]);
});

