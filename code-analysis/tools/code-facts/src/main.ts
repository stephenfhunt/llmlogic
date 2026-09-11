// code-facts — extract a TypeScript project into Datalog fact tables.
//
//   code-facts <tsconfig>... [-o DIR] [--root DIR] [--layers L,...] [--no-git]
//            [--git-since DATE] [--git-max-commits N] [--exclude GLOB]...
//
// `run()` is the whole pipeline and is what the tests call; the CLI below only
// parses arguments and prints the summary.

import * as path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { Context } from "./context.ts";
import { extractDataflow } from "./layers/dataflow.ts";
import { extractFlow } from "./layers/flow.ts";
import { extractGit, gitHead } from "./layers/git.ts";
import { extractQuality } from "./layers/quality.ts";
import { extractRefs } from "./layers/refs.ts";
import { extractStructure, Packages } from "./layers/structure.ts";
import { globToRegExp, load, relTo } from "./program.ts";
import { type Layer, LAYERS, OPTIONAL_LAYERS } from "./schema.ts";
import { Tables } from "./writer.ts";

export const TOOL_VERSION = "0.1.0";
const HERE = path.dirname(fileURLToPath(import.meta.url));
export const LIB_DIR = path.join(HERE, "..", "lib");

export interface Options {
  tsconfigs: string[];
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

  const loaded = timed("load", () => load(opts.tsconfigs, opts.root, (opts.exclude ?? []).map(globToRegExp)));
  const tables = new Tables();
  const packages = new Packages(loaded.root);
  const ctx = new Context(loaded, tables, (f) => packages.nameOf(f));

  timed("ids", () => ctx.prepass());
  timed("structure", () => extractStructure(ctx, packages));
  if (layers.has("refs")) timed("refs", () => extractRefs(ctx));
  if (layers.has("flow")) timed("flow", () => extractFlow(ctx));
  if (layers.has("dataflow")) timed("dataflow", () => extractDataflow(ctx));
  if (layers.has("quality")) timed("quality", () => extractQuality(ctx));
  if (layers.has("git")) {
    timed("git", () => extractGit(ctx, { since: opts.gitSince, maxCommits: opts.gitMaxCommits ?? 20000 }));
  }
  ctx.flushSymbols();

  const time = (opts.time ?? new Date()).toISOString().slice(0, 19);
  tables.add("extraction", {
    tool_version: TOOL_VERSION,
    typescript_version: ts.version,
    node_version: process.versions.node,
    root: loaded.root,
    tsconfigs: opts.tsconfigs.map((c) => relTo(loaded.root, path.resolve(c))).join(","),
    layers: LAYERS.filter((l) => layers.has(l)).join(","),
    time,
    git_head: gitHead(loaded.root),
  });
  tables.dedupe();
  if (opts.out !== undefined) timed("write", () => tables.write(opts.out as string, layers, LIB_DIR));
  return { tables, root: loaded.root, layers, timings };
}

function usage(): string {
  return [
    "usage: code-facts <tsconfig.json | dir>... [options]",
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
    else tsconfigs.push(a);
  }
  if (tsconfigs.length === 0) throw new Error(`code-facts: give at least one tsconfig\n\n${usage()}`);
  return { tsconfigs, out, root, layers, exclude, gitSince, gitMaxCommits };
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
