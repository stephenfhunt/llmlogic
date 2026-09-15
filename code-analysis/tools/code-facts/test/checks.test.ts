// lib/checks.dl finds nothing wrong with the facts extracted from every fixture,
// from this tool's own source, from ~/code/tsdl when it is there — a real
// TypeScript 7 project the tool reads with its own TypeScript 6 — and from
// sqlparse when the experiments harness has cached it.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { test } from "node:test";
import { run } from "../src/main.ts";
import { datalog, engineAvailable, FIXED_TIME, FIXTURES, gradleAvailable, javaAvailable, mavenAvailable, TOOL_DIR, tempDir, withMavenRepo } from "./helpers.ts";

function checkClean(label: string, tsconfigs: string[], root: string, python: string[] = [], java: string[] = []): void {
  const out = tempDir(`checks-${label}`);
  run({ tsconfigs, python, java, root, out, time: FIXED_TIME });
  const r = datalog(path.join(out, "lib", "checks.dl"));
  assert.equal(r.code, 1, `${label}: violations (exit ${r.code}):\n${r.stdout}${r.stderr}`);
  fs.rmSync(out, { recursive: true, force: true });
}

const skip = !engineAvailable();

for (const name of fs.readdirSync(FIXTURES).sort()) {
  const dir = path.join(FIXTURES, name);
  if (fs.existsSync(path.join(dir, "tsconfig.json"))) {
    test(`checks.dl is clean on fixture ${name}`, { skip }, () => checkClean(name, [path.join(dir, "tsconfig.json")], dir));
  } else if (fs.existsSync(path.join(dir, "pyproject.toml"))) {
    test(`checks.dl is clean on Python fixture ${name}`, { skip }, () => checkClean(name, [], dir, [dir]));
  } else if (fs.existsSync(path.join(dir, "pom.xml"))) {
    const noMaven = skip || !javaAvailable() || !mavenAvailable() ? "no engine, JDK or Maven" : false;
    test(`checks.dl is clean on Java fixture ${name}`, { skip: noMaven }, () => withMavenRepo(() => checkClean(name, [], dir, [], [dir])));
  } else if (fs.existsSync(path.join(dir, "settings.gradle"))) {
    const noGradle = skip || !javaAvailable() || !gradleAvailable(dir) ? "no engine or JDK, or the Gradle wrapper cannot run" : false;
    test(`checks.dl is clean on Java fixture ${name}`, { skip: noGradle }, () => checkClean(name, [], dir, [], [dir]));
  }
}

test("checks.dl is clean on Java fixture java-plain", { skip: skip || !javaAvailable() ? "no engine or JDK" : false }, () =>
  checkClean("java-plain", [], path.join(FIXTURES, "java-plain"), [], [path.join(FIXTURES, "java-plain")]),
);

test("checks.dl is clean on code-facts itself", { skip }, () => checkClean("self", [path.join(TOOL_DIR, "tsconfig.json")], TOOL_DIR));

const tsdl = path.join(os.homedir(), "code", "tsdl");
const tsdlConfigs = ["tsconfig.json", "tsconfig.test.json"].map((c) => path.join(tsdl, c));
test("checks.dl is clean on tsdl", { skip: skip || !tsdlConfigs.every((c) => fs.existsSync(c)) ? "no engine, or no ~/code/tsdl" : false }, () =>
  checkClean("tsdl", tsdlConfigs, tsdl),
);

// The harness caches the package itself, so the root is its parent and paths read `sqlparse/…`.
const sqlparse = path.join(os.homedir(), ".cache", "llmlogic-experiments", "corpora", "sqlparse-0.6.0");
test("checks.dl is clean on sqlparse", { skip: skip || !fs.existsSync(sqlparse) ? "no engine, or no cached sqlparse" : false }, () => {
  const proj = tempDir("sqlparse");
  fs.cpSync(sqlparse, path.join(proj, "sqlparse"), { recursive: true });
  checkClean("sqlparse", [], proj, [proj]);
  fs.rmSync(proj, { recursive: true, force: true });
});
