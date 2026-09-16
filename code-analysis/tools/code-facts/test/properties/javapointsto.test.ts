// P5-java — points-to soundness against execution, for Java. An independent
// oracle: the generated program is compiled by javac, run by the JVM, and
// `probe(k, v)` records which allocation (an `O`'s label, a record's, or the
// method a functional value runs) v actually holds. Every observation must be in
// what lib/pointsto.dl derives for the variable passed — the analysis may say
// more, never less.
//
// P5's model, rendered as Java: locals and fields of type `Object`, objects
// `new O(n)`, stores and loads through `instanceof O`, direct calls, method
// references held as a functional interface, and calls through it. Java's own
// three: a record built and read back through the accessors the compiler writes,
// a static field, and a constructor's parameter reaching a field of the object
// it constructs — `this_var` joined to the `new`'s receiver.
//
// A functional value has no run-time identity in Java, so each method answers
// with its own name when called with `Probes.WHO` — the question a program can
// actually ask of a function it holds.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { datalog, engineAvailable, extractJava, javaAvailable, tempDir, writeFiles } from "../helpers.ts";

// 40 runs. Guard rates over 200: a functional value observed 153, an object
// observed outside the method allocating it 134, one through a record component
// 134, through a static 128, through a field 119, through a constructor 117, and
// the rarest — an object returned through a call of a functional value — 91. At
// 0.455 a run that one is missed by 40 runs about once in 10^10.
const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "40", 10);
const SKIP = !javaAvailable() ? "needs a JDK (`javac` and `java`)" : !engineAvailable() ? "needs the datalog engine" : false;
const VARS = 5;

type Op =
  | { t: "alloc"; dst: number; src: number }
  | { t: "record"; dst: number; a: number; b: number }
  | { t: "copy"; dst: number; src: number }
  | { t: "store"; base: number; field: number; src: number }
  | { t: "load"; dst: number; base: number; field: number }
  | { t: "call"; dst: number; fn: number; a: number; b: number }
  | { t: "fnval"; dst: number; fn: number }
  | { t: "callvar"; dst: number; via: number; a: number; b: number }
  | { t: "roundtrip"; base: number; src: number; dst: number; field: number }
  // Forced, never generated: the four shapes the random ops rarely reach.
  | { t: "heap"; base: number; src: number; field: number }
  | { t: "rec"; src: number }
  | { t: "stat"; src: number }
  | { t: "ctor"; src: number }
  | { t: "indirect"; fn: number };

type Fn = {
  ops: Op[];
  ret: number;
  heap: { at: number; base: number; src: number; field: number };
  rec: { src: number };
  stat: { src: number };
  ctor: { src: number };
  indirect: number;
};
type Program = Fn[];

// Operands index v0..v4 then the parameters p0, p1.
const operand = fc.nat(VARS + 1);
const arbOp: fc.Arbitrary<Op> = fc.oneof(
  fc.record({ t: fc.constant("alloc" as const), dst: fc.nat(VARS - 1), src: operand }),
  fc.record({ t: fc.constant("record" as const), dst: fc.nat(VARS - 1), a: operand, b: operand }),
  fc.record({ t: fc.constant("copy" as const), dst: fc.nat(VARS - 1), src: operand }),
  fc.record({ t: fc.constant("store" as const), base: operand, field: fc.nat(1), src: operand }),
  fc.record({ t: fc.constant("load" as const), dst: fc.nat(VARS - 1), base: operand, field: fc.nat(1) }),
  fc.record({ t: fc.constant("call" as const), dst: fc.nat(VARS - 1), fn: fc.nat(8), a: operand, b: operand }),
  fc.record({ t: fc.constant("fnval" as const), dst: fc.nat(VARS - 1), fn: fc.nat(8) }),
  fc.record({ t: fc.constant("callvar" as const), dst: fc.nat(VARS - 1), via: operand, a: operand, b: operand }),
  fc.record({ t: fc.constant("roundtrip" as const), base: operand, src: operand, dst: fc.nat(VARS - 1), field: fc.nat(1) }),
);

// Five shapes are forced rather than left to the random ops, each into a variable
// nothing else writes: `h` through an instance field, `k` through a functional
// value's call, `q` through a record component, `sh` through a static field, `c`
// through a constructor's parameter. Left to chance, dropping the rule that
// carries one stays green — P5 and P5-go were both green that way once.
const arbProgram: fc.Arbitrary<Program> = fc.array(
  fc.record({
    ops: fc.array(arbOp, { minLength: 2, maxLength: 12 }),
    ret: operand,
    heap: fc.record({ at: fc.nat(12), base: fc.nat(1), src: operand, field: fc.nat(1) }),
    rec: fc.record({ src: operand }),
    stat: fc.record({ src: operand }),
    ctor: fc.record({ src: operand }),
    indirect: fc.nat(8),
  }),
  { minLength: 1, maxLength: 4 },
);

const name = (i: number) => (i < VARS ? `v${i}` : `p${i - VARS}`);

const SUPPORT: Record<string, string> = {
  "p/O.java": `package p;

final class O {
  final int s;
  Object f0;
  Object f1;

  O(int s, Object f0) {
    this.s = s;
    this.f0 = f0;
  }
}
`,
  "p/R.java": `package p;

record R(int s, Object f0, Object f1) {}
`,
  "p/Fn.java": `package p;

interface Fn {
  Object apply(Object a, Object b);
}
`,
  "p/Probes.java": `package p;

import java.util.ArrayList;
import java.util.List;

final class Probes {
  /** Asked of a functional value: every method answers this with its own name. */
  static final Object WHO = new Object();

  static final List<String> seen = new ArrayList<>();
  private static int depth;

  /** Bounds recursion through functional values, which the generator allows. */
  static boolean enter() {
    return ++depth > 40;
  }

  static void leave() {
    depth--;
  }

  static void probe(int k, Object v) {
    if (v instanceof O o) {
      seen.add(k + " o:" + o.s);
    } else if (v instanceof R r) {
      seen.add(k + " r:" + r.s());
    } else if (v instanceof Fn f) {
      seen.add(k + " fn:" + f.apply(WHO, null));
    }
  }
}
`,
};

function render(program: Program): { text: string; only: Map<number, string> } {
  let label = 0;
  let probe = 0;
  let bind = 0;
  /** Probes whose value can only have come through one modelled path. */
  const only = new Map<number, string>();
  const lines = ["package p;", "", "final class P {", "  static Object shared;", ""];
  program.forEach((f, i) => {
    const earlier = (j: number) => (i === 0 ? undefined : j % i);
    lines.push(
      `  static Object f${i}(Object p0, Object p1) {`,
      `    if (p0 == Probes.WHO) return "f${i}";`,
      "    if (Probes.enter()) { Probes.leave(); return null; }",
      `    Object ${[...Array(VARS).keys()].map((n) => `v${n} = null`).join(", ")}, h = null, g = null, k = null, q = null, sh = null, c = null;`,
    );
    const at = f.heap.at % (f.ops.length + 1);
    const ops: Op[] = [
      { t: "alloc", dst: 0, src: VARS },
      { t: "alloc", dst: 1, src: VARS },
      ...f.ops.slice(0, at),
      { t: "heap", base: f.heap.base, src: f.heap.src, field: f.heap.field },
      ...f.ops.slice(at),
      { t: "rec", src: f.rec.src },
      { t: "stat", src: f.stat.src },
      { t: "ctor", src: f.ctor.src },
      { t: "indirect", fn: f.indirect },
    ];
    const emit = (line: string) => lines.push(`    ${line}`);
    for (const o of ops) {
      switch (o.t) {
        case "alloc":
          emit(`v${o.dst} = new O(${++label}, ${name(o.src)});`);
          break;
        case "record":
          emit(`v${o.dst} = new R(${++label}, ${name(o.a)}, ${name(o.b)});`);
          break;
        case "copy":
          emit(`v${o.dst} = ${name(o.src)};`);
          break;
        case "store":
          emit(`if (${name(o.base)} instanceof O b${++bind}) { b${bind}.f${o.field} = ${name(o.src)}; }`);
          break;
        case "load":
          emit(`v${o.dst} = ${name(o.base)} instanceof O b${++bind} ? b${bind}.f${o.field} : null;`);
          break;
        case "call": {
          const j = earlier(o.fn);
          if (j !== undefined) emit(`v${o.dst} = f${j}(${name(o.a)}, ${name(o.b)});`);
          break;
        }
        case "fnval": {
          const j = earlier(o.fn);
          if (j !== undefined) emit(`v${o.dst} = (Fn) P::f${j};`);
          break;
        }
        case "callvar":
          emit(`v${o.dst} = ${name(o.via)} instanceof Fn b${++bind} ? b${bind}.apply(${name(o.a)}, ${name(o.b)}) : null;`);
          break;
        case "roundtrip":
          emit(`if (${name(o.base)} instanceof O b${++bind}) { b${bind}.f${o.field} = ${name(o.src)}; }`);
          emit(`v${o.dst} = ${name(o.base)} instanceof O b${++bind} ? b${bind}.f${o.field} : null;`);
          break;
        case "heap":
          emit(`if (${name(o.base)} instanceof O b${++bind}) { b${bind}.f${o.field} = ${name(o.src)}; }`);
          emit(`h = ${name(o.base)} instanceof O b${++bind} ? b${bind}.f${o.field} : null;`);
          break;
        case "rec":
          emit(`R r${++bind} = new R(${++label}, ${name(o.src)}, null);`);
          emit(`q = r${bind}.f0();`);
          break;
        case "stat":
          emit(`P.shared = ${name(o.src)};`);
          emit("sh = P.shared;");
          break;
        case "ctor":
          emit(`O o${++bind} = new O(${++label}, ${name(o.src)});`);
          emit(`c = o${bind}.f0;`);
          break;
        case "indirect": {
          const j = earlier(o.fn);
          if (j === undefined) break; // f0 has no earlier method to call
          emit(`g = (Fn) P::f${j};`);
          emit(`if (g instanceof Fn b${++bind}) { k = b${bind}.apply(v0, v1); }`);
          break;
        }
      }
    }
    for (const v of [...[...Array(VARS).keys()].map((n) => `v${n}`), "p0", "p1", "h", "g", "k", "q", "sh", "c"]) {
      emit(`Probes.probe(${++probe}, ${v});`);
      if (v === "h") only.set(probe, "heap");
      if (v === "k") only.set(probe, "indirect");
      if (v === "q") only.set(probe, "record");
      if (v === "sh") only.set(probe, "static");
      if (v === "c") only.set(probe, "constructor");
      if (v === "g") only.set(probe, "function");
    }
    emit(`Object result = ${name(f.ret)};`);
    emit("Probes.leave();");
    emit("return result;");
    lines.push("  }", "");
  });
  lines.push("}", "");
  return { text: `${lines.join("\n")}\n`, only };
}

function harness(last: number): string {
  return `package p;

public final class Main {
  public static void main(String[] args) {
    P.f${last}(null, null);
    for (String line : Probes.seen) {
      System.out.println(line);
    }
  }
}
`;
}

test("P5-java: every object a Java variable holds at run time is in its points-to set", { skip: SKIP }, () => {
  const rates = { crossMethod: 0, functionValues: 0, heap: 0, indirect: 0, record: 0, statics: 0, constructor: 0 };
  let observations = 0;
  fc.assert(
    fc.property(arbProgram, (program) => {
      const { text, only } = render(program);
      const dir = tempDir("p5-java");
      writeFiles(dir, { ...SUPPORT, "p/P.java": text, "p/Main.java": harness(program.length - 1) });
      const classes = path.join(dir, "classes");
      const sources = ["O", "R", "Fn", "Probes", "P", "Main"].map((c) => path.join(dir, "p", `${c}.java`));
      const javac = spawnSync("javac", ["-d", classes, ...sources], { encoding: "utf8" });
      assert.equal(javac.status, 0, `${javac.stderr}\n${text}`);
      const run = spawnSync("java", ["-cp", classes, "p.Main"], { encoding: "utf8", maxBuffer: 1 << 26 });
      assert.equal(run.status, 0, `${run.stderr}\n${text}`);
      const seen = run.stdout
        .split("\n")
        .filter((l) => l.trim() !== "")
        .map((l) => [Number(l.slice(0, l.indexOf(" "))), l.slice(l.indexOf(" ") + 1)] as [number, string]);
      if (seen.length === 0) {
        fs.rmSync(dir, { recursive: true, force: true });
        return;
      }
      const out = path.join(dir, "out");
      const { tables } = extractJava(dir, { out, layers: ["refs", "flow", "dataflow"] });

      const lines = text.split("\n");
      const probeVar = new Map<number, string>();
      const actual = new Map(tables.rows("actual").filter((a) => a.index === 1).map((a) => [a.call_site, a.var as string]));
      for (const cs of tables.rows("call_site")) {
        if (cs.file !== "p/P.java" || cs.callee_name !== "probe") continue;
        const m = /probe\((\d+),/.exec(lines[(cs.line as number) - 1] ?? "");
        const v = actual.get(cs.id);
        if (m !== null && v !== undefined) probeVar.set(Number(m[1]), v);
      }
      const labelOf = new Map<number, string>();
      for (const a of tables.rows("alloc")) {
        if (a.kind === "function") labelOf.set(a.site as number, `fn:${String(a.fn_target).replace(/^.*#P\./, "")}`);
        const source = a.file === "p/P.java" ? (lines[(a.line as number) - 1] ?? "") : "";
        const o = /new O\((\d+),/.exec(source);
        const r = /new R\((\d+),/.exec(source);
        if (a.kind === "instance" && o !== null) labelOf.set(a.site as number, `o:${o[1]}`);
        if (a.kind === "instance" && r !== null) labelOf.set(a.site as number, `r:${r[1]}`);
      }

      const wanted = [...new Set(seen.map(([k]) => probeVar.get(k)).filter((v): v is string => v !== undefined))];
      const q = path.join(out, "p5.dl");
      fs.writeFileSync(q, `import "lib/pointsto.dl".\n${wanted.map((v) => `want("${v}").`).join("\n")}\n`);
      const r = datalog(q, ["got(V, O) :- want(V), pts(V, O)"]);
      assert.ok(r.code === 0 || r.code === 1, r.stderr);
      const derived = new Set<string>();
      for (const line of r.stdout.split("\n")) {
        const m = /^got\("([^"]+)", (\d+)\)\.$/.exec(line);
        if (m !== null) derived.add(`${m[1]} ${labelOf.get(Number(m[2]))}`);
      }
      const hit = { crossMethod: false, functionValues: false, heap: false, indirect: false, record: false, statics: false, constructor: false };
      for (const [k, label] of seen) {
        const v = probeVar.get(k);
        assert.ok(v !== undefined, `probe ${k} has no argument variable\n${text}`);
        assert.ok(
          derived.has(`${v} ${label}`),
          `probe ${k}: ${v} held ${label} at run time; derived only ${[...derived].filter((d) => d.startsWith(`${v} `))}\n${text}`,
        );
        observations++;
        if (label.startsWith("fn:")) hit.functionValues = true;
        const via = only.get(k);
        if (via === "heap") hit.heap = true;
        if (via === "indirect" && !label.startsWith("fn:")) hit.indirect = true;
        if (via === "record") hit.record = true;
        if (via === "static") hit.statics = true;
        if (via === "constructor") hit.constructor = true;
        const probeLine = lines.findIndex((l) => l.includes(`probe(${k}, `));
        const probeFn = lines.findLastIndex((l, i) => i <= probeLine && /^ {2}static Object f\d+\(/.test(l));
        const allocLine = label.startsWith("o:") ? lines.findIndex((l) => l.includes(`new O(${label.slice(2)},`)) : -1;
        const allocFn = lines.findLastIndex((l, i) => i <= allocLine && /^ {2}static Object f\d+\(/.test(l));
        if (allocLine >= 0 && allocFn !== probeFn) hit.crossMethod = true;
      }
      for (const key of Object.keys(rates) as (keyof typeof rates)[]) if (hit[key]) rates[key]++;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  if (process.env.CODE_FACTS_RATES !== undefined) {
    console.error(`P5-java rates over ${RUNS} runs: ${Object.entries(rates).map(([k, v]) => `${k} ${v}`).join(", ")}, observations ${observations}`);
  }
  // Non-vacuity against the sentence: an object seen outside the method that
  // allocated it, a functional value, and a value that can only have come
  // through a field, a functional call, a record component or a static.
  assert.ok(observations >= RUNS, `only ${observations} observations`);
  assert.ok(rates.crossMethod >= 1, "no object was observed outside the method that allocated it");
  assert.ok(rates.functionValues >= 1, "no functional value was observed");
  assert.ok(rates.heap >= 1, "no observed value can only have come through a field");
  assert.ok(rates.indirect >= 1, "no object was observed returned through a call of a functional value");
  assert.ok(rates.record >= 1, "no object was observed through a record component");
  assert.ok(rates.statics >= 1, "no object was observed through a static field");
  assert.ok(rates.constructor >= 1, "no object reached a field through a constructor's parameter");
});
