// P5-go — points-to soundness against execution, for Go. An independent oracle:
// the generated program is compiled and run, and `probe(k, v)` records which
// allocation (an `&O{s: N}` label, or a function's name) v actually holds.
// Every observation must be in what lib/pointsto.dl derives for the variable
// passed — the analysis may say more, never less.
//
// P5's model, rendered as Go: variables and fields of type `any`, objects
// `&O{s: N}`, stores and loads through a type assertion to `*O`, direct calls,
// function values, and calls through a type assertion to the function type.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { datalog, engineAvailable, extractGo, goAvailable, tempDir, writeFiles } from "../helpers.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "40", 10);
const SKIP = !goAvailable() ? "needs the `go` command" : !engineAvailable() ? "needs the datalog engine" : false;
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
  | { t: "roundtrip"; base: number; src: number; dst: number; field: number }
  | { t: "heap"; base: number; src: number; field: number }
  | { t: "indirect"; fn: number };

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
// As in P5: one round trip through a field into `h`, which nothing else writes.
const arbProgram = fc.array(
  fc.record({
    ops: fc.array(arbOp, { minLength: 2, maxLength: 12 }),
    ret: operand,
    heap: fc.record({ at: fc.nat(12), base: fc.nat(1), src: operand, field: fc.nat(1) }),
    // A call through a function value into `k`, which nothing else writes: an
    // object in `k` was returned through the value. Left to the random ops, that
    // call rarely held a function, and dropping indirect calls' targets stayed green.
    indirect: fc.nat(8),
  }),
  { minLength: 1, maxLength: 4 },
);

const name = (i: number) => (i < 0 ? "h" : i < VARS ? `v${i}` : `p${i - VARS}`);

function heapOnly(ops: Op[]): Set<string> {
  const writers = new Map<number, string[]>();
  for (const op of ops) {
    if ("dst" in op) writers.set(op.dst, [...(writers.get(op.dst) ?? []), op.t]);
  }
  const out = new Set<string>();
  for (const [v, kinds] of writers) if (kinds.every((k) => k === "load" || k === "roundtrip")) out.add(`v${v}`);
  return out;
}

type Program = { ops: Op[]; ret: number; heap: { at: number; base: number; src: number; field: number }; indirect: number }[];

function render(program: Program): { text: string; viaHeap: Set<number>; viaIndirect: Set<number> } {
  let label = 0;
  let probe = 0;
  const viaHeap = new Set<number>();
  const viaIndirect = new Set<number>();
  const lines = ["package main", "", "type O struct {", "\ts      int", "\tf0, f1 any", "}", ""];
  const loadInto = (dst: string, base: string, field: number) => [
    `\tif o, ok := ${base}.(*O); ok {`,
    `\t\t${dst} = o.f${field}`,
    "\t} else {",
    `\t\t${dst} = nil`,
    "\t}",
  ];
  const storeInto = (base: string, field: number, src: string) => [`\tif o, ok := ${base}.(*O); ok {`, `\t\to.f${field} = ${src}`, "\t}"];
  program.forEach((f, i) => {
    const earlier = (j: number) => (i === 0 ? undefined : j % i);
    lines.push(`func f${i}(p0, p1 any) any {`, "\tdefer leave()", "\tif enter() {", "\t\treturn nil", "\t}");
    lines.push(`\tvar ${[...Array(VARS).keys()].map((k) => `v${k}`).join(", ")}, h, g, k any`);
    const at = f.heap.at % (f.ops.length + 1);
    const ops: Op[] = [
      { t: "alloc", dst: 0 },
      { t: "alloc", dst: 1 },
      ...f.ops.slice(0, at),
      { t: "heap", base: f.heap.base, src: f.heap.src, field: f.heap.field },
      ...f.ops.slice(at),
      { t: "indirect" as const, fn: f.indirect },
      ...[...Array(VARS + 2).keys()].map((v) => ({ t: "probe" as const, v })),
      { t: "probe" as const, v: -1 },
    ];
    const loadedOnly = heapOnly(f.ops);
    loadedOnly.add("h");
    for (const op of ops) {
      switch (op.t) {
        case "alloc":
          lines.push(`\tv${op.dst} = &O{s: ${++label}}`);
          break;
        case "copy":
          lines.push(`\tv${op.dst} = ${name(op.src)}`);
          break;
        case "store":
          lines.push(...storeInto(name(op.base), op.field, name(op.src)));
          break;
        case "load":
          lines.push(...loadInto(`v${op.dst}`, name(op.base), op.field));
          break;
        case "call": {
          const j = earlier(op.fn);
          if (j !== undefined) lines.push(`\tv${op.dst} = f${j}(${name(op.a)}, ${name(op.b)})`);
          break;
        }
        case "fnval": {
          const j = earlier(op.fn);
          if (j !== undefined) lines.push(`\tv${op.dst} = f${j}`);
          break;
        }
        case "callvar":
          lines.push(
            `\tif fn, ok := ${name(op.via)}.(func(any, any) any); ok {`,
            `\t\tv${op.dst} = fn(${name(op.a)}, ${name(op.b)})`,
            "\t} else {",
            `\t\tv${op.dst} = nil`,
            "\t}",
          );
          break;
        case "probe":
          lines.push(`\tprobe(${++probe}, ${name(op.v)})`);
          if (loadedOnly.has(name(op.v))) viaHeap.add(probe);
          break;
        case "heap":
          lines.push(...storeInto(name(op.base), op.field, name(op.src)), ...loadInto("h", name(op.base), op.field));
          break;
        case "roundtrip":
          lines.push(...storeInto(name(op.base), op.field, name(op.src)), ...loadInto(`v${op.dst}`, name(op.base), op.field));
          break;
        case "indirect": {
          const j = earlier(op.fn);
          if (j === undefined) {
            lines.push("\t_, _ = g, k"); // f0 has no earlier function to call
            break;
          }
          lines.push(`\tg = f${j}`, "\tif fn, ok := g.(func(any, any) any); ok {", "\t\tk = fn(v0, v1)", "\t}");
          lines.push(`\tprobe(${++probe}, g)`, `\tprobe(${++probe}, k)`);
          viaIndirect.add(probe);
          break;
        }
      }
    }
    lines.push(`\treturn ${name(f.ret)}`, "}", "");
  });
  return { text: `${lines.join("\n")}\n`, viaHeap, viaIndirect };
}

function harness(last: number): string {
  return `package main

import (
	"encoding/json"
	"fmt"
	"reflect"
	"runtime"
	"strings"
)

var depth int

// enter bounds recursion through function values, which the generator allows.
func enter() bool {
	depth++
	return depth > 40
}

func leave() { depth-- }

var seen [][2]any

func probe(k int, v any) {
	switch x := v.(type) {
	case *O:
		seen = append(seen, [2]any{k, fmt.Sprintf("o:%d", x.s)})
	case func(any, any) any:
		name := runtime.FuncForPC(reflect.ValueOf(x).Pointer()).Name()
		seen = append(seen, [2]any{k, "fn:" + name[strings.LastIndex(name, ".")+1:]})
	}
}

func main() {
	f${last}(nil, nil)
	b, _ := json.Marshal(seen)
	fmt.Println(string(b))
}
`;
}

test("P5-go: every object a Go variable holds at run time is in its points-to set", { skip: SKIP }, () => {
  // Guards count runs, so the default run count can be sized from per-run rates.
  let crossFunction = 0;
  let functionValues = 0;
  let throughHeap = 0;
  let throughIndirect = 0;
  let observations = 0;
  fc.assert(
    fc.property(arbProgram, (program) => {
      const { text, viaHeap, viaIndirect } = render(program);
      const dir = tempDir("p5-go");
      writeFiles(dir, { "go.mod": "module example.com/p5\n\ngo 1.26\n", "p.go": text, "main.go": harness(program.length - 1) });
      const run = spawnSync("go", ["run", "."], { cwd: dir, encoding: "utf8", maxBuffer: 1 << 26 });
      assert.equal(run.status, 0, `${run.stderr}\n${text}`);
      const seen = (JSON.parse(run.stdout) ?? []) as [number, string][];
      if (seen.length === 0) {
        fs.rmSync(dir, { recursive: true, force: true });
        return;
      }
      const out = path.join(dir, "out");
      const { tables } = extractGo(dir, { out, layers: ["refs", "dataflow"] });

      const lines = text.split("\n");
      const probeVar = new Map<number, string>();
      const actual = new Map(tables.rows("actual").filter((a) => a.index === 1).map((a) => [a.call_site, a.var as string]));
      for (const cs of tables.rows("call_site")) {
        if (cs.file !== "p.go" || cs.callee_name !== "probe") continue;
        const m = /^probe\((\d+),/.exec((lines[(cs.line as number) - 1] ?? "").slice((cs.col as number) - 1));
        const v = actual.get(cs.id);
        if (m !== null && v !== undefined) probeVar.set(Number(m[1]), v);
      }
      const labelOf = new Map<number, string>();
      for (const a of tables.rows("alloc")) {
        if (a.kind === "function") labelOf.set(a.site as number, `fn:${String(a.fn_target).replace(/^p\.go#/, "")}`);
        const m = /&O\{s: (\d+)\}/.exec(a.file === "p.go" ? (lines[(a.line as number) - 1] ?? "") : "");
        if (a.kind === "instance" && m !== null) labelOf.set(a.site as number, `o:${m[1]}`);
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
      let runCross = false;
      let runFunction = false;
      let runHeap = false;
      let runIndirect = false;
      for (const [k, label] of seen) {
        const v = probeVar.get(k);
        assert.ok(v !== undefined, `probe ${k} has no argument variable\n${text}`);
        assert.ok(derived.has(`${v} ${label}`), `probe ${k}: ${v} held ${label} at run time; derived only ${[...derived].filter((d) => d.startsWith(`${v} `))}\n${text}`);
        observations++;
        if (label.startsWith("fn:")) runFunction = true;
        if (viaHeap.has(k)) runHeap = true;
        if (viaIndirect.has(k) && label.startsWith("o:")) runIndirect = true;
        const probeLine = lines.findIndex((l) => l.includes(`probe(${k}, `));
        const probeFn = lines.findLastIndex((l, i) => i <= probeLine && /^func f\d+\(/.test(l));
        const allocLine = label.startsWith("o:") ? lines.findIndex((l) => l.includes(`&O{s: ${label.slice(2)}}`)) : -1;
        const allocFn = lines.findLastIndex((l, i) => i <= allocLine && /^func f\d+\(/.test(l));
        if (allocLine >= 0 && allocFn !== probeFn) runCross = true;
      }
      if (runCross) crossFunction++;
      if (runFunction) functionValues++;
      if (runHeap) throughHeap++;
      if (runIndirect) throughIndirect++;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  if (process.env.CODE_FACTS_RATES !== undefined) {
    console.error(`P5-go rates over ${RUNS} runs: cross-function ${crossFunction}, function values ${functionValues}, through the heap ${throughHeap}, through a function value ${throughIndirect}, observations ${observations}`);
  }
  // Non-vacuity against the sentence: objects observed after crossing a call,
  // function values observed, and values that can only have come through a field.
  assert.ok(observations >= RUNS, `only ${observations} observations`);
  assert.ok(crossFunction >= 1, "no object was observed outside the function that allocated it");
  assert.ok(functionValues >= 1, "no function value was observed");
  assert.ok(throughHeap >= 1, "no observed value can only have come through a field");
  assert.ok(throughIndirect >= 1, "no object was observed returned through a call of a function value");
});
