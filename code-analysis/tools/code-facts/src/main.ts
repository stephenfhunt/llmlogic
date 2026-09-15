// code-facts — extract a TypeScript project into Datalog fact tables.
//
//   code-facts <tsconfig>... [-o DIR] [--root DIR] [--layers L,...] [--no-git]
//            [--git-since DATE] [--git-max-commits N] [--exclude GLOB]...
//
// `run()` is the whole pipeline and is what the tests call; the CLI below only
// parses arguments and prints the summary.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as os from "node:os";
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

/** Above this, extraction is a minutes-and-gigabytes job and says so.
 *
 * The warning quotes **measured** anchors rather than extrapolating: cost is
 * sublinear in lines (156k lines is 1.6 GB, 2.9M is 13 GB, not 29), and a number
 * derived from a rate would have been wrong by more than a factor of two at the
 * size where it matters. This threshold is where "this is fine" stops being the
 * safe assumption, not a limit. */
const LARGE_PROJECT_LINES = 300_000;
const HERE = path.dirname(fileURLToPath(import.meta.url));
export const LIB_DIR = path.join(HERE, "..", "lib");
const PY_FRONTEND = path.join(HERE, "frontends", "python", "py_facts.py");
const GO_FRONTEND = path.join(HERE, "frontends", "go");
/** Where the Go frontend is compiled to — outside `src/`, which is what ships. */
const GO_BUILD = path.join(HERE, "..", "build", "go");
const JAVA_FRONTEND = path.join(HERE, "frontends", "java");
/** Where the Java frontend and its Maven extension are compiled to. */
const JAVA_BUILD = path.join(HERE, "..", "build", "java");

export interface Options {
  /** TypeScript: tsconfig files (or directories holding one). */
  tsconfigs: string[];
  /** Python: source roots (directories, or a pyproject.toml). */
  python?: string[] | undefined;
  /** Go: a go.mod or go.work, or a directory holding one. */
  go?: string[] | undefined;
  /** Java: a pom.xml, build.gradle(.kts) or settings.gradle(.kts), or a directory. */
  java?: string[] | undefined;
  out?: string | undefined;
  root?: string | undefined;
  layers?: ReadonlySet<Layer> | undefined;
  exclude?: string[] | undefined;
  gitSince?: string | undefined;
  gitMaxCommits?: number | undefined;
  /** Parse each file once across tsconfigs (default true). Off only as a test oracle. */
  shareSourceFiles?: boolean | undefined;
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
  const go = opts.go ?? [];
  const java = opts.java ?? [];
  const exclude = (opts.exclude ?? []).map(globToRegExp);
  // A Python, Go or Java target may be a directory; findRoot reads each target's directory.
  const anchors = [...opts.tsconfigs, ...[...python, ...go, ...java].map((p) => (fs.existsSync(p) && fs.statSync(p).isDirectory() ? path.join(p, "__target__") : p))];
  const root = opts.root !== undefined ? path.resolve(opts.root) : findRoot(anchors);
  const tables = new Tables();
  let ids: Counters = { callSite: 1, flowNode: 1 };

  if (opts.tsconfigs.length > 0) {
    const loaded = timed("load", () => load(opts.tsconfigs, root, exclude, opts.shareSourceFiles ?? true));
    // Say how big the job is *before* spending minutes on it. Extraction peaks
    // at roughly 1 GB per 100k lines, and the phase that blows a small heap is
    // `ids`, which comes next — so a user who is about to wait, or about to run
    // out of memory, finds out here rather than from a V8 stack trace.
    const lines = loaded.sources.reduce((n, s) => n + s.sf.getLineStarts().length, 0);
    log(`code-facts: ${loaded.sources.length} files, ${lines} lines`);
    if (lines > LARGE_PROJECT_LINES) {
      log(
        "code-facts: this is a large project — minutes and many GB. For scale: 156k lines " +
          "is 18 s and 1.6 GB with every layer; 2.9M lines needs `--layers refs,quality` " +
          "and is then 276 s and 13 GB, while every layer exhausts a 12 GB heap. " +
          "`--layers refs,quality` keeps the architecture, coupling, cohesion and " +
          "dependency libraries and drops flow, dominators, pointsto and taint.",
      );
    }
    const packages = new Packages(loaded.root);
    const ctx = new Context(loaded, tables, (f) => packages.nameOf(f));
    timed("ids", () => ctx.prepass());
    timed("structure", () => extractStructure(ctx, packages));
    if (layers.has("refs")) timed("refs", () => extractRefs(ctx));
    if (layers.has("flow")) timed("flow", () => extractFlow(ctx));
    if (layers.has("dataflow")) timed("dataflow", () => extractDataflow(ctx));
    if (layers.has("quality")) timed("quality", () => extractQuality(ctx));
    ctx.flushSymbols();
    ids = { callSite: ctx.callSitesUsed, flowNode: ctx.nextFlowNode };
  }
  let pythonVersion: string | null = null;
  if (python.length > 0) {
    pythonVersion = timed("python", () => {
      const r = runPython(python, root, layers, exclude, ids, tables);
      ids = r.ids;
      return r.version;
    });
  }
  let goVersion: string | null = null;
  if (go.length > 0) {
    goVersion = timed("go", () => {
      const r = runGo(go, root, layers, exclude, ids, tables, log);
      ids = r.ids;
      return r.version;
    });
  }
  let javaVersion: string | null = null;
  if (java.length > 0) {
    javaVersion = timed("java", () => {
      const r = runJava(java, root, layers, exclude, ids, tables, log);
      ids = r.ids;
      return r.version;
    });
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
    targets: [...opts.tsconfigs, ...python, ...go, ...java].map((c) => relTo(root, path.resolve(c))).join(","),
    layers: LAYERS.filter((l) => layers.has(l)).join(","),
    time,
    git_head: gitHead(root),
    go_version: goVersion,
    java_version: javaVersion,
  });
  tables.dedupe();
  if (opts.out !== undefined) timed("write", () => tables.write(opts.out as string, layers, LIB_DIR));
  return { tables, root, layers, timings };
}

/** The next free call-site and flow-node ids: one id-space across every frontend. */
interface Counters {
  callSite: number;
  flowNode: number;
}

/**
 * A frontend written in its language's own toolchain. It writes one
 * `{"relation": …, "row": {…}}` JSON object per line, each validated here against
 * schema.ts like any other row, and ends with a `__counters__` row naming the next
 * free ids, so the next frontend continues from them.
 *
 * Rows go to a file rather than a pipe: a large repository's facts outgrow any
 * buffer `spawnSync` could be given, and a file is read back in chunks.
 */
function runFrontend(label: string, command: string, args: string[], env: NodeJS.ProcessEnv, ids: Counters, tables: Tables, log?: (line: string) => void): Counters {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "code-facts-rows-"));
  const rowsFile = path.join(dir, "rows.jsonl");
  try {
    const fd = fs.openSync(rowsFile, "w");
    let r: ReturnType<typeof spawnSync>;
    try {
      r = spawnSync(command, args, { stdio: ["ignore", fd, "pipe"], encoding: "utf8", maxBuffer: 1 << 28, env });
    } finally {
      fs.closeSync(fd);
    }
    if (r.error !== undefined) throw new Error(`code-facts: cannot run ${command} for the ${label} frontend: ${r.error.message}`);
    if (r.status !== 0) throw new Error(`code-facts: the ${label} frontend failed (exit ${r.status}):\n${r.stderr}`);
    // What a frontend could not read, it says on stderr and goes on.
    if (log !== undefined) for (const line of String(r.stderr).split("\n")) if (line.trim() !== "") log(line);
    let next = ids;
    forEachLine(rowsFile, (line) => {
      const { relation, row } = JSON.parse(line) as { relation: string; row: Record<string, string | number | boolean | null> };
      if (relation === "__counters__") next = { callSite: row.call_site as number, flowNode: row.flow_node as number };
      else tables.add(relation, row);
    });
    return next;
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

/** Calls `f` with each non-empty line of a UTF-8 file, reading it in chunks. */
function forEachLine(file: string, f: (line: string) => void): void {
  const fd = fs.openSync(file, "r");
  try {
    const chunk = Buffer.alloc(1 << 24);
    let carry = Buffer.alloc(0);
    for (;;) {
      const n = fs.readSync(fd, chunk, 0, chunk.length, null);
      if (n === 0) break;
      // Splitting on the newline byte is safe in UTF-8: no multi-byte sequence contains it.
      const buf = carry.length > 0 ? Buffer.concat([carry, chunk.subarray(0, n)]) : chunk.subarray(0, n);
      let start = 0;
      for (let nl = buf.indexOf(10, start); nl !== -1; nl = buf.indexOf(10, start)) {
        if (nl > start) f(buf.toString("utf8", start, nl));
        start = nl + 1;
      }
      carry = Buffer.from(buf.subarray(start));
    }
    if (carry.length > 0) f(carry.toString("utf8"));
  } finally {
    fs.closeSync(fd);
  }
}

/** The Python frontend, `frontends/python/py_facts.py`. */
function runPython(
  targets: string[],
  root: string,
  layers: ReadonlySet<Layer>,
  exclude: RegExp[],
  ids: Counters,
  tables: Tables,
): { ids: Counters; version: string } {
  const args = [
    PY_FRONTEND,
    "--root",
    root,
    "--layers",
    [...layers].join(","),
    "--first-call-site",
    String(ids.callSite),
    "--first-flow-node",
    String(ids.flowNode),
    ...exclude.flatMap((re) => ["--exclude", re.source]),
    ...targets.map((t) => path.resolve(t)),
  ];
  const python = process.env.CODE_FACTS_PYTHON ?? "python3";
  // No __pycache__ written beside the frontend: it may live in an installed skill.
  const next = runFrontend("Python", python, args, { ...process.env, PYTHONDONTWRITEBYTECODE: "1" }, ids, tables);
  const v = spawnSync(python, ["-c", "import sys; print('.'.join(map(str, sys.version_info[:3])))"], { encoding: "utf8" });
  return { ids: next, version: v.stdout.trim() };
}

/**
 * The Go frontend, `frontends/go/`. It is Go source, compiled on first use by
 * the toolchain the project itself selects — so the type checker reads the
 * project's language version — and cached by its source and that toolchain.
 */
function runGo(
  targets: string[],
  root: string,
  layers: ReadonlySet<Layer>,
  exclude: RegExp[],
  ids: Counters,
  tables: Tables,
  log: (line: string) => void,
): { ids: Counters; version: string } {
  const first = path.resolve(targets[0] ?? ".");
  const anchor = fs.existsSync(first) && fs.statSync(first).isDirectory() ? first : path.dirname(first);
  const selected = spawnSync("go", ["env", "GOVERSION"], { cwd: anchor, encoding: "utf8" });
  if (selected.error !== undefined) throw new Error(`code-facts: reading Go needs the \`go\` command on PATH (${selected.error.message})`);
  if (selected.status !== 0) throw new Error(`code-facts: \`go env GOVERSION\` failed in ${anchor}:\n${selected.stderr}`);
  const toolchain = selected.stdout.trim().split(/\s+/)[0] ?? "";
  const binary = buildGoFrontend(toolchain, log);
  const args = [
    "--root",
    root,
    "--layers",
    [...layers].join(","),
    "--first-call-site",
    String(ids.callSite),
    "--first-flow-node",
    String(ids.flowNode),
    ...exclude.flatMap((re) => ["--exclude", re.source]),
    ...targets.map((t) => path.resolve(t)),
  ];
  const next = runFrontend("Go", binary, args, process.env, ids, tables, log);
  return { ids: next, version: toolchain.replace(/^go/, "") };
}

function buildGoFrontend(toolchain: string, log: (line: string) => void): string {
  const hash = createHash("sha256");
  for (const f of fs.readdirSync(GO_FRONTEND).filter((n) => n.endsWith(".go") || n === "go.mod" || n === "go.sum").sort()) {
    hash.update(`${f}\0`);
    hash.update(fs.readFileSync(path.join(GO_FRONTEND, f)));
  }
  hash.update(toolchain);
  const binary = path.join(GO_BUILD, `go-facts-${hash.digest("hex").slice(0, 16)}`);
  if (fs.existsSync(binary)) return binary;
  log(`code-facts: building its Go frontend with ${toolchain} (first use)…`);
  fs.mkdirSync(GO_BUILD, { recursive: true });
  const vendored = fs.existsSync(path.join(GO_FRONTEND, "vendor"));
  const partial = `${binary}.${process.pid}.partial`;
  const r = spawnSync("go", ["build", ...(vendored ? ["-mod=vendor"] : []), "-o", partial, "."], {
    cwd: GO_FRONTEND,
    encoding: "utf8",
    // The binary runs here, whatever GOOS and GOARCH the environment sets for reading the project.
    env: { ...process.env, GOTOOLCHAIN: toolchain, GOWORK: "off", GOFLAGS: "", GOOS: "", GOARCH: "" },
  });
  if (r.error !== undefined) throw new Error(`code-facts: reading Go needs the \`go\` command on PATH (${r.error.message})`);
  if (r.status !== 0) throw new Error(`code-facts: building the Go frontend with ${toolchain} failed; it needs Go 1.26 or later:\n${r.stderr}`);
  fs.renameSync(partial, binary);
  return binary;
}

/**
 * The Java frontend, `frontends/java/`. It is Java source, compiled on first use
 * by the JDK on PATH and cached by its source and that JDK. It asks each
 * target's build for its source sets and classpaths, and reads them with that
 * JDK's own compiler.
 */
function runJava(
  targets: string[],
  root: string,
  layers: ReadonlySet<Layer>,
  exclude: RegExp[],
  ids: Counters,
  tables: Tables,
  log: (line: string) => void,
): { ids: Counters; version: string } {
  const javac = spawnSync("javac", ["-version"], { encoding: "utf8" });
  if (javac.error !== undefined) throw new Error(`code-facts: reading Java needs a JDK — \`javac\` on PATH (${javac.error.message})`);
  if (javac.status !== 0) throw new Error(`code-facts: \`javac -version\` failed:\n${javac.stderr}`);
  const version = `${javac.stdout}${javac.stderr}`.trim().replace(/^javac\s+/, "");
  const classes = buildJavaFrontend(version, log);
  const args = [
    "-cp",
    classes,
    "codefacts.Main",
    "--root",
    root,
    "--resources",
    JAVA_FRONTEND,
    "--cache",
    JAVA_BUILD,
    "--layers",
    [...layers].join(","),
    "--first-call-site",
    String(ids.callSite),
    "--first-flow-node",
    String(ids.flowNode),
    ...exclude.flatMap((re) => ["--exclude", re.source]),
    ...targets.map((t) => path.resolve(t)),
  ];
  const next = runFrontend("Java", "java", args, process.env, ids, tables, log);
  return { ids: next, version };
}

function buildJavaFrontend(jdk: string, log: (line: string) => void): string {
  const hash = createHash("sha256");
  const sources = fs.readdirSync(JAVA_FRONTEND).filter((n) => n.endsWith(".java")).sort();
  for (const f of sources) {
    hash.update(`${f}\0`);
    hash.update(fs.readFileSync(path.join(JAVA_FRONTEND, f)));
  }
  hash.update(jdk);
  const classes = path.join(JAVA_BUILD, `java-facts-${hash.digest("hex").slice(0, 16)}`);
  if (fs.existsSync(classes)) return classes;
  log(`code-facts: building its Java frontend with javac ${jdk} (first use)…`);
  fs.mkdirSync(JAVA_BUILD, { recursive: true });
  const partial = `${classes}.${process.pid}.partial`;
  const r = spawnSync("javac", ["--release", "17", "-proc:none", "-d", partial, ...sources.map((f) => path.join(JAVA_FRONTEND, f))], { encoding: "utf8" });
  if (r.error !== undefined) throw new Error(`code-facts: reading Java needs a JDK — \`javac\` on PATH (${r.error.message})`);
  if (r.status !== 0) throw new Error(`code-facts: building the Java frontend with javac ${jdk} failed; it needs JDK 17 or later:\n${r.stderr}`);
  try {
    fs.renameSync(partial, classes);
  } catch (e) {
    // Another run built it first.
    fs.rmSync(partial, { recursive: true, force: true });
    if (!fs.existsSync(classes)) throw e;
  }
  return classes;
}

type Lang = "ts" | "python" | "go" | "java";

const JAVA_BUILD_FILES = ["pom.xml", "build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"];

/**
 * Which frontend reads a target: a tsconfig; a go.mod or go.work; a pom.xml or
 * Gradle build or settings script; a pyproject.toml; a directory by what it
 * holds — a tsconfig.json, then a go.work or go.mod, then a Maven or Gradle
 * build — and any other directory as Python.
 */
function detectLang(a: string): Lang {
  const base = path.basename(a);
  if (base === "go.mod" || base === "go.work") return "go";
  if (JAVA_BUILD_FILES.includes(base)) return "java";
  if (base === "pyproject.toml") return "python";
  if (!(fs.existsSync(a) && fs.statSync(a).isDirectory())) return "ts";
  if (fs.existsSync(path.join(a, "tsconfig.json"))) return "ts";
  if (fs.existsSync(path.join(a, "go.work")) || fs.existsSync(path.join(a, "go.mod"))) return "go";
  if (JAVA_BUILD_FILES.some((f) => fs.existsSync(path.join(a, f)))) return "java";
  return "python";
}

function usage(): string {
  return [
    "usage: code-facts <tsconfig.json | go.mod | go.work | pom.xml | build.gradle | pyproject.toml | directory>... [options]",
    "",
    "  A tsconfig is read as TypeScript, a go.mod or go.work as Go, a pom.xml or a",
    "  Gradle build or settings script as Java, a pyproject.toml as Python. A",
    "  directory is read by what it holds — a tsconfig.json, then a go.work or",
    "  go.mod, then a Maven or Gradle build — and any other directory as a Python",
    "  source root (`--lang java` reads it as Java sources with no build). Any mix",
    "  may be given.",
    "",
    "  -o, --out DIR          output directory (default ./code-facts-out)",
    "  --root DIR             root every path is relative to (default: the git top-level)",
    "  --lang ts|python|go|java  read the next target as this language, whatever it holds",
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
  const go: string[] = [];
  const java: string[] = [];
  const exclude: string[] = [];
  let lang: Lang | undefined;
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
    else if (a === "--lang") {
      const l = value(++i, a);
      if (l !== "ts" && l !== "python" && l !== "go" && l !== "java") throw new Error(`code-facts: unknown language \`${l}\` (languages: ts, python, go, java)`);
      lang = l;
    }
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
    else {
      const which = lang ?? detectLang(a);
      lang = undefined;
      if (which === "go") go.push(a);
      else if (which === "java") java.push(a);
      else if (which === "python") python.push(a);
      else tsconfigs.push(a);
    }
  }
  if (tsconfigs.length + python.length + go.length + java.length === 0) throw new Error(`code-facts: give a tsconfig, a go.mod, a Maven or Gradle build, or a Python root\n\n${usage()}`);
  return { tsconfigs, python, go, java, out, root, layers, exclude, gitSince, gitMaxCommits };
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
  console.log(`  in ${((performance.now() - started) / 1000).toFixed(1)}s. Next: check the facts with lib/checks.dl, then lib/orient.dl; schema/all.dl imports every relation.`);
}

if (process.argv[1] !== undefined && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
