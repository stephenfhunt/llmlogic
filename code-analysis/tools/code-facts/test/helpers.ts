// Shared test plumbing: run the extractor on a directory, and run the engine on
// its output. Deliberately thin — assertions stay in the tests, spelled out.

import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { run, type Result } from "../src/main.ts";
import type { Layer } from "../src/schema.ts";

export const TOOL_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const FIXTURES = path.join(TOOL_DIR, "test", "fixtures");
export const FIXED_TIME = new Date("2026-01-01T00:00:00Z");

export const DATALOG =
  process.env.DATALOG_BIN ?? path.resolve(TOOL_DIR, "..", "..", "..", "datalog", "target", "release", "datalog");

export function tempDir(label: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), `code-facts-${label}-`));
}

/** Extract a project directory (its tsconfig.json) with the root pinned to it. */
export function extract(dir: string, opts: { out?: string; layers?: Layer[]; tsconfigs?: string[] } = {}): Result {
  return run({
    tsconfigs: opts.tsconfigs ?? [path.join(dir, "tsconfig.json")],
    root: dir,
    out: opts.out,
    layers: opts.layers !== undefined ? new Set<Layer>(["meta", "structure", ...opts.layers]) : undefined,
    time: FIXED_TIME,
  });
}

export function fixture(name: string): string {
  return path.join(FIXTURES, name);
}

export interface EngineRun {
  code: number;
  stdout: string;
  stderr: string;
}

/** Run the datalog engine: `datalog <file> -q <query>…`. */
export function datalog(file: string, queries: string[] = [], cwd?: string): EngineRun {
  const args = [file];
  for (const q of queries) args.push("-q", q);
  const r = spawnSync(DATALOG, args, { encoding: "utf8", cwd, maxBuffer: 1 << 28 });
  if (r.error !== undefined) throw r.error;
  return { code: r.status ?? -1, stdout: r.stdout, stderr: r.stderr };
}

export function engineAvailable(): boolean {
  return fs.existsSync(DATALOG);
}

/** Write a small project: `files` maps relative paths to contents; a default tsconfig is added. */
export function writeProject(dir: string, files: Record<string, string>, tsconfig?: object): void {
  const config = tsconfig ?? {
    compilerOptions: {
      target: "es2022",
      module: "nodenext",
      moduleResolution: "nodenext",
      strict: true,
      noEmit: true,
      skipLibCheck: true,
      types: [],
    },
    include: ["src"],
  };
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "tsconfig.json"), JSON.stringify(config, null, 2));
  for (const [rel, text] of Object.entries(files)) {
    const abs = path.join(dir, rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, text);
  }
}

/** Every file under `dir`, relative, sorted, with contents — for byte-identity checks. */
export function snapshotDir(dir: string): Map<string, string> {
  const out = new Map<string, string>();
  const walk = (d: string) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const abs = path.join(d, e.name);
      if (e.isDirectory()) walk(abs);
      else out.set(path.relative(dir, abs), fs.readFileSync(abs, "utf8"));
    }
  };
  walk(dir);
  return out;
}
