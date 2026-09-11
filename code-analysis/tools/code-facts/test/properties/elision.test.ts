// P8 — import elision: `imports.runtime` is true exactly when TypeScript's own
// rule says the statement survives into JavaScript. The oracle restates the rule
// as the handbook gives it, without asking the emitter:
//
//   - a side-effect import (`import "x"`) survives; `import type` never does;
//   - under `verbatimModuleSyntax`, every other import survives as written — an
//     import whose names are all `type`-marked still loads the module (`import {}`);
//   - otherwise an import survives iff some binding not marked `type` is used as
//     a value; a use in a type position, `typeof` included, keeps nothing.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import { test } from "node:test";
import fc from "fast-check";
import { extract, tempDir, writeProject } from "../helpers.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "40", 10);

// What lib.ts exports: classes are values and types, interfaces only types,
// consts only values (as types, only through `typeof`).
const EXPORTS = [
  { name: "C0", kind: "class" },
  { name: "C1", kind: "class" },
  { name: "I0", kind: "interface" },
  { name: "V0", kind: "const" },
] as const;
type Use = "value" | "type" | "unused";

interface Binding {
  export: number;
  inlineType: boolean;
  use: Use;
}
type Stmt =
  | { form: "named"; bindings: Binding[] }
  | { form: "namespace"; use: Use }
  | { form: "type_only"; bindings: Binding[] }
  | { form: "side_effect" };

const arbBinding = fc.record({ export: fc.nat(EXPORTS.length - 1), inlineType: fc.boolean(), use: fc.constantFrom<Use>("value", "type", "unused") });
const arbStmt: fc.Arbitrary<Stmt> = fc.oneof(
  fc.record({ form: fc.constant("named" as const), bindings: fc.array(arbBinding, { minLength: 1, maxLength: 3 }) }),
  fc.record({ form: fc.constant("namespace" as const), use: fc.constantFrom<Use>("value", "type", "unused") }),
  fc.record({ form: fc.constant("type_only" as const), bindings: fc.array(arbBinding, { minLength: 1, maxLength: 2 }) }),
  fc.constant({ form: "side_effect" as const }),
);
const arbProject = fc.record({
  vms: fc.boolean(),
  files: fc.array(fc.array(arbStmt, { minLength: 1, maxLength: 3 }), { minLength: 1, maxLength: 3 }),
});

/** Makes a binding well-typed: an interface cannot be used as a value, and under
 * verbatimModuleSyntax must be imported with `type`; a type-only import's names
 * cannot be used as values. */
function legal(b: Binding, vms: boolean, typeOnlyImport: boolean): Binding {
  const e = EXPORTS[b.export];
  let { inlineType, use } = b;
  if (typeOnlyImport) inlineType = false;
  if ((e?.kind === "interface" || typeOnlyImport || inlineType) && use === "value") use = "type";
  if (e?.kind === "interface" && vms && !typeOnlyImport) inlineType = true;
  return { export: b.export, inlineType, use };
}

function useOf(local: string, kind: string, use: Use, k: number): string | undefined {
  if (use === "unused") return undefined;
  if (use === "value") return `  void ${local};`;
  return kind === "const" ? `  let t${k}: typeof ${local} | undefined; void t${k};` : `  let t${k}: ${local} | undefined; void t${k};`;
}

function render(files: Stmt[][], vms: boolean): { sources: Record<string, string>; expected: Map<string, boolean> } {
  const sources: Record<string, string> = {
    "src/lib.ts": "export class C0 {}\nexport class C1 {}\nexport interface I0 { x: number }\nexport const V0 = 1;\n",
  };
  const expected = new Map<string, boolean>();
  files.forEach((stmts, f) => {
    const imports: string[] = [];
    const body: string[] = [];
    let k = 0;
    stmts.forEach((st, s) => {
      const line = imports.length + 1;
      let survives: boolean;
      if (st.form === "side_effect") {
        imports.push(`import "./lib.js";`);
        survives = true;
      } else if (st.form === "namespace") {
        const ns = `ns${s}`;
        imports.push(`import * as ${ns} from "./lib.js";`);
        const use = st.use === "value" ? `  void ${ns}.V0;` : st.use === "type" ? `  let t${k}: ${ns}.I0 | undefined; void t${k};` : undefined;
        k++;
        if (use !== undefined) body.push(use);
        survives = vms || st.use === "value";
      } else {
        const typeOnly = st.form === "type_only";
        const bindings = st.bindings.map((b) => legal(b, vms, typeOnly));
        const specs = bindings.map((b, i) => `${b.inlineType ? "type " : ""}${EXPORTS[b.export]?.name} as b${s}_${i}`);
        imports.push(`import ${typeOnly ? "type " : ""}{ ${specs.join(", ")} } from "./lib.js";`);
        bindings.forEach((b, i) => {
          const use = useOf(`b${s}_${i}`, EXPORTS[b.export]?.kind ?? "", b.use, k++);
          if (use !== undefined) body.push(use);
        });
        survives = typeOnly ? false : vms || bindings.some((b) => !b.inlineType && b.use === "value");
      }
      expected.set(`src/f${f}.ts:${line}`, survives);
    });
    sources[`src/f${f}.ts`] = `${imports.join("\n")}\nexport function run(): void {\n${body.join("\n")}\n}\n`;
  });
  return { sources, expected };
}

test("P8: an import is runtime exactly when TypeScript's elision rule keeps it", () => {
  const seen = new Set<string>();
  fc.assert(
    fc.property(arbProject, ({ vms, files }) => {
      const { sources, expected } = render(files, vms);
      const dir = tempDir("p8");
      writeProject(dir, sources, {
        compilerOptions: { module: "esnext", moduleResolution: "bundler", strict: true, noEmit: true, noLib: true, types: [], verbatimModuleSyntax: vms },
        files: Object.keys(sources),
      });
      const { tables } = extract(dir, { layers: [] });
      const got = new Map(tables.rows("imports").map((r) => [`${r.file}:${r.line}`, r.runtime]));
      assert.deepEqual(got, expected, `verbatimModuleSyntax: ${vms}\n${Object.entries(sources).map(([p, t]) => `// ${p}\n${t}`).join("\n")}`);
      for (const r of tables.rows("imports")) seen.add(`${vms ? "vms" : "plain"}:${r.kind}:${r.runtime}`);
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  // Non-vacuity against the sentence: both outcomes of a value import in both
  // modes — including the case syntax cannot see, a plain import elided.
  for (const k of ["plain:static:false", "plain:static:true", "vms:static:true", "plain:type_only:false"]) {
    assert.ok(seen.has(k), `never saw ${k}; saw ${[...seen]}`);
  }
});
