// lib/checks.dl finds nothing wrong with the facts extracted from every fixture,
// from this tool's own source, and from ~/code/tsdl when it is there — a real
// TypeScript 7 project the tool reads with its own TypeScript 6.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { test } from "node:test";
import { run } from "../src/main.ts";
import { datalog, engineAvailable, FIXED_TIME, FIXTURES, TOOL_DIR, tempDir } from "./helpers.ts";

function checkClean(label: string, tsconfigs: string[], root: string): void {
  const out = tempDir(`checks-${label}`);
  run({ tsconfigs, root, out, time: FIXED_TIME });
  const r = datalog(path.join(out, "lib", "checks.dl"));
  assert.equal(r.code, 1, `${label}: violations (exit ${r.code}):\n${r.stdout}${r.stderr}`);
  fs.rmSync(out, { recursive: true, force: true });
}

const skip = !engineAvailable();

for (const name of fs.readdirSync(FIXTURES).sort()) {
  const dir = path.join(FIXTURES, name);
  if (!fs.existsSync(path.join(dir, "tsconfig.json"))) continue;
  test(`checks.dl is clean on fixture ${name}`, { skip }, () => checkClean(name, [path.join(dir, "tsconfig.json")], dir));
}

test("checks.dl is clean on code-facts itself", { skip }, () => checkClean("self", [path.join(TOOL_DIR, "tsconfig.json")], TOOL_DIR));

const tsdl = path.join(os.homedir(), "code", "tsdl");
const tsdlConfigs = ["tsconfig.json", "tsconfig.test.json"].map((c) => path.join(tsdl, c));
test("checks.dl is clean on tsdl", { skip: skip || !tsdlConfigs.every((c) => fs.existsSync(c)) ? "no engine, or no ~/code/tsdl" : false }, () =>
  checkClean("tsdl", tsdlConfigs, tsdl),
);
