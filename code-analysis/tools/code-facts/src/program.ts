// Loading: one or more tsconfig files, their `references` followed, each compiled
// into a TypeScript program. A file compiled by several projects is extracted once,
// by the first project (in command-line, then reference, order) that owns it.

import * as fs from "node:fs";
import * as path from "node:path";
import ts from "typescript";

export interface LoadedProject {
  /** Absolute path of the tsconfig. */
  readonly configPath: string;
  readonly program: ts.Program;
  readonly checker: ts.TypeChecker;
  readonly options: ts.CompilerOptions;
  /** Root-relative paths of the project's own source files under the root. */
  readonly files: string[];
  readonly configDiagnostics: readonly ts.Diagnostic[];
}

export interface SourceInfo {
  readonly sf: ts.SourceFile;
  /** Root-relative, forward slashes. */
  readonly path: string;
  readonly project: LoadedProject;
  readonly checker: ts.TypeChecker;
}

export interface Loaded {
  readonly root: string;
  readonly projects: LoadedProject[];
  /** Every source file to extract, sorted by path. */
  readonly sources: SourceInfo[];
  readonly byFileName: Map<string, SourceInfo>;
}

const TEST_CONFIG = /(^|[./-])(test|tests|spec|e2e)([./-]|$)/i;

export function isTestConfig(configPath: string): boolean {
  return TEST_CONFIG.test(path.basename(configPath));
}

export function toPosix(p: string): string {
  return p.split(path.sep).join("/");
}

export function relTo(root: string, abs: string): string {
  const r = toPosix(path.relative(root, abs));
  return r === "" ? "." : r;
}

/** The repository root: the git top-level above the first tsconfig, else the
 * deepest directory enclosing every tsconfig. */
export function findRoot(configPaths: readonly string[]): string {
  const dirs = configPaths.map((p) => path.dirname(path.resolve(p)));
  let dir = dirs[0] ?? process.cwd();
  for (let d = dir; ; d = path.dirname(d)) {
    if (fs.existsSync(path.join(d, ".git"))) {
      if (dirs.every((x) => x === d || x.startsWith(d + path.sep))) return d;
      break;
    }
    if (path.dirname(d) === d) break;
  }
  for (const other of dirs.slice(1)) {
    while (!(other === dir || other.startsWith(dir + path.sep))) dir = path.dirname(dir);
  }
  return dir;
}

function resolveConfigPath(p: string): string {
  const abs = path.resolve(p);
  if (fs.existsSync(abs) && fs.statSync(abs).isDirectory()) return path.join(abs, "tsconfig.json");
  return abs;
}

function loadOne(configPath: string, root: string): { project: LoadedProject; references: string[] } {
  const read = ts.readConfigFile(configPath, (f) => ts.sys.readFile(f));
  const diagnostics: ts.Diagnostic[] = [];
  if (read.error !== undefined) {
    throw new Error(`code-facts: cannot read ${configPath}: ${ts.flattenDiagnosticMessageText(read.error.messageText, "\n")}`);
  }
  const parsed = ts.parseJsonConfigFileContent(read.config, ts.sys, path.dirname(configPath), undefined, configPath);
  diagnostics.push(...parsed.errors.filter((d) => d.code !== 18003)); // 18003: no inputs (a solution-style config)
  // Emit stays possible — the structure layer asks the emitter which imports
  // survive into JavaScript — but only into memory, and only the JavaScript:
  // nothing here changes how the program is checked or resolved.
  const options: ts.CompilerOptions = {
    ...parsed.options,
    noEmit: false,
    emitDeclarationOnly: false,
    declaration: false,
    declarationMap: false,
    sourceMap: false,
    inlineSourceMap: false,
    noEmitOnError: false,
    rewriteRelativeImportExtensions: false,
  };
  const program = ts.createProgram({
    rootNames: parsed.fileNames,
    options,
    ...(parsed.projectReferences !== undefined ? { projectReferences: parsed.projectReferences } : {}),
  });
  const files = program
    .getSourceFiles()
    .filter((sf) => isProjectFile(program, sf, root))
    .map((sf) => relTo(root, sf.fileName))
    .sort();
  const references = (parsed.projectReferences ?? []).map((r) => resolveConfigPath(r.path));
  return {
    project: { configPath, program, checker: program.getTypeChecker(), options, files, configDiagnostics: diagnostics },
    references,
  };
}

export function isProjectFile(program: ts.Program, sf: ts.SourceFile, root: string): boolean {
  if (program.isSourceFileDefaultLibrary(sf) || program.isSourceFileFromExternalLibrary(sf)) return false;
  const abs = path.resolve(sf.fileName);
  if (!(abs === root || abs.startsWith(root + path.sep))) return false;
  return !toPosix(abs).includes("/node_modules/");
}

export function load(configArgs: readonly string[], rootArg: string | undefined, exclude: readonly RegExp[]): Loaded {
  const configs = configArgs.map(resolveConfigPath);
  for (const c of configs) {
    if (!fs.existsSync(c)) throw new Error(`code-facts: no such tsconfig: ${c}`);
  }
  const root = rootArg !== undefined ? path.resolve(rootArg) : findRoot(configs);
  const projects: LoadedProject[] = [];
  const seen = new Set<string>();
  const queue = [...configs];
  while (queue.length > 0) {
    const next = queue.shift();
    if (next === undefined || seen.has(next)) continue;
    seen.add(next);
    if (!fs.existsSync(next)) continue;
    const { project, references } = loadOne(next, root);
    projects.push(project);
    queue.push(...references);
  }

  const byFileName = new Map<string, SourceInfo>();
  const sources: SourceInfo[] = [];
  for (const project of projects) {
    for (const sf of project.program.getSourceFiles()) {
      if (!isProjectFile(project.program, sf, root)) continue;
      const rel = relTo(root, sf.fileName);
      if (exclude.some((re) => re.test(rel))) continue;
      const key = path.resolve(sf.fileName);
      if (byFileName.has(key)) continue;
      const info: SourceInfo = { sf, path: rel, project, checker: project.checker };
      byFileName.set(key, info);
      sources.push(info);
    }
  }
  sources.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  return { root, projects, sources, byFileName };
}

/** Converts a shell-style glob (`**`, `*`, `?`) to a regex over repo-relative paths. */
export function globToRegExp(glob: string): RegExp {
  let re = "";
  for (let i = 0; i < glob.length; i++) {
    const ch = glob[i] ?? "";
    if (ch === "*") {
      if (glob[i + 1] === "*" && glob[i + 2] === "/") {
        re += "(?:.*/)?";
        i += 2;
      } else if (glob[i + 1] === "*") {
        re += ".*";
        i++;
      } else re += "[^/]*";
    } else if (ch === "?") re += "[^/]";
    else re += ch.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  }
  return new RegExp(`^${re}$`);
}
