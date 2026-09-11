// The shared extraction context: the loaded programs, the output tables, and the
// id registry every layer draws from.
//
// Ids are keyed by *declaration position* (file, start, end, kind), never by
// `ts.Symbol` identity: each tsconfig is its own program with its own AST and
// symbol objects, and a symbol declared in one project and referenced from another
// must still come out as one id. Project ids are assigned by a pre-pass over every
// file in path order, so a collision suffix (`@line`) always lands on the same
// declaration however the files were listed.

import * as path from "node:path";
import ts from "typescript";
import { type Loaded, relTo, type SourceInfo, toPosix } from "./program.ts";
import type { SymbolKind } from "./schema.ts";
import type { Tables } from "./writer.ts";

type Named = ts.Node & { name?: ts.Node };

export function lineOf(node: ts.Node, sf: ts.SourceFile = node.getSourceFile()): number {
  return sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
}
export function colOf(node: ts.Node, sf: ts.SourceFile = node.getSourceFile()): number {
  return sf.getLineAndCharacterOfPosition(node.getStart(sf)).character + 1;
}
export function endLineOf(node: ts.Node, sf: ts.SourceFile = node.getSourceFile()): number {
  return sf.getLineAndCharacterOfPosition(node.getEnd()).line + 1;
}
/** Whitespace collapsed, and cut to at most `n` code points (never mid-pair). */
export function truncate(s: string, n: number): string {
  const one = s.replace(/\s+/g, " ").trim();
  if (one.length <= n) return one;
  const points = [...one];
  return points.length <= n ? one : `${points.slice(0, n - 1).join("")}…`;
}

/** Function-likes that can carry a body (signatures are not). */
export function isFunctionLike(node: ts.Node): node is ts.FunctionLikeDeclaration | ts.ClassStaticBlockDeclaration {
  switch (node.kind) {
    case ts.SyntaxKind.FunctionDeclaration:
    case ts.SyntaxKind.MethodDeclaration:
    case ts.SyntaxKind.Constructor:
    case ts.SyntaxKind.GetAccessor:
    case ts.SyntaxKind.SetAccessor:
    case ts.SyntaxKind.FunctionExpression:
    case ts.SyntaxKind.ArrowFunction:
    case ts.SyntaxKind.ClassStaticBlockDeclaration:
      return true;
    default:
      return false;
  }
}

export function bodyOf(node: ts.Node): ts.Node | undefined {
  if (ts.isClassStaticBlockDeclaration(node)) return node.body;
  if (isFunctionLike(node)) return (node as ts.FunctionLikeDeclaration).body;
  return undefined;
}

function isSignatureContainer(node: ts.Node): boolean {
  switch (node.kind) {
    case ts.SyntaxKind.MethodSignature:
    case ts.SyntaxKind.CallSignature:
    case ts.SyntaxKind.ConstructSignature:
    case ts.SyntaxKind.IndexSignature:
    case ts.SyntaxKind.FunctionType:
    case ts.SyntaxKind.ConstructorType:
      return true;
    default:
      return false;
  }
}

/** Nodes whose members are named relative to them. */
export function isContainer(node: ts.Node): boolean {
  return (
    ts.isSourceFile(node) ||
    ts.isModuleDeclaration(node) ||
    ts.isClassLike(node) ||
    ts.isInterfaceDeclaration(node) ||
    ts.isTypeAliasDeclaration(node) ||
    ts.isEnumDeclaration(node) ||
    ts.isTypeLiteralNode(node) ||
    ts.isObjectLiteralExpression(node) ||
    isFunctionLike(node) ||
    isSignatureContainer(node)
  );
}

/** Wrappers an initializer can sit inside and still be "the value of" its binding. */
function unwrapParent(node: ts.Node): ts.Node {
  let p = node.parent;
  let child = node;
  while (
    p !== undefined &&
    (ts.isParenthesizedExpression(p) ||
      ts.isAsExpression(p) ||
      ts.isSatisfiesExpression(p) ||
      ts.isTypeAssertionExpression(p) ||
      ts.isNonNullExpression(p))
  ) {
    child = p;
    p = p.parent;
  }
  void child;
  return p;
}

/** The declaration an anonymous function, class, object or type literal takes its
 * name from — `const f = () => …` makes the arrow *be* `f`. */
export function adoptingDeclaration(node: ts.Node): ts.Declaration | undefined {
  const anonymousValue =
    ts.isArrowFunction(node) || ts.isFunctionExpression(node) || ts.isClassExpression(node) || ts.isObjectLiteralExpression(node);
  if (anonymousValue) {
    const p = unwrapParent(node);
    if (p === undefined) return undefined;
    const isInit = (d: { initializer?: ts.Node }) => d.initializer !== undefined && skipWrappers(d.initializer) === node;
    if (ts.isVariableDeclaration(p) && ts.isIdentifier(p.name) && isInit(p)) return p;
    if (ts.isPropertyDeclaration(p) && isInit(p)) return p;
    if (ts.isPropertyAssignment(p) && isInit(p)) return p;
    if (ts.isExportAssignment(p) && skipWrappers(p.expression) === node) return p;
    return undefined;
  }
  if (ts.isTypeLiteralNode(node)) {
    const p = node.parent;
    if (
      p !== undefined &&
      (ts.isTypeAliasDeclaration(p) || ts.isPropertySignature(p) || ts.isPropertyDeclaration(p)) &&
      p.type === node
    ) {
      return p;
    }
  }
  return undefined;
}

export function skipWrappers(node: ts.Node): ts.Node {
  let n = node;
  while (
    ts.isParenthesizedExpression(n) ||
    ts.isAsExpression(n) ||
    ts.isSatisfiesExpression(n) ||
    ts.isTypeAssertionExpression(n) ||
    ts.isNonNullExpression(n)
  ) {
    n = n.expression;
  }
  return n;
}

function hasModifier(node: ts.Node, kind: ts.SyntaxKind): boolean {
  const mods = ts.canHaveModifiers(node) ? ts.getModifiers(node) : undefined;
  return mods?.some((m) => m.kind === kind) ?? false;
}

export function isStatic(node: ts.Node): boolean {
  return hasModifier(node, ts.SyntaxKind.StaticKeyword);
}

function isInAmbientContext(node: ts.Node): boolean {
  if (node.getSourceFile().isDeclarationFile) return true;
  for (let n: ts.Node | undefined = node; n !== undefined; n = n.parent) {
    if (hasModifier(n, ts.SyntaxKind.DeclareKeyword)) return true;
  }
  return false;
}

/** Package name from a path inside node_modules (`@types/x` names the package it types). */
export function packageFromPath(p: string): string | undefined {
  const posix = toPosix(p);
  const i = posix.lastIndexOf("/node_modules/");
  if (i < 0) return undefined;
  const rest = posix.slice(i + "/node_modules/".length).split("/");
  let name = rest[0]?.startsWith("@") === true ? `${rest[0]}/${rest[1] ?? ""}` : (rest[0] ?? "");
  if (name.startsWith("@types/")) {
    const typed = name.slice("@types/".length);
    name = typed.includes("__") ? `@${typed.replace("__", "/")}` : typed;
  }
  return name;
}

/** The package a bare module specifier names (`@scope/pkg/sub` → `@scope/pkg`). */
export function packageOfSpecifier(spec: string): string {
  if (spec.startsWith("node:")) return spec;
  const parts = spec.split("/");
  return spec.startsWith("@") ? `${parts[0]}/${parts[1] ?? ""}` : (parts[0] ?? spec);
}

export interface SymbolRow {
  id: string;
  name: string;
  kind: SymbolKind;
  origin: "project" | "external" | "lib";
  file: string | null;
  line: number | null;
  end_line: number | null;
  parent: string | null;
  package: string | null;
  exported: boolean;
  visibility: string | null;
  is_static: boolean;
  is_abstract: boolean;
  is_async: boolean;
  is_generator: boolean;
  is_readonly: boolean;
  is_optional: boolean;
  is_ambient: boolean;
}

export class Context {
  readonly loaded: Loaded;
  readonly tables: Tables;
  readonly root: string;

  private readonly idByKey = new Map<string, string>();
  private readonly keyById = new Map<string, string>();
  private readonly symbolRows = new Map<string, SymbolRow>();
  private readonly exportedKeys = new Set<string>();
  private readonly packageOfFile = new Map<string, string | null>();
  private readonly callSites = new Map<string, number>();
  private readonly fnBodies = new Set<string>();
  private readonly infoCache = new WeakMap<ts.SourceFile, SourceInfo | null>();
  private nextCallSite = 1;
  nextFlowNode = 1;
  nextAllocSite = 1;

  constructor(loaded: Loaded, tables: Tables, packageOf: (file: string) => string | null) {
    this.loaded = loaded;
    this.tables = tables;
    this.root = loaded.root;
    for (const s of loaded.sources) this.packageOfFile.set(s.path, packageOf(s.path));
  }

  infoOf(sf: ts.SourceFile): SourceInfo | undefined {
    let info = this.infoCache.get(sf);
    if (info === undefined) {
      info = this.loaded.byFileName.get(resolvedName(sf)) ?? null;
      this.infoCache.set(sf, info);
    }
    return info ?? undefined;
  }

  isProjectNode(node: ts.Node): boolean {
    return this.infoOf(node.getSourceFile()) !== undefined;
  }

  /** Assign every project id and call-site id, in path then source order. */
  prepass(): void {
    for (const info of this.loaded.sources) {
      this.markExports(info);
      this.idOfDecl(info.sf);
      const visit = (node: ts.Node): void => {
        if (isRegisteredDeclaration(node)) this.idOfDecl(node);
        if (ts.isParameter(node) && ts.isParameterPropertyDeclaration(node, node.parent)) this.idOfParameterProperty(node);
        if (isCallLike(node)) this.callSiteId(node);
        ts.forEachChild(node, visit);
      };
      ts.forEachChild(info.sf, visit);
    }
  }

  private markExports(info: SourceInfo): void {
    const moduleSymbol = info.checker.getSymbolAtLocation(info.sf);
    if (moduleSymbol === undefined) return;
    for (const exp of info.checker.getExportsOfModule(moduleSymbol)) {
      const target = this.canonicalDecl(this.resolveSymbol(exp, info.checker));
      if (target !== undefined && target.getSourceFile() === info.sf) this.exportedKeys.add(declKey(target));
    }
  }

  callSiteId(node: ts.Node): number {
    const key = declKey(node);
    let id = this.callSites.get(key);
    if (id === undefined) {
      id = this.nextCallSite++;
      this.callSites.set(key, id);
    }
    return id;
  }

  /** Follow aliases and local→export symbols to the symbol that is declared. */
  resolveSymbol(sym: ts.Symbol | undefined, checker: ts.TypeChecker): ts.Symbol | undefined {
    if (sym === undefined) return undefined;
    let s = sym;
    if (s.flags & ts.SymbolFlags.Alias) {
      try {
        s = checker.getAliasedSymbol(s);
      } catch {
        return undefined;
      }
    }
    // An alias that resolves nowhere lands on the checker's `unknown` symbol,
    // which has no declarations — `canonicalDecl` then reports it unresolved.
    return checker.getExportSymbolOfSymbol(s);
  }

  canonicalDecl(sym: ts.Symbol | undefined): ts.Declaration | undefined {
    const decls = sym?.declarations;
    if (decls === undefined || decls.length === 0) return undefined;
    let best: ts.Declaration | undefined;
    let bestRank: [number, string, number] | undefined;
    for (const d of decls) {
      const sf = d.getSourceFile();
      const info = this.infoOf(sf);
      const rank: [number, string, number] = [
        info !== undefined ? 0 : 1,
        info !== undefined ? info.path : toPosix(sf.fileName),
        d.pos,
      ];
      if (bestRank === undefined || compareRank(rank, bestRank) < 0) {
        best = d;
        bestRank = rank;
      }
    }
    return best;
  }

  /** The id of whatever `sym` denotes, registering an external symbol on first sight. */
  idOfSymbol(sym: ts.Symbol | undefined, checker: ts.TypeChecker): string | undefined {
    const resolved = this.resolveSymbol(sym, checker);
    const decl = this.canonicalDecl(resolved);
    if (decl === undefined || resolved === undefined) return undefined;
    // A constructor parameter property is one node declaring two symbols.
    if (ts.isParameter(decl) && resolved.flags & ts.SymbolFlags.Property) return this.idOfParameterProperty(decl);
    return this.idOfDecl(decl);
  }

  /** The id of a declaration node. */
  idOfDecl(decl: ts.Node): string {
    const key = declKey(decl);
    const known = this.idByKey.get(key);
    if (known !== undefined) return known;
    const info = this.infoOf(decl.getSourceFile());
    const id = info !== undefined ? this.projectId(decl, info) : this.externalId(decl);
    this.idByKey.set(key, id);
    return id;
  }

  private idOfParameterProperty(param: ts.ParameterDeclaration): string {
    const key = `${declKey(param)}:property`;
    const known = this.idByKey.get(key);
    if (known !== undefined) return known;
    const ctor = param.parent;
    const cls = ctor.parent;
    const info = this.infoOf(param.getSourceFile());
    const name = ts.isIdentifier(param.name) ? param.name.text : "<pattern>";
    let id: string;
    if (info !== undefined && cls !== undefined) {
      id = this.claim(`${this.idOfDecl(cls)}.${name}`, key, param);
      this.addSymbolRow({
        ...this.baseRow(id, name, "property", param, info),
        parent: this.idOfDecl(cls),
        visibility: visibilityOf(param),
        is_readonly: hasModifier(param, ts.SyntaxKind.ReadonlyKeyword),
      });
    } else {
      id = `${this.externalId(cls ?? param)}.${name}`;
    }
    this.idByKey.set(key, id);
    return id;
  }

  private claim(candidate: string, key: string, node: ts.Node): string {
    const sf = node.getSourceFile();
    const tries = [candidate, `${candidate}@${lineOf(node, sf)}`, `${candidate}@${lineOf(node, sf)}:${colOf(node, sf)}`];
    for (const t of tries) {
      const holder = this.keyById.get(t);
      if (holder === undefined || holder === key) {
        this.keyById.set(t, key);
        return t;
      }
    }
    for (let n = 2; ; n++) {
      const t = `${tries[2]}#${n}`;
      if (!this.keyById.has(t)) {
        this.keyById.set(t, key);
        return t;
      }
    }
  }

  private projectId(decl: ts.Node, info: SourceInfo): string {
    const key = declKey(decl);
    if (ts.isSourceFile(decl)) {
      const id = this.claim(`${info.path}#<module>`, key, decl);
      this.addSymbolRow({ ...this.baseRow(id, "<module>", "module", decl, info), parent: null, line: 1 });
      return id;
    }
    // An anonymous value or type literal named by its binding *is* that binding.
    const adopter = adoptingDeclaration(decl);
    if (adopter !== undefined) return this.idOfDecl(adopter);

    // A declaration of an already-seen symbol shares its id — overloads, merged
    // interfaces, a getter with its setter — except that two *bodies* are two
    // functions, and the later one gets an id of its own.
    const sym = declSymbol(decl, info.checker);
    const resolved = sym !== undefined ? info.checker.getExportSymbolOfSymbol(sym) : undefined;
    const canonical = this.canonicalDecl(resolved);
    const hasBody = bodyOf(decl) !== undefined || (ts.isVariableDeclaration(decl) && adoptedFunction(decl) !== undefined);
    if (canonical !== undefined && canonical !== decl && !ts.isSourceFile(canonical)) {
      const canonicalId = this.idOfDecl(canonical);
      if (!hasBody || !this.fnBodies.has(canonicalId)) {
        if (hasBody) this.fnBodies.add(canonicalId);
        return canonicalId;
      }
      const id = this.claim(`${canonicalId}@${lineOf(decl)}`, key, decl);
      this.addSymbolRow(this.describe(id, decl, info));
      return id;
    }

    const container = containerOf(decl);
    const containerId = container !== undefined ? this.idOfDecl(container) : `${info.path}#<module>`;
    const segment = segmentOf(decl);
    const candidate =
      container === undefined || ts.isSourceFile(container) ? `${info.path}#${segment}` : `${containerId}.${segment}`;
    const id = this.claim(candidate, key, decl);
    if (hasBody) this.fnBodies.add(id);
    this.addSymbolRow(this.describe(id, decl, info));
    return id;
  }

  private externalId(decl: ts.Node): string {
    const sf = decl.getSourceFile();
    const program = this.programOf(sf);
    const isLib = program?.isSourceFileDefaultLibrary(sf) ?? /[\\/]typescript[\\/]lib[\\/]lib\.[^\\/]*\.d\.ts$/.test(sf.fileName);
    const pkg = isLib ? undefined : packageFromPath(sf.fileName);
    const prefix = isLib ? "lib#" : pkg !== undefined ? `ext:${pkg}#` : `ext:${relTo(this.root, sf.fileName)}#`;
    const chain: string[] = [];
    let parentId: string | null = null;
    const container = ts.isSourceFile(decl) ? undefined : containerOf(decl);
    if (container !== undefined && !ts.isSourceFile(container)) {
      parentId = this.idOfDecl(container);
    }
    if (ts.isSourceFile(decl)) chain.push("<module>");
    else chain.push(segmentOf(decl));
    const id = parentId !== null ? `${parentId}.${chain.join(".")}` : `${prefix}${chain.join(".")}`;
    if (!this.symbolRows.has(id)) {
      const kind = ts.isSourceFile(decl) ? "module" : kindOf(decl);
      this.symbolRows.set(id, {
        id,
        name: ts.isSourceFile(decl) ? "<module>" : nameOf(decl),
        kind,
        origin: isLib ? "lib" : "external",
        file: null,
        line: null,
        end_line: null,
        parent: parentId,
        package: pkg ?? null,
        exported: false,
        visibility: null,
        is_static: isStatic(decl),
        is_abstract: hasModifier(decl, ts.SyntaxKind.AbstractKeyword),
        is_async: hasModifier(decl, ts.SyntaxKind.AsyncKeyword),
        is_generator: isGenerator(decl),
        is_readonly: hasModifier(decl, ts.SyntaxKind.ReadonlyKeyword),
        is_optional: hasQuestion(decl),
        is_ambient: true,
      });
    }
    return id;
  }

  private programOf(sf: ts.SourceFile): ts.Program | undefined {
    for (const p of this.loaded.projects) {
      if (p.program.getSourceFile(sf.fileName) === sf) return p.program;
    }
    return undefined;
  }

  private baseRow(id: string, name: string, kind: SymbolKind, decl: ts.Node, info: SourceInfo): SymbolRow {
    return {
      id,
      name,
      kind,
      origin: "project",
      file: info.path,
      line: ts.isSourceFile(decl) ? 1 : lineOf(decl, info.sf),
      end_line: endLineOf(decl, info.sf),
      parent: null,
      package: this.packageOfFile.get(info.path) ?? null,
      exported: this.exportedKeys.has(declKey(decl)),
      visibility: null,
      is_static: false,
      is_abstract: false,
      is_async: false,
      is_generator: false,
      is_readonly: false,
      is_optional: false,
      is_ambient: isInAmbientContext(decl),
    };
  }

  private describe(id: string, decl: ts.Node, info: SourceInfo): SymbolRow {
    const container = containerOf(decl);
    const fn = ts.isVariableDeclaration(decl) || ts.isPropertyDeclaration(decl) || ts.isPropertyAssignment(decl) ? adoptedFunction(decl) : undefined;
    const inClass = decl.parent !== undefined && ts.isClassLike(decl.parent);
    return {
      ...this.baseRow(id, nameOf(decl), kindOf(decl), decl, info),
      parent: container !== undefined ? this.idOfDecl(container) : null,
      exported: this.exportedKeys.has(declKey(decl)) || hasModifier(decl, ts.SyntaxKind.ExportKeyword),
      visibility: inClass ? visibilityOf(decl) : null,
      is_static: isStatic(decl),
      is_abstract: hasModifier(decl, ts.SyntaxKind.AbstractKeyword),
      is_async: hasModifier(decl, ts.SyntaxKind.AsyncKeyword) || (fn !== undefined && hasModifier(fn, ts.SyntaxKind.AsyncKeyword)),
      is_generator: isGenerator(decl) || (fn !== undefined && isGenerator(fn)),
      is_readonly:
        hasModifier(decl, ts.SyntaxKind.ReadonlyKeyword) ||
        (ts.isVariableDeclaration(decl) && ts.isVariableDeclarationList(decl.parent) && (decl.parent.flags & ts.NodeFlags.Const) !== 0),
      is_optional: hasQuestion(decl),
    };
  }

  private addSymbolRow(row: SymbolRow): void {
    if (!this.symbolRows.has(row.id)) this.symbolRows.set(row.id, row);
  }

  symbolRow(id: string): SymbolRow | undefined {
    return this.symbolRows.get(id);
  }

  private readonly vars = new Map<string, { fn: string; kind: string }>();

  /** Registers a dataflow variable; the first registration's owner and kind stand. */
  declareVar(id: string, fn: string, kind: string): void {
    if (!this.vars.has(id)) this.vars.set(id, { fn, kind });
  }

  flushVars(): void {
    for (const [id, v] of this.vars) this.tables.add("var", { id, fn: v.fn, kind: v.kind });
  }

  /** Emits `symbol` — last, since every layer can register external symbols. */
  flushSymbols(): void {
    for (const row of this.symbolRows.values()) this.tables.add("symbol", { ...row });
  }

  /** The innermost enclosing named declaration — what a reference is *from*. */
  ownerOf(node: ts.Node): string {
    for (let n: ts.Node | undefined = node.parent; n !== undefined; n = n.parent) {
      if (isOwner(n)) return this.idOfDecl(n);
    }
    return this.idOfDecl(node.getSourceFile());
  }

  /** The innermost enclosing function-like (or the module) — what *executes* a node. */
  executorOf(node: ts.Node): ts.Node {
    for (let n: ts.Node | undefined = node.parent; n !== undefined; n = n.parent) {
      if (isFunctionLike(n) || ts.isSourceFile(n)) return n;
    }
    return node.getSourceFile();
  }

  /** Symbol kind of an id's declaration, or undefined for an unregistered id. */
  kindOfId(id: string): SymbolKind | undefined {
    return this.symbolRows.get(id)?.kind;
  }

  fileOf(node: ts.Node): string {
    return this.infoOf(node.getSourceFile())?.path ?? relTo(this.root, node.getSourceFile().fileName);
  }
}

function compareRank(a: [number, string, number], b: [number, string, number]): number {
  if (a[0] !== b[0]) return a[0] - b[0];
  if (a[1] !== b[1]) return a[1] < b[1] ? -1 : 1;
  return a[2] - b[2];
}

const resolvedNames = new WeakMap<ts.SourceFile, string>();
function resolvedName(sf: ts.SourceFile): string {
  let name = resolvedNames.get(sf);
  if (name === undefined) {
    name = path.resolve(sf.fileName);
    resolvedNames.set(sf, name);
  }
  return name;
}

export function declKey(node: ts.Node): string {
  return `${resolvedName(node.getSourceFile())}:${node.pos}:${node.end}:${node.kind}`;
}

export function declSymbol(decl: ts.Node, checker: ts.TypeChecker): ts.Symbol | undefined {
  const name = (decl as Named).name;
  if (name !== undefined && (ts.isIdentifier(name) || ts.isPrivateIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name) || ts.isComputedPropertyName(name))) {
    const s = checker.getSymbolAtLocation(ts.isComputedPropertyName(name) ? decl : name) ?? (decl as ts.Node & { symbol?: ts.Symbol }).symbol;
    if (s !== undefined) return s;
  }
  return (decl as ts.Node & { symbol?: ts.Symbol }).symbol;
}

/** Declarations the pre-pass names. */
export function isRegisteredDeclaration(node: ts.Node): boolean {
  switch (node.kind) {
    case ts.SyntaxKind.VariableDeclaration:
      return ts.isIdentifier((node as ts.VariableDeclaration).name);
    case ts.SyntaxKind.BindingElement:
      return ts.isIdentifier((node as ts.BindingElement).name);
    case ts.SyntaxKind.Parameter:
    case ts.SyntaxKind.FunctionDeclaration:
    case ts.SyntaxKind.ClassDeclaration:
    case ts.SyntaxKind.ClassExpression:
    case ts.SyntaxKind.InterfaceDeclaration:
    case ts.SyntaxKind.TypeAliasDeclaration:
    case ts.SyntaxKind.EnumDeclaration:
    case ts.SyntaxKind.EnumMember:
    case ts.SyntaxKind.ModuleDeclaration:
    case ts.SyntaxKind.MethodDeclaration:
    case ts.SyntaxKind.MethodSignature:
    case ts.SyntaxKind.PropertyDeclaration:
    case ts.SyntaxKind.PropertySignature:
    case ts.SyntaxKind.GetAccessor:
    case ts.SyntaxKind.SetAccessor:
    case ts.SyntaxKind.Constructor:
    case ts.SyntaxKind.TypeParameter:
    case ts.SyntaxKind.PropertyAssignment:
    case ts.SyntaxKind.ShorthandPropertyAssignment:
    case ts.SyntaxKind.FunctionExpression:
    case ts.SyntaxKind.ArrowFunction:
    case ts.SyntaxKind.ClassStaticBlockDeclaration:
    case ts.SyntaxKind.ObjectLiteralExpression:
    case ts.SyntaxKind.TypeLiteral:
    case ts.SyntaxKind.CallSignature:
    case ts.SyntaxKind.ConstructSignature:
    case ts.SyntaxKind.IndexSignature:
    case ts.SyntaxKind.FunctionType:
    case ts.SyntaxKind.ConstructorType:
      return true;
    default:
      return false;
  }
}

export function isCallLike(node: ts.Node): boolean {
  return (
    ts.isCallExpression(node) ||
    ts.isNewExpression(node) ||
    ts.isTaggedTemplateExpression(node) ||
    ts.isDecorator(node) ||
    ts.isJsxOpeningElement(node) ||
    ts.isJsxSelfClosingElement(node)
  );
}

/** Declarations that own the references inside them. */
function isOwner(n: ts.Node): boolean {
  if (isFunctionLike(n)) return true;
  if (ts.isClassLike(n) || ts.isInterfaceDeclaration(n) || ts.isTypeAliasDeclaration(n) || ts.isEnumDeclaration(n)) return true;
  if (ts.isModuleDeclaration(n)) return true;
  if (ts.isPropertyDeclaration(n) || ts.isPropertySignature(n) || ts.isMethodSignature(n)) return true;
  if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name)) {
    // Module-level (or namespace-level) variables own their initializers; a
    // function's locals do not — their references belong to the function.
    const c = containerOf(n);
    return c === undefined || ts.isSourceFile(c) || ts.isModuleDeclaration(c);
  }
  return false;
}

export function containerOf(node: ts.Node): ts.Node | undefined {
  for (let n = node.parent; n !== undefined; n = n.parent) {
    if (ts.isModuleBlock(n)) continue;
    if (isContainer(n)) {
      // `declare global { … }` is not a naming scope.
      if (ts.isModuleDeclaration(n) && (n.flags & ts.NodeFlags.GlobalAugmentation) !== 0) continue;
      return n;
    }
  }
  return undefined;
}

function anonymousSegment(label: string, node: ts.Node): string {
  const sf = node.getSourceFile();
  return `<${label}@${lineOf(node, sf)}:${colOf(node, sf)}>`;
}

export function nameText(name: ts.Node): string {
  if (ts.isIdentifier(name) || ts.isPrivateIdentifier(name)) return name.text;
  if (ts.isStringLiteral(name) || ts.isNumericLiteral(name) || ts.isNoSubstitutionTemplateLiteral(name)) return name.text;
  if (ts.isComputedPropertyName(name)) return `[${truncate(name.expression.getText(), 60)}]`;
  return anonymousSegment("pattern", name);
}

function segmentOf(decl: ts.Node): string {
  if (ts.isConstructorDeclaration(decl)) return "constructor";
  if (ts.isExportAssignment(decl)) return "default";
  if (ts.isModuleDeclaration(decl) && ts.isStringLiteral(decl.name)) return decl.name.text;
  const name = (decl as Named).name;
  if (name !== undefined) return nameText(name);
  if (ts.isArrowFunction(decl)) return anonymousSegment("arrow", decl);
  if (ts.isFunctionExpression(decl)) return anonymousSegment("function", decl);
  if (ts.isFunctionDeclaration(decl) || ts.isClassDeclaration(decl)) return "default";
  if (ts.isClassExpression(decl)) return anonymousSegment("class", decl);
  if (ts.isObjectLiteralExpression(decl)) return anonymousSegment("object", decl);
  if (ts.isTypeLiteralNode(decl)) return anonymousSegment("type", decl);
  if (ts.isClassStaticBlockDeclaration(decl)) return anonymousSegment("static", decl);
  if (ts.isCallSignatureDeclaration(decl)) return anonymousSegment("call", decl);
  if (ts.isConstructSignatureDeclaration(decl)) return anonymousSegment("new", decl);
  if (ts.isIndexSignatureDeclaration(decl)) return anonymousSegment("index", decl);
  if (ts.isFunctionTypeNode(decl) || ts.isConstructorTypeNode(decl)) return anonymousSegment("fntype", decl);
  return anonymousSegment(ts.SyntaxKind[decl.kind] ?? "node", decl);
}

function nameOf(decl: ts.Node): string {
  if (ts.isConstructorDeclaration(decl)) return "constructor";
  if (ts.isExportAssignment(decl)) return "default";
  if (ts.isModuleDeclaration(decl) && ts.isStringLiteral(decl.name)) return decl.name.text;
  const name = (decl as Named).name;
  if (name !== undefined && (ts.isIdentifier(name) || ts.isPrivateIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name))) {
    return name.text;
  }
  if (name !== undefined && ts.isComputedPropertyName(name)) return nameText(name);
  if (name !== undefined) return "<pattern>";
  if (ts.isArrowFunction(decl)) return "<arrow>";
  if (ts.isFunctionExpression(decl)) return "<function>";
  if (ts.isFunctionDeclaration(decl) || ts.isClassDeclaration(decl)) return "default";
  if (ts.isClassExpression(decl)) return "<class>";
  if (ts.isObjectLiteralExpression(decl)) return "<object>";
  if (ts.isTypeLiteralNode(decl)) return "<type>";
  if (ts.isClassStaticBlockDeclaration(decl)) return "<static>";
  return "<signature>";
}

/** The function an initializer is, when a binding is initialized with one. */
export function adoptedFunction(decl: ts.Node): ts.FunctionLikeDeclaration | ts.ClassLikeDeclaration | undefined {
  const init = (decl as { initializer?: ts.Node }).initializer ?? (ts.isExportAssignment(decl) ? decl.expression : undefined);
  if (init === undefined) return undefined;
  const v = skipWrappers(init);
  if (ts.isArrowFunction(v) || ts.isFunctionExpression(v)) return v;
  return undefined;
}

function isInFunction(node: ts.Node): boolean {
  const c = containerOf(node);
  return c !== undefined && isFunctionLike(c);
}

export function kindOf(decl: ts.Node): SymbolKind {
  switch (decl.kind) {
    case ts.SyntaxKind.SourceFile:
      return "module";
    case ts.SyntaxKind.ModuleDeclaration:
      return ts.isStringLiteral((decl as ts.ModuleDeclaration).name) ? "module" : "namespace";
    case ts.SyntaxKind.ClassDeclaration:
    case ts.SyntaxKind.ClassExpression:
      return "class";
    case ts.SyntaxKind.InterfaceDeclaration:
      return "interface";
    case ts.SyntaxKind.TypeAliasDeclaration:
      return "type_alias";
    case ts.SyntaxKind.EnumDeclaration:
      return "enum";
    case ts.SyntaxKind.EnumMember:
      return "enum_member";
    case ts.SyntaxKind.FunctionDeclaration:
    case ts.SyntaxKind.FunctionExpression:
    case ts.SyntaxKind.ArrowFunction:
      return "function";
    case ts.SyntaxKind.MethodDeclaration:
    case ts.SyntaxKind.MethodSignature:
      return "method";
    case ts.SyntaxKind.Constructor:
      return "constructor";
    case ts.SyntaxKind.GetAccessor:
      return "getter";
    case ts.SyntaxKind.SetAccessor:
      return "setter";
    case ts.SyntaxKind.PropertyDeclaration:
    case ts.SyntaxKind.PropertyAssignment:
      return adoptedFunction(decl) !== undefined ? "method" : "property";
    case ts.SyntaxKind.PropertySignature:
    case ts.SyntaxKind.ShorthandPropertyAssignment:
      return "property";
    case ts.SyntaxKind.VariableDeclaration:
    case ts.SyntaxKind.BindingElement: {
      if (adoptedFunction(decl) !== undefined) return "function";
      if (ts.isVariableDeclaration(decl) && ts.isCatchClause(decl.parent)) return "local";
      return isInFunction(decl) ? "local" : "variable";
    }
    case ts.SyntaxKind.ExportAssignment:
      return adoptedFunction(decl) !== undefined ? "function" : "variable";
    case ts.SyntaxKind.Parameter:
      return "parameter";
    case ts.SyntaxKind.TypeParameter:
      return "type_parameter";
    case ts.SyntaxKind.ObjectLiteralExpression:
      return "object";
    case ts.SyntaxKind.TypeLiteral:
      return "type_literal";
    case ts.SyntaxKind.ClassStaticBlockDeclaration:
      return "static_block";
    default:
      return "signature";
  }
}

function visibilityOf(decl: ts.Node): string {
  const name = (decl as Named).name;
  if (name !== undefined && ts.isPrivateIdentifier(name)) return "hash_private";
  if (hasModifier(decl, ts.SyntaxKind.PrivateKeyword)) return "private";
  if (hasModifier(decl, ts.SyntaxKind.ProtectedKeyword)) return "protected";
  return "public";
}

function isGenerator(decl: ts.Node): boolean {
  return (decl as { asteriskToken?: ts.Node }).asteriskToken !== undefined;
}

function hasQuestion(decl: ts.Node): boolean {
  return (decl as { questionToken?: ts.Node }).questionToken !== undefined;
}
