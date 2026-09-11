// P4-py — hierarchies: for generated classes spread over modules, with multiple
// inheritance, the extends and overrides facts are exactly what the interpreter
// says.
//
// The oracle is Python itself, not a restatement of its rules: the generated
// package is imported by `python3`, which reports each class's `__bases__` and,
// for each member a class defines and each direct base, the class the base's
// `__mro__` finds that member on. So the property checks C3 linearization without
// the test having to implement it — and a breadth-first or depth-first lookup
// in the frontend disagrees with it on the diamonds the generator draws.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import { test } from "node:test";
import fc from "fast-check";
import { extractPython, tempDir, writeFiles } from "../helpers.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "25", 10);
const PYTHON = process.env.CODE_FACTS_PYTHON ?? "python3";

interface ClassModel {
  bases: number[]; // earlier classes, in declaration order
  members: number[]; // m0..m3 defined in the body
  viaModule: boolean; // spell bases as `tK_mod.TK` instead of importing the name
}

// Shaped so the case that matters is common: bases listed most-derived first
// (C3's local-precedence rule, so few hierarchies are rejected) except that
// sometimes two unrelated ones are swapped, usually two or three of them, and
// two member names at density 1/2 so lookups collide.
// Measured over 200 runs: 54 had a lookup where C3 and breadth-first disagree,
// and 8 drew a hierarchy Python rejects.
const arbHierarchy: fc.Arbitrary<ClassModel[]> = fc
  .array(
    fc.record({
      count: fc.integer({ min: 2, max: 3 }),
      picks: fc.array(fc.nat(1000), { minLength: 3, maxLength: 3 }),
      swap: fc.integer({ min: 0, max: 3 }),
      members: fc.tuple(fc.boolean(), fc.boolean()),
      viaModule: fc.boolean(),
    }),
    { minLength: 10, maxLength: 12 },
  )
  .map((raw) => {
    const ancestors: Set<number>[] = [];
    return raw.map((r, i) => {
      const pool = Array.from({ length: i }, (_, k) => k);
      const chosen: number[] = [];
      for (const p of r.picks.slice(0, Math.min(i, r.count))) chosen.push(...pool.splice(p % pool.length, 1));
      chosen.sort((a, b) => b - a);
      const [first, second] = chosen;
      if (r.swap === 0 && first !== undefined && second !== undefined && !ancestors[first]?.has(second)) {
        [chosen[0], chosen[1]] = [second, first];
      }
      ancestors.push(new Set(chosen.flatMap((b) => [b, ...(ancestors[b] ?? [])])));
      return {
        bases: chosen,
        members: r.members.flatMap((on, m) => (on ? [m] : [])),
        viaModule: r.viaModule,
      };
    });
  });

function render(model: ClassModel[]): Record<string, string> {
  const out: Record<string, string> = { "src/__init__.py": "" };
  model.forEach((c, i) => {
    const lines: string[] = [];
    const spelled = c.bases.map((k) => {
      if (c.viaModule) {
        lines.push(`import src.t${k} as t${k}_mod`);
        return `t${k}_mod.T${k}`;
      }
      lines.push(`from src.t${k} import T${k}`);
      return `T${k}`;
    });
    lines.push("", `class T${i}${spelled.length > 0 ? `(${spelled.join(", ")})` : ""}:`);
    if (c.members.length === 0) lines.push("    pass");
    for (const m of c.members) lines.push(`    def m${m}(self):`, "        pass");
    out[`src/t${i}.py`] = `${lines.join("\n")}\n`;
  });
  return out;
}

// Imports every generated module; prints {"extends": [...], "overrides": [...], "bfs_differs": n}
// or {"invalid": "..."} when Python rejects the hierarchy (no consistent MRO).
const ORACLE = String.raw`
import importlib, json, os, sys
root, n = sys.argv[1], int(sys.argv[2])
sys.path.insert(0, root)
def cid(c):
    return os.path.relpath(sys.modules[c.__module__].__file__, root).replace(os.sep, "/") + "#" + c.__qualname__
def bfs_find(c, m):
    queue, seen = [c], []
    while queue:
        k = queue.pop(0)
        if k in seen or k is object: continue
        seen.append(k)
        if m in vars(k): return k
        queue.extend(k.__bases__)
try:
    classes = [getattr(importlib.import_module(f"src.t{i}"), f"T{i}") for i in range(n)]
except TypeError as e:
    print(json.dumps({"invalid": str(e)})); sys.exit(0)
ext, over, differs = [], [], 0
for c in classes:
    for b in c.__bases__:
        if b is not object: ext.append([cid(c), cid(b)])
    for m in [k for k in vars(c) if k.startswith("m") and k[1:].isdigit()]:
        for b in c.__bases__:
            at = next((k for k in b.__mro__ if m in vars(k)), None)
            if at is not None and at is not object:
                over.append([cid(c) + "." + m, cid(at) + "." + m])
                differs += bfs_find(b, m) is not at
print(json.dumps({"extends": ext, "overrides": over, "bfs_differs": differs}))
`;

const sortRows = (xs: string[][]) => [...new Set(xs.map((x) => x.join(" ")))].sort();

test("P4-py: extends and overrides are exactly what the interpreter's MRO says", () => {
  let overridesSeen = 0;
  let multiple = 0;
  let orderMatters = 0;
  let invalid = 0;
  fc.assert(
    fc.property(arbHierarchy, (model) => {
      const dir = tempDir("p4py");
      writeFiles(dir, render(model));
      const py = spawnSync(PYTHON, ["-c", ORACLE, dir, String(model.length)], { encoding: "utf8" });
      assert.equal(py.status, 0, py.stderr);
      const truth = JSON.parse(py.stdout) as { invalid?: string; extends: string[][]; overrides: string[][]; bfs_differs: number };
      if (truth.invalid !== undefined) {
        // Python refuses the hierarchy; what the facts should say is undefined.
        invalid++;
        fs.rmSync(dir, { recursive: true, force: true });
        return;
      }
      const { tables } = extractPython(dir, { layers: ["refs"] });
      assert.deepEqual(sortRows(tables.rows("extends").map((r) => [r.child as string, r.parent as string])), sortRows(truth.extends));
      assert.deepEqual(sortRows(tables.rows("overrides").map((r) => [r.member as string, r.base as string])), sortRows(truth.overrides));
      overridesSeen += truth.overrides.length > 0 ? 1 : 0;
      multiple += model.some((c) => c.bases.length > 1) ? 1 : 0;
      orderMatters += truth.bfs_differs > 0 ? 1 : 0;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  // Non-vacuity: overrides happened, multiple inheritance happened, and some
  // lookup came out differently under C3 than breadth-first — the case the
  // header says this property exists for. Rejected hierarchies stay a minority.
  assert.ok(overridesSeen >= RUNS / 4, `only ${overridesSeen}/${RUNS} runs had an override`);
  assert.ok(multiple >= RUNS / 4, `only ${multiple}/${RUNS} runs had multiple inheritance`);
  assert.ok(orderMatters >= 1, "no run had a lookup where C3 and breadth-first disagree");
  assert.ok(invalid <= RUNS / 2, `${invalid}/${RUNS} hierarchies were rejected by Python`);
});
