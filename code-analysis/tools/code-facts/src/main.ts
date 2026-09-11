// code-facts — extract a TypeScript project into Datalog fact tables.
//
//   code-facts <tsconfig>... [-o DIR] [--root DIR] [--layers L,...] [--no-git]
//            [--git-since DATE] [--git-max-commits N] [--exclude GLOB]...
//
// `run()` is the whole pipeline and is what the tests call; the CLI below only
// parses arguments and prints the summary.

import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { Context } from "./context.ts";
import { extractDataflow } from "./layers/dataflow.ts";
import { extractFlow } from "./layers/flow.ts";
import { extractGit, gitHead } from "./layers/git.ts";
import { extractQuality } from "./layers/quality.ts";
import { extractRefs } from "./layers/refs.ts";
import { emitDirectories, extractStructure, Packages } from "./layers/structure.ts";
import { findRoot, globToRegExp, load, relTo } from "./program.ts";
import { type Layer, LAYERS, OPTIONAL_LAYERS } from "./schema.ts";
import { Tables } from "./writer.ts";

export const TOOL_VERSION = "0.1.0";
const HERE = path.dirname(fileURLToPath(import.meta.url));
export const LIB_DIR = path.join(HERE, "..", "lib");
const PY_FRONTEND = path.join(HERE, "frontends", "python", "py_facts.py");

export interface Options {
  /** TypeScript: tsconfig files (or directories holding one). */
  tsconfigs: string[];
  /** Python: source roots (directories, or a pyproject.toml). */
  python?: string[] | undefined;
  out?: string | undefined;
  root?: string | undefined;
  layers?: ReadonlySet<Layer> | undefined;
  exclude?: string[] | undefined;
  gitSince?: string | undefined;
  gitMaxCommits?: number | undefined;
  /** Fixed extraction time, for byte-identical output in tests. */
  time?: Date | undefined;
  log?: ((line: string) => void) | undefined;
}

export interface Result {
  tables: Tables;
  root: string;
  layers: ReadonlySet<Layer>;
  timings: [string, number][];
}

export function run(opts: Options): Result {
  const log = opts.log ?? (() => {});
  const layers = opts.layers ?? new Set(LAYERS);
  const timings: [string, number][] = [];
  const timed = <T>(label: string, f: () => T): T => {
    const start = performance.now();
    const v = f();
    const ms = performance.now() - start;
    timings.push([label, ms]);
    log(`code-facts: ${label} ${(ms / 1000).toFixed(2)}s`);
    return v;
  };

  const python = opts.python ?? [];
  const exclude = (opts.exclude ?? []).map(globToRegExp);
  // A Python root is a directory; findRoot reads each target's directory.
  const anchors = [...opts.tsconfigs, ...python.map((p) => (fs.existsSync(p) && fs.statSync(p).isDirectory() ? path.join(p, "__target__") : p))];
  const root = opts.root !== undefined ? path.resolve(opts.root) : findRoot(anchors);
  const tables = new Tables();
  let callSites = 1;
  let flowNodes = 1;

  if (opts.tsconfigs.length > 0) {
    const loaded = timed("load", () => load(opts.tsconfigs, root, exclude));
    const packages = new Packages(loaded.root);
    const ctx = new Context(loaded, tables, (f) => packages.nameOf(f));
    timed("ids", () => ctx.prepass());
    timed("structure", () => extractStructure(ctx, packages));
    if (layers.has("refs")) timed("refs", () => extractRefs(ctx));
    if (layers.has("flow")) timed("flow", () => extractFlow(ctx));
    if (layers.has("dataflow")) timed("dataflow", () => extractDataflow(ctx));
    if (layers.has("quality")) timed("quality", () => extractQuality(ctx));
    ctx.flushSymbols();
    callSites = ctx.callSitesUsed;
    flowNodes = ctx.nextFlowNode;
  }
  let pythonVersion: string | null = null;
  if (python.length > 0) {
    pythonVersion = timed("python", () => runPython(python, root, layers, exclude, callSites, flowNodes, tables));
  }
  emitDirectories(tables);
  if (layers.has("git")) {
    timed("git", () => extractGit({ root, tables }, { since: opts.gitSince, maxCommits: opts.gitMaxCommits ?? 20000 }));
  }

  const time = (opts.time ?? new Date()).toISOString().slice(0, 19);
  tables.add("extraction", {
    tool_version: TOOL_VERSION,
    typescript_version: ts.version,
    python_version: pythonVersion,
    node_version: process.versions.node,
    root,
    targets: [...opts.tsconfigs, ...python].map((c) => relTo(root, path.resolve(c))).join(","),
    layers: LAYERS.filter((l) => layers.has(l)).join(","),
    time,
    git_head: gitHead(root),
  });
  tables.dedupe();
  if (opts.out !== undefined) timed("write", () => tables.write(opts.out as string, layers, LIB_DIR));
  return { tables, root, layers, timings };
}

/**
 * The Python frontend, `frontends/python/py_facts.py`: it streams rows as JSON
 * lines, and each one is validated here against schema.ts like any other. Its
 * call-site and flow-node ids continue from the TypeScript side's.
 */
function runPython(
  targets: string[],
  root: string,
  layers: ReadonlySet<Layer>,
  exclude: RegExp[],
  firstCallSite: number,
  firstFlowNode: number,
  tables: Tables,
): string {
  const args = [
    PY_FRONTEND,
    "--root",
    root,
    "--layers",
    [...layers].join(","),
    "--first-call-site",
    String(firstCallSite),
    "--first-flow-node",
    String(firstFlowNode),
    ...exclude.flatMap((re) => ["--exclude", re.source]),
    ...targets.map((t) => path.resolve(t)),
  ];
  const python = process.env.CODE_FACTS_PYTHON ?? "python3";
  // No __pycache__ written beside the frontend: it may live in an installed skill.
  const r = spawnSync(python, args, { encoding: "utf8", maxBuffer: 1 << 30, env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" } });
  if (r.error !== undefined) throw new Error(`code-facts: cannot run ${python} for the Python frontend: ${r.error.message}`);
  if (r.status !== 0) throw new Error(`code-facts: the Python frontend failed (exit ${r.status}):\n${r.stderr}`);
  for (const line of r.stdout.split("\n")) {
    if (line === "") continue;
    const { relation, row } = JSON.parse(line) as { relation: string; row: Record<string, string | number | boolean | null> };
    if (relation === "__counters__") continue;
    tables.add(relation, row);
  }
  const v = spawnSync(python, ["-c", "import sys; print('.'.join(map(str, sys.version_info[:3])))"], { encoding: "utf8" });
  return v.stdout.trim();
}

/** A pyproject.toml, or a directory with no tsconfig.json in it. */
function isPythonTarget(a: string): boolean {
  if (path.basename(a) === "pyproject.toml") return true;
  return fs.existsSync(a) && fs.statSync(a).isDirectory() && !fs.existsSync(path.join(a, "tsconfig.json"));
}

function usage(): string {
  return [
    "usage: code-facts <tsconfig.json | python-root | pyproject.toml>... [options]",
    "",
    "  A tsconfig (or a directory holding one) is read as TypeScript; any other",
    "  directory, or a pyproject.toml, as a Python source root. Both may be given.",
    "",
    "  -o, --out DIR          output directory (default ./code-facts-out)",
    "  --root DIR             root every path is relative to (default: the git top-level)",
    `  --layers L,...         layers to extract (default all: ${OPTIONAL_LAYERS.join(",")}); structure always runs`,
    "  --no-git               skip the git history layer",
    "  --git-since DATE       only commits after DATE (anything `git log --since` takes)",
    "  --git-max-commits N    most recent N commits (default 20000; 0 = all)",
    "  --exclude GLOB         skip files matching GLOB (repo-relative; repeatable)",
    "",
    "Writes facts/*.jsonl, schema/*.dl (import these), lib/*.dl (rule library), SCHEMA.md.",
  ].join("\n");
}

function parseArgs(argv: string[]): Options {
  const tsconfigs: string[] = [];
  const python: string[] = [];
  const exclude: string[] = [];
  let out = "code-facts-out";
  let root: string | undefined;
  let layers = new Set<Layer>(LAYERS);
  let gitSince: string | undefined;
  let gitMaxCommits: number | undefined;
  const value = (i: number, flag: string): string => {
    const v = argv[i];
    if (v === undefined) throw new Error(`code-facts: ${flag} needs a value`);
    return v;
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i] ?? "";
    if (a === "-h" || a === "--help") {
      console.log(usage());
      process.exit(0);
    } else if (a === "-o" || a === "--out") out = value(++i, a);
    else if (a === "--root") root = value(++i, a);
    else if (a === "--exclude") exclude.push(value(++i, a));
    else if (a === "--no-git") layers.delete("git");
    else if (a === "--git-since") gitSince = value(++i, a);
    else if (a === "--git-max-commits") gitMaxCommits = Number.parseInt(value(++i, a), 10);
    else if (a === "--layers") {
      const wanted = value(++i, a)
        .split(",")
        .map((s) => s.trim())
        .filter((s) => s !== "");
      for (const w of wanted) {
        if (!(OPTIONAL_LAYERS as readonly string[]).includes(w) && w !== "structure") {
          throw new Error(`code-facts: unknown layer \`${w}\` (layers: structure,${OPTIONAL_LAYERS.join(",")})`);
        }
      }
      layers = new Set<Layer>(["meta", "structure", ...(wanted.filter((w) => w !== "structure") as Layer[])]);
    } else if (a.startsWith("-")) throw new Error(`code-facts: unknown option ${a}\n\n${usage()}`);
    else if (isPythonTarget(a)) python.push(a);
    else tsconfigs.push(a);
  }
  if (tsconfigs.length + python.length === 0) throw new Error(`code-facts: give a tsconfig or a Python root\n\n${usage()}`);
  return { tsconfigs, python, out, root, layers, exclude, gitSince, gitMaxCommits };
}

function main(): void {
  let opts: Options;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (e) {
    console.error((e as Error).message);
    process.exit(2);
  }
  const started = performance.now();
  const result = run({ ...opts, log: (l) => console.error(l) });
  const out = path.resolve(opts.out ?? "code-facts-out");
  const rows = result.tables.rows("relation_rows");
  const total = rows.reduce((n, r) => n + (r.rows as number), 0);
  console.log(`code-facts ${TOOL_VERSION}: ${total} facts in ${rows.length} relations → ${out}`);
  console.log(`  root ${result.root}`);
  const byLayer = new Map<string, string[]>();
  for (const r of rows) {
    const l = byLayer.get(r.layer as string) ?? [];
    l.push(`${r.relation} ${r.rows}`);
    byLayer.set(r.layer as string, l);
  }
  for (const [layer, list] of byLayer) console.log(`  ${layer}: ${list.join(", ")}`);
  const edges = result.tables.count("call_site");
  if (edges > 20000) {
    console.log(`  note: ${edges} call sites — a full call-graph closure (lib/callreach.dl) will be slow; anchor on edges or narrow first`);
  }
  console.log(`  in ${((performance.now() - started) / 1000).toFixed(1)}s. Start with: import "${relTo(process.cwd(), out)}/schema/all.dl".`);
}

if (process.argv[1] !== undefined && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
