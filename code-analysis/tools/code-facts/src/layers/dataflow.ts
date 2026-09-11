// The dataflow layer: every function's expressions in three-address normal form,
// as Doop-style input facts — `assign`, `alloc`, `load`, `store`, and the
// formal/actual parameter and return slots that join functions at call sites.
// Flow-insensitive (statement order is the flow layer's business), field-based
// (a field is its name), context-insensitive. lib/pointsto.dl computes points-to
// and the call graph of indirect calls from these; lib/taint.dl follows values.
//
// Named variables are their symbol ids, so a value flows across modules through
// an import with no extra fact. Everything else — a subexpression's value, a
// function's `this` and return slot — is a temporary named after its function.
// Not modelled: accessor calls hidden in property reads, spread into objects,
// method values read off class instances (a class's methods are reached through
// `call_site` instead), generators' resumption values.

import ts from "typescript";
import { type Context, bodyOf, isFunctionLike, lineOf, nameText, skipWrappers } from "../context.ts";
import type { SourceInfo } from "../program.ts";

type VarKind = "local" | "param" | "module" | "temp" | "this" | "ret" | "catch" | "thrown";

const THROWN = "$thrown";
const VALUE_KINDS = new Set(["local", "parameter", "variable", "function", "class", "method", "property", "enum"]);
const DERIVING_OPERATORS = new Set([
  ts.SyntaxKind.PlusToken,
  ts.SyntaxKind.MinusToken,
  ts.SyntaxKind.AsteriskToken,
  ts.SyntaxKind.AsteriskAsteriskToken,
  ts.SyntaxKind.SlashToken,
  ts.SyntaxKind.PercentToken,
  ts.SyntaxKind.AmpersandToken,
  ts.SyntaxKind.BarToken,
  ts.SyntaxKind.CaretToken,
  ts.SyntaxKind.LessThanLessThanToken,
  ts.SyntaxKind.GreaterThanGreaterThanToken,
  ts.SyntaxKind.GreaterThanGreaterThanGreaterThanToken,
]);

function fieldName(name: ts.Node | undefined): string {
  if (name === undefined) return "[]";
  if (ts.isIdentifier(name) || ts.isPrivateIdentifier(name) || ts.isStringLiteral(name) || ts.isNoSubstitutionTemplateLiteral(name)) {
    return name.text;
  }
  if (ts.isNumericLiteral(name)) return "[]";
  if (ts.isComputedPropertyName(name)) {
    const e = name.expression;
    return ts.isStringLiteral(e) || ts.isNoSubstitutionTemplateLiteral(e) ? e.text : "[]";
  }
  return "[]";
}

function elementField(arg: ts.Expression): string {
  const a = skipWrappers(arg);
  return ts.isStringLiteral(a) || ts.isNoSubstitutionTemplateLiteral(a) ? a.text : "[]";
}

class Fn {
  private temps = 0;
  private readonly ctx: Context;
  private readonly info: SourceInfo;
  readonly id: string;
  readonly node: ts.Node;
  readonly thisVar: string;
  private retEmitted = false;

  constructor(ctx: Context, info: SourceInfo, node: ts.Node) {
    this.ctx = ctx;
    this.info = info;
    this.node = node;
    this.id = ctx.idOfDecl(node);
    this.thisVar = thisVarOf(ctx, node);
  }

  private get t() {
    return this.ctx.tables;
  }

  declareVar(id: string, kind: VarKind, fn: string = this.id): string {
    this.ctx.declareVar(id, fn, kind);
    return id;
  }

  temp(): string {
    return this.declareVar(`${this.id}$t${++this.temps}`, "temp");
  }

  private ret(): string {
    const r = `${this.id}$ret`;
    if (!this.retEmitted) {
      this.retEmitted = true;
      this.declareVar(r, "ret");
      this.t.add("formal_ret", { fn: this.id, var: r });
    }
    return r;
  }

  assign(to: string | undefined, from: string | undefined, kind: "copy" | "derive" = "copy"): void {
    if (to === undefined || from === undefined || to === from) return;
    this.t.add("assign", { to, from, kind, fn: this.id });
  }

  private alloc(at: ts.Node, kind: string, type: string | null, fnTarget: string | null, into?: string): string {
    const v = into ?? this.temp();
    const site = this.ctx.nextAllocSite++;
    this.t.add("alloc", { var: v, site, kind, type, fn_target: fnTarget, file: this.info.path, line: lineOf(at, this.info.sf) });
    return v;
  }

  /** The variable a named symbol's value lives in, if it holds values at all. */
  private named(node: ts.Node): string | undefined {
    const { checker } = this.info;
    let sym: ts.Symbol | undefined;
    const p = node.parent;
    if (p !== undefined && ts.isShorthandPropertyAssignment(p) && p.name === node) sym = checker.getShorthandAssignmentValueSymbol(p);
    else sym = checker.getSymbolAtLocation(node);
    const id = this.ctx.idOfSymbol(sym, checker);
    if (id === undefined) return undefined;
    const row = this.ctx.symbolRow(id);
    if (row === undefined || !VALUE_KINDS.has(row.kind)) return undefined;
    if (row.kind === "method" || row.kind === "property") return undefined; // members are fields, not variables
    this.ctx.declareVar(id, row.parent ?? this.id, varKindOf(this.ctx, id));
    return id;
  }

  // ── expressions ────────────────────────────────────────────────────────

  value(e: ts.Node | undefined): string | undefined {
    if (e === undefined) return undefined;
    if (ts.isParenthesizedExpression(e) || ts.isAsExpression(e) || ts.isSatisfiesExpression(e) ||
      ts.isTypeAssertionExpression(e) || ts.isNonNullExpression(e) || ts.isAwaitExpression(e)) {
      return this.value(e.expression);
    }
    if (ts.isIdentifier(e)) return this.named(e);
    if (e.kind === ts.SyntaxKind.ThisKeyword || e.kind === ts.SyntaxKind.SuperKeyword) return this.thisVar;
    if (isFunctionLike(e)) return this.alloc(e, "function", null, this.ctx.idOfDecl(e));
    if (ts.isClassExpression(e)) return this.alloc(e, "class", null, this.ctx.idOfDecl(e));
    if (ts.isObjectLiteralExpression(e)) return this.object(e);
    if (ts.isArrayLiteralExpression(e)) {
      const a = this.alloc(e, "array", null, null);
      for (const el of e.elements) {
        if (ts.isSpreadElement(el)) {
          const src = this.value(el.expression);
          if (src !== undefined) this.store(a, "[]", this.load(src, "[]"));
        } else this.store(a, "[]", this.value(el));
      }
      return a;
    }
    if (ts.isNewExpression(e)) return this.call(e);
    if (ts.isCallExpression(e) || ts.isTaggedTemplateExpression(e)) return this.call(e);
    if (ts.isJsxElement(e)) {
      const r = this.call(e.openingElement);
      for (const c of e.children) if (ts.isJsxExpression(c)) this.value(c.expression);
      return r;
    }
    if (ts.isJsxSelfClosingElement(e)) return this.call(e);
    if (ts.isJsxFragment(e)) {
      for (const c of e.children) if (ts.isJsxExpression(c)) this.value(c.expression);
      return undefined;
    }
    if (ts.isJsxExpression(e)) return this.value(e.expression);
    if (ts.isPropertyAccessExpression(e)) {
      const base = this.value(e.expression);
      return base !== undefined ? this.load(base, fieldName(e.name)) : undefined;
    }
    if (ts.isElementAccessExpression(e)) {
      const base = this.value(e.expression);
      this.value(e.argumentExpression);
      return base !== undefined ? this.load(base, elementField(e.argumentExpression)) : undefined;
    }
    if (ts.isBinaryExpression(e)) return this.binary(e);
    if (ts.isConditionalExpression(e)) {
      this.value(e.condition);
      const t = this.temp();
      this.assign(t, this.value(e.whenTrue));
      this.assign(t, this.value(e.whenFalse));
      return t;
    }
    if (ts.isTemplateExpression(e)) {
      const t = this.temp();
      for (const span of e.templateSpans) this.assign(t, this.value(span.expression), "derive");
      return t;
    }
    if (ts.isPrefixUnaryExpression(e) || ts.isPostfixUnaryExpression(e)) {
      const v = this.value(e.operand);
      if (e.operator === ts.SyntaxKind.ExclamationToken) return undefined;
      if (e.operator === ts.SyntaxKind.PlusPlusToken || e.operator === ts.SyntaxKind.MinusMinusToken) return v;
      const t = this.temp();
      this.assign(t, v, "derive");
      return t;
    }
    if (ts.isYieldExpression(e)) {
      this.value(e.expression);
      return undefined;
    }
    if (ts.isTypeOfExpression(e) || ts.isVoidExpression(e) || ts.isDeleteExpression(e)) {
      this.value(e.expression);
      return undefined;
    }
    if (ts.isSpreadElement(e)) return this.value(e.expression);
    return undefined; // literals and the like carry no tracked value
  }

  private load(base: string, field: string): string {
    const t = this.temp();
    this.t.add("load", { to: t, base, field, fn: this.id });
    return t;
  }

  private store(base: string | undefined, field: string, from: string | undefined): void {
    if (base === undefined || from === undefined) return;
    this.t.add("store", { base, field, from, fn: this.id });
  }

  private object(e: ts.ObjectLiteralExpression): string {
    // An object literal named by its binding (`const cfg = { … }`) is still an
    // allocation; its properties are fields of that allocation.
    const o = this.alloc(e, "object", null, null);
    for (const p of e.properties) {
      if (ts.isPropertyAssignment(p)) this.store(o, fieldName(p.name), this.value(p.initializer));
      else if (ts.isShorthandPropertyAssignment(p)) this.store(o, p.name.text, this.named(p.name));
      else if (ts.isMethodDeclaration(p) || ts.isGetAccessorDeclaration(p)) {
        if (ts.isMethodDeclaration(p)) this.store(o, fieldName(p.name), this.alloc(p, "function", null, this.ctx.idOfDecl(p)));
      } else if (ts.isSpreadAssignment(p)) this.value(p.expression);
    }
    return o;
  }

  private binary(e: ts.BinaryExpression): string | undefined {
    const op = e.operatorToken.kind;
    if (op === ts.SyntaxKind.EqualsToken) {
      const rhs = this.value(e.right);
      this.assignTo(e.left, rhs);
      return rhs;
    }
    if (op >= ts.SyntaxKind.FirstCompoundAssignment && op <= ts.SyntaxKind.LastCompoundAssignment) {
      const rhs = this.value(e.right);
      const logical =
        op === ts.SyntaxKind.AmpersandAmpersandEqualsToken || op === ts.SyntaxKind.BarBarEqualsToken || op === ts.SyntaxKind.QuestionQuestionEqualsToken;
      const left = skipWrappers(e.left);
      if (ts.isIdentifier(left)) {
        const v = this.named(left);
        this.assign(v, rhs, logical ? "copy" : "derive");
        return v;
      }
      const cur = this.value(e.left);
      const t = this.temp();
      this.assign(t, cur, logical ? "copy" : "derive");
      this.assign(t, rhs, logical ? "copy" : "derive");
      this.assignTo(e.left, t);
      return t;
    }
    if (op === ts.SyntaxKind.AmpersandAmpersandToken || op === ts.SyntaxKind.BarBarToken || op === ts.SyntaxKind.QuestionQuestionToken) {
      const t = this.temp();
      this.assign(t, this.value(e.left));
      this.assign(t, this.value(e.right));
      return t;
    }
    if (op === ts.SyntaxKind.CommaToken) {
      this.value(e.left);
      return this.value(e.right);
    }
    const l = this.value(e.left);
    const r = this.value(e.right);
    if (!DERIVING_OPERATORS.has(op)) return undefined; // comparisons, instanceof, in: booleans
    const t = this.temp();
    this.assign(t, l, "derive");
    this.assign(t, r, "derive");
    return t;
  }

  /** `target = value`, for any assignable target: a name, a property, an element, a pattern. */
  assignTo(target: ts.Node, value: string | undefined): void {
    const t = skipWrappers(target);
    if (ts.isIdentifier(t)) {
      this.assign(this.named(t), value);
    } else if (ts.isPropertyAccessExpression(t)) {
      this.store(this.value(t.expression), fieldName(t.name), value);
    } else if (ts.isElementAccessExpression(t)) {
      this.value(t.argumentExpression);
      this.store(this.value(t.expression), elementField(t.argumentExpression), value);
    } else if (ts.isObjectLiteralExpression(t)) {
      for (const p of t.properties) {
        if (ts.isShorthandPropertyAssignment(p)) this.assign(this.namedTarget(p.name), value !== undefined ? this.load(value, p.name.text) : undefined);
        else if (ts.isPropertyAssignment(p)) this.assignTo(p.initializer, value !== undefined ? this.load(value, fieldName(p.name)) : undefined);
        else if (ts.isSpreadAssignment(p)) this.assignTo(p.expression, value);
      }
    } else if (ts.isArrayLiteralExpression(t)) {
      for (const el of t.elements) {
        if (ts.isOmittedExpression(el)) continue;
        this.assignTo(ts.isSpreadElement(el) ? el.expression : el, value !== undefined ? this.load(value, "[]") : undefined);
      }
    } else if (ts.isBinaryExpression(t) && t.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
      // a default in a destructuring assignment: `[a = d] = xs`
      this.assignTo(t.left, value);
      this.assignTo(t.left, this.value(t.right));
    }
  }

  private namedTarget(id: ts.Identifier): string | undefined {
    const sym = this.info.checker.getShorthandAssignmentValueSymbol(id.parent);
    const vid = this.ctx.idOfSymbol(sym, this.info.checker);
    if (vid === undefined) return undefined;
    this.ctx.declareVar(vid, this.ctx.symbolRow(vid)?.parent ?? this.id, varKindOf(this.ctx, vid));
    return vid;
  }

  /** Binds a declaration's name (or pattern) to a value. */
  bind(name: ts.BindingName, value: string | undefined): void {
    if (ts.isIdentifier(name)) {
      this.assign(this.named(name), value);
      return;
    }
    for (const el of name.elements) {
      if (ts.isOmittedExpression(el)) continue;
      let v: string | undefined;
      if (value !== undefined) {
        if (el.dotDotDotToken !== undefined) v = value;
        else v = this.load(value, ts.isObjectBindingPattern(name) ? fieldName(el.propertyName ?? el.name) : "[]");
      }
      if (el.initializer !== undefined) {
        const d = this.value(el.initializer);
        if (ts.isIdentifier(el.name)) this.assign(this.named(el.name), d);
        else this.bind(el.name, d);
      }
      this.bind(el.name, v);
    }
  }

  private call(e: ts.CallExpression | ts.NewExpression | ts.TaggedTemplateExpression | ts.JsxOpeningLikeElement): string | undefined {
    const cs = this.ctx.callSiteId(e);
    const t = this.t;
    let calleeExpr: ts.Expression | undefined;
    if (ts.isCallExpression(e) || ts.isNewExpression(e)) calleeExpr = e.expression;
    else if (ts.isTaggedTemplateExpression(e)) calleeExpr = e.tag;

    // The callee value, and the receiver of a method call.
    if (calleeExpr !== undefined && calleeExpr.kind !== ts.SyntaxKind.ImportKeyword) {
      const c = skipWrappers(calleeExpr);
      if ((ts.isPropertyAccessExpression(c) || ts.isElementAccessExpression(c)) && !ts.isNewExpression(e)) {
        const recv = this.value(c.expression);
        if (recv !== undefined) {
          t.add("receiver", { call_site: cs, var: recv });
          const field = ts.isPropertyAccessExpression(c) ? fieldName(c.name) : elementField(c.argumentExpression);
          t.add("callee_var", { call_site: cs, var: this.load(recv, field) });
        }
      } else if (c.kind === ts.SyntaxKind.SuperKeyword) {
        t.add("receiver", { call_site: cs, var: this.thisVar });
      } else {
        const cv = this.value(c);
        if (cv !== undefined) t.add("callee_var", { call_site: cs, var: cv });
      }
    }

    let args: readonly ts.Expression[] = [];
    if (ts.isCallExpression(e)) args = e.arguments;
    else if (ts.isNewExpression(e)) args = e.arguments ?? [];
    else if (ts.isTaggedTemplateExpression(e)) {
      args = ts.isTemplateExpression(e.template) ? e.template.templateSpans.map((s) => s.expression) : [];
    }
    if (ts.isJsxOpeningElement(e) || ts.isJsxSelfClosingElement(e)) {
      // Props are one object argument.
      const props = this.alloc(e, "object", null, null);
      for (const a of e.attributes.properties) {
        if (ts.isJsxAttribute(a)) {
          const init = a.initializer;
          const v = init !== undefined && ts.isJsxExpression(init) ? this.value(init.expression) : undefined;
          this.store(props, nameText(a.name), v);
        } else this.value(a.expression);
      }
      t.add("actual", { call_site: cs, index: 0, var: props });
    }
    args.forEach((a, i) => {
      if (ts.isSpreadElement(a)) {
        const src = this.value(a.expression);
        if (src !== undefined) t.add("actual", { call_site: cs, index: i, var: this.load(src, "[]") });
        return;
      }
      const v = this.value(a);
      if (v !== undefined) t.add("actual", { call_site: cs, index: i, var: v });
    });

    if (ts.isNewExpression(e)) {
      const sym = this.info.checker.getSymbolAtLocation(skipWrappers(e.expression));
      const cls = this.ctx.idOfSymbol(sym, this.info.checker) ?? null;
      const o = this.alloc(e, "instance", cls, null);
      t.add("receiver", { call_site: cs, var: o });
      return o;
    }
    const r = this.temp();
    t.add("actual_ret", { call_site: cs, var: r });
    return r;
  }

  // ── statements ─────────────────────────────────────────────────────────

  statement(s: ts.Node): void {
    if (ts.isVariableStatement(s)) this.declarations(s.declarationList);
    else if (ts.isExpressionStatement(s)) this.value(s.expression);
    else if (ts.isReturnStatement(s)) {
      if (s.expression !== undefined) this.assign(this.ret(), this.value(s.expression));
    } else if (ts.isThrowStatement(s)) {
      this.declareVar(THROWN, "thrown", this.id);
      this.assign(THROWN, this.value(s.expression));
    } else if (ts.isIfStatement(s)) {
      this.value(s.expression);
      this.statement(s.thenStatement);
      if (s.elseStatement !== undefined) this.statement(s.elseStatement);
    } else if (ts.isBlock(s)) for (const x of s.statements) this.statement(x);
    else if (ts.isWhileStatement(s) || ts.isDoStatement(s)) {
      this.value(s.expression);
      this.statement(s.statement);
    } else if (ts.isForStatement(s)) {
      if (s.initializer !== undefined) {
        if (ts.isVariableDeclarationList(s.initializer)) this.declarations(s.initializer);
        else this.value(s.initializer);
      }
      this.value(s.condition);
      this.value(s.incrementor);
      this.statement(s.statement);
    } else if (ts.isForOfStatement(s) || ts.isForInStatement(s)) {
      const iterable = this.value(s.expression);
      const element = ts.isForOfStatement(s) && iterable !== undefined ? this.load(iterable, "[]") : undefined;
      if (ts.isVariableDeclarationList(s.initializer)) {
        for (const d of s.initializer.declarations) this.bind(d.name, element);
      } else this.assignTo(s.initializer, element);
      this.statement(s.statement);
    } else if (ts.isSwitchStatement(s)) {
      this.value(s.expression);
      for (const c of s.caseBlock.clauses) {
        if (ts.isCaseClause(c)) this.value(c.expression);
        for (const x of c.statements) this.statement(x);
      }
    } else if (ts.isTryStatement(s)) {
      this.statement(s.tryBlock);
      const cc = s.catchClause;
      if (cc !== undefined) {
        const decl = cc.variableDeclaration;
        if (decl !== undefined) {
          this.declareVar(THROWN, "thrown", this.id);
          this.bind(decl.name, THROWN);
        }
        this.statement(cc.block);
      }
      if (s.finallyBlock !== undefined) this.statement(s.finallyBlock);
    } else if (ts.isLabeledStatement(s)) this.statement(s.statement);
    else if (ts.isFunctionDeclaration(s)) {
      if (s.body !== undefined) {
        const id = this.ctx.idOfDecl(s);
        this.alloc(s, "function", null, id, this.declareVar(id, varKindOf(this.ctx, id), this.ctx.symbolRow(id)?.parent ?? this.id));
      }
    } else if (ts.isClassDeclaration(s)) this.classDeclaration(s);
    else if (ts.isExportAssignment(s)) {
      const id = this.ctx.idOfDecl(s);
      this.assign(this.declareVar(id, "module"), this.value(s.expression));
    }
  }

  private declarations(list: ts.VariableDeclarationList): void {
    for (const d of list.declarations) {
      if (d.initializer === undefined) continue;
      this.bind(d.name, this.value(d.initializer));
    }
  }

  private classDeclaration(c: ts.ClassDeclaration): void {
    if (ts.getModifiers(c)?.some((m) => m.kind === ts.SyntaxKind.DeclareKeyword) === true) return;
    const id = this.ctx.idOfDecl(c);
    this.alloc(c, "class", null, id, this.declareVar(id, varKindOf(this.ctx, id), this.ctx.symbolRow(id)?.parent ?? this.id));
    for (const h of c.heritageClauses ?? []) for (const ty of h.types) this.value(ty.expression);
    this.classFields(c, id);
  }

  classFields(c: ts.ClassLikeDeclaration, id: string): void {
    // Field initializers run with `this` the new instance (or the class, for statics).
    const thisVar = `${id}$this`;
    for (const m of c.members) {
      if (!ts.isPropertyDeclaration(m) || m.initializer === undefined) continue;
      const v = this.value(m.initializer);
      const isStatic = ts.getModifiers(m)?.some((x) => x.kind === ts.SyntaxKind.StaticKeyword) === true;
      if (!isStatic) this.ctx.declareVar(thisVar, id, "this");
      this.store(isStatic ? id : thisVar, fieldName(m.name), v);
    }
  }

  /** A function's own entry: parameters and their defaults. */
  entry(): void {
    const n = this.node;
    if (!isFunctionLike(n) || ts.isClassStaticBlockDeclaration(n)) return;
    n.parameters.forEach((p, i) => {
      let v: string | undefined;
      if (ts.isIdentifier(p.name)) v = this.named(p.name);
      else v = this.temp();
      if (v === undefined) return;
      this.t.add("formal", { fn: this.id, index: i, var: v });
      if (!ts.isIdentifier(p.name)) this.bind(p.name, v);
      if (p.initializer !== undefined) this.assign(v, this.value(p.initializer));
      // A parameter property is also a field of the instance.
      if (ts.isParameterPropertyDeclaration(p, p.parent)) this.store(this.thisVar, ts.isIdentifier(p.name) ? p.name.text : "[]", v);
    });
    // An arrow's `this` is lexical: a receiver never binds it.
    if (this.thisVar.endsWith("$this") && !ts.isArrowFunction(n)) this.t.add("this_var", { fn: this.id, var: this.thisVar });
  }

  body(): void {
    const n = this.node;
    if (ts.isSourceFile(n)) {
      for (const s of n.statements) this.statement(s);
      return;
    }
    const b = bodyOf(n);
    if (b === undefined) return;
    if (ts.isBlock(b)) for (const s of b.statements) this.statement(s);
    else this.assign(this.ret(), this.value(b));
  }
}

function varKindOf(ctx: Context, id: string): VarKind {
  const row = ctx.symbolRow(id);
  if (row === undefined) return "module";
  if (row.kind === "parameter") return "param";
  if (row.kind === "local") return "local";
  const parentKind = row.parent !== null ? ctx.kindOfId(row.parent) : undefined;
  return parentKind === "module" || parentKind === "namespace" || parentKind === undefined ? "module" : "local";
}

/** The variable `this` lives in, inside `fn`: shared by a class's instance
 * members, lexical for arrows, per-function otherwise. */
function thisVarOf(ctx: Context, fn: ts.Node): string {
  let n: ts.Node = fn;
  while (ts.isArrowFunction(n)) {
    const outer = ctx.executorOf(n);
    if (outer === n) break;
    n = outer;
  }
  const parent = n.parent;
  if (parent !== undefined && ts.isClassLike(parent) && !ts.isClassStaticBlockDeclaration(n)) {
    const isStatic = ts.canHaveModifiers(n) && ts.getModifiers(n)?.some((m) => m.kind === ts.SyntaxKind.StaticKeyword) === true;
    const cls = ctx.idOfDecl(parent);
    if (isStatic) return cls;
    ctx.declareVar(`${cls}$this`, cls, "this");
    return `${cls}$this`;
  }
  const id = ctx.idOfDecl(n);
  ctx.declareVar(`${id}$this`, id, "this");
  return `${id}$this`;
}

export function extractDataflow(ctx: Context): void {
  for (const info of ctx.loaded.sources) {
    if (info.sf.isDeclarationFile) continue;
    const fns: ts.Node[] = [info.sf];
    const find = (n: ts.Node): void => {
      if (isFunctionLike(n) && bodyOf(n) !== undefined) fns.push(n);
      ts.forEachChild(n, find);
    };
    ts.forEachChild(info.sf, find);
    for (const node of fns) {
      const f = new Fn(ctx, info, node);
      f.entry();
      f.body();
    }
  }
  ctx.flushVars();
}
