// The structure layer: projects, files, directories, packages, module imports and
// exports, parameters, documentation and decorators. `symbol` itself is filled by
// the context's id registry and flushed at the end of the run.

import * as fs from "node:fs";
import { isBuiltin } from "node:module";
import * as path from "node:path";
import ts from "typescript";
import { type Context, isFunctionLike, lineOf, nameText, packageFromPath, packageOfSpecifier, truncate } from "../context.ts";
import { isTestConfig, relTo, type SourceInfo } from "../program.ts";

const TEST_PATH = /(^|\/)(__tests__|__mocks__|tests?|spec|e2e)\/|\.(test|spec|e2e)\.[cm]?[jt]sx?$/;
const GENERATED = /@generated|auto-?generated|do not edit/i;

interface PackageJson {
  name?: unknown;
  version?: unknown;
  private?: unknown;
  dependencies?: unknown;
  devDependencies?: unknown;
  peerDependencies?: unknown;
  optionalDependencies?: unknown;
}

/** Finds the nearest package.json at or above each directory, within the root. */
export class Packages {
  private readonly byDir = new Map<string, { dir: string; json: PackageJson } | null>();
  private readonly root: string;

  constructor(root: string) {
    this.root = root;
  }

  nearest(relDir: string): { dir: string; json: PackageJson } | null {
    const cached = this.byDir.get(relDir);
    if (cached !== undefined) return cached;
    const abs = path.join(this.root, relDir);
    let found: { dir: string; json: PackageJson } | null = null;
    const pj = path.join(abs, "package.json");
    if (fs.existsSync(pj)) {
      try {
        found = { dir: relDir, json: JSON.parse(fs.readFileSync(pj, "utf8")) as PackageJson };
      } catch {
        found = null;
      }
    }
    if (found === null && relDir !== ".") found = this.nearest(parentDir(relDir));
    this.byDir.set(relDir, found);
    return found;
  }

  nameOf(relFile: string): string | null {
    const p = this.nearest(parentDir(relFile));
    if (p === null) return null;
    return typeof p.json.name === "string" ? p.json.name : p.dir;
  }

  all(): { dir: string; json: PackageJson }[] {
    const seen = new Map<string, { dir: string; json: PackageJson }>();
    for (const v of this.byDir.values()) if (v !== null) seen.set(v.dir, v);
    return [...seen.values()].sort((a, b) => (a.dir < b.dir ? -1 : a.dir > b.dir ? 1 : 0));
  }
}

export function parentDir(rel: string): string {
  const i = rel.lastIndexOf("/");
  return i < 0 ? "." : rel.slice(0, i);
}

function langOf(p: string): string {
  if (p.endsWith(".d.ts") || p.endsWith(".d.mts") || p.endsWith(".d.cts")) return "dts";
  const ext = path.extname(p).slice(1);
  return ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"].includes(ext) ? ext : "ts";
}

/** Lines as `wc -l` counts them: a final newline does not start another. */
export function lineCount(sf: ts.SourceFile): number {
  const n = sf.getLineStarts().length;
  return sf.text.endsWith("\n") ? n - 1 : n;
}

function sloc(sf: ts.SourceFile): number {
  const scanner = ts.createScanner(sf.languageVersion, true, sf.languageVariant, sf.text);
  const lines = new Set<number>();
  for (let tok = scanner.scan(); tok !== ts.SyntaxKind.EndOfFileToken; tok = scanner.scan()) {
    lines.add(sf.getLineAndCharacterOfPosition(scanner.getTokenStart()).line);
  }
  return lines.size;
}

export function extractStructure(ctx: Context, packages: Packages): void {
  const t = ctx.tables;
  const projectOfFile = new Map<string, string[]>();

  for (const project of ctx.loaded.projects) {
    const id = relTo(ctx.root, project.configPath);
    const o = project.options;
    t.add("project", {
      id,
      dir: relTo(ctx.root, path.dirname(project.configPath)),
      files: project.files.length,
      strict: o.strict === true,
      module: o.module !== undefined ? (ts.ModuleKind[o.module] ?? String(o.module)).toLowerCase() : null,
      target: o.target !== undefined ? (ts.ScriptTarget[o.target] ?? String(o.target)).toLowerCase() : null,
    });
    for (const f of project.files) {
      t.add("project_file", { project: id, file: f });
      const list = projectOfFile.get(f) ?? [];
      list.push(project.configPath);
      projectOfFile.set(f, list);
    }
  }

  const dirs = new Set<string>();
  for (const info of ctx.loaded.sources) {
    const p = info.path;
    const dir = parentDir(p);
    const projects = projectOfFile.get(p) ?? [];
    const head = info.sf.text.slice(0, 600);
    t.add("file", {
      path: p,
      dir,
      package: packages.nameOf(p),
      lang: langOf(p),
      loc: lineCount(info.sf),
      sloc: sloc(info.sf),
      is_test: TEST_PATH.test(p) || (projects.length > 0 && projects.every(isTestConfig)),
      is_decl: info.sf.isDeclarationFile,
      is_generated: GENERATED.test(head),
    });
    for (let d = dir; ; d = parentDir(d)) {
      dirs.add(d);
      t.add("file_ancestor", { file: p, dir: d, depth: depthOf(d) });
      if (d === ".") break;
    }
  }
  for (const d of [...dirs].sort()) {
    t.add("dir", { path: d, parent: d === "." ? null : parentDir(d), name: d === "." ? "." : path.posix.basename(d), depth: depthOf(d) });
  }

  for (const p of packages.all()) {
    const name = typeof p.json.name === "string" ? p.json.name : p.dir;
    t.add("package", {
      name,
      dir: p.dir,
      version: typeof p.json.version === "string" ? p.json.version : null,
      private: p.json.private === true,
    });
    const blocks: [string, unknown][] = [
      ["prod", p.json.dependencies],
      ["dev", p.json.devDependencies],
      ["peer", p.json.peerDependencies],
      ["optional", p.json.optionalDependencies],
    ];
    for (const [kind, deps] of blocks) {
      if (deps === null || typeof deps !== "object") continue;
      for (const [dep, range] of Object.entries(deps as Record<string, unknown>)) {
        t.add("package_dep", { package: name, dep, kind, range: String(range), types_for: typesFor(dep) });
      }
    }
  }

  for (const info of ctx.loaded.sources) {
    extractModuleEdges(ctx, info);
    extractExports(ctx, info);
    extractDeclarationDetail(ctx, info);
  }
}

/** The package an `@types/` package types: `@types/node` → `node`, `@types/a__b` → `@a/b`. */
function typesFor(dep: string): string | null {
  if (!dep.startsWith("@types/")) return null;
  const typed = dep.slice("@types/".length);
  return typed.includes("__") ? `@${typed.replace("__", "/")}` : typed;
}

function depthOf(dir: string): number {
  return dir === "." ? 0 : dir.split("/").length;
}

function resolveSpecifier(
  ctx: Context,
  info: SourceInfo,
  specNode: ts.StringLiteralLike,
): { target_file: string | null; target_package: string | null; resolved: boolean } {
  const spec = specNode.text;
  const opts = info.project.options;
  const mode = ts.getModeForUsageLocation(info.sf, specNode, opts);
  const res = ts.resolveModuleName(spec, info.sf.fileName, opts, ts.sys, undefined, undefined, mode).resolvedModule;
  if (res !== undefined) {
    const abs = path.resolve(res.resolvedFileName);
    const fromPkg = packageFromPath(abs);
    if (fromPkg === undefined && (abs === ctx.root || abs.startsWith(ctx.root + path.sep))) {
      return { target_file: relTo(ctx.root, abs), target_package: null, resolved: true };
    }
    return { target_file: null, target_package: fromPkg ?? packageOfSpecifier(spec), resolved: true };
  }
  const bare = !spec.startsWith(".") && !spec.startsWith("/");
  if (bare && isBuiltin(spec)) {
    return { target_file: null, target_package: spec.startsWith("node:") ? spec : `node:${spec}`, resolved: true };
  }
  // An ambient `declare module "x"` resolves in the checker though not on disk.
  const moduleSymbol = info.checker.getSymbolAtLocation(specNode);
  if (moduleSymbol !== undefined) {
    const decl = moduleSymbol.declarations?.[0];
    const file = decl !== undefined ? decl.getSourceFile().fileName : undefined;
    const pkg = file !== undefined ? packageFromPath(file) : undefined;
    return { target_file: null, target_package: bare ? packageOfSpecifier(spec) : (pkg ?? null), resolved: true };
  }
  return { target_file: null, target_package: bare ? packageOfSpecifier(spec) : null, resolved: false };
}

/**
 * The import and export declarations of `sf` that survive into the emitted
 * JavaScript, or undefined when the emitter would not run. TypeScript elides an
 * import whose bindings are only used as types — and keeps one written without
 * `type` under `verbatimModuleSyntax`, keeps one a JSX factory or decorator
 * metadata needs — so rather than restate those rules, ask the emitter: an
 * after-transformer sees the final tree, and every surviving statement (an
 * `import`, or the `require` CommonJS made of it) points back to its source
 * declaration through `ts.getOriginalNode`.
 */
function emittedDeclarations(info: SourceInfo): Set<ts.Node> | undefined {
  if (info.sf.isDeclarationFile) return new Set();
  const kept = new Set<ts.Node>();
  const collect: ts.TransformerFactory<ts.SourceFile> = () => (out) => {
    const visit = (n: ts.Node): void => {
      const o = ts.getOriginalNode(n);
      if (o.getSourceFile() === info.sf && (ts.isImportDeclaration(o) || ts.isExportDeclaration(o) || ts.isImportEqualsDeclaration(o))) {
        kept.add(o);
      }
      ts.forEachChild(n, visit);
    };
    visit(out);
    return out;
  };
  try {
    const result = info.project.program.emit(info.sf, () => {}, undefined, false, { after: [collect] });
    return result.emitSkipped ? undefined : kept;
  } catch {
    return undefined;
  }
}

function extractModuleEdges(ctx: Context, info: SourceInfo): void {
  const t = ctx.tables;
  const file = info.path;
  const emitted = emittedDeclarations(info);
  const edge = (node: ts.Node, specNode: ts.StringLiteralLike, kind: string) => {
    let runtime: boolean | null;
    if (kind === "dynamic" || kind === "require") runtime = true;
    else if (kind === "type_query") runtime = false;
    else runtime = emitted === undefined ? null : emitted.has(node);
    const spec = specNode.text;
    const builtin = !spec.startsWith(".") && !spec.startsWith("/") && isBuiltin(spec);
    t.add("imports", { file, line: lineOf(node, info.sf), specifier: spec, kind, runtime, builtin, ...resolveSpecifier(ctx, info, specNode) });
  };
  const name = (node: ts.Node, local: string, imported: string, target: ts.Node | undefined, typeOnly: boolean) => {
    const sym = target !== undefined ? info.checker.getSymbolAtLocation(target) : undefined;
    t.add("import_name", {
      file,
      line: lineOf(node, info.sf),
      local,
      imported,
      target: ctx.idOfSymbol(sym, info.checker) ?? null,
      type_only: typeOnly,
    });
  };

  const visit = (node: ts.Node): void => {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      const clause = node.importClause;
      const kind = clause === undefined ? "side_effect" : clause.isTypeOnly ? "type_only" : "static";
      edge(node, node.moduleSpecifier, kind);
      if (clause !== undefined) {
        const typeOnly = clause.isTypeOnly;
        if (clause.name !== undefined) name(node, clause.name.text, "default", clause.name, typeOnly);
        const nb = clause.namedBindings;
        if (nb !== undefined && ts.isNamespaceImport(nb)) name(node, nb.name.text, "*", nb.name, typeOnly);
        if (nb !== undefined && ts.isNamedImports(nb)) {
          for (const el of nb.elements) {
            name(node, el.name.text, nameText(el.propertyName ?? el.name), el.name, typeOnly || el.isTypeOnly);
          }
        }
      }
    } else if (ts.isExportDeclaration(node) && node.moduleSpecifier !== undefined && ts.isStringLiteral(node.moduleSpecifier)) {
      const ec = node.exportClause;
      edge(node, node.moduleSpecifier, ec === undefined ? "reexport_all" : "reexport");
      if (ec !== undefined && ts.isNamespaceExport(ec)) name(node, nameText(ec.name), "*", ec.name, node.isTypeOnly);
      if (ec !== undefined && ts.isNamedExports(ec)) {
        for (const el of ec.elements) {
          name(node, nameText(el.name), nameText(el.propertyName ?? el.name), el.name, node.isTypeOnly || el.isTypeOnly);
        }
      }
    } else if (
      ts.isImportEqualsDeclaration(node) &&
      ts.isExternalModuleReference(node.moduleReference) &&
      ts.isStringLiteral(node.moduleReference.expression)
    ) {
      edge(node, node.moduleReference.expression, "import_equals");
      name(node, node.name.text, "*", node.name, node.isTypeOnly);
    } else if (ts.isCallExpression(node) && node.arguments.length >= 1) {
      const arg = node.arguments[0];
      if (arg !== undefined && ts.isStringLiteralLike(arg)) {
        if (node.expression.kind === ts.SyntaxKind.ImportKeyword) edge(node, arg, "dynamic");
        else if (ts.isIdentifier(node.expression) && node.expression.text === "require") edge(node, arg, "require");
      }
    } else if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument) && ts.isStringLiteral(node.argument.literal)) {
      edge(node, node.argument.literal, "type_query");
    }
    ts.forEachChild(node, visit);
  };
  ts.forEachChild(info.sf, visit);
}

function extractExports(ctx: Context, info: SourceInfo): void {
  const moduleSymbol = info.checker.getSymbolAtLocation(info.sf);
  if (moduleSymbol === undefined) return;
  for (const exp of info.checker.getExportsOfModule(moduleSymbol)) {
    const resolved = ctx.resolveSymbol(exp, info.checker);
    const decl = ctx.canonicalDecl(resolved);
    const id = ctx.idOfSymbol(exp, info.checker) ?? null;
    ctx.tables.add("exports", {
      file: info.path,
      name: ts.symbolName(exp),
      symbol: id,
      kind: decl !== undefined && ctx.infoOf(decl.getSourceFile()) === info ? "local" : "reexport",
      is_type: resolved !== undefined && (resolved.flags & ts.SymbolFlags.Value) === 0,
    });
  }
}

function hasBodyOrSignature(node: ts.Node): node is ts.SignatureDeclaration {
  return isFunctionLike(node) || ts.isMethodSignature(node) || ts.isCallSignatureDeclaration(node) || ts.isConstructSignatureDeclaration(node);
}

/** Declarations documented at API level: not locals, parameters, or members of anonymous literals. */
function isApiLevel(node: ts.Node): boolean {
  switch (node.kind) {
    case ts.SyntaxKind.ClassDeclaration:
    case ts.SyntaxKind.InterfaceDeclaration:
    case ts.SyntaxKind.TypeAliasDeclaration:
    case ts.SyntaxKind.EnumDeclaration:
    case ts.SyntaxKind.EnumMember:
    case ts.SyntaxKind.FunctionDeclaration:
    case ts.SyntaxKind.ModuleDeclaration:
      return true;
    case ts.SyntaxKind.MethodDeclaration:
    case ts.SyntaxKind.MethodSignature:
    case ts.SyntaxKind.PropertyDeclaration:
    case ts.SyntaxKind.PropertySignature:
    case ts.SyntaxKind.GetAccessor:
    case ts.SyntaxKind.SetAccessor:
    case ts.SyntaxKind.Constructor: {
      const p = node.parent;
      return p !== undefined && (ts.isClassLike(p) || ts.isInterfaceDeclaration(p) || (ts.isTypeLiteralNode(p) && ts.isTypeAliasDeclaration(p.parent)));
    }
    case ts.SyntaxKind.VariableDeclaration: {
      const stmt = node.parent?.parent;
      return stmt !== undefined && ts.isVariableStatement(stmt) && (ts.isSourceFile(stmt.parent) || ts.isModuleBlock(stmt.parent));
    }
    default:
      return false;
  }
}

function extractDeclarationDetail(ctx: Context, info: SourceInfo): void {
  const t = ctx.tables;
  const visit = (node: ts.Node): void => {
    if (hasBodyOrSignature(node)) {
      const fnId = ctx.idOfDecl(node);
      node.parameters.forEach((p, index) => {
        t.add("param", {
          fn: fnId,
          index,
          symbol: ctx.idOfDecl(p),
          name: ts.isIdentifier(p.name) ? p.name.text : "<pattern>",
          optional: p.questionToken !== undefined || p.initializer !== undefined,
          rest: p.dotDotDotToken !== undefined,
          has_default: p.initializer !== undefined,
        });
      });
    }
    if (isApiLevel(node)) {
      const id = ctx.idOfDecl(node);
      const docs = ts.getJSDocCommentsAndTags(node).filter(ts.isJSDoc);
      let lines = 0;
      for (const d of docs) lines += lineSpan(d, info.sf);
      t.add("doc", { symbol: id, has_doc: docs.length > 0, lines });
      for (const tag of ts.getJSDocTags(node)) {
        const text = ts.getTextOfJSDocComment(tag.comment);
        t.add("jsdoc_tag", { symbol: id, tag: tag.tagName.text, text: text !== undefined && text !== "" ? truncate(text, 200) : null });
      }
    }
    const decorators = ts.canHaveDecorators(node) ? ts.getDecorators(node) : undefined;
    for (const dec of decorators ?? []) {
      const callee = ts.isCallExpression(dec.expression) ? dec.expression.expression : dec.expression;
      const nameNode = ts.isPropertyAccessExpression(callee) ? callee.name : callee;
      t.add("decorator", {
        target: ctx.idOfDecl(node),
        decorator: ctx.idOfSymbol(info.checker.getSymbolAtLocation(nameNode), info.checker) ?? null,
        name: truncate(callee.getText(info.sf), 100),
        line: lineOf(dec, info.sf),
      });
    }
    ts.forEachChild(node, visit);
  };
  ts.forEachChild(info.sf, visit);
}

function lineSpan(node: ts.Node, sf: ts.SourceFile): number {
  const start = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line;
  const end = sf.getLineAndCharacterOfPosition(node.getEnd()).line;
  return end - start + 1;
}
