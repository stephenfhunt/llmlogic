// P4-go — hierarchies for Go: `implements` and `overrides` equal what the
// compiled program's own method sets say, and `extends` and `embeds` equal the
// generated embedding.
//
// Go writes no `implements`: the extractor asks go/types. The oracle is the
// running program instead. reflect says whether *T (and T) implement I, and each
// method called through reflect prints the declaration that ran — the method
// that satisfies an interface's, or that an embedded type's is hidden by.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import { test } from "node:test";
import fc from "fast-check";
import { extractGo, goAvailable, tempDir, writeFiles } from "../helpers.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "25", 10);
const NO_GO = goAvailable() ? false : "needs the `go` command";
const METHODS = ["M0", "M1", "M2", "M3"];

interface Iface {
  own: string[];
  embeds: number[];
}
interface Type {
  /** [method, pointer receiver] */
  declares: [string, boolean][];
  /** [embedded type, embedded as a pointer] — lower indices only, so no cycle */
  embeds: [number, boolean][];
  /** declared `T[X any]`, and used as `T[int]` */
  generic: boolean;
}
interface Model {
  ifaces: Iface[];
  types: Type[];
}

/** An interface's whole method set, each method with the interface that lists it. */
function methodsOf(ifaces: Iface[], i: number): Map<string, number> {
  const out = new Map<string, number>();
  const it = ifaces[i];
  if (it === undefined) return out;
  for (const e of it.embeds) for (const [m, d] of methodsOf(ifaces, e)) out.set(m, d);
  for (const m of it.own) out.set(m, i);
  return out;
}

function normalize(raw: Model): Model {
  const ifaces: Iface[] = [];
  raw.ifaces.forEach((r, i) => {
    const embeds: number[] = [];
    const seen = new Set<string>();
    for (const e of [...new Set(r.embeds)].filter((e) => e < i).sort()) {
      const ms = [...methodsOf(ifaces, e).keys()];
      // Two embedded interfaces listing one method: which lists it is ambiguous.
      if (ms.some((m) => seen.has(m))) continue;
      embeds.push(e);
      for (const m of ms) seen.add(m);
    }
    let own = r.own.filter((m) => !seen.has(m));
    if (own.length === 0 && embeds.length === 0) own = ["M0"];
    ifaces.push({ own, embeds });
  });
  const types = raw.types.map((t, k) => ({
    declares: [...new Map(t.declares).entries()],
    embeds: [...new Map(t.embeds.filter(([e]) => e < k)).entries()],
    generic: t.generic,
  }));
  return { ifaces, types };
}

const arbModel: fc.Arbitrary<Model> = fc
  .record({
    ifaces: fc.array(fc.record({ own: fc.subarray(METHODS), embeds: fc.array(fc.nat(2), { maxLength: 2 }) }), { minLength: 1, maxLength: 3 }),
    types: fc.array(
      fc.record({
        declares: fc.array(fc.tuple(fc.constantFrom(...METHODS), fc.boolean()), { maxLength: 4 }),
        embeds: fc.array(fc.tuple(fc.nat(3), fc.boolean()), { maxLength: 2 }),
        generic: fc.boolean(),
      }),
      { minLength: 2, maxLength: 4 },
    ),
  })
  .map(normalize);

function render(m: Model): string {
  const lines = ["package main", "", "import (", '\t"fmt"', '\t"reflect"', ")", ""];
  m.ifaces.forEach((it, i) => {
    lines.push(`type I${i} interface {`, ...it.embeds.map((e) => `\tI${e}`), ...it.own.map((x) => `\t${x}()`), "}", "");
  });
  const use = (k: number) => `T${k}${m.types[k]?.generic ? "[int]" : ""}`;
  m.types.forEach((t, k) => {
    lines.push(`type T${k}${t.generic ? "[X any]" : ""} struct {`, ...t.embeds.map(([e, ptr]) => `\t${ptr ? "*" : ""}${use(e)}`), "}", "");
    const recv = `T${k}${t.generic ? "[X]" : ""}`;
    for (const [x, ptr] of t.declares) lines.push(`func (t ${ptr ? "*" : ""}${recv}) ${x}() { fmt.Println("ran T${k} ${x}") }`, "");
    const fields = t.embeds.map(([e, ptr]) => `T${e}: ${ptr ? "" : "*"}newT${e}()`).join(", ");
    lines.push(`func newT${k}() *${use(k)} { return &${use(k)}{${fields}} }`, "");
  });
  lines.push(
    "func main() {",
    `\tifaces := []reflect.Type{${m.ifaces.map((_, i) => `reflect.TypeOf((*I${i})(nil)).Elem()`).join(", ")}}`,
    `\tvalues := []any{${m.types.map((_, k) => `newT${k}()`).join(", ")}}`,
    "\tfor t, v := range values {",
    "\t\trv := reflect.ValueOf(v)",
    "\t\tfor i, it := range ifaces {",
    '\t\t\tfmt.Println("impl", t, i, rv.Type().Implements(it), rv.Elem().Type().Implements(it))',
    "\t\t}",
    `\t\tfor _, name := range []string{${METHODS.map((x) => `"${x}"`).join(", ")}} {`,
    "\t\t\tif mv := rv.MethodByName(name); mv.IsValid() {",
    '\t\t\t\tfmt.Println("call", t, name)',
    "\t\t\t\tmv.Call(nil)",
    "\t\t\t}",
    "\t\t}",
    "\t}",
    "}",
  );
  return `${lines.join("\n")}\n`;
}

interface Observed {
  /** "t i" pairs where *T implements I, and whether T itself does */
  impl: Map<string, boolean>;
  /** "t M" → the type whose declaration ran */
  ran: Map<string, number>;
}

function observe(dir: string): Observed {
  const r = spawnSync("go", ["run", "."], { cwd: dir, encoding: "utf8" });
  assert.equal(r.status, 0, r.stderr);
  const impl = new Map<string, boolean>();
  const ran = new Map<string, number>();
  const lines = r.stdout.split("\n");
  for (let n = 0; n < lines.length; n++) {
    const p = (lines[n] ?? "").split(" ");
    if (p[0] === "impl" && p[3] === "true") impl.set(`${p[1]} ${p[2]}`, p[4] === "true");
    if (p[0] === "call") {
      const q = (lines[n + 1] ?? "").split(" ");
      ran.set(`${p[1]} ${p[2]}`, Number((q[1] ?? "").slice(1)));
      n++;
    }
  }
  return { impl, ran };
}

const id = (name: string) => `h.go#${name}`;

function sortedUnique(rows: unknown[][]): string[] {
  return [...new Set(rows.map((r) => JSON.stringify(r)))].sort();
}

test("P4-go: implements and overrides are what the program's method sets say; extends and embeds are the embedding", { skip: NO_GO }, () => {
  let implemented = 0;
  let notImplemented = 0;
  let promoted = 0;
  let pointerOnly = 0;
  let embeddedIface = 0;
  let hidden = 0;
  let genericImplemented = 0;
  fc.assert(
    fc.property(arbModel, (m) => {
      const dir = tempDir("p4-go");
      writeFiles(dir, { "go.mod": "module example.com/p4\n\ngo 1.26\n", "h.go": render(m) });
      const seen = observe(dir);
      const { tables } = extractGo(dir, { layers: ["refs"] });

      const implements_: unknown[][] = [];
      const overrides: unknown[][] = [];
      let pairs = 0;
      m.types.forEach((_, t) => {
        m.ifaces.forEach((_, i) => {
          pairs++;
          const byValue = seen.impl.get(`${t} ${i}`);
          if (byValue === undefined) return;
          implements_.push([id(`T${t}`), id(`I${i}`), !byValue]);
          if (!byValue) pointerOnly++;
          if (m.types[t]?.generic) genericImplemented++;
          for (const [x, declarer] of methodsOf(m.ifaces, i)) {
            const r = seen.ran.get(`${t} ${x}`);
            assert.ok(r !== undefined, `*T${t} implements I${i} but has no ${x}`);
            if (r !== t) promoted++;
            overrides.push([id(`T${r}.${x}`), id(`I${declarer}.${x}`)]);
          }
        });
      });
      m.types.forEach((t, k) => {
        for (const [e] of t.embeds) {
          for (const [x] of t.declares) {
            const r = seen.ran.get(`${e} ${x}`);
            if (r === undefined) continue;
            overrides.push([id(`T${k}.${x}`), id(`T${r}.${x}`)]);
            hidden++;
          }
        }
      });
      implemented += implements_.length > 0 ? 1 : 0;
      notImplemented += implements_.length < pairs ? 1 : 0;
      embeddedIface += m.ifaces.some((it) => it.embeds.length > 0) ? 1 : 0;

      assert.deepEqual(sortedUnique(tables.rows("implements").map((r) => [r.class, r.interface, r.pointer])), sortedUnique(implements_));
      assert.deepEqual(sortedUnique(tables.rows("overrides").map((r) => [r.member, r.base])), sortedUnique(overrides));
      assert.deepEqual(
        sortedUnique(tables.rows("extends").map((r) => [r.child, r.parent])),
        sortedUnique(m.ifaces.flatMap((it, i) => it.embeds.map((e) => [id(`I${i}`), id(`I${e}`)]))),
      );
      assert.deepEqual(
        sortedUnique(tables.rows("embeds").map((r) => [r.outer, r.inner, r.pointer])),
        sortedUnique(m.types.flatMap((t, k) => t.embeds.map(([e, ptr]) => [id(`T${k}`), id(`T${e}`), ptr]))),
      );
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  // Non-vacuity, read against the sentence: types that implement and types that
  // do not, an interface satisfied by a promoted method and one only through the
  // pointer, an embedded interface, and a declaration hiding an embedded method.
  assert.ok(implemented >= 1 && notImplemented >= 1, `implemented ${implemented}, not ${notImplemented} of ${RUNS}`);
  assert.ok(promoted >= 1, "no interface was satisfied by a promoted method");
  assert.ok(pointerOnly >= 1, "no interface was satisfied only through the pointer");
  assert.ok(embeddedIface >= 1, "no interface embedded another");
  assert.ok(hidden >= 1, "no declaration hid an embedded method");
  // Acceptance for the generic widening: generic types compiled, and implemented.
  assert.ok(genericImplemented >= 1, "no generic type implemented an interface");
});
