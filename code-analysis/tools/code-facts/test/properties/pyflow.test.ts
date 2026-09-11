// P1-py — CFG soundness against execution, for Python. An independent oracle:
// python3 runs the generated function, and every step of the real trace must
// be a path in the extracted control-flow graph.
//
// Probes as in P1: `s(k)` a statement, `c(k)` a condition (also an `assert`'s
// and a `case` guard's), `t(k)` a statement that may raise ValueError or
// KeyError, `v(k)` a `match` subject, `a(k)` a `for` iterable, `w(k)` a context
// manager whose `__exit__` suppresses the exception on some inputs. For
// consecutive probes j, k of a trace there must be a path node(j) → node(k)
// through nodes holding no probe. Handlers are typed (ValueError, KeyError) or
// bare, so an exception can pass a clause that does not match it; loops and
// `try` have `else:` clauses.
//
// P3-py — two spellings of cyclomatic complexity agree: on programs without
// exceptions, `fn.cyclomatic` (1 + decisions) equals E − N + 2 over the
// function's graph (N without `throw_exit`).

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { extractPython, tempDir } from "../helpers.ts";

// 100 runs of three programs: the rarest guard (an executed `continue`) fired in
// 35 of 200 runs, so a suite misses it about once in 10^8.
const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "100", 10);
const PYTHON = process.env.CODE_FACTS_PYTHON ?? "python3";

type Handler = { type: "ValueError" | "KeyError" | null; lead: boolean; body: Stmt[] };
type Stmt =
  | { t: "probe" }
  | { t: "throw" }
  | { t: "raise"; key: boolean }
  | { t: "assert" }
  | { t: "return" }
  | { t: "break"; guarded: boolean }
  | { t: "continue"; guarded: boolean }
  | { t: "if"; then: Stmt[]; elifs: Stmt[][]; else: Stmt[] | null }
  | { t: "while" | "for"; body: Stmt[]; else: Stmt[] | null }
  | { t: "match"; cases: { body: Stmt[]; guard: boolean }[]; wildcard: Stmt[] | null }
  | { t: "try"; lead: boolean; body: Stmt[]; handlers: Handler[]; else: Stmt[] | null; fin: Stmt[] | null }
  | { t: "with"; body: Stmt[] };

// Weighted towards what the guards below need and random programs rarely
// reach: loops, jumps inside them, and exceptions inside `try` and `with`. At
// equal weights, 300 runs executed a `continue` in 2 and a `break` in 10.
function programArb(withExceptions: boolean): fc.Arbitrary<Stmt[]> {
  const { stmts } = fc.letrec<{ stmt: Stmt; stmts: Stmt[] }>((tie) => {
    const body = tie("stmts") as fc.Arbitrary<Stmt[]>;
    const w = (weight: number, arbitrary: fc.Arbitrary<Stmt>) => ({ weight, arbitrary });
    const choices = [
      w(3, fc.constant({ t: "probe" as const })),
      w(1, fc.constant({ t: "return" as const })),
      // `if c(k): break` as well as a bare break, which ends its loop on the first pass
      w(4, fc.record({ t: fc.constant("break" as const), guarded: fc.boolean() })),
      w(3, fc.record({ t: fc.constant("continue" as const), guarded: fc.boolean() })),
      w(2, fc.record({ t: fc.constant("if" as const), then: body, elifs: fc.array(body, { maxLength: 2 }), else: fc.option(body, { nil: null }) })),
      w(5, fc.record({ t: fc.constantFrom("while" as const, "for" as const), body, else: fc.option(body, { nil: null }) })),
      w(1, fc.record({
        t: fc.constant("match" as const),
        cases: fc.array(fc.record({ body, guard: fc.boolean() }), { minLength: 1, maxLength: 3 }),
        wildcard: fc.option(body, { nil: null }),
      })),
    ];
    if (withExceptions) {
      // `lead`: a handler body that opens with a probe, so entering it is observed
      const handler = fc.record({ type: fc.constantFrom("ValueError" as const, "KeyError" as const, null), lead: fc.boolean(), body });
      choices.push(
        w(3, fc.constant({ t: "throw" as const })),
        w(1, fc.record({ t: fc.constant("raise" as const), key: fc.boolean() })),
        w(1, fc.constant({ t: "assert" as const })),
        // `lead`: the body opens with a statement that may raise; a `with` body always does
        w(3, fc.record({
          t: fc.constant("try" as const),
          lead: fc.boolean(),
          body,
          handlers: fc.array(handler, { maxLength: 2 }),
          else: fc.option(body, { nil: null }),
          fin: fc.option(body, { nil: null }),
        })),
        w(2, fc.record({ t: fc.constant("with" as const), body })),
      );
    }
    return {
      stmt: fc.oneof({ maxDepth: 4, depthSize: "medium" }, ...choices) as fc.Arbitrary<Stmt>,
      stmts: fc.array(tie("stmt") as fc.Arbitrary<Stmt>, { maxLength: 4 }),
    };
  });
  return stmts;
}

interface Scope {
  loop: boolean; // inside a loop: break and continue are legal
  fin: boolean; // inside a finally: no jumps (legal, but PEP 765 warns)
}

/** Renders statements at `indent`; probe numbers come from `next`; `roles` tags the probes a guard looks for. */
function render(stmts: Stmt[], indent: string, scope: Scope, next: () => number, roles: Map<number, string>, role?: string): string[] {
  const out: string[] = [];
  const probe = () => {
    const k = next();
    if (role !== undefined && !roles.has(k)) roles.set(k, role);
    return `${indent}s(${k})`;
  };
  const block = (b: Stmt[], sc: Scope, r?: string) => {
    const lines = render(b, `${indent}    `, sc, next, roles, r);
    return lines.length > 0 ? lines : [`${indent}    pass`];
  };
  for (const st of stmts) {
    switch (st.t) {
      case "probe":
        out.push(probe());
        break;
      case "throw":
        out.push(`${indent}t(${next()})`);
        break;
      case "raise":
        out.push(`${indent}raise ${st.key ? "KeyError" : "ValueError"}()`);
        break;
      case "assert":
        out.push(`${indent}assert c(${next()})`);
        break;
      case "return":
        out.push(scope.fin ? probe() : `${indent}return`);
        break;
      case "break":
      case "continue":
        if (!scope.loop || scope.fin) out.push(probe());
        else if (st.guarded) out.push(`${indent}if c(${next()}):`, `${indent}    ${st.t}`);
        else out.push(`${indent}${st.t}`);
        break;
      case "if":
        out.push(`${indent}if c(${next()}):`, ...block(st.then, scope));
        for (const e of st.elifs) out.push(`${indent}elif c(${next()}):`, ...block(e, scope, "elif"));
        if (st.else !== null) out.push(`${indent}else:`, ...block(st.else, scope));
        break;
      case "while":
      case "for": {
        const head = st.t === "while" ? `while c(${next()}):` : (() => { const k = next(); return `for x${k} in a(${k}):`; })();
        out.push(`${indent}${head}`, ...block(st.body, { ...scope, loop: true }));
        if (st.else !== null) out.push(`${indent}else:`, ...block(st.else, scope, "loop_else"));
        break;
      }
      case "match":
        out.push(`${indent}match v(${next()}):`);
        st.cases.forEach((cs, i) => {
          const guard = cs.guard ? ` if c(${next()})` : "";
          out.push(`${indent}    case ${i}${guard}:`, ...render(cs.body, `${indent}        `, scope, next, roles).concat([]), ...(cs.body.length === 0 ? [`${indent}        pass`] : []));
        });
        if (st.wildcard !== null) {
          const lines = render(st.wildcard, `${indent}        `, scope, next, roles, "wildcard");
          out.push(`${indent}    case _:`, ...(lines.length > 0 ? lines : [`${indent}        pass`]));
        }
        break;
      case "try": {
        // A bare `except:` must be the last clause, and there can be only one.
        const typed = st.handlers.filter((h) => h.type !== null);
        const bare = st.handlers.find((h) => h.type === null);
        const handlers: Handler[] = st.handlers.length === 0 && st.fin === null ? [{ type: null, lead: false, body: [] }] : [...typed, ...(bare !== undefined ? [bare] : [])];
        out.push(`${indent}try:`, ...block(st.lead ? [{ t: "throw" }, ...st.body] : st.body, scope));
        for (const h of handlers) {
          const hb: Stmt[] = h.lead ? [{ t: "probe" }, ...h.body] : h.body;
          out.push(`${indent}except${h.type !== null ? ` ${h.type}` : ""}:`, ...block(hb, scope, h.type !== null ? "typed_handler" : undefined));
        }
        if (st.else !== null && handlers.length > 0) out.push(`${indent}else:`, ...block(st.else, scope, "try_else"));
        if (st.fin !== null) out.push(`${indent}finally:`, ...block(st.fin, { ...scope, fin: true }));
        break;
      }
      case "with":
        out.push(`${indent}with w(${next()}):`, ...block([{ t: "throw" }, ...st.body], scope));
        out.push(probe()); // after the block: reached by suppression too
        break;
    }
  }
  return out;
}

// Runs each function run0 … run{n-1} once per input set; prints
// [[{trace, outcome, suppressed}, …per input set], …per function].
const HARNESS = String.raw`
import json, sys
src, inputs, n = open(sys.argv[1]).read(), json.loads(sys.argv[2]), int(sys.argv[3])
class Budget(BaseException): pass
ns = {}
exec(compile(src, "p.py", "exec"), ns)
out = []
for f in range(n):
    results = []
    for ins in inputs:
        trace, i, suppressed = [], [0], [0]
        def nxt():
            x = ins[i[0] % len(ins)]; i[0] += 1; return x
        def s(k):
            trace.append(k)
            if len(trace) > 2000: raise Budget()
        def c(k):
            s(k); return len(trace) < 200 and nxt() % 2 == 1
        def t(k):
            s(k); r = nxt() % 5
            if r == 0: raise ValueError()
            if r == 1: raise KeyError()
        def v(k):
            s(k); return nxt() % 4
        def a(k):
            s(k); return [1, 2][: nxt() % 3] if len(trace) < 200 else []
        class W:
            def __enter__(self): return self
            def __exit__(self, et, ev, tb):
                if et is None or et is Budget or nxt() % 2 == 0: return False
                suppressed[0] += 1; return True
        def w(k):
            s(k); return W()
        try:
            ns[f"run{f}"](s, c, t, v, a, w); outcome = "return"
        except BaseException:
            outcome = "throw"
        results.append({"trace": trace, "outcome": outcome, "suppressed": suppressed[0]})
    out.append(results)
print(json.dumps(out))
`;

interface Graph {
  succ: Map<number, { to: number; kind: string }[]>;
  probeNode: Map<number, number>;
  probeNodes: Set<number>;
  entry: number;
  exit: number;
  kinds: Map<number, string>;
}

/** Extracts p.py once; the graph, cyclomatic number and E, N of each function run0 … run{count-1}. */
function graphsOf(dir: string, source: string, count: number) {
  const { tables } = extractPython(dir, { layers: ["refs", "flow"] });
  const lines = source.split("\n");
  const nodeOfCall = new Map(tables.rows("call_at").map((r) => [r.call_site as number, r.node as number]));
  return [...Array(count).keys()].map((f) => {
    const fnId = `p.py#run${f}`;
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
    const probeNode = new Map<number, number>();
    for (const cs of tables.rows("call_site")) {
      if (cs.caller !== fnId) continue;
      const m = /^[sctvaw]\((\d+)\)/.exec((lines[(cs.line as number) - 1] ?? "").slice((cs.col as number) - 1));
      const node = nodeOfCall.get(cs.id as number);
      if (m !== null && node !== undefined) probeNode.set(Number(m[1]), node);
    }
    const fn = tables.rows("fn").find((r) => r.id === fnId);
    const graph: Graph = {
      succ,
      probeNode,
      probeNodes: new Set(probeNode.values()),
      entry: nodes.find((r) => r.kind === "entry")?.id as number,
      exit: nodes.find((r) => r.kind === "exit")?.id as number,
      kinds,
    };
    return {
      graph,
      cyclomatic: fn?.cyclomatic as number,
      e,
      n: nodes.filter((r) => r.kind !== "throw_exit").length,
      decisions: tables.rows("decision").filter((d) => d.fn === fnId).map((d) => d.kind as string),
    };
  });
}

/** A path from → to through probe-free nodes, as its [from-kind, edge-kind, to-kind] steps; undefined if none. */
function pathBetween(g: Graph, from: number, to: number): string[][] | undefined {
  const seen = new Set<number>([from]);
  const queue: [number, string[][]][] = [[from, []]];
  while (queue.length > 0) {
    const [at, steps] = queue.shift() as [number, string[][]];
    for (const { to: nxt, kind } of g.succ.get(at) ?? []) {
      const step = [...steps, [g.kinds.get(at) ?? "", kind, g.kinds.get(nxt) ?? ""]];
      if (nxt === to) return step;
      if (seen.has(nxt) || g.probeNodes.has(nxt)) continue;
      seen.add(nxt);
      queue.push([nxt, step]);
    }
  }
  return undefined;
}

/** Renders the programs as run0 … run{n-1} in one p.py; probe numbers are unique across them. */
function projectFor(programs: Stmt[][]) {
  let k = 1;
  const roles = new Map<number, string>();
  const lines: string[] = [];
  programs.forEach((stmts, f) => {
    const body = render(stmts, "    ", { loop: false, fin: false }, () => k++, roles);
    lines.push(`def run${f}(s, c, t, v, a, w):`, ...(body.length > 0 ? body : ["    pass"]), "");
  });
  const source = lines.join("\n");
  const dir = tempDir("pyflow");
  fs.writeFileSync(path.join(dir, "p.py"), source);
  return { dir, source, roles };
}

// Three programs per run, extracted and executed together: the rarer guards
// below need several hundred programs, and a program costs far less than a run.
const PROGRAMS = 3;

test("P1-py: every step of a real Python execution is a path in the control-flow graph", () => {
  const seen = new Map<string, number>();
  const roles = new Set<string>();
  let steps = 0;
  let suppressed = 0;
  fc.assert(
    fc.property(
      fc.array(programArb(true), { minLength: PROGRAMS, maxLength: PROGRAMS }),
      fc.array(fc.array(fc.nat(9), { minLength: 1, maxLength: 24 }), { minLength: 4, maxLength: 4 }),
      (programs, inputSets) => {
        const { dir, source, roles: probeRoles } = projectFor(programs);
        const py = spawnSync(PYTHON, ["-c", HARNESS, path.join(dir, "p.py"), JSON.stringify(inputSets), String(PROGRAMS)], { encoding: "utf8", maxBuffer: 1 << 26 });
        assert.equal(py.status, 0, `${py.stderr}\n${source}`);
        const perFn = JSON.parse(py.stdout) as { trace: number[]; outcome: string; suppressed: number }[][];
        const graphs = graphsOf(dir, source, PROGRAMS);
        const record = (fromKind: string, kind: string, toKind: string) => {
          for (const key of [kind, `${fromKind}>${kind}`, `>${toKind}`]) seen.set(key, (seen.get(key) ?? 0) + 1);
        };
        perFn.forEach((runs, f) => {
          const { graph } = graphs[f] as (typeof graphs)[number];
          for (const [r, run] of runs.entries()) {
            let at = graph.entry;
            for (const k of run.trace) {
              const node = graph.probeNode.get(k);
              assert.ok(node !== undefined, `probe ${k} has no flow node\n${source}`);
              const p = pathBetween(graph, at, node);
              assert.ok(p !== undefined, `run${f}: no path from node ${at} to probe ${k} (node ${node})\ninputs ${inputSets[r]}\ntrace ${run.trace}\n${source}`);
              for (const [fromKind, kind, toKind] of p) record(fromKind as string, kind as string, toKind as string);
              const role = probeRoles.get(k);
              if (role !== undefined) roles.add(role);
              steps++;
              at = node;
            }
            if (run.outcome === "return") {
              const p = pathBetween(graph, at, graph.exit);
              assert.ok(p !== undefined, `run${f}: no path from node ${at} to exit\ntrace ${run.trace}\n${source}`);
              for (const [fromKind, kind, toKind] of p) record(fromKind as string, kind as string, toKind as string);
            }
            suppressed += run.suppressed > 0 && run.outcome === "return" ? 1 : 0;
          }
        });
        fs.rmSync(dir, { recursive: true, force: true });
      },
    ),
    { numRuns: RUNS },
  );
  // Non-vacuity, clause by clause: back edges, break, continue, an exception
  // caught, a finally, cases and default; an exception passing a typed clause
  // that did not match; loop and try `else:`, an elif, a wildcard case, a typed
  // handler entered; and a `with` that swallowed an exception and carried on.
  for (const kind of ["back", "break", "continue", "throw", ">finally", ">catch", "case", "default", "catch>on_false"]) {
    assert.ok((seen.get(kind) ?? 0) > 0, `no executed step used ${kind}; saw ${[...seen.keys()]}`);
  }
  for (const role of ["loop_else", "try_else", "typed_handler", "elif", "wildcard"]) assert.ok(roles.has(role), `no execution reached a ${role} probe`);
  assert.ok(suppressed > 0, "no execution had a with-block suppress an exception and return normally");
  assert.ok(steps > RUNS * 4, `only ${steps} steps checked`);
});

test("P3-py: cyclomatic complexity from decisions equals E − N + 2 over the graph", () => {
  const kinds = new Set<string>();
  fc.assert(
    fc.property(fc.array(programArb(false), { minLength: PROGRAMS, maxLength: PROGRAMS }), (programs) => {
      const { dir, source } = projectFor(programs);
      for (const [f, { cyclomatic, e, n, decisions }] of graphsOf(dir, source, PROGRAMS).entries()) {
        assert.equal(cyclomatic, e - n + 2, `run${f}: decisions ${decisions}\n${source}`);
        for (const d of decisions) kinds.add(d);
      }
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  for (const k of ["if", "while", "for_of", "case"]) assert.ok(kinds.has(k), `no program had a ${k} decision`);
});
