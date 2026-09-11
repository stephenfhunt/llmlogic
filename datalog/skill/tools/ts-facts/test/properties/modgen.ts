// A generator of small multi-file projects whose module graph is known by
// construction: which file imports which names from where, through which
// re-export, and which function calls which. The model is the oracle; the
// source text is rendered from it.

import * as path from "node:path";
import fc from "fast-check";

export interface ImportModel {
  /** Index of the file imported from. */
  from: number;
  /** "named": `import { a, b }`; "namespace": `import * as ns`; "via": named, through a re-exporting barrel. */
  style: "named" | "namespace" | "via";
  /** Function indices (into the *origin* file's functions) that are imported. */
  names: number[];
}

export interface FileModel {
  path: string;
  fnCount: number;
  imports: ImportModel[];
  /** For each local function j: the calls it makes, as [file, fn] pairs (file === own index for a local call). */
  calls: [number, number][][];
  /** Functions re-exported by this file as a barrel: [file, fn]. */
  reexports: [number, number][];
}

export interface ProjectModel {
  files: FileModel[];
}

export function fnName(file: number, fn: number): string {
  return `f${file}_${fn}`;
}

export function specifier(fromPath: string, toPath: string): string {
  let rel = path.posix.relative(path.posix.dirname(fromPath), toPath).replace(/\.ts$/, ".js");
  if (!rel.startsWith(".")) rel = `./${rel}`;
  return rel;
}

const arbDir = fc.constantFrom("src", "src/a", "src/b", "src/a/deep");

export const arbProject: fc.Arbitrary<ProjectModel> = fc
  .record({
    dirs: fc.array(arbDir, { minLength: 2, maxLength: 6 }),
    fnCounts: fc.array(fc.integer({ min: 1, max: 3 }), { minLength: 6, maxLength: 6 }),
    seed: fc.array(fc.nat(1000), { minLength: 64, maxLength: 64 }),
  })
  .map(({ dirs, fnCounts, seed }) => {
    let s = 0;
    const next = (n: number) => (n <= 0 ? 0 : (seed[s++ % seed.length] ?? 0) % n);
    const n = dirs.length;
    const files: FileModel[] = dirs.map((d, i) => ({
      path: `${d}/m${i}.ts`,
      fnCount: fnCounts[i] ?? 1,
      imports: [],
      calls: [],
      reexports: [],
    }));
    // The last file is sometimes a barrel re-exporting the others' first functions.
    const barrel = n >= 3 && next(2) === 0 ? n - 1 : -1;
    if (barrel >= 0) {
      const fb = files[barrel];
      if (fb !== undefined) for (let k = 0; k < barrel; k++) if (next(2) === 0) fb.reexports.push([k, 0]);
    }
    files.forEach((f, i) => {
      if (i === barrel) return;
      for (let k = 0; k < n; k++) {
        if (k === i || k === barrel || next(3) !== 0) continue;
        const origin = files[k];
        if (origin === undefined) continue;
        const style = next(3) === 0 ? "namespace" : "named";
        const names = [...new Set([next(origin.fnCount), next(origin.fnCount)])].sort();
        f.imports.push({ from: k, style, names });
      }
      const fb = barrel >= 0 ? files[barrel] : undefined;
      if (fb !== undefined && fb.reexports.length > 0 && next(2) === 0) {
        const pick = fb.reexports.filter(([k]) => k !== i).map(([k]) => k);
        if (pick.length > 0) f.imports.push({ from: barrel, style: "via", names: [...new Set(pick)].sort() });
      }
      for (let j = 0; j < f.fnCount; j++) {
        const calls: [number, number][] = [];
        for (const imp of f.imports) {
          if (imp.style === "via") {
            for (const k of imp.names) if (next(2) === 0) calls.push([k, 0]);
          } else {
            for (const fn of imp.names) if (next(2) === 0) calls.push([imp.from, fn]);
          }
        }
        if (j > 0 && next(2) === 0) calls.push([i, next(j)]);
        f.calls.push(calls);
      }
    });
    return { files };
  });

/** Renders the model as source files. A namespace import of file k is bound as `ns{k}`. */
export function render(model: ProjectModel): Record<string, string> {
  const out: Record<string, string> = {};
  model.files.forEach((f, i) => {
    const lines: string[] = [];
    for (const imp of f.imports) {
      const from = model.files[imp.from];
      if (from === undefined) continue;
      const spec = specifier(f.path, from.path);
      if (imp.style === "namespace") lines.push(`import * as ns${imp.from} from "${spec}";`);
      else if (imp.style === "named") lines.push(`import { ${imp.names.map((j) => fnName(imp.from, j)).join(", ")} } from "${spec}";`);
      else lines.push(`import { ${imp.names.map((k) => fnName(k, 0)).join(", ")} } from "${spec}";`);
    }
    for (const [k, j] of f.reexports) {
      const from = model.files[k];
      if (from !== undefined) lines.push(`export { ${fnName(k, j)} } from "${specifier(f.path, from.path)}";`);
    }
    for (let j = 0; j < f.fnCount && f.reexports.length === 0; j++) {
      const body = (f.calls[j] ?? []).map(([k, fn]) => {
        const imp = f.imports.find((x) => x.from === k && x.style === "namespace");
        return imp !== undefined && k !== i ? `  ns${k}.${fnName(k, fn)}();` : `  ${fnName(k, fn)}();`;
      });
      lines.push(`export function ${fnName(i, j)}(): void {`, ...body, "}");
    }
    if (f.reexports.length === 0 && f.fnCount === 0) lines.push("export {};");
    out[f.path] = `${lines.join("\n")}\n`;
  });
  return out;
}

/** Makes sure a call through a namespace import names a function that import actually brought in. */
export function normalizeCalls(model: ProjectModel): ProjectModel {
  for (const [i, f] of model.files.entries()) {
    f.calls = f.calls.map((calls) =>
      calls.filter(([k, fn]) => {
        if (k === i) return true;
        return f.imports.some((imp) =>
          imp.style === "via" ? imp.names.includes(k) && fn === 0 : imp.from === k && imp.names.includes(fn),
        );
      }),
    );
  }
  return model;
}
