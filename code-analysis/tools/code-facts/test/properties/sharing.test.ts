// P9 — shared parsing: giving every tsconfig that would parse and bind a file the
// same way one parsed copy of it (`program.ts`, `shareParsedFiles`) changes no
// output byte.

import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { extract, snapshotDir, tempDir, writeFiles } from "../helpers.ts";
import { arbProject, normalizeCalls, render } from "./modgen.ts";

const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "25", 10);

// noLib, as in P2 and P6: the standard library would dominate the run.
const BASE = { module: "nodenext", moduleResolution: "nodenext", strict: true, noEmit: true, noLib: true, types: [] };

// `paths` differs from `base` only where module resolution looks, so the two
// share parsed files; `es2017` parses at another language version, and
// `moduleDetection: legacy` (drawn separately) binds a file with no imports as a
// script — under `nodenext` the default binds every `.ts` file here as a module —
// so each must get files of its own.
const VARIANTS = {
  base: BASE,
  paths: { ...BASE, paths: { "#src/*": ["./src/*"] } },
  es2017: { ...BASE, target: "es2017" },
};
type Variant = keyof typeof VARIANTS;

interface Config {
  variant: Variant;
  legacy: boolean;
  files: string[];
}

const sharingGroup = (c: Config): string => `${c.variant === "paths" ? "base" : c.variant}|${c.legacy}`;

// A script declares a global, and a module calls it. Bound as a script the call
// resolves; bound as a module it does not — so a program handed a copy bound under
// the other `moduleDetection` changes `ref` and `unresolved_ref`. Nothing imports
// either file, so a config holds them exactly when it lists them: every config
// lists the script and only the last lists its caller, so the caller's program
// takes the script from the first config whenever their detections differ (a
// random placement made that case 4% of runs).
const SCRIPT = "src/script.ts";
const USER = "src/user.ts";
const SCRIPT_FILES = {
  [SCRIPT]: "function g(): number {\n  return 1;\n}\n",
  [USER]: "export function useG(): number {\n  return g();\n}\n",
};

const arbConfigs = (paths: string[]): fc.Arbitrary<Config[]> =>
  fc
    .array(
      fc.record({
        variant: fc.constantFrom<Variant>("base", "paths", "es2017"),
        legacy: fc.boolean(),
        files: fc.subarray(paths),
      }),
      { minLength: 2, maxLength: 3 },
    )
    .map((configs) => configs.map((c, i) => ({ ...c, files: [...c.files, SCRIPT, ...(i === configs.length - 1 ? [USER] : [])] })));

test("P9: sharing parsed files across tsconfigs changes no output byte", (t) => {
  let shared = 0;
  let crossDetection = 0;
  fc.assert(
    fc.property(
      arbProject.chain((raw) => {
        const model = render(normalizeCalls(raw));
        return fc.record({ files: fc.constant({ ...model, ...SCRIPT_FILES }), configs: arbConfigs(Object.keys(model)) });
      }),
      ({ files, configs }) => {
        const dir = tempDir("p9");
        writeFiles(dir, files);
        const tsconfigs = configs.map((c, i) => {
          const p = path.join(dir, `tsconfig.p${i}.json`);
          const compilerOptions = c.legacy ? { ...VARIANTS[c.variant], moduleDetection: "legacy" } : VARIANTS[c.variant];
          fs.writeFileSync(p, JSON.stringify({ compilerOptions, files: c.files }));
          return p;
        });
        const on = path.join(dir, "on");
        const off = path.join(dir, "off");
        extract(dir, { tsconfigs, out: on, layers: ["refs", "flow", "dataflow", "quality"], shareSourceFiles: true });
        extract(dir, { tsconfigs, out: off, layers: ["refs", "flow", "dataflow", "quality"], shareSourceFiles: false });
        assert.deepEqual(snapshotDir(on), snapshotDir(off));

        const pairs = configs.flatMap((a, i) => configs.slice(i + 1).map((b) => [a, b] as const));
        if (pairs.some(([a, b]) => sharingGroup(a) === sharingGroup(b) && a.files.some((f) => b.files.includes(f)))) shared++;
        // The case a key without `moduleDetection` gets wrong: the program that
        // extracts the caller (the first to list it) holds the script, and the
        // script was first parsed by a program with the other detection.
        const firstScript = configs.find((c) => c.files.includes(SCRIPT));
        const firstUser = configs.find((c) => c.files.includes(USER));
        if (firstScript !== undefined && firstUser !== undefined && firstUser.files.includes(SCRIPT) && firstScript.legacy !== firstUser.legacy) {
          crossDetection++;
        }
        fs.rmSync(dir, { recursive: true, force: true });
      },
    ),
    { numRuns: RUNS },
  );
  t.diagnostic(`shared ${shared}/${RUNS}, cross-detection ${crossDetection}/${RUNS}`);
  assert.ok(shared >= RUNS / 10, `only ${shared}/${RUNS} runs listed one file in two configs that share parsing`);
  assert.ok(crossDetection >= RUNS / 10, `only ${crossDetection}/${RUNS} runs handed the script's caller a script parsed under the other moduleDetection`);
});
