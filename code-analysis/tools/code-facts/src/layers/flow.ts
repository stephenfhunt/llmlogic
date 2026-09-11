// The flow layer: a statement-level control-flow graph per function body (and
// one per file for its top-level code), with def/use of variables at each node,
// the branch points that make up cyclomatic complexity, and per-function metrics.
//
// Exceptions are an over-approximation, and deliberately so: every node created
// inside a `try` gets a `throw` edge to its handler, since any expression can
// throw. Outside a `try`, only an explicit `throw` reaches `throw_exit`. A
// `finally` is entered by every jump that crosses it, and its end re-issues each
// of those jumps outward — so it has an edge to every place a jump through it
// was headed, not only to the one that actually entered it this time.
// test/properties/flow.test.ts checks the graph against real executions.

import ts from "typescript";
import { type Context, bodyOf, endLineOf, isFunctionLike, lineOf } from "../context.ts";
import type { SourceInfo } from "../program.ts";
import type { DecisionKind, FlowEdgeKind, FlowNodeKind, FnKind } from "../schema.ts";

interface Pending {
  from: number;
  kind: FlowEdgeKind;
}

type JumpKind = "break" | "continue" | "return" | "throw";

interface LoopFrame {
  kind: "loop";
  labels: string[];
  breaks: Pending[];
  continueTarget: number;
}
interface BreakFrame {
  kind: "switch" | "label";
  labels: string[];
  breaks: Pending[];
}
interface CatchFrame {
  kind: "catch";
  catchNode: number;
}
interface FinallyFrame {
  kind: "finally";
  finallyNode: number;
  routed: Map<string, { kind: JumpKind; label: string | undefined }>;
}
type Frame = LoopFrame | BreakFrame | CatchFrame | FinallyFrame;

const VAR_KINDS = new Set(["local", "parameter", "variable"]);
const FUNCTION_KINDS = new Set(["function", "method", "constructor", "getter", "setter", "static_block"]);
const NO_IMPLICIT_THROW = new Set<FlowNodeKind>(["entry", "exit", "throw_exit", "catch", "finally", "break", "continue"]);

function fnKindOf(node: ts.Node): FnKind {
  switch (node.kind) {
    case ts.SyntaxKind.SourceFile:
      return "module";
    case ts.SyntaxKind.FunctionDeclaration:
      return "function";
    case ts.SyntaxKind.MethodDeclaration:
      return "method";
    case ts.SyntaxKind.Constructor:
      return "constructor";
    case ts.SyntaxKind.GetAccessor:
      return "getter";
    case ts.SyntaxKind.SetAccessor:
      return "setter";
    case ts.SyntaxKind.ArrowFunction:
      return "arrow";
    case ts.SyntaxKind.ClassStaticBlockDeclaration:
      return "static_block";
    default:
      return "function_expression";
  }
}

/** Statements that do nothing at run time and get no node. */
function isInert(s: ts.Statement): boolean {
  if (ts.isEmptyStatement(s) || ts.isInterfaceDeclaration(s) || ts.isTypeAliasDeclaration(s)) return true;
  if (ts.isImportDeclaration(s) || ts.isImportEqualsDeclaration(s) || ts.isFunctionDeclaration(s)) return true;
  if (ts.isExportDeclaration(s)) return true;
  // `declare …` emits nothing.
  const mods = ts.canHaveModifiers(s) ? ts.getModifiers(s) : undefined;
  return mods?.some((m) => m.kind === ts.SyntaxKind.DeclareKeyword) ?? false;
}

function isAssignmentPatternTarget(node: ts.Node): boolean {
  let child: ts.Node = node;
  for (let p = node.parent; p !== undefined; child = p, p = p.parent) {
    if (ts.isArrayLiteralExpression(p) || ts.isObjectLiteralExpression(p) || ts.isSpreadElement(p) || ts.isParenthesizedExpression(p)) continue;
    if (ts.isShorthandPropertyAssignment(p) && p.name === child) continue;
    if (ts.isPropertyAssignment(p) && p.initializer === child) continue;
    if (ts.isSpreadAssignment(p)) continue;
    if (ts.isBinaryExpression(p)) return p.operatorToken.kind === ts.SyntaxKind.EqualsToken && p.left === child && child !== node;
    if (ts.isForOfStatement(p) || ts.isForInStatement(p)) return p.initializer === child;
    return false;
  }
  return false;
}

type Access = "def" | "use" | "both";

function accessOf(node: ts.Identifier): Access | undefined {
  const p = node.parent;
  if (ts.isVariableDeclaration(p) && p.name === node) {
    // `let x;` binds without defining: a read it reaches has no definition.
    // A catch binding and a for-of/for-in binding are defined by the construct.
    const bound = ts.isCatchClause(p.parent) || ts.isForOfStatement(p.parent.parent) || ts.isForInStatement(p.parent.parent);
    return p.initializer !== undefined || bound ? "def" : undefined;
  }
  if ((ts.isBindingElement(p) || ts.isParameter(p)) && p.name === node) return "def";
  let n: ts.Node = node;
  while (n.parent !== undefined && (ts.isParenthesizedExpression(n.parent) || ts.isNonNullExpression(n.parent))) n = n.parent;
  const q = n.parent;
  if (q !== undefined && ts.isBinaryExpression(q) && q.left === n) {
    const op = q.operatorToken.kind;
    if (op === ts.SyntaxKind.EqualsToken) return "def";
    if (op >= ts.SyntaxKind.FirstCompoundAssignment && op <= ts.SyntaxKind.LastCompoundAssignment) return "both";
  }
  if (q !== undefined && (ts.isPrefixUnaryExpression(q) || ts.isPostfixUnaryExpression(q)) &&
    (q.operator === ts.SyntaxKind.PlusPlusToken || q.operator === ts.SyntaxKind.MinusMinusToken)) {
    return "both";
  }
  if (q !== undefined && (ts.isForOfStatement(q) || ts.isForInStatement(q)) && q.initializer === n) return "def";
  if (isAssignmentPatternTarget(node)) return "def";
  return "use";
}

function logicalKind(op: ts.SyntaxKind): DecisionKind | undefined {
  switch (op) {
    case ts.SyntaxKind.AmpersandAmpersandToken:
      return "and";
    case ts.SyntaxKind.BarBarToken:
      return "or";
    case ts.SyntaxKind.QuestionQuestionToken:
      return "nullish";
    case ts.SyntaxKind.AmpersandAmpersandEqualsToken:
      return "and_assign";
    case ts.SyntaxKind.BarBarEqualsToken:
      return "or_assign";
    case ts.SyntaxKind.QuestionQuestionEqualsToken:
      return "nullish_assign";
    default:
      return undefined;
  }
}

function isOptionalLink(node: ts.Node): boolean {
  return (
    (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node) || ts.isCallExpression(node)) &&
    node.questionDotToken !== undefined
  );
}

class FnBuilder {
  private readonly ctx: Context;
  private readonly info: SourceInfo;
  private readonly fnNode: ts.Node;
  readonly fnId: string;
  private readonly frames: Frame[] = [];
  private readonly nodeIds = new Map<number, { kind: FlowNodeKind; ast: ts.Node }>();
  private readonly edges: [number, number, FlowEdgeKind][] = [];
  private readonly roots: [number, ts.Node[]][] = [];
  private nesting = 0;
  private readonly nestingOf = new Map<number, number>();
  readonly entry: number;
  readonly exit: number;
  readonly throwExit: number;
  private decisions = 0;

  constructor(ctx: Context, info: SourceInfo, fnNode: ts.Node) {
    this.ctx = ctx;
    this.info = info;
    this.fnNode = fnNode;
    this.fnId = ctx.idOfDecl(fnNode);
    this.entry = this.node("entry", fnNode, false);
    this.exit = this.node("exit", fnNode, false);
    this.throwExit = this.node("throw_exit", fnNode, false);
  }

  private node(kind: FlowNodeKind, ast: ts.Node, implicitThrows = true): number {
    const id = this.ctx.nextFlowNode++;
    this.nodeIds.set(id, { kind, ast });
    this.nestingOf.set(id, this.nesting);
    if (implicitThrows && !NO_IMPLICIT_THROW.has(kind)) this.implicitThrow(id);
    return id;
  }

  /** Joins pending edges to `to`. `relabel` renames plain fall-through only: a
   * branch edge keeps its kind, or two branches of one decision meeting at the
   * same node would collapse into one edge. */
  private connect(preds: Pending[], to: number, relabel?: FlowEdgeKind): void {
    for (const p of preds) this.edges.push([p.from, to, relabel !== undefined && p.kind === "next" ? relabel : p.kind]);
  }

  private evaluates(node: number, ...exprs: (ts.Node | undefined)[]): void {
    this.roots.push([node, exprs.filter((e): e is ts.Node => e !== undefined)]);
  }

  private decision(kind: DecisionKind, node: number | null, at: ts.Node, nesting: number): void {
    this.decisions++;
    this.ctx.tables.add("decision", {
      fn: this.fnId,
      node,
      kind,
      nesting,
      file: this.info.path,
      line: lineOf(at, this.info.sf),
    });
  }

  /** An exception raised at `node`: to the innermost handler, if there is one. */
  private implicitThrow(node: number): void {
    for (let i = this.frames.length - 1; i >= 0; i--) {
      const f = this.frames[i];
      if (f?.kind === "catch" || f?.kind === "finally") {
        this.jump([{ from: node, kind: "throw" }], "throw", undefined, this.frames.length);
        return;
      }
    }
  }

  /** Sends `preds` along a jump, resolving it from frame depth `depth` outward. */
  private jump(preds: Pending[], kind: JumpKind, label: string | undefined, depth: number): void {
    for (let i = depth - 1; i >= 0; i--) {
      const f = this.frames[i];
      if (f === undefined) continue;
      if (f.kind === "finally") {
        this.connect(preds, f.finallyNode, kind);
        f.routed.set(`${kind}:${label ?? ""}:${i}`, { kind, label });
        return;
      }
      if (kind === "throw" && f.kind === "catch") {
        this.connect(preds, f.catchNode, "throw");
        return;
      }
      if (kind === "break" && (f.kind === "loop" || f.kind === "switch" || f.kind === "label")) {
        if (label === undefined ? f.kind !== "label" : f.labels.includes(label)) {
          for (const p of preds) f.breaks.push({ from: p.from, kind: p.kind === "next" ? "break" : p.kind });
          return;
        }
      }
      if (kind === "continue" && f.kind === "loop" && (label === undefined || f.labels.includes(label))) {
        this.connect(preds, f.continueTarget, "continue");
        return;
      }
    }
    if (kind === "return") this.connect(preds, this.exit, "return");
    else if (kind === "throw") this.connect(preds, this.throwExit, "throw");
    // An unmatched break/continue is a syntax error the compiler reports; drop it.
  }

  buildBody(): void {
    const fn = this.fnNode;
    if (ts.isSourceFile(fn)) {
      const out = this.statements(fn.statements, [{ from: this.entry, kind: "next" }]);
      this.connect(out, this.exit, "next");
      return;
    }
    if (isFunctionLike(fn) && !ts.isClassStaticBlockDeclaration(fn)) this.evaluates(this.entry, ...fn.parameters);
    const body = bodyOf(fn);
    if (body === undefined) return;
    if (ts.isBlock(body)) {
      const out = this.statements(body.statements, [{ from: this.entry, kind: "next" }]);
      this.connect(out, this.exit, "next");
    } else {
      // An expression-bodied arrow evaluates and returns it.
      const n = this.node("return", body);
      this.evaluates(n, body);
      this.connect([{ from: this.entry, kind: "next" }], n);
      this.jump([{ from: n, kind: "return" }], "return", undefined, this.frames.length);
    }
  }

  private statements(list: readonly ts.Statement[], preds: Pending[]): Pending[] {
    let cur = preds;
    for (const s of list) cur = this.statement(s, cur, []);
    return cur;
  }

  private nested<T>(f: () => T): T {
    this.nesting++;
    try {
      return f();
    } finally {
      this.nesting--;
    }
  }

  private statement(s: ts.Statement, preds: Pending[], labels: string[]): Pending[] {
    if (isInert(s)) {
      if (ts.isFunctionDeclaration(s) && s.body !== undefined) this.evaluates(this.entry, s);
      return preds;
    }
    if (ts.isBlock(s)) {
      if (labels.length > 0) return this.labelled(labels, () => this.statements(s.statements, preds));
      return this.statements(s.statements, preds);
    }
    if (ts.isLabeledStatement(s)) return this.statement(s.statement, preds, [...labels, s.label.text]);
    if (ts.isIfStatement(s)) {
      const cond = this.node("cond", s.expression);
      this.evaluates(cond, s.expression);
      this.decision("if", cond, s, this.nesting);
      this.connect(preds, cond);
      const inner = (): Pending[] => {
        const thenOut = this.nested(() => this.statement(s.thenStatement, [{ from: cond, kind: "on_true" }], []));
        const elseOut =
          s.elseStatement !== undefined
            ? this.nested(() => this.statement(s.elseStatement as ts.Statement, [{ from: cond, kind: "on_false" }], []))
            : [{ from: cond, kind: "on_false" as const }];
        return [...thenOut, ...elseOut];
      };
      return labels.length > 0 ? this.labelled(labels, inner) : inner();
    }
    if (ts.isWhileStatement(s)) {
      const head = this.node("loop_head", s.expression);
      this.evaluates(head, s.expression);
      this.decision("while", head, s, this.nesting);
      this.connect(preds, head);
      return this.loopBody(s.statement, head, head, labels, [{ from: head, kind: "on_false" }]);
    }
    if (ts.isDoStatement(s)) {
      const head = this.node("loop_head", s, false);
      const cond = this.node("cond", s.expression);
      this.evaluates(cond, s.expression);
      this.decision("do", cond, s, this.nesting);
      this.connect(preds, head);
      const frame: LoopFrame = { kind: "loop", labels, breaks: [], continueTarget: cond };
      this.frames.push(frame);
      const out = this.nested(() => this.statement(s.statement, [{ from: head, kind: "next" }], []));
      this.frames.pop();
      this.connect(out, cond, "next");
      this.connect([{ from: cond, kind: "back" }], head);
      return [{ from: cond, kind: "on_false" }, ...frame.breaks];
    }
    if (ts.isForStatement(s)) {
      let cur = preds;
      if (s.initializer !== undefined) {
        const init = this.node("stmt", s.initializer);
        this.evaluates(init, s.initializer);
        this.connect(cur, init);
        cur = [{ from: init, kind: "next" }];
      }
      const head = this.node("loop_head", s.condition ?? s);
      this.evaluates(head, s.condition);
      if (s.condition !== undefined) this.decision("for", head, s, this.nesting);
      this.connect(cur, head);
      let incr: number | undefined;
      if (s.incrementor !== undefined) {
        incr = this.node("stmt", s.incrementor);
        this.evaluates(incr, s.incrementor);
      }
      const exits: Pending[] = s.condition !== undefined ? [{ from: head, kind: "on_false" }] : [];
      return this.loopBody(s.statement, head, incr ?? head, labels, exits, incr);
    }
    if (ts.isForOfStatement(s) || ts.isForInStatement(s)) {
      // The iterable is evaluated once; the head fetches each element and binds it.
      const iterable = this.node("stmt", s.expression);
      this.evaluates(iterable, s.expression);
      this.connect(preds, iterable);
      const head = this.node("loop_head", s.initializer);
      this.evaluates(head, s.initializer);
      this.decision(ts.isForOfStatement(s) ? "for_of" : "for_in", head, s, this.nesting);
      if (ts.isForOfStatement(s) && s.awaitModifier !== undefined) this.ctx.tables.add("await_at", { node: head });
      this.connect([{ from: iterable, kind: "next" }], head);
      return this.loopBody(s.statement, head, head, labels, [{ from: head, kind: "on_false" }]);
    }
    if (ts.isSwitchStatement(s)) return this.switchStatement(s, preds, labels);
    if (ts.isTryStatement(s)) {
      if (labels.length > 0) return this.labelled(labels, () => this.tryStatement(s, preds));
      return this.tryStatement(s, preds);
    }
    if (ts.isReturnStatement(s)) {
      const n = this.node("return", s);
      this.evaluates(n, s.expression);
      this.connect(preds, n);
      this.jump([{ from: n, kind: "return" }], "return", undefined, this.frames.length);
      return [];
    }
    if (ts.isThrowStatement(s)) {
      const n = this.node("throw", s);
      this.evaluates(n, s.expression);
      this.connect(preds, n);
      this.jump([{ from: n, kind: "throw" }], "throw", undefined, this.frames.length);
      return [];
    }
    if (ts.isBreakStatement(s) || ts.isContinueStatement(s)) {
      const kind = ts.isBreakStatement(s) ? "break" : "continue";
      const n = this.node(kind, s);
      this.connect(preds, n);
      this.jump([{ from: n, kind }], kind, s.label?.text, this.frames.length);
      return [];
    }
    // Everything else is one straight-line node.
    const n = this.node("stmt", s);
    if (ts.isClassDeclaration(s)) this.evaluates(n, ...(s.heritageClauses ?? []), ...(ts.getDecorators(s) ?? []));
    else if (ts.isVariableStatement(s)) this.evaluates(n, s.declarationList);
    else if (ts.isExpressionStatement(s)) this.evaluates(n, s.expression);
    else if (ts.isExportAssignment(s)) this.evaluates(n, s.expression);
    else if (!ts.isModuleDeclaration(s)) this.evaluates(n, s);
    this.connect(preds, n);
    const out: Pending[] = [{ from: n, kind: "next" }];
    return labels.length > 0 ? this.labelled(labels, () => out) : out;
  }

  private labelled(labels: string[], inner: () => Pending[]): Pending[] {
    const frame: BreakFrame = { kind: "label", labels, breaks: [] };
    this.frames.push(frame);
    const out = inner();
    this.frames.pop();
    return [...out, ...frame.breaks];
  }

  private loopBody(body: ts.Statement, head: number, continueTarget: number, labels: string[], exits: Pending[], incr?: number): Pending[] {
    const frame: LoopFrame = { kind: "loop", labels, breaks: [], continueTarget };
    this.frames.push(frame);
    const out = this.nested(() => this.statement(body, [{ from: head, kind: "on_true" }], []));
    this.frames.pop();
    if (incr !== undefined) {
      this.connect(out, incr, "next");
      this.connect([{ from: incr, kind: "back" }], head);
    } else {
      this.connect(out, head, "back");
    }
    return [...exits, ...frame.breaks];
  }

  private switchStatement(s: ts.SwitchStatement, preds: Pending[], labels: string[]): Pending[] {
    const sw = this.node("switch", s.expression);
    this.evaluates(sw, s.expression);
    this.connect(preds, sw);
    // The tests run in source order, skipping `default`; failing them all lands
    // on `default` (wherever it is written) or leaves the switch.
    const entries = new Map<ts.CaseOrDefaultClause, Pending[]>();
    let testPreds: Pending[] = [{ from: sw, kind: "next" }];
    for (const clause of s.caseBlock.clauses) {
      if (ts.isCaseClause(clause)) {
        const test = this.node("case_test", clause.expression);
        this.evaluates(test, clause.expression);
        this.decision("case", test, clause, this.nesting);
        this.connect(testPreds, test);
        testPreds = [{ from: test, kind: "on_false" }];
        entries.set(clause, [{ from: test, kind: "case" }]);
      }
    }
    const fallOut: Pending[] = [];
    const deflt = s.caseBlock.clauses.find(ts.isDefaultClause);
    if (deflt !== undefined) entries.set(deflt, testPreds.map((p) => ({ from: p.from, kind: "default" as const })));
    else fallOut.push(...testPreds);

    const frame: BreakFrame = { kind: "switch", labels, breaks: [] };
    this.frames.push(frame);
    let fall: Pending[] = [];
    this.nested(() => {
      for (const clause of s.caseBlock.clauses) {
        const into = [...(entries.get(clause) ?? []), ...fall];
        fall = this.statements(clause.statements, into);
      }
    });
    this.frames.pop();
    return [...fall, ...fallOut, ...frame.breaks];
  }

  private tryStatement(s: ts.TryStatement, preds: Pending[]): Pending[] {
    let fin: FinallyFrame | undefined;
    if (s.finallyBlock !== undefined) {
      fin = { kind: "finally", finallyNode: this.node("finally", s.finallyBlock, false), routed: new Map() };
    }
    const catchNode = s.catchClause !== undefined ? this.node("catch", s.catchClause, false) : undefined;
    if (catchNode !== undefined && s.catchClause !== undefined) {
      this.evaluates(catchNode, s.catchClause.variableDeclaration);
      this.decision("catch", catchNode, s.catchClause, this.nesting);
    }
    if (fin !== undefined) this.frames.push(fin);
    if (catchNode !== undefined) this.frames.push({ kind: "catch", catchNode });
    const tryOut = this.nested(() => this.statements(s.tryBlock.statements, preds));
    if (catchNode !== undefined) this.frames.pop();
    let out = tryOut;
    if (catchNode !== undefined && s.catchClause !== undefined) {
      const catchOut = this.nested(() => this.statements((s.catchClause as ts.CatchClause).block.statements, [{ from: catchNode, kind: "next" }]));
      out = [...tryOut, ...catchOut];
    }
    if (fin === undefined) return out;
    this.frames.pop();
    this.connect(out, fin.finallyNode, "next");
    const finOut = this.nested(() => this.statements((s.finallyBlock as ts.Block).statements, [{ from: fin.finallyNode, kind: "next" }]));
    // Re-issue every jump that entered the finally, from its end, outward.
    const depth = this.frames.length;
    for (const r of fin.routed.values()) this.jump(finOut, r.kind, r.label, depth);
    return out.length > 0 ? finOut : [];
  }

  /** Emits nodes, edges, and everything each node's expressions do. */
  flush(): void {
    const t = this.ctx.tables;
    const sf = this.info.sf;
    const file = this.info.path;
    for (const [id, { kind, ast }] of this.nodeIds) {
      const atEnd = kind === "exit" || kind === "throw_exit";
      const pos = atEnd ? ast.getEnd() : ast.getStart(sf);
      const lc = sf.getLineAndCharacterOfPosition(Math.max(0, atEnd ? pos - 1 : pos));
      t.add("flow_node", { id, fn: this.fnId, kind, file, line: lc.line + 1, col: lc.character + 1 });
    }
    for (const [from, to, kind] of this.edges) t.add("flow_edge", { from, to, kind });
    for (const [node, exprs] of this.roots) for (const e of exprs) this.walkExpression(node, e);
  }

  private walkExpression(node: number, root: ts.Node): void {
    const t = this.ctx.tables;
    const { checker } = this.info;
    const nesting = this.nestingOf.get(node) ?? 0;
    const visit = (n: ts.Node): void => {
      if (isFunctionLike(n) && n !== this.fnNode) {
        t.add("closure", { node, fn: this.ctx.idOfDecl(n) });
        return;
      }
      if (ts.isClassExpression(n)) {
        for (const h of n.heritageClauses ?? []) visit(h);
        return;
      }
      if (isCallLikeForFlow(n)) t.add("call_at", { node, call_site: this.ctx.callSiteId(n) });
      if (ts.isAwaitExpression(n)) t.add("await_at", { node });
      if (ts.isYieldExpression(n)) t.add("yield_at", { node });
      if (ts.isConditionalExpression(n)) this.decision("conditional", node, n, nesting);
      if (ts.isBinaryExpression(n)) {
        const k = logicalKind(n.operatorToken.kind);
        if (k !== undefined) this.decision(k, node, n, nesting);
      }
      if (isOptionalLink(n)) this.decision("optional_chain", node, n, nesting);
      if ((ts.isParameter(n) || ts.isBindingElement(n)) && n.initializer !== undefined) this.decision("default_value", node, n, nesting);
      if (ts.isIdentifier(n)) this.variableAccess(node, n, checker);
      ts.forEachChild(n, visit);
    };
    visit(root);
  }

  private variableAccess(node: number, id: ts.Identifier, checker: ts.TypeChecker): void {
    const p = id.parent;
    if (ts.isPropertyAccessExpression(p) && p.name === id) return;
    if (ts.isPropertyAssignment(p) && p.name === id) return;
    if (ts.isBindingElement(p) && p.propertyName === id) return;
    let sym: ts.Symbol | undefined;
    if (ts.isShorthandPropertyAssignment(p) && p.name === id && !isAssignmentPatternTarget(id)) {
      sym = checker.getShorthandAssignmentValueSymbol(p);
    } else {
      sym = checker.getSymbolAtLocation(id);
    }
    const varId = this.ctx.idOfSymbol(sym, checker);
    if (varId === undefined) return;
    const row = this.ctx.symbolRow(varId);
    if (row === undefined || !VAR_KINDS.has(row.kind)) return;
    const access = accessOf(id);
    if (access === undefined) return;
    const t = this.ctx.tables;
    if (access === "def" || access === "both") t.add("def", { node, var: varId });
    if (access === "use" || access === "both") t.add("use", { node, var: varId });
    // A variable declared in an enclosing function is captured by this one.
    const owner = row.parent;
    if (owner !== null && owner !== this.fnId && FUNCTION_KINDS.has(this.ctx.kindOfId(owner) ?? "")) {
      t.add("captures", { fn: this.fnId, var: varId });
    }
  }

  get decisionCount(): number {
    return this.decisions;
  }
}

function isCallLikeForFlow(n: ts.Node): boolean {
  return (
    ts.isCallExpression(n) ||
    ts.isNewExpression(n) ||
    ts.isTaggedTemplateExpression(n) ||
    ts.isJsxOpeningElement(n) ||
    ts.isJsxSelfClosingElement(n) ||
    (ts.isDecorator(n) && !ts.isCallExpression(n.expression))
  );
}

// ── per-function metrics ───────────────────────────────────────────────────

interface Metrics {
  statements: number;
  maxNesting: number;
  cognitive: number;
  returns: number;
  awaits: number;
  yields: number;
  throws: number;
  operators: number;
  operands: number;
  distinctOperators: Set<string>;
  distinctOperands: Set<string>;
}

function isNestingStructure(n: ts.Node): boolean {
  return (
    ts.isIfStatement(n) ||
    ts.isIterationStatement(n, false) ||
    ts.isSwitchStatement(n) ||
    ts.isTryStatement(n) ||
    ts.isCatchClause(n) ||
    ts.isConditionalExpression(n)
  );
}

function measure(fn: ts.Node, sf: ts.SourceFile): Metrics {
  const m: Metrics = {
    statements: 0,
    maxNesting: 0,
    cognitive: 0,
    returns: 0,
    awaits: 0,
    yields: 0,
    throws: 0,
    operators: 0,
    operands: 0,
    distinctOperators: new Set(),
    distinctOperands: new Set(),
  };
  const body = ts.isSourceFile(fn) ? fn : bodyOf(fn);
  if (body === undefined) return m;

  // Counts, nesting depth and cognitive complexity in one walk.
  const walk = (n: ts.Node, depth: number, cogNesting: number): void => {
    if (n !== fn && isFunctionLike(n)) return;
    if (ts.isStatement(n) && !ts.isBlock(n)) m.statements++;
    if (ts.isReturnStatement(n)) m.returns++;
    if (ts.isThrowStatement(n)) m.throws++;
    if (ts.isAwaitExpression(n)) m.awaits++;
    if (ts.isYieldExpression(n)) m.yields++;
    let d = depth;
    const elseIf = ts.isIfStatement(n) && n.parent !== undefined && ts.isIfStatement(n.parent) && n.parent.elseStatement === n;
    if (isNestingStructure(n) && !ts.isTryStatement(n) && !elseIf) {
      d = depth + 1;
      if (d > m.maxNesting) m.maxNesting = d;
    }
    // SonarSource cognitive complexity.
    let childNesting = cogNesting;
    if (ts.isIfStatement(n)) {
      const isElseIf = n.parent !== undefined && ts.isIfStatement(n.parent) && n.parent.elseStatement === n;
      m.cognitive += isElseIf ? 1 : 1 + cogNesting;
      if (n.elseStatement !== undefined && !ts.isIfStatement(n.elseStatement)) m.cognitive += 1;
      // An else-if is walked at its chain's level, so its body, like the if's
      // and the else's, is one deeper.
      const inner = cogNesting + 1;
      walk(n.expression, d, cogNesting);
      walk(n.thenStatement, d, inner);
      if (n.elseStatement !== undefined) walk(n.elseStatement, d, ts.isIfStatement(n.elseStatement) ? cogNesting : inner);
      return;
    }
    if (ts.isConditionalExpression(n) || ts.isSwitchStatement(n) || ts.isIterationStatement(n, false) || ts.isCatchClause(n)) {
      m.cognitive += 1 + cogNesting;
      childNesting = cogNesting + 1;
    }
    if ((ts.isBreakStatement(n) || ts.isContinueStatement(n)) && n.label !== undefined) m.cognitive += 1;
    if (ts.isBinaryExpression(n)) {
      const k = logicalKind(n.operatorToken.kind);
      if (k === "and" || k === "or" || k === "nullish") {
        let p = n.parent;
        while (p !== undefined && ts.isParenthesizedExpression(p)) p = p.parent;
        const sameSequence = p !== undefined && ts.isBinaryExpression(p) && p.operatorToken.kind === n.operatorToken.kind;
        if (!sameSequence) m.cognitive += 1;
      }
    }
    ts.forEachChild(n, (c) => walk(c, d, childNesting));
  };
  ts.forEachChild(body, (c) => walk(c, 0, 0));

  // Halstead: token leaves of the body, nested functions excluded.
  const tokens = (n: ts.Node): void => {
    if (n !== fn && n !== body && isFunctionLike(n)) return;
    const children = n.getChildren(sf);
    if (children.length === 0) {
      const text = n.getText(sf);
      if (text === "") return;
      const operand =
        ts.isIdentifier(n) ||
        ts.isPrivateIdentifier(n) ||
        ts.isLiteralExpression(n) ||
        ts.isNoSubstitutionTemplateLiteral(n) ||
        n.kind === ts.SyntaxKind.TrueKeyword ||
        n.kind === ts.SyntaxKind.FalseKeyword ||
        n.kind === ts.SyntaxKind.NullKeyword ||
        n.kind === ts.SyntaxKind.ThisKeyword ||
        n.kind === ts.SyntaxKind.TemplateHead ||
        n.kind === ts.SyntaxKind.TemplateMiddle ||
        n.kind === ts.SyntaxKind.TemplateTail;
      if (operand) {
        m.operands++;
        m.distinctOperands.add(text);
      } else if (n.kind !== ts.SyntaxKind.EndOfFileToken) {
        m.operators++;
        m.distinctOperators.add(text);
      }
      return;
    }
    for (const c of children) tokens(c);
  };
  tokens(body);
  return m;
}

export function extractFlow(ctx: Context): void {
  for (const info of ctx.loaded.sources) {
    if (info.sf.isDeclarationFile) continue;
    const fns: ts.Node[] = [info.sf];
    const find = (n: ts.Node): void => {
      if (isFunctionLike(n) && bodyOf(n) !== undefined) fns.push(n);
      ts.forEachChild(n, find);
    };
    ts.forEachChild(info.sf, find);
    for (const fn of fns) {
      const b = new FnBuilder(ctx, info, fn);
      b.buildBody();
      b.flush();
      const m = measure(fn, info.sf);
      const sf = info.sf;
      const line = ts.isSourceFile(fn) ? 1 : lineOf(fn, sf);
      const endLine = endLineOf(fn, sf);
      ctx.tables.add("fn", {
        id: b.fnId,
        kind: fnKindOf(fn),
        file: info.path,
        line,
        end_line: endLine,
        loc: endLine - line + 1,
        statements: m.statements,
        params: isFunctionLike(fn) && !ts.isClassStaticBlockDeclaration(fn) ? fn.parameters.length : 0,
        max_nesting: m.maxNesting,
        cyclomatic: 1 + b.decisionCount,
        cognitive: m.cognitive,
        returns: m.returns,
        awaits: m.awaits,
        yields: m.yields,
        throws: m.throws,
        halstead_operators: m.operators,
        halstead_operands: m.operands,
        halstead_distinct_operators: m.distinctOperators.size,
        halstead_distinct_operands: m.distinctOperands.size,
      });
    }
  }
}
