// The quality layer: what the compiler says about the code, and the places its
// authors overrode, suppressed or deferred something — diagnostics, `@ts-`
// directives, lint suppressions, TODO markers, where `any` enters, type
// assertions, literals (for connascence of meaning), discarded promises, and
// throw and catch sites.

import ts from "typescript";
import { type Context, isFunctionLike, lineOf, skipWrappers, truncate } from "../context.ts";
import type { SourceInfo } from "../program.ts";
import { isThenable } from "./refs.ts";

const CATEGORY: Record<number, string> = {
  [ts.DiagnosticCategory.Error]: "error",
  [ts.DiagnosticCategory.Warning]: "warning",
  [ts.DiagnosticCategory.Suggestion]: "suggestion",
  [ts.DiagnosticCategory.Message]: "message",
};

export function extractQuality(ctx: Context): void {
  for (const project of ctx.loaded.projects) {
    for (const d of project.configDiagnostics) diagnostic(ctx, d);
  }
  for (const info of ctx.loaded.sources) {
    const { program } = info.project;
    for (const d of program.getSyntacticDiagnostics(info.sf)) diagnostic(ctx, d);
    for (const d of program.getSemanticDiagnostics(info.sf)) diagnostic(ctx, d);
    comments(ctx, info);
    walk(ctx, info);
  }
}

function diagnostic(ctx: Context, d: ts.Diagnostic): void {
  const info = d.file !== undefined ? ctx.infoOf(d.file) : undefined;
  const line = d.file !== undefined && d.start !== undefined ? d.file.getLineAndCharacterOfPosition(d.start).line + 1 : null;
  const message = ts.flattenDiagnosticMessageText(d.messageText, "\n").split("\n")[0] ?? "";
  ctx.tables.add("diagnostic", {
    file: info?.path ?? null,
    line: info !== undefined ? line : null,
    code: d.code,
    category: CATEGORY[d.category] ?? "message",
    message: truncate(message, 300),
  });
}

const TS_DIRECTIVE = /@ts-(ignore|expect-error|nocheck|check)\b/;
const MARKER = /\b(TODO|FIXME|HACK|XXX)\b[:\s-]*(.*)/;

/** A lint or coverage suppression in one comment line: [tool, directive, rules]. */
function lintDirective(line: string): [string, string, string] | undefined {
  let m = /\beslint-(disable-next-line|disable-line|disable|enable)\b\s*(.*)/.exec(line);
  if (m !== null) return ["eslint", m[1] ?? "", m[2] ?? ""];
  m = /\bbiome-ignore(-all|-start|-end)?\b\s*(.*)/.exec(line);
  if (m !== null) return ["biome", `ignore${m[1] ?? ""}`, m[2] ?? ""];
  m = /\btslint:(disable-next-line|disable-line|disable|enable)\b\s*(.*)/.exec(line);
  if (m !== null) return ["tslint", m[1] ?? "", m[2] ?? ""];
  if (/\bprettier-ignore\b/.test(line)) return ["prettier", "ignore", ""];
  m = /\b(istanbul|c8) ignore (next|if|else|file|start|stop)\b/.exec(line);
  if (m !== null) return [m[1] ?? "istanbul", `ignore ${m[2] ?? ""}`, ""];
  return undefined;
}

/** Every comment in the file, found through the parser's trivia (so a `//`
 * inside a string or a regular expression is not one). */
function commentRanges(sf: ts.SourceFile): ts.CommentRange[] {
  const seen = new Map<number, ts.CommentRange>();
  const add = (ranges: ts.CommentRange[] | undefined) => {
    for (const r of ranges ?? []) seen.set(r.pos, r);
  };
  const visit = (n: ts.Node): void => {
    add(ts.getLeadingCommentRanges(sf.text, n.getFullStart()));
    add(ts.getTrailingCommentRanges(sf.text, n.getEnd()));
    ts.forEachChild(n, visit);
  };
  visit(sf);
  add(ts.getLeadingCommentRanges(sf.text, sf.endOfFileToken.getFullStart()));
  return [...seen.values()].sort((a, b) => a.pos - b.pos);
}

function comments(ctx: Context, info: SourceInfo): void {
  const t = ctx.tables;
  const { sf } = info;
  for (const range of commentRanges(sf)) {
    const text = sf.text.slice(range.pos, range.end);
    // Each line of a block comment is its own place to look.
    let offset = 0;
    for (const raw of text.split("\n")) {
      const line = sf.getLineAndCharacterOfPosition(range.pos + offset).line + 1;
      offset += raw.length + 1;
      const body = raw.replace(/\*\/\s*$/, "");
      const d = TS_DIRECTIVE.exec(body);
      if (d !== null) t.add("ts_directive", { file: info.path, line, kind: `ts_${(d[1] ?? "").replace("-", "_")}` });
      const lint = lintDirective(body);
      if (lint !== undefined) {
        const [tool, directive, rules] = lint;
        t.add("lint_directive", { file: info.path, line, tool, directive, rules: rules.trim() === "" ? null : truncate(rules, 200) });
      }
      const mk = MARKER.exec(body);
      if (mk !== null) t.add("comment_marker", { file: info.path, line, kind: (mk[1] ?? "todo").toLowerCase(), text: truncate(mk[2] ?? "", 200) });
    }
  }
}

function inTypePosition(node: ts.Node): boolean {
  for (let n: ts.Node | undefined = node.parent; n !== undefined; n = n.parent) {
    if (ts.isTypeNode(n)) return true;
    if (ts.isExpression(n) || ts.isStatement(n)) return false;
  }
  return false;
}

/** Literals that are names rather than values: keys, specifiers, directives, types. */
function isNameLiteral(node: ts.Node): boolean {
  const p = node.parent;
  if (p === undefined) return true;
  if ((ts.isPropertyAssignment(p) || ts.isPropertyDeclaration(p) || ts.isPropertySignature(p) || ts.isMethodDeclaration(p) ||
    ts.isEnumMember(p) || ts.isGetAccessor(p) || ts.isSetAccessor(p)) && p.name === node) {
    return true;
  }
  if (ts.isImportDeclaration(p) || ts.isExportDeclaration(p) || ts.isExternalModuleReference(p) || ts.isModuleDeclaration(p)) return true;
  if (ts.isLiteralTypeNode(p) || inTypePosition(node)) return true;
  if (ts.isExpressionStatement(p) && ts.isStringLiteral(node)) return true; // "use strict"
  if (ts.isCallExpression(p) && p.arguments[0] === node) {
    if (p.expression.kind === ts.SyntaxKind.ImportKeyword) return true;
    if (ts.isIdentifier(p.expression) && p.expression.text === "require") return true;
  }
  return false;
}

function containsThrow(block: ts.Node): boolean {
  let found = false;
  const visit = (n: ts.Node): void => {
    if (found || isFunctionLike(n)) return;
    if (ts.isThrowStatement(n)) found = true;
    else ts.forEachChild(n, visit);
  };
  visit(block);
  return found;
}

function walk(ctx: Context, info: SourceInfo): void {
  const t = ctx.tables;
  const { checker, sf } = info;
  const file = info.path;
  const typeOf = (n: ts.Node): ts.Type | undefined => {
    try {
      return checker.getTypeAtLocation(n);
    } catch {
      return undefined;
    }
  };
  const isAny = (n: ts.Node) => ((typeOf(n)?.flags ?? 0) & ts.TypeFlags.Any) !== 0;

  const visit = (node: ts.Node): void => {
    const line = () => lineOf(node, sf);
    if (node.kind === ts.SyntaxKind.AnyKeyword) {
      t.add("any_site", { fn: ctx.ownerOf(node), file, line: line(), kind: "explicit" });
    } else if (ts.isParameter(node) && node.type === undefined && ts.isIdentifier(node.name) && isAny(node.name)) {
      t.add("any_site", { fn: ctx.ownerOf(node), file, line: line(), kind: "implicit_param" });
    } else if (ts.isVariableDeclaration(node) && node.type === undefined && ts.isIdentifier(node.name) &&
      (node.initializer === undefined || !ts.isCallExpression(skipWrappers(node.initializer))) && isAny(node.name)) {
      t.add("any_site", { fn: ctx.ownerOf(node), file, line: line(), kind: "implicit_var" });
    } else if (ts.isCallExpression(node) && isAny(node)) {
      t.add("any_site", { fn: ctx.ownerOf(node), file, line: line(), kind: "call_result" });
    }

    if (ts.isAsExpression(node) || ts.isTypeAssertionExpression(node) || ts.isSatisfiesExpression(node) || ts.isNonNullExpression(node)) {
      let kind = "non_null";
      let toType: string | null = null;
      let toAny = false;
      if (!ts.isNonNullExpression(node)) {
        toType = truncate(node.type.getText(sf), 200);
        toAny = node.type.kind === ts.SyntaxKind.AnyKeyword;
        if (ts.isAsExpression(node)) kind = ts.isConstTypeReference(node.type) ? "as_const" : "cast";
        else if (ts.isTypeAssertionExpression(node)) kind = ts.isConstTypeReference(node.type) ? "as_const" : "angle_cast";
        else kind = "satisfies";
      }
      t.add("assertion", { fn: ctx.ownerOf(node), file, line: line(), kind, to_type: toType, from_any: isAny(node.expression), to_any: toAny });
    }

    const literal = (kind: string, value: string) =>
      t.add("literal", { fn: ctx.ownerOf(node), file, line: line(), kind, value: truncate(value, 100) });
    if ((ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) && !isNameLiteral(node)) {
      literal(ts.isStringLiteral(node) ? "string" : "template", node.text);
    } else if (ts.isNumericLiteral(node) && !isNameLiteral(node)) literal("number", node.text);
    else if (ts.isBigIntLiteral(node) && !inTypePosition(node)) literal("bigint", node.text);
    else if (ts.isRegularExpressionLiteral(node)) literal("regexp", node.text);
    else if (ts.isTemplateExpression(node)) literal("template", node.getText(sf).slice(1, -1));

    if (ts.isExpressionStatement(node)) {
      const e = skipWrappers(node.expression);
      if (ts.isCallExpression(e)) {
        const ty = typeOf(e);
        if (ty !== undefined && isThenable(checker, ty)) {
          t.add("floating_promise", { call_site: ctx.callSiteId(e), fn: ctx.ownerOf(node), file, line: line() });
        }
      }
    }

    if (ts.isThrowStatement(node)) {
      const e = skipWrappers(node.expression);
      const type = ts.isNewExpression(e) ? (ctx.idOfSymbol(checker.getSymbolAtLocation(skipWrappers(e.expression)), checker) ?? null) : null;
      t.add("throw_site", { fn: ctx.ownerOf(node), file, line: line(), type });
    }
    if (ts.isCatchClause(node)) {
      t.add("catch_site", {
        fn: ctx.ownerOf(node),
        file,
        line: line(),
        binds: node.variableDeclaration !== undefined,
        empty: node.block.statements.length === 0,
        rethrows: containsThrow(node.block),
      });
    }
    ts.forEachChild(node, visit);
  };
  ts.forEachChild(sf, visit);
}
