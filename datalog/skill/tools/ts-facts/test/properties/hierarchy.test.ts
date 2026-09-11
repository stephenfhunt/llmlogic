// P4 — hierarchies: for generated classes and interfaces spread over files, the
// extends / implements / overrides facts are exactly the model's.
//
// The oracle restates TypeScript's member lookup rather than asking the checker:
// a type's member `m` is its own declaration, else — for a class — its extends
// base's, else — for an interface — the first extended interface's that has one.
// (`implements` contributes no members to a class type.)

import assert from "node:assert/strict";
import * as fs from "node:fs";
import { test } from "node:test";
import fc from "fast-check";
import { extract, tempDir, writeProject } from "../helpers.ts";

const RUNS = Number.parseInt(process.env.TS_FACTS_RUNS ?? "25", 10);

interface TypeModel {
  isClass: boolean;
  extends: number[]; // a class: at most one earlier class; an interface: earlier interfaces
  implements: number[]; // classes only: earlier interfaces
  members: number[]; // indices into m0..m3
}

const arbHierarchy: fc.Arbitrary<TypeModel[]> = fc
  .array(
    fc.record({
      isClass: fc.boolean(),
      picks: fc.array(fc.nat(20), { minLength: 3, maxLength: 3 }),
      members: fc.uniqueArray(fc.integer({ min: 0, max: 3 }), { maxLength: 3 }),
    }),
    { minLength: 2, maxLength: 7 },
  )
  .map((raw) =>
    raw.map((r, i) => {
      const earlierClasses = raw.slice(0, i).flatMap((x, k) => (x.isClass ? [k] : []));
      const earlierIfaces = raw.slice(0, i).flatMap((x, k) => (x.isClass ? [] : [k]));
      const pick = (from: number[], n: number) => from.filter((_, k) => ((r.picks[k % 3] ?? 0) >> k) % 2 === 1).slice(0, n);
      const [p0 = 0] = r.picks;
      return r.isClass
        ? {
            isClass: true,
            extends: earlierClasses.length > 0 && p0 % 2 === 0 ? [earlierClasses[p0 % earlierClasses.length] ?? 0] : [],
            implements: pick(earlierIfaces, 2),
            members: r.members.sort(),
          }
        : { isClass: false, extends: pick(earlierIfaces, 2), implements: [], members: r.members.sort() };
    }),
  );

function render(model: TypeModel[]): Record<string, string> {
  const out: Record<string, string> = {};
  model.forEach((t, i) => {
    const bases = [...t.extends, ...t.implements];
    const lines = [...new Set(bases)].map((k) => `import type { T${k} } from "./t${k}.js";`);
    const ext = t.extends.length > 0 ? ` extends ${t.extends.map((k) => `T${k}`).join(", ")}` : "";
    const impl = t.implements.length > 0 ? ` implements ${t.implements.map((k) => `T${k}`).join(", ")}` : "";
    if (t.isClass) {
      lines.push(`export class T${i}${ext}${impl} {`, ...t.members.map((m) => `  m${m}(): void {}`), "}");
    } else {
      lines.push(`export interface T${i}${ext} {`, ...t.members.map((m) => `  m${m}(): void;`), "}");
    }
    out[`src/t${i}.ts`] = `${lines.join("\n")}\n`;
  });
  return out;
}

/** Where member `m` of type `k` is declared, by TypeScript's lookup (see header). */
function lookup(model: TypeModel[], k: number, m: number): number | undefined {
  const t = model[k];
  if (t === undefined) return undefined;
  if (t.members.includes(m)) return k;
  for (const b of t.extends) {
    const found = lookup(model, b, m);
    if (found !== undefined) return found;
  }
  return undefined;
}

const id = (k: number) => `src/t${k}.ts#T${k}`;
const sortRows = (xs: string[][]) => xs.map((x) => x.join(" ")).sort();

test("P4: extends, implements and overrides are exactly the generated hierarchy's", () => {
  let overridesSeen = 0;
  let throughInterface = 0;
  fc.assert(
    fc.property(arbHierarchy, (model) => {
      const dir = tempDir("p4");
      const files = render(model);
      writeProject(dir, files, { compilerOptions: { module: "nodenext", moduleResolution: "nodenext", strict: true, noEmit: true, noLib: true, types: [] }, files: Object.keys(files) });
      const { tables } = extract(dir, { layers: ["refs"] });

      const ext: string[][] = [];
      const impl: string[][] = [];
      const over: string[][] = [];
      model.forEach((t, i) => {
        for (const b of t.extends) ext.push([id(i), id(b)]);
        for (const b of t.implements) impl.push([id(i), id(b)]);
        for (const m of t.members) {
          for (const b of [...t.extends, ...t.implements]) {
            const at = lookup(model, b, m);
            if (at !== undefined) over.push([`${id(i)}.m${m}`, `${id(at)}.m${m}`]);
          }
        }
      });
      assert.deepEqual(sortRows(tables.rows("extends").map((r) => [r.child as string, r.parent as string])), sortRows(ext));
      assert.deepEqual(sortRows(tables.rows("implements").map((r) => [r.class as string, r.interface as string])), sortRows(impl));
      const got = [...new Set(sortRows(tables.rows("overrides").map((r) => [r.member as string, r.base as string])))];
      assert.deepEqual(got, [...new Set(sortRows(over))]);
      overridesSeen += over.length > 0 ? 1 : 0;
      throughInterface += over.some(([, b]) => model[Number(/t(\d+)\.ts/.exec(b ?? "")?.[1])]?.isClass === false) ? 1 : 0;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  assert.ok(overridesSeen >= RUNS / 4, `only ${overridesSeen}/${RUNS} runs had an override`);
  assert.ok(throughInterface >= 1, "no run overrode an interface member");
});
