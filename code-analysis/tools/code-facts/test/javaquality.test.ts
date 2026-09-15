// The Java frontend's quality layer, on test/fixtures/java-quality: suppressions
// written as annotations and as comments, markers, raw types, casts, literals,
// throws and catches, and a future nothing keeps — with javac's own rawtypes lint
// as the oracle for where a raw type is written.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import { extractJava, fixture, javaAvailable, tempDir, writeFiles } from "./helpers.ts";

const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";
const quality = NO_JAVA === false ? extractJava(fixture("java-quality"), { out: tempDir("java-quality"), layers: ["refs", "quality"] }) : undefined;
const rows = (rel: string) => quality?.tables.rows(rel) ?? [];
const cols = (rel: string, ...names: string[]) => rows(rel).map((r) => names.map((n) => r[n]));
const S = "q/Suppressed.java#Suppressed";
const T = "q/Types.java#Types";

test("diagnostic: what javac reports, by its key", { skip: NO_JAVA }, () => {
  assert.deepEqual(cols("diagnostic", "file", "line", "code", "category", "key"), [
    ["q/Suppressed.java", 3, 0, "error", "compiler.err.doesnt.exist"],
    ["q/Suppressed.java", 8, 0, "error", "compiler.err.cant.resolve.location"],
  ]);
});

test("lint_directive: comments but not a string's text; @SuppressWarnings a row per tool; @SuppressFBWarnings by name when unresolved", { skip: NO_JAVA }, () => {
  assert.deepEqual(cols("lint_directive", "file", "line", "tool", "directive", "rules"), [
    // Line 12's string holds `// NOSONAR` too: one row, the comment's.
    ["q/Suppressed.java", 12, "sonar", "nosonar", null],
    ["q/Suppressed.java", 13, "pmd", "nopmd", null],
    ["q/Suppressed.java", 14, "checkstyle", "off", null],
    ["q/Suppressed.java", 15, "checkstyle", "suppress", "MagicNumber"],
    ["q/Suppressed.java", 16, "checkstyle", "on", null],
    ["q/Suppressed.java", 17, "spotless", "off", null],
    ["q/Suppressed.java", 6, "javac", "SuppressWarnings", "unchecked"],
    ["q/Suppressed.java", 6, "checkstyle", "SuppressWarnings", "checkstyle:magicnumber"],
    ["q/Suppressed.java", 6, "sonar", "SuppressWarnings", "java:S106"],
    ["q/Suppressed.java", 6, "pmd", "SuppressWarnings", "PMD.SystemPrintln"],
    ["q/Suppressed.java", 8, "spotbugs", "SuppressFBWarnings", "EI_EXPOSE_REP"],
  ]);
});

test("comment_marker: in a Javadoc comment, a line comment, and a line of a block comment", { skip: NO_JAVA }, () => {
  assert.deepEqual(cols("comment_marker", "line", "kind", "text"), [
    [5, "todo", "document the rest"],
    [18, "fixme", "split this"],
    [20, "hack", "around the cache"],
  ]);
});

test("any_site: raw types written — not a class literal, a qualifier, a cast, instanceof or var — and a call returning one", { skip: NO_JAVA }, () => {
  assert.deepEqual(cols("any_site", "fn", "line", "kind"), [
    [`${T}.raw`, 10, "raw"],
    [`${T}.legacy`, 14, "raw"],
    // `List made = new ArrayList();`: two raw types, one fact of a line.
    [`${T}.cast`, 20, "raw"],
    [`${T}.cast`, 23, "raw"],
    // `var it = legacy();`
    [`${T}.cast`, 25, "call_result"],
    [`${T}.Pair.left`, 51, "raw"],
    [`${T}.Legacy`, 53, "raw"],
    [`${T}.Legacy.nested`, 54, "raw"],
    // An anonymous class's raw supertype.
    [`${T}.Legacy.anonymous`, 55, "raw"],
  ]);
});

test("any_site raw is where javac's own rawtypes lint warns", { skip: NO_JAVA }, () => {
  const dir = fixture("java-quality");
  const out = tempDir("java-quality-lint");
  const r = spawnSync("javac", ["-Xlint:rawtypes", "-XDrawDiagnostics", "-implicit:none", "-d", out, "q/Types.java", "q/Suppressed.java"], { cwd: dir, encoding: "utf8" });
  // A row is a line's: javac warns of each position, and twice of a record component's.
  const warned = new Set<string>();
  for (const m of `${r.stdout}${r.stderr}`.matchAll(/^(\S+\.java):(\d+):\d+: compiler\.warn\.raw\.class\.use/gm)) warned.add(`${m[1]}:${m[2]}`);
  assert.ok(warned.size > 0, `javac warned of no raw type:\n${r.stderr}`);
  const ours = rows("any_site").filter((a) => a.kind === "raw").map((a) => `${path.basename(String(a.file))}:${a.line}`);
  assert.deepEqual(ours.sort(), [...warned].sort());
  fs.rmSync(out, { recursive: true, force: true });
});

test("assertion: each cast, to a raw type or from one", { skip: NO_JAVA }, () => {
  assert.deepEqual(cols("assertion", "fn", "line", "kind", "to_type", "from_any", "to_any"), [
    [`${T}.cast`, 22, "cast", "List<String>", false, false],
    [`${T}.cast`, 23, "cast", "List", false, true],
    [`${T}.cast`, 24, "cast", "int", false, false],
  ]);
});

test("literal: strings, characters, text blocks and numbers as written; not an annotation's arguments, true, false or null", { skip: NO_JAVA }, () => {
  assert.deepEqual(cols("literal", "fn", "line", "kind", "value"), [
    [`${S}.values`, 9, "number", "1"],
    [`${S}.values`, 9, "number", "2"],
    [`${S}.run`, 12, "string", "// NOSONAR is no comment in a string"],
    [`${S}.run`, 13, "number", "42"],
    [`${S}.run`, 22, "number", "0"],
    [`${T}.cast`, 24, "number", "3.5"],
    [`${T}.cast`, 26, "string", "x"],
    [`${T}.cast`, 27, "number", "0xFFL"],
    [`${T}.cast`, 28, "string", "hello\n"],
    [`${T}.cast`, 31, "number", "0"],
    [`${T}.fail`, 36, "number", "0"],
    [`${T}.fail`, 36, "string", "bad"],
    [`${T}.fail`, 38, "string", "kept"],
  ]);
});

test("throw_site is typed by what javac says is thrown; catch_site's rethrow is its own block's, not a lambda's", { skip: NO_JAVA }, () => {
  assert.deepEqual(cols("throw_site", "fn", "line", "type"), [
    [`${T}.fail`, 36, "ext:java.lang#IllegalStateException"],
    // `throw e` from a multi-catch: a union, no one class.
    [`${T}.fail`, 41, null],
    [`${T}.fail.<lambda@44:20>`, 45, "ext:java.lang#RuntimeException"],
  ]);
  assert.deepEqual(cols("catch_site", "fn", "line", "binds", "empty", "rethrows"), [
    [`${T}.fail`, 40, true, false, true],
    [`${T}.fail`, 42, true, true, false],
    [`${T}.fail`, 43, true, false, false],
  ]);
});

test("floating_promise: a future made as a statement, at the call site refs names", { skip: NO_JAVA }, () => {
  assert.deepEqual(cols("floating_promise", "fn", "line"), [[`${T}.fail`, 37]]);
  const site = rows("call_site").find((c) => c.id === rows("floating_promise")[0]?.call_site);
  assert.deepEqual([site?.callee_name, site?.line], ["runAsync", 37]);
});

const JDK = Number.parseInt(/(\d+)/.exec(spawnSync("javac", ["-version"], { encoding: "utf8" }).stdout ?? "")?.[1] ?? "0", 10);

test("catch_site: an unnamed catch parameter binds nothing", { skip: NO_JAVA || (JDK < 22 ? "needs JDK 22 for unnamed variables" : false) }, () => {
  const dir = tempDir("java-unnamed");
  writeFiles(dir, { "p/U.java": "package p;\n\nclass U {\n  void f() {\n    try {\n      f();\n    } catch (RuntimeException _) {\n      f();\n    }\n  }\n}\n" });
  const r = extractJava(dir, { out: path.join(dir, "out"), layers: ["quality"] });
  assert.deepEqual(r.tables.rows("catch_site").map((c) => [c.line, c.binds, c.empty, c.rethrows]), [[7, false, false, false]]);
  fs.rmSync(dir, { recursive: true, force: true });
});
