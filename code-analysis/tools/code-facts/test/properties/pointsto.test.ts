// P5 — points-to soundness against execution. An independent oracle: Node runs
// a generated program of allocations, copies, field stores and loads, direct
// calls, and calls through variables holding functions; `probe(k, v)` records
// which allocation (object label, or function name) v actually holds. Every
// observation must be in what lib/pointsto.dl derives for that variable —
// the analysis may say more (it is an over-approximation), never less.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { datalog, engineAvailable, extract, tempDir, writeProject } from "../helpers.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "40", 10);
const VARS = 5;

type Op =
  | { t: "alloc"; dst: number }
  | { t: "copy"; dst: number; src: number }
  | { t: "store"; base: number; field: number; src: number }
  | { t: "load"; dst: number; base: number; field: number }
  | { t: "call"; dst: number; fn: number; a: number; b: number }
  | { t: "fnval"; dst: number; fn: number }
  | { t: "callvar"; dst: number; via: number; a: number; b: number }
  | { t: "probe"; v: number }
  | { t: "roundtrip"; base: number; src: number; dst: number; field: number };

// Operands index v0..v4 then the parameters p0, p1.
const operand = fc.nat(VARS + 1);
const arbOp: fc.Arbitrary<Op> = fc.oneof(
  fc.record({ t: fc.constant("alloc" as const), dst: fc.nat(VARS - 1) }),
  fc.record({ t: fc.constant("copy" as const), dst: fc.nat(VARS - 1), src: operand }),
  fc.record({ t: fc.constant("store" as const), base: operand, field: fc.nat(1), src: operand }),
  fc.record({ t: fc.constant("load" as const), dst: fc.nat(VARS - 1), base: operand, field: fc.nat(1) }),
  fc.record({ t: fc.constant("call" as const), dst: fc.nat(VARS - 1), fn: fc.nat(8), a: operand, b: operand }),
  fc.record({ t: fc.constant("fnval" as const), dst: fc.nat(VARS - 1), fn: fc.nat(8) }),
  fc.record({ t: fc.constant("callvar" as const), dst: fc.nat(VARS - 1), via: operand, a: operand, b: operand }),
  fc.record({ t: fc.constant("probe" as const), v: operand }),
  fc.record({ t: fc.constant("roundtrip" as const), base: operand, src: operand, dst: fc.nat(VARS - 1), field: fc.nat(1) }),
);
const arbProgram = fc.array(fc.record({ ops: fc.array(arbOp, { minLength: 2, maxLength: 12 }), ret: operand }), {
  minLength: 1,
  maxLength: 4,
});

const name = (i: number) => (i < VARS ? `v${i}` : `p${i - VARS}`);

/** Variables of each function assigned only by field loads: whatever they hold came through the heap. */
function heapOnly(ops: Op[]): Set<string> {
  const writers = new Map<number, string[]>();
  for (const op of ops) {
    if ("dst" in op) writers.set(op.dst, [...(writers.get(op.dst) ?? []), op.t]);
  }
  const out = new Set<string>();
  for (const [v, kinds] of writers) if (kinds.every((k) => k === "load" || k === "roundtrip")) out.add(`v${v}`);
  return out;
}

function render(program: { ops: Op[]; ret: number }[], typed: boolean): { text: string; probes: number; viaHeap: Set<number> } {
  let label = 0;
  let probe = 0;
  const viaHeap = new Set<number>();
  const any = typed ? ": any" : "";
  const lines: string[] = typed ? ["declare function probe(k: number, v: unknown): void;"] : [];
  program.forEach((f, i) => {
    const earlier = (j: number) => (i === 0 ? undefined : j % i);
    lines.push(`${typed ? "export " : ""}function f${i}(p0${any}, p1${any})${any} {`);
    lines.push(`  let ${[...Array(VARS).keys()].map((k) => `v${k}${any} = null`).join(", ")};`);
    // Two objects to start from, and every variable probed on the way out, so
    // that most runs observe something.
    const ops: Op[] = [{ t: "alloc", dst: 0 }, { t: "alloc", dst: 1 }, ...f.ops, ...[...Array(VARS + 2).keys()].map((v) => ({ t: "probe" as const, v }))];
    const loadedOnly = heapOnly(f.ops);
    for (const op of ops) {
      switch (op.t) {
        case "alloc":
          lines.push(`  v${op.dst} = { s: ${++label} };`);
          break;
        case "copy":
          lines.push(`  v${op.dst} = ${name(op.src)};`);
          break;
        case "store":
          lines.push(`  if (${name(op.base)} !== null && typeof ${name(op.base)} === "object") ${name(op.base)}.f${op.field} = ${name(op.src)};`);
          break;
        case "load":
          lines.push(`  v${op.dst} = ${name(op.base)} !== null && typeof ${name(op.base)} === "object" ? ${name(op.base)}.f${op.field} : null;`);
          break;
        case "call": {
          const j = earlier(op.fn);
          if (j !== undefined) lines.push(`  v${op.dst} = f${j}(${name(op.a)}, ${name(op.b)});`);
          break;
        }
        case "fnval": {
          const j = earlier(op.fn);
          if (j !== undefined) lines.push(`  v${op.dst} = f${j};`);
          break;
        }
        case "callvar":
          lines.push(`  v${op.dst} = typeof ${name(op.via)} === "function" ? ${name(op.via)}(${name(op.a)}, ${name(op.b)}) : null;`);
          break;
        case "probe":
          lines.push(`  probe(${++probe}, ${name(op.v)});`);
          if (loadedOnly.has(name(op.v))) viaHeap.add(probe);
          break;
        case "roundtrip": {
          const b = name(op.base);
          lines.push(`  if (${b} !== null && typeof ${b} === "object") ${b}.f${op.field} = ${name(op.src)};`);
          lines.push(`  v${op.dst} = ${b} !== null && typeof ${b} === "object" ? ${b}.f${op.field} : null;`);
          break;
        }
      }
    }
    lines.push(`  return ${name(f.ret)};`, "}");
  });
  return { text: `${lines.join("\n")}\n`, probes: probe, viaHeap };
}

/** Runs the program's last function; returns each observation as [probe k, label]. */
function execute(js: string, last: number): [number, string][] {
  const seen: [number, string][] = [];
  const probe = (k: number, v: unknown) => {
    if (typeof v === "function") seen.push([k, `fn:${v.name}`]);
    else if (v !== null && typeof v === "object" && typeof (v as { s?: unknown }).s === "number") seen.push([k, `o:${(v as { s: number }).s}`]);
  };
  const entry = new Function("probe", `${js}\nreturn f${last};`)(probe) as (a: unknown, b: unknown) => unknown;
  try {
    entry(null, null);
  } catch {
    // Unbounded recursion through a function-valued variable: what ran was real.
  }
  return seen;
}

test("P5: every object a variable holds at run time is in its points-to set", { skip: !engineAvailable() }, () => {
  let crossFunction = 0;
  let functionValues = 0;
  let throughHeap = 0;
  let observations = 0;
  fc.assert(
    fc.property(arbProgram, (program) => {
      const ts = render(program, true);
      const js = render(program, false);
      const seen = execute(js.text, program.length - 1);
      if (seen.length === 0) return;
      const dir = tempDir("p5");
      writeProject(dir, { "src/p.ts": ts.text }, {
        compilerOptions: { strict: false, noEmit: true, noLib: true, types: [] },
        files: ["src/p.ts"],
      });
      const out = path.join(dir, "out");
      const { tables } = extract(dir, { out, layers: ["refs", "flow", "dataflow"] });

      // probe k → the variable its second argument is; allocation site → label.
      const lines = ts.text.split("\n");
      const probeVar = new Map<number, string>();
      const actual = new Map(tables.rows("actual").filter((a) => a.index === 1).map((a) => [a.call_site, a.var as string]));
      for (const cs of tables.rows("call_site")) {
        const m = /probe\((\d+),/.exec((lines[(cs.line as number) - 1] ?? "").slice((cs.col as number) - 1));
        const v = actual.get(cs.id);
        if (cs.callee_name === "probe" && m !== null && v !== undefined) probeVar.set(Number(m[1]), v);
      }
      const labelOf = new Map<number, string>();
      for (const a of tables.rows("alloc")) {
        if (a.kind === "function") labelOf.set(a.site as number, `fn:${String(a.fn_target).replace(/^src\/p\.ts#/, "")}`);
        const m = /\{ s: (\d+) \}/.exec(lines[(a.line as number) - 1] ?? "");
        if (a.kind === "object" && m !== null) labelOf.set(a.site as number, `o:${m[1]}`);
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
      for (const [k, label] of seen) {
        const v = probeVar.get(k);
        assert.ok(v !== undefined, `probe ${k} has no argument variable`);
        assert.ok(derived.has(`${v} ${label}`), `probe ${k}: ${v} held ${label} at run time; derived only ${[...derived].filter((d) => d.startsWith(`${v} `))}\n${ts.text}`);
        observations++;
        if (label.startsWith("fn:")) functionValues++;
        if (ts.viaHeap.has(k)) throughHeap++;
        const fnOfProbe = v.replace(/^src\/p\.ts#(f\d+)\..*$/, "$1");
        const allocFn = label.startsWith("o:") ? lines.findIndex((l) => l.includes(`{ s: ${label.slice(2)} }`)) : -1;
        const ownerLine = lines.findLastIndex((l, i) => i <= allocFn && /^export function f\d+/.test(l));
        if (allocFn >= 0 && !(lines[ownerLine] ?? "").includes(`function ${fnOfProbe}(`)) crossFunction++;
      }
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  // Non-vacuity against the sentence: objects were observed after crossing a
  // call boundary, function values were observed at all, and some values can
  // only have arrived through the heap (a variable written by loads alone).
  assert.ok(observations >= RUNS, `only ${observations} observations`);
  assert.ok(crossFunction >= 1, "no object was observed outside the function that allocated it");
  assert.ok(functionValues >= 1, "no function value was observed");
  assert.ok(throughHeap >= 1, "no observed value can only have come through a field");
});
