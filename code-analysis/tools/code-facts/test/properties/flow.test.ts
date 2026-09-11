// P1 — CFG soundness against execution. An independent oracle: Node runs the
// generated function, and every step of the real execution trace must be a path
// in the extracted control-flow graph.
//
// Programs are built from probes — `s(k)` a statement, `c(k)` a condition, `t(k)`
// a statement that may throw, `v(k)` a switch discriminant, `a(k)` a for-of
// iterable — each recording `k` when evaluated, each in its own flow node. For
// consecutive probes j, k of a trace there must be a path node(j) → node(k)
// through nodes that hold no probe (merges, loop heads, catch and finally
// entries, jumps). The inputs drive the branches.
//
// P3 — two spellings of cyclomatic complexity agree. On programs without
// exceptions, short-circuits or defaults, `fn.cyclomatic` (1 + decisions) equals
// E − N + 2 over the function's graph (N without `throw_exit`).

import assert from "node:assert/strict";
import * as fs from "node:fs";
import { test } from "node:test";
import fc from "fast-check";
import { extract, tempDir, writeProject } from "../helpers.ts";

// P1's non-vacuity guard needs rare shapes (a break taken between two probes, a
// caught exception) to occur somewhere in the sample: at 40 programs it missed
// one about a run in four, at 200 it has not missed in 40 runs.
const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "200", 10);

type Stmt =
  | { t: "probe" }
  | { t: "throw" }
  | { t: "return" }
  | { t: "break"; which: number }
  | { t: "continue"; which: number }
  | { t: "if"; then: Stmt[]; else: Stmt[] | null }
  | { t: "while" | "do" | "for" | "forof"; body: Stmt[]; label: boolean }
  | { t: "switch"; cases: { body: Stmt[]; brk: boolean }[]; hasDefault: boolean }
  | { t: "block"; body: Stmt[] }
  | { t: "try"; body: Stmt[]; catch: Stmt[] | null; fin: Stmt[] | null };

function programArb(withExceptions: boolean): fc.Arbitrary<Stmt[]> {
  const { stmts } = fc.letrec<{ stmt: Stmt; stmts: Stmt[] }>((tie) => {
    const leaves: fc.Arbitrary<Stmt>[] = [
      fc.constant({ t: "probe" as const }),
      fc.constant({ t: "return" as const }),
      fc.record({ t: fc.constant("break" as const), which: fc.nat(5) }),
      fc.record({ t: fc.constant("continue" as const), which: fc.nat(5) }),
    ];
    // Exceptions weighted up: a caught one needs a `t(k)` that throws inside a
    // `try` with a probe after it, and the guard below insists on seeing one.
    if (withExceptions) leaves.push(fc.constant({ t: "throw" as const }), fc.constant({ t: "throw" as const }));
    const compound: fc.Arbitrary<Stmt>[] = [
      fc.record({ t: fc.constant("if" as const), then: tie("stmts"), else: fc.option(tie("stmts") as fc.Arbitrary<Stmt[]>, { nil: null }) }),
      fc.record({ t: fc.constantFrom("while" as const, "do" as const, "for" as const, "forof" as const), body: tie("stmts"), label: fc.boolean() }),
      fc.record({
        t: fc.constant("switch" as const),
        cases: fc.array(fc.record({ body: tie("stmts"), brk: fc.boolean() }), { maxLength: 3 }),
        hasDefault: fc.boolean(),
      }),
      fc.record({ t: fc.constant("block" as const), body: tie("stmts") }),
    ];
    if (withExceptions) {
      const tryArb = fc.record({
        t: fc.constant("try" as const),
        body: tie("stmts"),
        catch: fc.option(tie("stmts") as fc.Arbitrary<Stmt[]>, { nil: null }),
        fin: fc.option(tie("stmts") as fc.Arbitrary<Stmt[]>, { nil: null }),
      });
      compound.push(tryArb, tryArb);
    }
    return {
      stmt: fc.oneof({ maxDepth: 4, depthSize: "small" }, ...leaves, ...compound) as fc.Arbitrary<Stmt>,
      stmts: fc.array(tie("stmt") as fc.Arbitrary<Stmt>, { maxLength: 4 }),
    };
  });
  return stmts;
}

interface Scope {
  loops: (string | null)[]; // labels of enclosing loops (null: unlabelled)
  labels: string[]; // every label in scope (loops and blocks)
  breakable: boolean; // inside a loop or switch
}

/** Renders statements; probe numbers are allocated in order through `next`. */
function render(stmts: Stmt[], scope: Scope, next: () => number, labelNo: { n: number }): string[] {
  const out: string[] = [];
  const probe = () => `s(${next()});`;
  for (const st of stmts) {
    switch (st.t) {
      case "probe":
        out.push(probe());
        break;
      case "throw":
        out.push(`t(${next()});`);
        break;
      case "return":
        out.push("return;");
        break;
      case "break": {
        if (st.which % 2 === 0 && scope.breakable) out.push("break;");
        else if (scope.labels.length > 0) out.push(`break ${scope.labels[st.which % scope.labels.length]};`);
        else out.push(probe());
        break;
      }
      case "continue": {
        const labelled = scope.loops.filter((l): l is string => l !== null);
        if (st.which % 2 === 0 && scope.loops.length > 0) out.push("continue;");
        else if (labelled.length > 0) out.push(`continue ${labelled[st.which % labelled.length]};`);
        else out.push(probe());
        break;
      }
      case "if": {
        out.push(`if (c(${next()})) {`, ...render(st.then, scope, next, labelNo), "}");
        if (st.else !== null) out.push("else {", ...render(st.else, scope, next, labelNo), "}");
        break;
      }
      case "while":
      case "do":
      case "for":
      case "forof": {
        const label = st.label ? `L${labelNo.n++}` : null;
        const inner: Scope = { loops: [...scope.loops, label], labels: label !== null ? [...scope.labels, label] : scope.labels, breakable: true };
        const prefix = label !== null ? `${label}: ` : "";
        if (st.t === "while") out.push(`${prefix}while (c(${next()})) {`, ...render(st.body, inner, next, labelNo), "}");
        else if (st.t === "do") {
          out.push(`${prefix}do {`, ...render(st.body, inner, next, labelNo));
          out.push(`} while (c(${next()}));`);
        } else if (st.t === "for") {
          const init = next();
          const cond = next();
          const incr = next();
          out.push(`${prefix}for (s(${init}); c(${cond}); s(${incr})) {`, ...render(st.body, inner, next, labelNo), "}");
        } else {
          const k = next();
          out.push(`${prefix}for (const x${k} of a(${k})) {`, ...render(st.body, inner, next, labelNo), "}");
        }
        break;
      }
      case "switch": {
        out.push(`switch (v(${next()})) {`);
        const inner: Scope = { ...scope, breakable: true };
        st.cases.forEach((cs, i) => {
          out.push(`case ${i}: {`, ...render(cs.body, inner, next, labelNo), "}");
          if (cs.brk) out.push("break;");
        });
        if (st.hasDefault) out.push("default: {", probe(), "}");
        out.push("}");
        break;
      }
      case "block": {
        const label = `B${labelNo.n++}`;
        out.push(`${label}: {`, ...render(st.body, { ...scope, labels: [...scope.labels, label] }, next, labelNo), "}");
        break;
      }
      case "try": {
        out.push("try {", ...render(st.body, scope, next, labelNo), "}");
        const handler = st.catch ?? (st.fin === null ? [] : null);
        if (handler !== null) out.push("catch {", ...render(handler, scope, next, labelNo), "}");
        if (st.fin !== null) out.push("finally {", ...render(st.fin, scope, next, labelNo), "}");
        break;
      }
    }
  }
  return out;
}

const HEADER =
  "export function run(s: (k: number) => void, c: (k: number) => boolean, t: (k: number) => void, v: (k: number) => number, a: (k: number) => number[]): void {";

class Budget extends Error {}

/** Runs the body with probes driven by `inputs`; returns the trace and how it ended. */
function execute(body: string, inputs: number[]): { trace: number[]; outcome: "return" | "throw" } {
  const trace: number[] = [];
  let i = 0;
  const next = () => inputs[i++ % inputs.length] ?? 0;
  const s = (k: number) => {
    trace.push(k);
    if (trace.length > 2000) throw new Budget();
  };
  const c = (k: number) => {
    s(k);
    return trace.length < 200 && next() % 2 === 1;
  };
  const t = (k: number) => {
    s(k);
    if (next() % 3 === 0) throw new Error("t");
  };
  const v = (k: number) => {
    s(k);
    return next() % 4;
  };
  const a = (k: number) => {
    s(k);
    return trace.length < 200 ? [1, 2].slice(0, next() % 3) : [];
  };
  const fn = new Function("s", "c", "t", "v", "a", body) as (...args: unknown[]) => void;
  try {
    fn(s, c, t, v, a);
    return { trace, outcome: "return" };
  } catch {
    return { trace, outcome: "throw" };
  }
}

interface Graph {
  succ: Map<number, { to: number; kind: string }[]>;
  probeNode: Map<number, number>; // probe k → node
  probeNodes: Set<number>;
  entry: number;
  exit: number;
  kinds: Map<number, string>;
}

function graphOf(dir: string, source: string): { graph: Graph; cyclomatic: number; e: number; n: number; decisions: string[] } {
  const { tables } = extract(dir, { layers: ["refs", "flow"] });
  const fnId = "src/p.ts#run";
  const nodes = tables.rows("flow_node").filter((r) => r.fn === fnId);
  const ids = new Set(nodes.map((r) => r.id as number));
  const kinds = new Map(nodes.map((r) => [r.id as number, r.kind as string]));
  const succ = new Map<number, { to: number; kind: string }[]>();
  let e = 0;
  for (const r of tables.rows("flow_edge")) {
    if (!ids.has(r.from as number)) continue;
    e++;
    const list = succ.get(r.from as number) ?? [];
    list.push({ to: r.to as number, kind: r.kind as string });
    succ.set(r.from as number, list);
  }
  const lineStarts = [0];
  for (let i = 0; i < source.length; i++) if (source[i] === "\n") lineStarts.push(i + 1);
  const nodeOfCall = new Map(tables.rows("call_at").map((r) => [r.call_site as number, r.node as number]));
  const probeNode = new Map<number, number>();
  for (const cs of tables.rows("call_site")) {
    if (cs.caller !== fnId) continue;
    const at = (lineStarts[(cs.line as number) - 1] ?? 0) + (cs.col as number) - 1;
    const m = /^[sctva]\((\d+)\)/.exec(source.slice(at));
    const node = nodeOfCall.get(cs.id as number);
    if (m !== null && node !== undefined) probeNode.set(Number(m[1]), node);
  }
  const entry = nodes.find((r) => r.kind === "entry")?.id as number;
  const exit = nodes.find((r) => r.kind === "exit")?.id as number;
  const fn = tables.rows("fn").find((r) => r.id === fnId);
  const n = nodes.filter((r) => r.kind !== "throw_exit").length;
  const decisions = tables.rows("decision").filter((d) => d.fn === fnId).map((d) => d.kind as string);
  return {
    graph: { succ, probeNode, probeNodes: new Set(probeNode.values()), entry, exit, kinds },
    cyclomatic: fn?.cyclomatic as number,
    e,
    n,
    decisions,
  };
}

/** A path from → to through probe-free nodes; returns the edge kinds used, or undefined. */
function pathBetween(g: Graph, from: number, to: number): string[] | undefined {
  const seen = new Set<number>([from]);
  const queue: [number, string[]][] = [[from, []]];
  while (queue.length > 0) {
    const [at, kinds] = queue.shift() as [number, string[]];
    for (const { to: nxt, kind } of g.succ.get(at) ?? []) {
      if (nxt === to) return [...kinds, kind, g.kinds.get(nxt) ?? ""];
      if (seen.has(nxt) || g.probeNodes.has(nxt)) continue;
      seen.add(nxt);
      queue.push([nxt, [...kinds, kind, g.kinds.get(nxt) ?? ""]]);
    }
  }
  return undefined;
}

function projectFor(stmts: Stmt[]): { dir: string; source: string; body: string } {
  let k = 1;
  const body = render(stmts, { loops: [], labels: [], breakable: false }, () => k++, { n: 0 }).join("\n");
  const source = `${HEADER}\n${body}\n}\n`;
  const dir = tempDir("flow");
  writeProject(dir, { "src/p.ts": source }, {
    compilerOptions: { strict: false, noEmit: true, noLib: true, types: [], allowUnreachableCode: true, allowUnusedLabels: true },
    files: ["src/p.ts"],
  });
  return { dir, source, body };
}

test("P1: every step of a real execution is a path in the control-flow graph", () => {
  const seen = new Map<string, number>();
  let steps = 0;
  fc.assert(
    fc.property(programArb(true), fc.array(fc.array(fc.nat(9), { minLength: 1, maxLength: 24 }), { minLength: 4, maxLength: 4 }), (stmts, inputSets) => {
      const { dir, source, body } = projectFor(stmts);
      const { graph } = graphOf(dir, source);
      for (const inputs of inputSets) {
        const { trace, outcome } = execute(body, inputs);
        let at = graph.entry;
        for (const k of trace) {
          const node = graph.probeNode.get(k);
          assert.ok(node !== undefined, `probe ${k} has no flow node`);
          const path = pathBetween(graph, at, node);
          assert.ok(path !== undefined, `no path from node ${at} to probe ${k} (node ${node})\ninputs ${inputs}\ntrace ${trace}\n${source}`);
          for (const kind of path) seen.set(kind, (seen.get(kind) ?? 0) + 1);
          steps++;
          at = node;
        }
        if (outcome === "return") {
          assert.ok(pathBetween(graph, at, graph.exit) !== undefined, `no path from node ${at} to exit\ntrace ${trace}\n${source}`);
        }
      }
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  // Non-vacuity against the sentence: real executions crossed back edges,
  // breaks, continues, a caught exception, and a finally.
  for (const kind of ["back", "break", "continue", "throw", "finally", "catch", "case", "default"]) {
    assert.ok((seen.get(kind) ?? 0) > 0, `no executed step used ${kind}; saw ${[...seen.keys()]}`);
  }
  assert.ok(steps > RUNS * 4, `only ${steps} steps checked`);
});

test("P3: cyclomatic complexity from decisions equals E − N + 2 over the graph", () => {
  const kinds = new Set<string>();
  fc.assert(
    fc.property(programArb(false), (stmts) => {
      const { dir, source } = projectFor(stmts);
      const { cyclomatic, e, n, decisions } = graphOf(dir, source);
      assert.equal(cyclomatic, e - n + 2, `decisions ${decisions}\n${source}`);
      for (const d of decisions) kinds.add(d);
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  for (const k of ["if", "while", "do", "for", "for_of", "case"]) assert.ok(kinds.has(k), `no program had a ${k} decision`);
});
