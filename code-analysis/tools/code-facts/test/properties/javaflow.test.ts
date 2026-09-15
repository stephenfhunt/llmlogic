// P1-java — CFG soundness against execution, for Java. An independent oracle:
// the generated methods are compiled and run, and every step of a real trace
// must be a path in the extracted control-flow graph.
//
// Probes, each a static method: `s(k)` a statement, `c(k)` a condition, `t(k)` a
// statement that may throw, `v(k)` a switch selector, `a(k)` an array to iterate,
// `r(k)` a resource and `lock(k)` a monitor. For consecutive probes j, k of a
// trace there must be a path node(j) → node(k) through nodes holding no probe,
// and from the last probe a path to `exit` when the method returned. (An
// exception outside a `try` leaves the method with no edge, as in the TypeScript
// model, so a throw's end is not followed.) Jumps are guarded (`if (c(k)) break;`) so that no
// statement after them is unreachable, which javac rejects.
//
// P3-java — two spellings of cyclomatic complexity agree: on programs without
// exceptions, `fn.cyclomatic` (1 + decisions) equals E − N + 2 over the method's
// graph (N without `throw_exit`).

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { extractJava, javaAvailable, tempDir, writeFiles } from "../helpers.ts";

// 50 runs of three methods. The rarest guard, an exception caught, was measured at
// 41 of 200 runs, so 50 miss it about one suite in 100,000; an exception passing a
// catch that does not match (48 of 200), a labelled jump taken (73) and an else-if
// reached (98) are commoner.
const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "50", 10);
const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";

type Stmt =
  | { t: "probe" }
  | { t: "throw" }
  | { t: "return" }
  | { t: "break" | "continue"; outer: boolean }
  | { t: "if"; then: Stmt[]; elifs: Stmt[][]; else: Stmt[] | null }
  | { t: "loop"; form: "while" | "do" | "for" | "foreach"; body: Stmt[] }
  | { t: "switch"; arrow: boolean; cases: { body: Stmt[]; brk: boolean }[]; deflt: Stmt[] | null }
  | { t: "yield"; cases: Stmt[][]; deflt: Stmt[] }
  | { t: "try"; body: Stmt[]; catch: Stmt[] | null; fin: Stmt[] | null; narrow: boolean }
  | { t: "resources"; body: Stmt[]; catch: Stmt[] | null; fin: Stmt[] | null }
  | { t: "sync"; body: Stmt[] }
  | { t: "nest"; jump: "break" | "continue"; body: Stmt[] };

function programArb(withExceptions: boolean): fc.Arbitrary<Stmt[]> {
  const { stmts } = fc.letrec<{ stmt: Stmt; stmts: Stmt[] }>((tie) => {
    const body = tie("stmts") as fc.Arbitrary<Stmt[]>;
    const w = (weight: number, arbitrary: fc.Arbitrary<Stmt>) => ({ weight, arbitrary });
    const choices = [
      w(3, fc.constant({ t: "probe" as const })),
      w(1, fc.constant({ t: "return" as const })),
      w(3, fc.record({ t: fc.constant("break" as const), outer: fc.boolean() })),
      w(3, fc.record({ t: fc.constant("continue" as const), outer: fc.boolean() })),
      w(2, fc.record({ t: fc.constant("if" as const), then: body, elifs: fc.array(body, { maxLength: 2 }), else: fc.option(body, { nil: null }) })),
      w(5, fc.record({ t: fc.constant("loop" as const), form: fc.constantFrom("while" as const, "do" as const, "for" as const, "foreach" as const), body })),
      w(2, fc.record({
        t: fc.constant("switch" as const),
        arrow: fc.boolean(),
        cases: fc.array(fc.record({ body, brk: fc.boolean() }), { minLength: 1, maxLength: 3 }),
        deflt: fc.option(body, { nil: null }),
      })),
      w(2, fc.record({ t: fc.constant("yield" as const), cases: fc.array(body, { minLength: 1, maxLength: 3 }), deflt: body })),
      w(2, fc.record({ t: fc.constant("nest" as const), jump: fc.constantFrom("break" as const, "continue" as const), body })),
    ];
    if (withExceptions) {
      choices.push(
        w(3, fc.constant({ t: "throw" as const })),
        // A narrow catch never matches what `t` throws: the exception passes it by.
        w(2, fc.record({ t: fc.constant("try" as const), body, catch: fc.option(body, { nil: null }), fin: fc.option(body, { nil: null }), narrow: fc.boolean() })),
        w(2, fc.record({ t: fc.constant("resources" as const), body, catch: fc.option(body, { nil: null }), fin: fc.option(body, { nil: null }) })),
        w(1, fc.record({ t: fc.constant("sync" as const), body })),
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
  loops: { label: string }[];
  /** inside a loop or a switch statement, where `break` is legal */
  breakable: boolean;
  /** inside a switch expression's case, which no jump may leave */
  inExpression: boolean;
  labels: { next: number };
}

/** Renders statements at `indent`; probe numbers come from `next`; `roles` tags the first probe of a body a guard looks for. */
function render(stmts: Stmt[], indent: string, scope: Scope, next: () => number, roles: Map<number, string>, role?: string): string[] {
  const out: string[] = [];
  let tagged = role === undefined;
  const probe = () => {
    const k = next();
    if (!tagged && role !== undefined) {
      roles.set(k, role);
      tagged = true;
    }
    return `${indent}s(${k});`;
  };
  const inner = `${indent}  `;
  const block = (b: Stmt[], sc: Scope, r?: string) => {
    // A body's role marks a probe that opens it, so reaching that probe is entering the body.
    if (r === undefined) return render(b, inner, sc, next, roles);
    const k = next();
    roles.set(k, r);
    return [`${inner}s(${k});`, ...render(b, inner, sc, next, roles)];
  };
  for (const st of stmts) {
    switch (st.t) {
      case "probe":
        out.push(probe());
        break;
      case "throw":
        out.push(`${indent}t(${next()});`);
        break;
      case "return":
        out.push(scope.inExpression ? probe() : `${indent}if (c(${next()})) return;`);
        break;
      case "break":
      case "continue": {
        const legal = st.t === "break" ? scope.breakable : scope.loops.length > 0;
        if (!legal) {
          out.push(probe());
          break;
        }
        if (st.outer && scope.loops.length >= 2) {
          const loop = scope.loops[scope.loops.length - 2] as { label: string };
          const k = next();
          roles.set(k, `labeled_${st.t}`);
          out.push(`${indent}if (c(${next()})) {`, `${inner}s(${k});`, `${inner}${st.t} ${loop.label};`, `${indent}}`);
        } else {
          out.push(`${indent}if (c(${next()})) ${st.t};`);
        }
        break;
      }
      case "if":
        out.push(`${indent}if (c(${next()})) {`, ...block(st.then, scope));
        for (const e of st.elifs) out.push(`${indent}} else if (c(${next()})) {`, ...block(e, scope, "elif"));
        if (st.else !== null) out.push(`${indent}} else {`, ...block(st.else, scope));
        out.push(`${indent}}`);
        break;
      case "loop": {
        const loop = { label: `L${scope.labels.next++}` };
        const sc = { ...scope, loops: [...scope.loops, loop], breakable: true };
        const k = next();
        if (st.form === "while") out.push(`${indent}${loop.label}: while (c(${k})) {`, ...block(st.body, sc), `${indent}}`);
        else if (st.form === "for") out.push(`${indent}${loop.label}: for (int i${k} = 0; c(${k}); i${k}++) {`, ...block(st.body, sc), `${indent}}`);
        else if (st.form === "foreach") out.push(`${indent}${loop.label}: for (int x${k} : a(${k})) {`, ...block(st.body, sc), `${indent}}`);
        else out.push(`${indent}${loop.label}: do {`, ...block(st.body, sc), `${indent}} while (c(${k}));`);
        break;
      }
      case "switch": {
        const sc = { ...scope, breakable: true };
        out.push(`${indent}switch (v(${next()})) {`);
        st.cases.forEach((cs, i) => {
          if (st.arrow) out.push(`${indent}case ${i} -> {`, ...block(cs.body, sc), `${indent}}`);
          else out.push(`${indent}case ${i}:`, ...block(cs.body, sc), ...(cs.brk ? [`${inner}break;`] : []));
        });
        if (st.deflt !== null) {
          if (st.arrow) out.push(`${indent}default -> {`, ...block(st.deflt, sc, "default"), `${indent}}`);
          else out.push(`${indent}default:`, ...block(st.deflt, sc, "default"));
        }
        out.push(`${indent}}`);
        break;
      }
      case "yield": {
        const sc: Scope = { ...scope, loops: [], breakable: false, inExpression: true };
        const k = next();
        out.push(`${indent}int y${k} = switch (v(${next()})) {`);
        st.cases.forEach((cs, i) => out.push(`${indent}case ${i} -> {`, ...block(cs, sc, "switch_expression"), `${inner}yield ${i};`, `${indent}}`));
        out.push(`${indent}default -> {`, ...block(st.deflt, sc, "switch_expression"), `${inner}yield 9;`, `${indent}}`, `${indent}};`);
        break;
      }
      case "try":
      case "resources": {
        const head = st.t === "resources" ? `try (AutoCloseable r${next()} = r(${next()})) {` : "try {";
        out.push(`${indent}${head}`, ...block(st.body, scope, st.t === "resources" ? "resource_body" : undefined));
        const caught = st.catch ?? (st.t === "try" && st.fin === null ? [] : null);
        const type = st.t === "try" && st.narrow ? "IllegalArgumentException" : "RuntimeException";
        if (caught !== null) out.push(`${indent}} catch (${type} e${next()}) {`, ...block(caught, scope, "caught"));
        if (st.fin !== null) out.push(`${indent}} finally {`, ...block(st.fin, scope, "finally"));
        out.push(`${indent}}`);
        break;
      }
      case "sync":
        out.push(`${indent}synchronized (lock(${next()})) {`, ...block(st.body, scope, "synchronized"), `${indent}}`);
        break;
      case "nest": {
        const outer = { label: `L${scope.labels.next++}` };
        const innerLoop = { label: `L${scope.labels.next++}` };
        const sc = { ...scope, loops: [...scope.loops, outer, innerLoop], breakable: true };
        const k = next();
        const j = next();
        const lines = render(st.body, `${inner}  `, sc, next, roles);
        const p = next();
        roles.set(p, `labeled_${st.jump}`);
        out.push(
          `${indent}${outer.label}: while (c(${k})) {`,
          `${inner}${innerLoop.label}: while (c(${j})) {`,
          ...lines,
          `${inner}  if (c(${next()})) {`,
          `${inner}    s(${p});`,
          `${inner}    ${st.jump} ${outer.label};`,
          `${inner}  }`,
          `${inner}}`,
          `${indent}}`,
        );
        break;
      }
    }
  }
  return out;
}

const PROBES = `package p;

import java.util.ArrayList;

final class Probes {
  static final class Budget extends Error {}

  static ArrayList<Integer> trace = new ArrayList<>();
  static int[] inputs = {0};
  static int at;

  static int next() {
    int v = inputs[at % inputs.length];
    at++;
    return v;
  }

  static void s(int k) {
    trace.add(k);
    if (trace.size() > 2000) throw new Budget();
  }

  static boolean c(int k) {
    s(k);
    return trace.size() < 200 && next() % 2 == 1;
  }

  static void t(int k) {
    s(k);
    if (next() % 5 == 0) throw new IllegalStateException();
  }

  static int v(int k) {
    s(k);
    return next() % 4;
  }

  static int[] a(int k) {
    s(k);
    return trace.size() >= 200 ? new int[0] : new int[next() % 3];
  }

  static AutoCloseable r(int k) {
    s(k);
    return () -> {};
  }

  static Object lock(int k) {
    s(k);
    return new Object();
  }
}
`;

function harness(count: number): string {
  const calls = [...Array(count).keys()].map((f) => `        case ${f} -> P.run${f}();`).join("\n");
  return `package p;

public final class Main {
  public static void main(String[] args) {
    String[] sets = args[0].split(";");
    StringBuilder out = new StringBuilder();
    for (int f = 0; f < ${count}; f++) {
      for (int i = 0; i < sets.length; i++) {
        String[] parts = sets[i].split(",");
        Probes.inputs = new int[parts.length];
        for (int j = 0; j < parts.length; j++) Probes.inputs[j] = Integer.parseInt(parts[j]);
        Probes.at = 0;
        Probes.trace = new java.util.ArrayList<>();
        String outcome = "return";
        try {
          switch (f) {
${calls}
            default -> {}
          }
        } catch (Probes.Budget b) {
          outcome = "budget";
        } catch (Throwable e) {
          outcome = "throw";
        }
        out.append(f).append(' ').append(i).append(' ').append(outcome);
        for (int k : Probes.trace) out.append(' ').append(k);
        out.append('\\n');
      }
    }
    System.out.print(out);
  }
}
`;
}

interface Graph {
  succ: Map<number, { to: number; kind: string }[]>;
  probeNode: Map<number, number>;
  probeNodes: Set<number>;
  entry: number;
  exit: number;
  throwExit: number;
  kinds: Map<number, string>;
}

function graphsOf(dir: string, source: string, count: number) {
  const { tables } = extractJava(dir, { layers: ["refs", "flow"] });
  const lines = source.split("\n");
  const nodeOfCall = new Map(tables.rows("call_at").map((r) => [r.call_site as number, r.node as number]));
  return [...Array(count).keys()].map((f) => {
    const fnId = `p/P.java#P.run${f}`;
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
      const m = /^(?:s|c|t|v|a|r|lock)\((\d+)\)/.exec((lines[(cs.line as number) - 1] ?? "").slice((cs.col as number) - 1));
      const node = nodeOfCall.get(cs.id as number);
      if (m !== null && node !== undefined) probeNode.set(Number(m[1]), node);
    }
    const kinds = new Map(nodes.map((r) => [r.id as number, r.kind as string]));
    const fn = tables.rows("fn").find((r) => r.id === fnId);
    const graph: Graph = {
      succ,
      probeNode,
      probeNodes: new Set(probeNode.values()),
      entry: nodes.find((r) => r.kind === "entry")?.id as number,
      exit: nodes.find((r) => r.kind === "exit")?.id as number,
      throwExit: nodes.find((r) => r.kind === "throw_exit")?.id as number,
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

function projectFor(programs: Stmt[][]) {
  let k = 1;
  const roles = new Map<number, string>();
  const lines = ["package p;", "", "import static p.Probes.*;", "", "final class P {"];
  programs.forEach((stmts, f) => {
    const scope: Scope = { loops: [], breakable: false, inExpression: false, labels: { next: 0 } };
    lines.push(`  static void run${f}() throws Exception {`, ...render(stmts, "    ", scope, () => k++, roles), "  }", "");
  });
  lines.push("}", "");
  const source = lines.join("\n");
  const dir = tempDir("javaflow");
  writeFiles(dir, { "p/P.java": source, "p/Probes.java": PROBES, "p/Main.java": harness(programs.length) });
  return { dir, source, roles };
}

const PROGRAMS = 3;

test("P1-java: every step of a real Java execution is a path in the control-flow graph", { skip: NO_JAVA }, () => {
  const seen = new Map<string, number>();
  const roleRuns = new Map<string, number>();
  let steps = 0;
  fc.assert(
    fc.property(
      fc.array(programArb(true), { minLength: PROGRAMS, maxLength: PROGRAMS }),
      fc.array(fc.array(fc.nat(9), { minLength: 1, maxLength: 24 }), { minLength: 4, maxLength: 4 }),
      (programs, inputSets) => {
        const { dir, source, roles } = projectFor(programs);
        const classes = path.join(dir, "out");
        const javac = spawnSync("javac", ["-d", classes, ...["P", "Probes", "Main"].map((c) => path.join(dir, "p", `${c}.java`))], { encoding: "utf8" });
        assert.equal(javac.status, 0, `${javac.stderr}\n${source}`);
        const run = spawnSync("java", ["-cp", classes, "p.Main", inputSets.map((s) => s.join(",")).join(";")], { encoding: "utf8", maxBuffer: 1 << 26 });
        assert.equal(run.status, 0, `${run.stderr}\n${source}`);
        const graphs = graphsOf(dir, source, PROGRAMS);
        const record = (fromKind: string, kind: string, toKind: string) => {
          for (const key of [kind, `${fromKind}>${kind}`, `>${toKind}`]) seen.set(key, (seen.get(key) ?? 0) + 1);
        };
        const reached = new Set<string>();
        for (const line of run.stdout.trim().split("\n")) {
          const [fs0, is0, outcome, ...ks] = line.split(" ");
          const f = Number(fs0);
          const { graph } = graphs[f] as (typeof graphs)[number];
          const trace = ks.map(Number);
          let at = graph.entry;
          let previous: number | undefined;
          for (const k of trace) {
            const node = graph.probeNode.get(k);
            assert.ok(node !== undefined, `probe ${k} has no flow node\n${source}`);
            const role = roles.get(k);
            if (role !== undefined) reached.add(role);
            steps++;
            // Two probes of one node (a statement evaluating two) are no step between nodes.
            if (node === at && previous !== undefined && previous !== k) {
              previous = k;
              continue;
            }
            previous = k;
            const p = pathBetween(graph, at, node);
            assert.ok(p !== undefined, `run${f}: no path from node ${at} to probe ${k} (node ${node})\ninputs ${inputSets[Number(is0)]}\ntrace ${trace}\n${source}`);
            for (const [a, b, c] of p) record(a as string, b as string, c as string);
            at = node;
          }
          // An exception outside a try leaves with no edge to say so, so only a return is followed to its end.
          if (outcome === "return") {
            const p = pathBetween(graph, at, graph.exit);
            assert.ok(p !== undefined, `run${f}: no path from node ${at} to exit\ninputs ${inputSets[Number(is0)]}\ntrace ${trace}\n${source}`);
            for (const [a, b, c] of p) record(a as string, b as string, c as string);
          }
        }
        for (const role of reached) roleRuns.set(role, (roleRuns.get(role) ?? 0) + 1);
        fs.rmSync(dir, { recursive: true, force: true });
      },
    ),
    { numRuns: RUNS },
  );
  if (process.env.CODE_FACTS_RATES !== undefined) {
    console.error(`P1-java rates over ${RUNS} runs: roles ${JSON.stringify([...roleRuns])}, steps ${JSON.stringify([...seen])}`);
  }
  // Non-vacuity, clause by clause.
  for (const kind of ["back", "break", "continue", "throw", "case", "default", ">finally", ">catch", "catch>on_false"]) {
    assert.ok((seen.get(kind) ?? 0) > 0, `no executed step used ${kind}; saw ${[...seen.keys()]}`);
  }
  for (const role of ["elif", "default", "labeled_break", "labeled_continue", "switch_expression", "caught", "finally", "resource_body", "synchronized"]) {
    assert.ok((roleRuns.get(role) ?? 0) > 0, `no execution reached a ${role} probe`);
  }
  assert.ok(steps > RUNS * 4, `only ${steps} steps checked`);
});

test("P3-java: cyclomatic complexity from decisions equals E − N + 2 over the graph", { skip: NO_JAVA }, () => {
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
  for (const k of ["if", "for", "for_of", "while", "do", "case"]) assert.ok(kinds.has(k), `no program had a ${k} decision`);
});
