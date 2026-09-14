// P1-go — CFG soundness against execution, for Go. An independent oracle: the
// generated functions are compiled and run, and every step of a real trace must
// be a path in the extracted control-flow graph.
//
// Probes, each a function the harness passes in: `s(k)` a statement, `c(k)` a
// condition, `t(k)` a statement that may panic, `v(k)` a switch tag, `a(k)` a
// range slice, `ch(k)` a select case's channel (ready, or nil and never ready),
// `d(k)` a deferred call and `r(k)` a deferred call that recovers. For
// consecutive probes j, k of a trace there must be a path node(j) → node(k)
// through nodes holding no probe; a deferred probe's node is the function's
// `finally`, where deferred calls run.
//
// P3-go — two spellings of cyclomatic complexity agree: on programs without
// panics or defers, `fn.cyclomatic` (1 + decisions) equals E − N + 2 over the
// function's graph (N without `throw_exit`).

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { extractGo, goAvailable, tempDir } from "../helpers.ts";

// 100 runs of three programs. The rarest guard, an elif reached, was measured at
// 22 of 200 runs and 8 of 100: at 50 runs a suite would miss it about one time in
// 100, at 100 about one in 3,000. A labeled jump taken (the `nest` shape) and a
// recovery (the `risky` shape and the doomed function) are far more common.
const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "100", 10);
const NO_GO = goAvailable() ? false : "needs the `go` command";

type Stmt =
  | { t: "probe" }
  | { t: "throw" }
  | { t: "panic" }
  | { t: "return" }
  | { t: "break" | "continue"; guarded: boolean; outer: boolean }
  | { t: "goto"; guarded: boolean }
  | { t: "if"; then: Stmt[]; elifs: Stmt[][]; else: Stmt[] | null }
  | { t: "loop"; form: "cond" | "three" | "bare" | "range"; body: Stmt[] }
  | { t: "switch"; tagged: boolean; cases: { body: Stmt[]; fall: boolean }[]; deflt: Stmt[] | null }
  | { t: "select"; cases: Stmt[][]; deflt: Stmt[] }
  | { t: "nest"; jump: "break" | "continue"; body: Stmt[] }
  | { t: "defer"; recover: boolean }
  | { t: "risky" };

function programArb(withPanics: boolean): fc.Arbitrary<Stmt[]> {
  const { stmts } = fc.letrec<{ stmt: Stmt; stmts: Stmt[] }>((tie) => {
    const body = tie("stmts") as fc.Arbitrary<Stmt[]>;
    const w = (weight: number, arbitrary: fc.Arbitrary<Stmt>) => ({ weight, arbitrary });
    const choices = [
      w(3, fc.constant({ t: "probe" as const })),
      w(1, fc.constant({ t: "return" as const })),
      w(3, fc.record({ t: fc.constant("break" as const), guarded: fc.boolean(), outer: fc.boolean() })),
      w(3, fc.record({ t: fc.constant("continue" as const), guarded: fc.boolean(), outer: fc.boolean() })),
      w(1, fc.record({ t: fc.constant("goto" as const), guarded: fc.boolean() })),
      w(2, fc.record({ t: fc.constant("if" as const), then: body, elifs: fc.array(body, { maxLength: 2 }), else: fc.option(body, { nil: null }) })),
      w(5, fc.record({ t: fc.constant("loop" as const), form: fc.constantFrom("cond" as const, "three" as const, "bare" as const, "range" as const), body })),
      w(2, fc.record({
        t: fc.constant("switch" as const),
        tagged: fc.boolean(),
        cases: fc.array(fc.record({ body, fall: fc.boolean() }), { minLength: 1, maxLength: 3 }),
        deflt: fc.option(body, { nil: null }),
      })),
      w(1, fc.record({ t: fc.constant("select" as const), cases: fc.array(body, { minLength: 1, maxLength: 2 }), deflt: body })),
      // Random nesting rarely executes a labeled jump: this shape is one.
      w(2, fc.record({ t: fc.constant("nest" as const), jump: fc.constantFrom("break" as const, "continue" as const), body })),
    ];
    if (withPanics) {
      choices.push(
        w(3, fc.constant({ t: "throw" as const })),
        w(1, fc.constant({ t: "panic" as const })),
        w(2, fc.record({ t: fc.constant("defer" as const), recover: fc.boolean() })),
        // A recovering defer just before a statement that may panic.
        w(2, fc.constant({ t: "risky" as const })),
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
  loops: { label: string; used: boolean }[];
  breakable: boolean; // inside a loop, switch or select
  end: { used: boolean };
  labels: { next: number };
}

/** Renders statements at `indent`; probe numbers come from `next`; `roles` tags the probes a guard looks for. */
function render(stmts: Stmt[], indent: string, scope: Scope, next: () => number, roles: Map<number, string>, role?: string): string[] {
  const out: string[] = [];
  const probe = () => {
    const k = next();
    if (role !== undefined && !roles.has(k)) roles.set(k, role);
    return `${indent}s(${k})`;
  };
  const inner = `${indent}\t`;
  const block = (b: Stmt[], sc: Scope, r?: string) => render(b, inner, sc, next, roles, r);
  const guard = (...lines: string[]) => [`${indent}if c(${next()}) {`, ...lines.map((l) => `${inner}${l}`), `${indent}}`];
  for (const st of stmts) {
    switch (st.t) {
      case "probe":
        out.push(probe());
        break;
      case "throw":
        out.push(`${indent}t(${next()})`);
        break;
      case "panic":
        out.push(...guard("panic(0)"));
        break;
      case "return":
        out.push(`${indent}return`);
        break;
      case "break":
      case "continue": {
        const legal = st.t === "break" ? scope.breakable : scope.loops.length > 0;
        if (!legal) {
          out.push(probe());
          break;
        }
        const jump: string[] = [st.t];
        if (st.outer && scope.loops.length >= 2) {
          const loop = scope.loops[scope.loops.length - 2] as { label: string; used: boolean };
          loop.used = true;
          // A probe just before the jump: reaching it is the labeled jump taken.
          const k = next();
          roles.set(k, `labeled_${st.t}`);
          jump.splice(0, 1, `s(${k})`, `${st.t} ${loop.label}`);
        }
        out.push(...(st.guarded ? guard(...jump) : jump.map((l) => `${indent}${l}`)));
        break;
      }
      case "goto":
        scope.end.used = true;
        out.push(...(st.guarded ? guard("goto end") : [`${indent}goto end`]));
        break;
      case "if":
        out.push(`${indent}if c(${next()}) {`, ...block(st.then, scope));
        for (const e of st.elifs) out.push(`${indent}} else if c(${next()}) {`, ...block(e, scope, "elif"));
        if (st.else !== null) out.push(`${indent}} else {`, ...block(st.else, scope));
        out.push(`${indent}}`);
        break;
      case "loop": {
        const loop = { label: `L${scope.labels.next++}`, used: false };
        const sc = { ...scope, loops: [...scope.loops, loop], breakable: true };
        let head: string[];
        if (st.form === "cond") head = [`${indent}for c(${next()}) {`];
        else if (st.form === "three") {
          const k = next();
          head = [`${indent}for i${k} := 0; c(${k}); i${k}++ {`];
        } else if (st.form === "range") head = [`${indent}for range a(${next()}) {`];
        else head = [`${indent}for {`, `${inner}if !c(${next()}) {`, `${inner}\tbreak`, `${inner}}`];
        const lines = block(st.body, sc);
        out.push(...(loop.used ? [`${indent}${loop.label}:`] : []), ...head, ...lines, `${indent}}`);
        break;
      }
      case "switch": {
        const sc = { ...scope, breakable: true };
        out.push(st.tagged ? `${indent}switch v(${next()}) {` : `${indent}switch {`);
        st.cases.forEach((cs, i) => {
          out.push(st.tagged ? `${indent}case ${i}:` : `${indent}case c(${next()}):`, ...block(cs.body, sc));
          // `fallthrough` may not leave the last clause.
          if (cs.fall && (i < st.cases.length - 1 || st.deflt !== null)) out.push(`${inner}fallthrough`);
        });
        if (st.deflt !== null) out.push(`${indent}default:`, ...block(st.deflt, sc, "default"));
        out.push(`${indent}}`);
        break;
      }
      case "select": {
        const sc = { ...scope, breakable: true };
        out.push(`${indent}select {`);
        for (const cs of st.cases) out.push(`${indent}case <-ch(${next()}):`, ...block(cs, sc, "select_case"));
        out.push(`${indent}default:`, ...block(st.deflt, sc), `${indent}}`);
        break;
      }
      case "nest": {
        const outer = { label: `L${scope.labels.next++}`, used: true };
        const innerLoop = { label: `L${scope.labels.next++}`, used: false };
        const sc = { ...scope, loops: [...scope.loops, outer, innerLoop], breakable: true };
        const k = next();
        const j = next();
        const lines = render(st.body, `${inner}\t`, sc, next, roles);
        const p = next();
        roles.set(p, `labeled_${st.jump}`);
        out.push(
          `${indent}${outer.label}:`,
          `${indent}for c(${k}) {`,
          ...(innerLoop.used ? [`${inner}${innerLoop.label}:`] : []),
          `${inner}for c(${j}) {`,
          ...lines,
          `${inner}\tif c(${next()}) {`,
          `${inner}\t\ts(${p})`,
          `${inner}\t\t${st.jump} ${outer.label}`,
          `${inner}\t}`,
          `${inner}}`,
          `${indent}}`,
        );
        break;
      }
      case "defer":
        out.push(`${indent}defer ${st.recover ? "r" : "d"}(${next()})`);
        break;
      case "risky":
        out.push(`${indent}defer r(${next()})`, `${indent}t(${next()})`);
        break;
    }
  }
  return out;
}

const PARAMS = "s func(int), c func(int) bool, t func(int), v func(int) int, a func(int) []int, ch func(int) chan int, d func(int), r func(int)";

// Runs each function once per input set; prints [[{trace, outcome, recovered}, …per input], …per function].
function harness(count: number): string {
  return `package main

import (
	"encoding/json"
	"fmt"
	"os"
)

type budget struct{}

type result struct {
	Trace     []int  \`json:"trace"\`
	Outcome   string \`json:"outcome"\`
	Recovered int    \`json:"recovered"\`
}

func main() {
	var inputs [][]int
	if err := json.Unmarshal([]byte(os.Args[1]), &inputs); err != nil {
		panic(err)
	}
	fns := []func(${PARAMS}){${[...Array(count).keys()].map((f) => `run${f}`).join(", ")}}
	out := [][]result{}
	for _, f := range fns {
		var results []result
		for _, ins := range inputs {
			res := &result{Trace: []int{}, Outcome: "return"}
			i := 0
			nxt := func() int { x := ins[i%len(ins)]; i++; return x }
			s := func(k int) {
				res.Trace = append(res.Trace, k)
				if len(res.Trace) > 2000 {
					panic(budget{})
				}
			}
			c := func(k int) bool { s(k); return len(res.Trace) < 200 && nxt()%2 == 1 }
			t := func(k int) {
				s(k)
				if nxt()%5 == 0 {
					panic(k)
				}
			}
			v := func(k int) int { s(k); return nxt() % 4 }
			a := func(k int) []int {
				s(k)
				if len(res.Trace) >= 200 {
					return nil
				}
				return make([]int, nxt()%3)
			}
			ch := func(k int) chan int {
				s(k)
				if nxt()%2 == 0 {
					return nil
				}
				x := make(chan int, 1)
				x <- 1
				return x
			}
			d := func(k int) { s(k) }
			r := func(k int) {
				s(k)
				if recover() != nil {
					res.Recovered++
				}
			}
			func() {
				defer func() {
					if recover() != nil {
						res.Outcome = "throw"
					}
				}()
				f(s, c, t, v, a, ch, d, r)
			}()
			results = append(results, *res)
		}
		out = append(out, results)
	}
	b, _ := json.Marshal(out)
	fmt.Println(string(b))
}
`;
}

interface Graph {
  succ: Map<number, { to: number; kind: string }[]>;
  probeNode: Map<number, number>;
  probeNodes: Set<number>;
  entry: number;
  exit: number;
  kinds: Map<number, string>;
}

function graphsOf(dir: string, source: string, count: number) {
  const { tables } = extractGo(dir, { layers: ["refs", "flow"] });
  const lines = source.split("\n");
  const nodeOfCall = new Map(tables.rows("call_at").map((r) => [r.call_site as number, r.node as number]));
  return [...Array(count).keys()].map((f) => {
    const fnId = `p.go#run${f}`;
    const nodes = tables.rows("flow_node").filter((r) => r.fn === fnId);
    const ids = new Set(nodes.map((r) => r.id as number));
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
      const m = /^(?:s|c|t|v|a|ch|d|r)\((\d+)\)/.exec((lines[(cs.line as number) - 1] ?? "").slice((cs.col as number) - 1));
      const node = nodeOfCall.get(cs.id as number);
      if (m !== null && node !== undefined) probeNode.set(Number(m[1]), node);
    }
    const fn = tables.rows("fn").find((r) => r.id === fnId);
    const kinds = new Map(nodes.map((r) => [r.id as number, r.kind as string]));
    const graph: Graph = {
      succ,
      probeNode,
      // A `finally` node runs whichever calls were deferred — perhaps none, when
      // no `defer` executed — so a path may pass it without its probes firing.
      probeNodes: new Set([...probeNode.values()].filter((n) => kinds.get(n) !== "finally")),
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

/** Renders the programs as run0 … run{n-1}. With `doomed`, the last opens with a
 * recovering defer and ends in a panic: unless it returns early, only a recovery
 * ends it normally, so its graph needs the edge from `finally` to `exit` that
 * recovery adds. */
function projectFor(programs: Stmt[][], doomed: boolean) {
  let k = 1;
  const roles = new Map<number, string>();
  const lines = ["package main", ""];
  programs.forEach((stmts, f) => {
    const scope: Scope = { loops: [], breakable: false, end: { used: false }, labels: { next: 0 } };
    const last = doomed && f === programs.length - 1;
    const opening = last ? [`\tdefer r(${k++})`] : [];
    const body = render(stmts, "\t", scope, () => k++, roles);
    lines.push(`func run${f}(${PARAMS}) {`, ...opening, ...body);
    if (scope.end.used) lines.push("end:", `\ts(${k++})`);
    if (last) lines.push("\tpanic(0)");
    lines.push("}", "");
  });
  const source = lines.join("\n");
  const dir = tempDir("goflow");
  fs.writeFileSync(path.join(dir, "go.mod"), "module example.com/p1\n\ngo 1.26\n");
  fs.writeFileSync(path.join(dir, "p.go"), source);
  fs.writeFileSync(path.join(dir, "main.go"), harness(programs.length));
  return { dir, source, roles };
}

const PROGRAMS = 3;

test("P1-go: every step of a real Go execution is a path in the control-flow graph", { skip: NO_GO }, () => {
  const seen = new Map<string, number>(); // edge kinds used, in steps
  const roleRuns = new Map<string, number>(); // runs reaching a probe of each role
  let recoveredRuns = 0;
  let doomedRecoveredRuns = 0;
  let steps = 0;
  fc.assert(
    fc.property(
      fc.array(programArb(true), { minLength: PROGRAMS, maxLength: PROGRAMS }),
      fc.array(fc.array(fc.nat(9), { minLength: 1, maxLength: 24 }), { minLength: 4, maxLength: 4 }),
      (programs, inputSets) => {
        const { dir, source, roles: probeRoles } = projectFor(programs, true);
        const run = spawnSync("go", ["run", ".", JSON.stringify(inputSets)], { cwd: dir, encoding: "utf8", maxBuffer: 1 << 26 });
        assert.equal(run.status, 0, `${run.stderr}\n${source}`);
        const perFn = JSON.parse(run.stdout) as { trace: number[]; outcome: string; recovered: number }[][];
        const graphs = graphsOf(dir, source, PROGRAMS);
        const record = (fromKind: string, kind: string, toKind: string) => {
          for (const key of [kind, `${fromKind}>${kind}`, `>${toKind}`]) seen.set(key, (seen.get(key) ?? 0) + 1);
        };
        const reached = new Set<string>();
        let recovered = false;
        let doomedRecovered = false;
        perFn.forEach((runs, f) => {
          const { graph } = graphs[f] as (typeof graphs)[number];
          for (const [i, exec] of runs.entries()) {
            let at = graph.entry;
            let previous: number | undefined;
            for (const k of exec.trace) {
              const node = graph.probeNode.get(k);
              assert.ok(node !== undefined, `probe ${k} has no flow node\n${source}`);
              const role = probeRoles.get(k);
              if (role !== undefined) reached.add(role);
              steps++;
              // Two probes of one node are evaluated together, in source order — a
              // select's channel operands, say — so that step is no edge. The same
              // probe again (a loop head) still needs a path round.
              if (node === at && previous !== undefined && previous !== k) {
                previous = k;
                continue;
              }
              previous = k;
              const p = pathBetween(graph, at, node);
              assert.ok(p !== undefined, `run${f}: no path from node ${at} to probe ${k} (node ${node})\ninputs ${inputSets[i]}\ntrace ${exec.trace}\n${source}`);
              for (const [fromKind, kind, toKind] of p) record(fromKind as string, kind as string, toKind as string);
              at = node;
            }
            if (exec.outcome === "return") {
              const p = pathBetween(graph, at, graph.exit);
              assert.ok(p !== undefined, `run${f}: no path from node ${at} to exit\ninputs ${inputSets[i]}\ntrace ${exec.trace}\n${source}`);
              for (const [fromKind, kind, toKind] of p) record(fromKind as string, kind as string, toKind as string);
              if (exec.recovered > 0) recovered = true;
              if (exec.recovered > 0 && f === PROGRAMS - 1) doomedRecovered = true;
            }
          }
        });
        for (const role of reached) roleRuns.set(role, (roleRuns.get(role) ?? 0) + 1);
        if (recovered) recoveredRuns++;
        if (doomedRecovered) doomedRecoveredRuns++;
        fs.rmSync(dir, { recursive: true, force: true });
      },
    ),
    { numRuns: RUNS },
  );
  if (process.env.CODE_FACTS_RATES !== undefined) {
    console.error(`P1-go rates over ${RUNS} runs: roles ${JSON.stringify([...roleRuns])}, recovered ${recoveredRuns}, doomed recovered ${doomedRecoveredRuns}, steps ${JSON.stringify([...seen])}`);
  }
  // Non-vacuity, clause by clause: back edges, break, continue, a panic into
  // deferred calls, cases, default, fallthrough, goto, a select; an elif, a
  // select case, a default clause and both labeled jumps taken; and a panic a
  // deferred call recovered, the function then returning normally.
  for (const kind of ["back", "break", "continue", "throw", ">finally", "case", "default", "fallthrough", "goto", ">select"]) {
    assert.ok((seen.get(kind) ?? 0) > 0, `no executed step used ${kind}; saw ${[...seen.keys()]}`);
  }
  for (const role of ["elif", "select_case", "default", "labeled_break", "labeled_continue"]) {
    assert.ok((roleRuns.get(role) ?? 0) > 0, `no execution reached a ${role} probe`);
  }
  assert.ok(recoveredRuns > 0, "no execution recovered from a panic and returned normally");
  assert.ok(doomedRecoveredRuns > 0, "no function that ends in a panic recovered and returned normally");
  assert.ok(steps > RUNS * 4, `only ${steps} steps checked`);
});

test("P3-go: cyclomatic complexity from decisions equals E − N + 2 over the graph", { skip: NO_GO }, () => {
  const kinds = new Set<string>();
  fc.assert(
    fc.property(fc.array(programArb(false), { minLength: PROGRAMS, maxLength: PROGRAMS }), (programs) => {
      const { dir, source } = projectFor(programs, false);
      for (const [f, { cyclomatic, e, n, decisions }] of graphsOf(dir, source, PROGRAMS).entries()) {
        assert.equal(cyclomatic, e - n + 2, `run${f}: decisions ${decisions}\n${source}`);
        for (const d of decisions) kinds.add(d);
      }
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  for (const k of ["if", "for", "for_of", "case"]) assert.ok(kinds.has(k), `no program had a ${k} decision`);
});
