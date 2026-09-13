// Everything the skill ships is read by an agent working in someone else's
// project, with none of this repository's history: SKILL.md, reference/, the
// library's headers, and the text the extractor writes into an output directory
// or prints. Provenance — defect ids, dates, the codebases the tools were tried
// on, paths into this repository — belongs in decisions.md, bugs/ and notes/.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const TOOL = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SKILL = path.resolve(TOOL, "../../skill");

const LEAKS: [string, RegExp][] = [
  ["a defect id", /\bbugs\/\d/],
  ["dogfooding", /dogfood/i],
  ["a decisions-log section", /§\s*17\b/],
  ["a project document", /\b(spec\.md|decisions\.md|ROADMAP|worklog|AGENTS\.md|package\.sh)|\bnotes\//],
  ["a subject nobody introduced", /\b(project|crate|codebase) above\b/i],
  ["a codebase the tools were tried on", /\b(tsdl|sqlparse|grafana|vs ?code|vs\/base|llmlogic)\b/i],
  ["the experiments", /\bexperiments?\b|\bH-CA\d/i],
  ["a dated event", /(?<![@\d-])\b20\d\d-\d\d-\d\d\b/],
  ["a link out of the bundle", /\]\(\.\.\//],
];

test("each leak pattern catches its leak and passes its lookalike", () => {
  const leaks = [
    "(bugs/004)",
    "found dogfooding",
    "spec §17",
    "see decisions.md",
    "on the project above",
    "on `@grafana/ui`",
    "the experiments' answer key",
    "measured 2026-09-12",
    "[guide](../docs/agent-skill.md)",
  ];
  leaks.forEach((text, i) => assert.ok(LEAKS[i]![1].test(text), `${LEAKS[i]![0]} misses ${text}`));
  const lookalikes = ["`@2024-03-01T10:30:00`", "--disable-warning=ExperimentalWarning", "the flow layer's notes"];
  for (const text of lookalikes) {
    for (const [what, re] of LEAKS) assert.ok(!re.test(text), `${what} flags ${text}`);
  }
});

test("nothing the skill ships or the extractor writes carries this repository's history", () => {
  const md = (dir: string) => fs.readdirSync(dir).filter((f) => f.endsWith(".md")).map((f) => path.join(dir, f));
  const dl = fs.readdirSync(path.join(TOOL, "lib")).filter((f) => f.endsWith(".dl")).map((f) => path.join(TOOL, "lib", f));
  const whole = [path.join(SKILL, "SKILL.md"), path.join(SKILL, "code-facts"), ...md(path.join(SKILL, "reference")), ...dl, path.join(TOOL, "src/schema.ts")];
  assert.ok(whole.length >= 25, `only ${whole.length} files found to scan`);
  // writer.ts and main.ts are code; only the lines that emit text are shipped.
  const emitting = [path.join(TOOL, "src/writer.ts"), path.join(TOOL, "src/main.ts")];
  const emits = (line: string) => /lines\.push\(|console\.(log|error)\(|log\(`|new Error\(/.test(line);

  const hits: string[] = [];
  const scan = (file: string, keep: (line: string) => boolean) => {
    fs.readFileSync(file, "utf8")
      .split("\n")
      .forEach((line, i) => {
        if (!keep(line)) return;
        for (const [what, re] of LEAKS) {
          if (re.test(line)) hits.push(`${path.relative(TOOL, file)}:${i + 1}: ${what}: ${line.trim()}`);
        }
      });
  };
  for (const file of whole) scan(file, () => true);
  for (const file of emitting) scan(file, emits);
  assert.deepEqual(hits, []);
});
