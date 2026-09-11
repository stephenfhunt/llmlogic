// The refs layer: every name the checker can resolve, as a `ref` edge from its
// enclosing declaration; every call site with its resolved target and dispatch
// kind; inheritance, overriding, member access, type positions and types.

import ts from "typescript";
import {
  type Context,
  declSymbol,
  isFunctionLike,
  isStatic,
  lineOf,
  colOf,
  skipWrappers,
  truncate,
} from "../context.ts";
import type { SourceInfo } from "../program.ts";

const SKIPPED_TARGET_KINDS = new Set(["local", "parameter", "type_parameter"]);

/** Parents whose `name` is a declaration, not a reference. */
function isDeclarationName(node: ts.Node): boolean {
  const p = node.parent;
  if (p === undefined) return false;
  if ((p as { name?: ts.Node }).name !== node) return false;
  switch (p.kind) {
    case ts.SyntaxKind.PropertyAccessExpression:
    case ts.SyntaxKind.QualifiedName:
    case ts.SyntaxKind.JsxAttribute:
    case ts.SyntaxKind.ShorthandPropertyAssignment:
      return false;
    default:
      return true;
  }
}

/** A string literal naming a member in brackets: `obj["key"]`. TypeScript lets
 * this reach a `private` member, so it is where content coupling hides. */
function isBracketMemberName(node: ts.Node): node is ts.StringLiteralLike {
  const p = node.parent;
  return ts.isStringLiteralLike(node) && p !== undefined && ts.isElementAccessExpression(p) && p.argumentExpression === node;
}

function isSkippedName(node: ts.Node): boolean {
  const p = node.parent;
  if (p === undefined) return true;
  return (
    ts.isImportSpecifier(p) ||
    ts.isExportSpecifier(p) ||
    ts.isImportClause(p) ||
    ts.isNamespaceImport(p) ||
    ts.isNamespaceExport(p) ||
    ts.isImportEqualsDeclaration(p) ||
    ts.isLabeledStatement(p) ||
    ts.isBreakOrContinueStatement(p) ||
    ts.isJsxAttribute(p) ||
    ts.isJsxClosingElement(p) ||
    ts.isMetaProperty(p) ||
    ts.isExportAssignment(p) ||
    ts.isTypePredicateNode(p)
  );
}

/** Walks up through property-access / qualified-name chains to the expression whose position decides a name's role. */
function roleNode(node: ts.Node): ts.Node {
  let n = node;
  while (n.parent !== undefined) {
    const p = n.parent;
    if (ts.isPropertyAccessExpression(p) && p.name === n) n = p;
    else if (ts.isElementAccessExpression(p) && p.argumentExpression === n) n = p;
    else if (ts.isQualifiedName(p) && p.right === n) n = p;
    else if (ts.isParenthesizedExpression(p) || ts.isNonNullExpression(p)) n = p;
    else break;
  }
  return n;
}

function inTypePosition(node: ts.Node): boolean {
  for (let n: ts.Node | undefined = node.parent; n !== undefined; n = n.parent) {
    if (ts.isTypeQueryNode(n)) return false;
    if (ts.isExpressionWithTypeArguments(n)) {
      // `implements X` and interface `extends X` are types; class `extends X` is a value.
      const clause = n.parent;
      if (ts.isHeritageClause(clause)) return clause.token === ts.SyntaxKind.ImplementsKeyword || ts.isInterfaceDeclaration(clause.parent);
    }
    if (ts.isTypeNode(n)) return true;
    if (ts.isExpression(n) || ts.isStatement(n) || ts.isSourceFile(n)) return false;
  }
  return false;
}

function inTypeQuery(node: ts.Node): boolean {
  for (let n: ts.Node | undefined = node.parent; n !== undefined; n = n.parent) {
    if (ts.isTypeQueryNode(n)) return true;
    if (!ts.isQualifiedName(n) && !ts.isPropertyAccessExpression(n) && !ts.isIdentifier(n)) return false;
  }
  return false;
}

function heritageKind(node: ts.Node): "extends" | "implements" | undefined {
  const r = roleNode(node);
  const ewta = r.parent;
  if (ewta === undefined || !ts.isExpressionWithTypeArguments(ewta) || ewta.expression !== r) return undefined;
  const clause = ewta.parent;
  if (!ts.isHeritageClause(clause)) return undefined;
  return clause.token === ts.SyntaxKind.ImplementsKeyword ? "implements" : "extends";
}

type Mode = "read" | "write" | "readwrite";

function accessMode(expr: ts.Node): Mode {
  let n = expr;
  while (n.parent !== undefined && (ts.isParenthesizedExpression(n.parent) || ts.isNonNullExpression(n.parent))) n = n.parent;
  const p = n.parent;
  if (p === undefined) return "read";
  if (ts.isBinaryExpression(p) && p.left === n) {
    const op = p.operatorToken.kind;
    if (op === ts.SyntaxKind.EqualsToken) return "write";
    if (op >= ts.SyntaxKind.FirstCompoundAssignment && op <= ts.SyntaxKind.LastCompoundAssignment) return "readwrite";
  }
  if ((ts.isPrefixUnaryExpression(p) || ts.isPostfixUnaryExpression(p)) &&
    (p.operator === ts.SyntaxKind.PlusPlusToken || p.operator === ts.SyntaxKind.MinusMinusToken)) {
    return "readwrite";
  }
  if (ts.isDeleteExpression(p)) return "write";
  return "read";
}

function calleeOf(node: ts.Node): ts.Node | undefined {
  if (ts.isCallExpression(node) || ts.isNewExpression(node)) return node.expression;
  if (ts.isTaggedTemplateExpression(node)) return node.tag;
  if (ts.isDecorator(node)) return node.expression;
  if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) return node.tagName;
  return undefined;
}

/** The expression a call's callee is, if it is being called (not merely mentioned). */
function callRole(r: ts.Node): "call" | "new" | "jsx" | "decorator" | undefined {
  const p = r.parent;
  if (p === undefined) return undefined;
  if (ts.isCallExpression(p) && p.expression === r) return ts.isDecorator(p.parent) ? "decorator" : "call";
  if (ts.isTaggedTemplateExpression(p) && p.tag === r) return "call";
  if (ts.isNewExpression(p) && p.expression === r) return "new";
  if (ts.isDecorator(p) && p.expression === r) return "decorator";
  if ((ts.isJsxOpeningElement(p) || ts.isJsxSelfClosingElement(p)) && p.tagName === r) return "jsx";
  return undefined;
}

function nameNodeOf(callee: ts.Node): ts.Node {
  const c = skipWrappers(callee);
  if (ts.isPropertyAccessExpression(c)) return c.name;
  if (ts.isElementAccessExpression(c)) return c.argumentExpression;
  return c;
}

function typePosition(node: ts.Node): string {
  for (let n: ts.Node | undefined = node.parent; n !== undefined; n = n.parent) {
    if (ts.isParameter(n)) return "param";
    if (ts.isPropertyDeclaration(n) || ts.isPropertySignature(n)) return "property";
    if (ts.isVariableDeclaration(n)) return "variable";
    if (ts.isHeritageClause(n)) return n.token === ts.SyntaxKind.ImplementsKeyword ? "implements" : "extends";
    if (ts.isTypeAliasDeclaration(n)) return "alias";
    if (ts.isAsExpression(n) || ts.isTypeAssertionExpression(n) || ts.isSatisfiesExpression(n)) return "assertion";
    if (ts.isCallExpression(n) || ts.isNewExpression(n) || ts.isTaggedTemplateExpression(n)) return "type_arg";
    if (isFunctionLike(n) || ts.isMethodSignature(n) || ts.isCallSignatureDeclaration(n) || ts.isFunctionTypeNode(n)) return "return";
    if (ts.isIndexSignatureDeclaration(n)) return "property";
    if (ts.isStatement(n) || ts.isClassLike(n) || ts.isInterfaceDeclaration(n)) return "other";
  }
  return "other";
}

/** Types whose members `member_access` records: classes, interfaces, and object
 * types — `type Config = { … }` and inline `{ … }` — which in a class-less
 * codebase carry most of its records. */
const MEMBER_OWNERS = new Set(["class", "interface", "type_alias", "type_literal"]);

function isMemberOwner(ctx: Context, id: string | null | undefined): boolean {
  if (id === null || id === undefined) return false;
  return MEMBER_OWNERS.has(ctx.kindOfId(id) ?? "");
}

export function extractRefs(ctx: Context): void {
  for (const info of ctx.loaded.sources) {
    extractFile(ctx, info);
  }
}

function extractFile(ctx: Context, info: SourceInfo): void {
  const t = ctx.tables;
  const { checker, sf } = info;
  const file = info.path;
  const typed = new Set<string>();

  const visit = (node: ts.Node): void => {
    if (ts.isIdentifier(node) || ts.isPrivateIdentifier(node) || isBracketMemberName(node)) name(node);
    if (ts.isCallExpression(node) || ts.isNewExpression(node) || ts.isTaggedTemplateExpression(node) ||
      ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node) ||
      (ts.isDecorator(node) && !ts.isCallExpression(node.expression))) {
      call(node);
    }
    if (ts.isClassLike(node) || ts.isInterfaceDeclaration(node)) inheritance(node);
    if (hasValueType(node)) symbolType(node);
    ts.forEachChild(node, visit);
  };

  const name = (node: ts.Identifier | ts.PrivateIdentifier | ts.StringLiteralLike): void => {
    if (isSkippedName(node)) return;
    let sym: ts.Symbol | undefined;
    if (ts.isShorthandPropertyAssignment(node.parent) && node.parent.name === node) {
      sym = checker.getShorthandAssignmentValueSymbol(node.parent);
    } else {
      if (isDeclarationName(node)) return;
      sym = checker.getSymbolAtLocation(node);
    }
    const r = roleNode(node);
    const role = callRole(r);
    const typeish = inTypePosition(node);
    const line = lineOf(node, sf);
    const from = ctx.ownerOf(node);
    const id = ctx.idOfSymbol(sym, checker);
    if (id === undefined) {
      // A property of an `any` receiver resolves to nothing by design; record
      // only names that should have resolved.
      const p = node.parent;
      if (ts.isPropertyAccessExpression(p) && p.name === node) {
        const recv = checker.getTypeAtLocation(p.expression);
        if (recv.flags & (ts.TypeFlags.Any | ts.TypeFlags.Unknown)) return;
      }
      if (ts.isIdentifier(node) && (node.text === "undefined" || node.text === "arguments")) return;
      if (isBracketMemberName(node)) return; // `record["key"]` on an index signature names no member
      const kind = role === "call" || role === "new" ? "call" : role === "jsx" ? "jsx" : typeish ? "type" : "read";
      t.add("unresolved_ref", { from, name: node.text, kind, file, line });
      return;
    }
    const target = ctx.symbolRow(id);
    if (target !== undefined && SKIPPED_TARGET_KINDS.has(target.kind)) return;

    const heritage = heritageKind(node);
    let kind: string;
    if (heritage !== undefined) kind = heritage;
    else if (inTypeQuery(node)) kind = "typeof";
    else if (typeish) kind = "type";
    else if (role === "call") kind = "call";
    else if (role === "new") kind = "new";
    else if (role === "jsx") kind = "jsx";
    else if (role === "decorator") kind = "decorator";
    else {
      const mode = accessMode(r);
      const tk = target?.kind;
      const callable = tk === "function" || tk === "method" || tk === "class" || tk === "constructor";
      kind = mode !== "read" ? mode : callable ? "value" : "read";
    }
    t.add("ref", { from, to: id, kind, file, line });

    if (kind === "type") t.add("type_ref", { from, to: id, position: typePosition(node), file, line });

    // Member access on a project type: the raw material of cohesion and stamp coupling.
    const tk = target?.kind;
    if (target !== undefined && target.origin === "project" && (tk === "property" || tk === "method" || tk === "getter" || tk === "setter") &&
      isMemberOwner(ctx, target.parent) && !typeish && role !== "jsx") {
      const p = node.parent;
      const receiver = ts.isPropertyAccessExpression(p) && p.name === node ? p.expression
        : ts.isElementAccessExpression(p) && p.argumentExpression === node ? p.expression
          : undefined;
      const viaThis = receiver !== undefined &&
        (receiver.kind === ts.SyntaxKind.ThisKeyword || receiver.kind === ts.SyntaxKind.SuperKeyword);
      const mode = role === "call" ? "call" : accessMode(r);
      t.add("member_access", { fn: from, member: id, owner: target.parent, mode, via_this: viaThis, file, line });
    }
  };

  const call = (node: ts.Node): void => {
    const id = ctx.callSiteId(node);
    const caller = ctx.ownerOf(node);
    const calleeExpr = calleeOf(node);
    let kind: string;
    if (ts.isNewExpression(node)) kind = "new";
    else if (ts.isTaggedTemplateExpression(node)) kind = "tagged";
    else if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) kind = "jsx";
    else if (ts.isDecorator(node) || ts.isDecorator(node.parent)) kind = "decorator";
    else if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.SuperKeyword) kind = "super";
    else kind = "call";

    let callee: string | null = null;
    let dispatch = "unresolved";
    let sig: ts.Signature | undefined;
    try {
      sig = checker.getResolvedSignature(node as ts.CallLikeExpression);
    } catch {
      sig = undefined;
    }
    const decl = sig?.declaration;
    const nameNode = calleeExpr !== undefined ? nameNodeOf(calleeExpr) : undefined;
    const calleeSym = nameNode !== undefined && calleeExpr !== undefined && calleeExpr.kind !== ts.SyntaxKind.SuperKeyword
      ? checker.getSymbolAtLocation(nameNode)
      : undefined;

    if (decl !== undefined && !ts.isJSDocSignature(decl) &&
      (isFunctionLike(decl) || ts.isMethodSignature(decl) || ts.isFunctionDeclaration(decl) || ts.isMethodDeclaration(decl))) {
      callee = ctx.idOfDecl(decl);
      const container = decl.parent;
      const isMember = container !== undefined && (ts.isClassLike(container) || ts.isInterfaceDeclaration(container) || ts.isTypeLiteralNode(container));
      const memberHolder = ts.isArrowFunction(decl) || ts.isFunctionExpression(decl) ? skipWrappersUp(decl) : decl;
      const isClassProperty = memberHolder !== undefined && ts.isPropertyDeclaration(memberHolder) && !isStatic(memberHolder);
      if (kind === "new" || kind === "super") dispatch = "static";
      else if ((isMember && !isStatic(decl) && !ts.isConstructorDeclaration(decl)) || isClassProperty) dispatch = "virtual";
      else dispatch = "static";
    } else if (kind === "new" || kind === "super") {
      // An implicit constructor: the class itself is the target.
      const cls = kind === "super" ? superClassSymbol(node, checker) : calleeSym;
      callee = ctx.idOfSymbol(cls, checker) ?? null;
      dispatch = callee !== null ? "static" : "unresolved";
    } else if (calleeSym !== undefined) {
      // A function-typed value — a parameter, variable or property holding one.
      // Outside the project there is no value to follow (a library's facts
      // stop at its declarations), so the declaration is the target: vitest's
      // `expect(x).toBe(…)` calls `Assertion.toBe`, and says so.
      callee = ctx.idOfSymbol(calleeSym, checker) ?? null;
      const row = callee !== null ? ctx.symbolRow(callee) : undefined;
      if (callee === null) dispatch = "unresolved";
      else if (row !== undefined && row.origin !== "project") dispatch = row.kind === "property" || row.kind === "method" ? "virtual" : "static";
      else dispatch = "indirect";
    }
    if (kind === "jsx") {
      // An element names its component (or intrinsic tag) directly.
      callee ??= ctx.idOfSymbol(calleeSym, checker) ?? null;
      dispatch = callee !== null ? "static" : "unresolved";
    }

    const args = ts.isCallExpression(node) || ts.isNewExpression(node) ? (node.arguments ?? ts.factory.createNodeArray()) : undefined;
    let parent = node.parent;
    while (parent !== undefined && ts.isParenthesizedExpression(parent)) parent = parent.parent;
    t.add("call_site", {
      id,
      caller,
      callee,
      callee_name: nameNode !== undefined ? truncate(nameNode.getText(sf), 100) : kind === "super" ? "super" : null,
      dispatch,
      kind,
      file,
      line: lineOf(node, sf),
      col: colOf(node, sf),
      args: args?.length ?? 0,
      awaited: parent !== undefined && ts.isAwaitExpression(parent),
      optional: ts.isCallExpression(node) && (node.questionDotToken !== undefined || ts.isOptionalChain(node)),
      spread: args?.some((a) => ts.isSpreadElement(a)) ?? false,
    });
  };

  const inheritance = (node: ts.ClassLikeDeclaration | ts.InterfaceDeclaration): void => {
    const self = ctx.idOfDecl(node);
    const bases: { type: ts.Type; staticSide: ts.Type | undefined }[] = [];
    for (const clause of node.heritageClauses ?? []) {
      for (const h of clause.types) {
        const sym = checker.getSymbolAtLocation(h.expression) ?? checker.getTypeAtLocation(h).getSymbol();
        const baseId = ctx.idOfSymbol(sym, checker);
        if (baseId !== undefined) {
          if (clause.token === ts.SyntaxKind.ImplementsKeyword) t.add("implements", { class: self, interface: baseId });
          else t.add("extends", { child: self, parent: baseId });
        }
        const isClassExtends = clause.token === ts.SyntaxKind.ExtendsKeyword && ts.isClassLike(node);
        bases.push({ type: checker.getTypeAtLocation(h), staticSide: isClassExtends ? checker.getTypeAtLocation(h.expression) : undefined });
      }
    }
    for (const member of node.members) {
      const nm = member.name;
      if (nm === undefined || ts.isPrivateIdentifier(nm) || ts.isConstructorDeclaration(member)) continue;
      const memberSym = declSymbol(member, checker);
      if (memberSym === undefined) continue;
      const memberName = ts.symbolName(memberSym);
      const memberId = ctx.idOfDecl(member);
      for (const b of bases) {
        const side = isStatic(member) ? b.staticSide : b.type;
        if (side === undefined) continue;
        const baseProp = checker.getPropertyOfType(side, memberName);
        const baseId = ctx.idOfSymbol(baseProp, checker);
        if (baseId !== undefined && baseId !== memberId) t.add("overrides", { member: memberId, base: baseId });
      }
    }
  };

  const symbolType = (node: ts.Node): void => {
    const id = ctx.idOfDecl(node);
    if (typed.has(id)) return;
    typed.add(id);
    const sym = declSymbol(node, checker);
    if (sym === undefined) return;
    let type: ts.Type;
    try {
      type = checker.getTypeOfSymbolAtLocation(sym, node);
    } catch {
      return;
    }
    t.add("symbol_type", {
      symbol: id,
      text: truncate(checker.typeToString(type, undefined, ts.TypeFormatFlags.NoTruncation), 200),
      is_any: (type.flags & ts.TypeFlags.Any) !== 0,
      is_unknown: (type.flags & ts.TypeFlags.Unknown) !== 0,
      is_promise: isThenable(checker, type),
      is_function: type.getCallSignatures().length > 0,
      is_union: type.isUnion(),
    });
  };

  ts.forEachChild(sf, visit);
}

function skipWrappersUp(node: ts.Node): ts.Node | undefined {
  let p = node.parent;
  while (p !== undefined && (ts.isParenthesizedExpression(p) || ts.isAsExpression(p) || ts.isSatisfiesExpression(p))) p = p.parent;
  return p;
}

function superClassSymbol(node: ts.Node, checker: ts.TypeChecker): ts.Symbol | undefined {
  for (let n: ts.Node | undefined = node.parent; n !== undefined; n = n.parent) {
    if (ts.isClassLike(n)) {
      const ext = n.heritageClauses?.find((c) => c.token === ts.SyntaxKind.ExtendsKeyword)?.types[0];
      return ext !== undefined ? checker.getSymbolAtLocation(ext.expression) : undefined;
    }
  }
  return undefined;
}

export function isThenable(checker: ts.TypeChecker, type: ts.Type): boolean {
  if (type.flags & ts.TypeFlags.Any) return false;
  const then = checker.getPropertyOfType(type, "then");
  return then !== undefined;
}

/** Declarations whose value carries a type worth recording. */
function hasValueType(node: ts.Node): boolean {
  switch (node.kind) {
    case ts.SyntaxKind.VariableDeclaration:
    case ts.SyntaxKind.BindingElement:
    case ts.SyntaxKind.Parameter:
      return ts.isIdentifier((node as ts.VariableDeclaration).name);
    case ts.SyntaxKind.FunctionDeclaration:
    case ts.SyntaxKind.MethodDeclaration:
    case ts.SyntaxKind.MethodSignature:
    case ts.SyntaxKind.PropertyDeclaration:
    case ts.SyntaxKind.PropertySignature:
    case ts.SyntaxKind.GetAccessor:
    case ts.SyntaxKind.SetAccessor:
    case ts.SyntaxKind.PropertyAssignment:
    case ts.SyntaxKind.EnumMember:
      return true;
    default:
      return false;
  }
}
