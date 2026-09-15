// P4-java — hierarchies for Java: `extends`, `implements` and `overrides` equal
// what the JVM says of the compiled classes, and every method that runs is
// reached from its call's declared target through `overrides`.
//
// The oracle is the running program. Reflection gives each type's superclass and
// interfaces, and each direct supertype's view of a method name (`getMethod`'s
// declaring class): the member a declaration overrides. Each method of each
// concrete class, invoked through each of its supertypes, prints the declaration
// that ran. Models are normalized so that no type sees one method name through
// two unrelated interfaces, where reflection may answer either; javac repairs
// what a model leaves uncompilable — an abstract method a concrete class must
// implement, defaults a type inherits twice.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import fc from "fast-check";
import { extractJava, javaAvailable, tempDir, writeFiles } from "../helpers.ts";

// 40 for P4-java: its rarest guard, a class implementing an interface's method
// with one it inherits, fires in 46 of 200 runs (class overrides 50, interface
// extends 65), so 40 misses it about one suite in 35,000.
const RUNS = Number.parseInt(process.env.CODE_FACTS_RUNS ?? "40", 10);
const NO_JAVA = javaAvailable() ? false : "needs a JDK (`javac` and `java`)";
const METHODS = ["M0", "M1", "M2", "M3"];

interface Iface {
  /** method → a default method */
  own: Map<string, boolean>;
  /** lower indices only */
  extends: number[];
}
interface Klass {
  abstract: boolean;
  /** a lower class index, or none */
  superclass: number | null;
  implements: number[];
  /** method → declared abstract (only in an abstract class) */
  declares: Map<string, boolean>;
}
interface Model {
  ifaces: Iface[];
  classes: Klass[];
}

function ifaceNames(ifaces: Iface[], i: number): Set<string> {
  const it = ifaces[i];
  const out = new Set<string>(it === undefined ? [] : it.own.keys());
  for (const e of it?.extends ?? []) for (const n of ifaceNames(ifaces, e)) out.add(n);
  return out;
}

/** Every method name a class sees through the interfaces of its superclass chain and its own. */
function classIfaceNames(m: Model, k: number | null): Set<string> {
  const out = new Set<string>();
  for (let c = k; c !== null; c = m.classes[c]?.superclass ?? null) {
    for (const i of m.classes[c]?.implements ?? []) for (const n of ifaceNames(m.ifaces, i)) out.add(n);
  }
  return out;
}

interface Raw {
  ifaces: { own: [string, boolean][]; extends: number[] }[];
  classes: { abstract: boolean; superclass: number; implements: number[]; declares: [string, boolean][]; borrow: boolean }[];
}

function normalize(raw: Raw): Model {
  const m: Model = { ifaces: [], classes: [] };
  raw.ifaces.forEach((r, i) => {
    const kept: number[] = [];
    const seen = new Set<string>();
    for (const e of [...new Set(r.extends)].filter((e) => e < i).sort()) {
      const names = ifaceNames(m.ifaces, e);
      if ([...names].some((n) => seen.has(n))) continue;
      kept.push(e);
      for (const n of names) seen.add(n);
    }
    const own = new Map(r.own);
    if (own.size === 0 && kept.length === 0) own.set("M0", false);
    m.ifaces.push({ own, extends: kept });
  });
  raw.classes.forEach((r, k) => {
    const superclass = r.superclass < k ? r.superclass : null;
    const seen = classIfaceNames(m, superclass);
    const kept: number[] = [];
    // Borrowing: implement the interfaces whose methods the superclass already
    // declares, and leave them to it — an implementation the class inherits.
    const declared = new Map(r.declares.map(([x, a]) => [x, a && r.abstract]));
    let candidates = [...new Set(r.implements)];
    if (r.borrow && superclass !== null) {
      const inherited = new Set([...(m.classes[superclass]?.declares ?? new Map())].filter(([, abs]) => !abs).map(([x]) => x));
      candidates = [...candidates, ...m.ifaces.map((_, i) => i).filter((i) => [...ifaceNames(m.ifaces, i)].some((x) => inherited.has(x)))];
      for (const x of inherited) declared.delete(x);
    }
    // An interface the superclass chain already implements adds no name reflection could resolve two ways.
    const chain = new Set<number>();
    for (let c = superclass; c !== null; c = m.classes[c]?.superclass ?? null) for (const i of m.classes[c]?.implements ?? []) chain.add(i);
    for (const i of [...new Set(candidates)].filter((i) => i < m.ifaces.length).sort()) {
      const names = ifaceNames(m.ifaces, i);
      if (!chain.has(i) && [...names].some((n) => seen.has(n))) continue;
      kept.push(i);
      for (const n of names) seen.add(n);
    }
    m.classes.push({ abstract: r.abstract, superclass, implements: kept, declares: declared });
  });
  return m;
}

const arbModel: fc.Arbitrary<Model> = fc
  .record({
    ifaces: fc.array(fc.record({ own: fc.array(fc.tuple(fc.constantFrom(...METHODS), fc.boolean()), { maxLength: 3 }), extends: fc.array(fc.nat(2), { maxLength: 2 }) }), {
      minLength: 1,
      maxLength: 3,
    }),
    classes: fc.array(
      fc.record({
        abstract: fc.boolean(),
        superclass: fc.nat(3),
        implements: fc.array(fc.nat(2), { maxLength: 2 }),
        declares: fc.array(fc.tuple(fc.constantFrom(...METHODS), fc.boolean()), { maxLength: 3 }),
        borrow: fc.boolean(),
      }),
      { minLength: 2, maxLength: 4 },
    ),
  })
  .map(normalize);

const names = (m: Model) => [...m.ifaces.map((_, i) => `I${i}`), ...m.classes.map((_, k) => `T${k}`)];

function render(m: Model): Record<string, string> {
  const files: Record<string, string> = {};
  m.ifaces.forEach((it, i) => {
    const ext = it.extends.length > 0 ? ` extends ${it.extends.map((e) => `I${e}`).join(", ")}` : "";
    const body = [...it.own].map(([x, dflt]) => (dflt ? `  default void ${x}() {\n    System.out.println("ran I${i} ${x}");\n  }` : `  void ${x}();`));
    files[`p/I${i}.java`] = `package p;\n\npublic interface I${i}${ext} {\n${body.join("\n")}\n}\n`;
  });
  m.classes.forEach((c, k) => {
    const sup = c.superclass !== null ? ` extends T${c.superclass}` : "";
    const impl = c.implements.length > 0 ? ` implements ${c.implements.map((i) => `I${i}`).join(", ")}` : "";
    const body = [...c.declares].map(([x, abs]) => (abs ? `  public abstract void ${x}();` : `  public void ${x}() {\n    System.out.println("ran T${k} ${x}");\n  }`));
    files[`p/T${k}.java`] = `package p;\n\npublic ${c.abstract ? "abstract " : ""}class T${k}${sup}${impl} {\n${body.join("\n")}\n}\n`;
  });
  files["oracle/Oracle.java"] = [
    "package oracle;",
    "",
    "import java.lang.reflect.Method;",
    "import java.lang.reflect.Modifier;",
    "import java.util.ArrayList;",
    "import java.util.LinkedHashSet;",
    "import java.util.List;",
    "import java.util.Set;",
    "",
    "public class Oracle {",
    `  static final String[] TYPES = {${names(m).map((n) => `"${n}"`).join(", ")}};`,
    `  static final String[] METHODS = {${METHODS.map((x) => `"${x}"`).join(", ")}};`,
    "",
    "  public static void main(String[] args) throws Exception {",
    "    for (String name : TYPES) {",
    '      Class<?> c = Class.forName("p." + name);',
    '      if (!c.isInterface() && c.getSuperclass() != Object.class) System.out.println("extends " + name + " " + c.getSuperclass().getSimpleName());',
    '      for (Class<?> i : c.getInterfaces()) System.out.println((c.isInterface() ? "extends " : "implements ") + name + " " + i.getSimpleName());',
    "      List<Class<?>> supers = new ArrayList<>();",
    "      if (!c.isInterface()) supers.add(c.getSuperclass());",
    "      for (Class<?> i : c.getInterfaces()) supers.add(i);",
    "      for (Method m : c.getDeclaredMethods()) {",
    "        for (Class<?> s : supers) {",
    "          try {",
    '            System.out.println("overrides " + name + " " + m.getName() + " " + s.getMethod(m.getName()).getDeclaringClass().getSimpleName());',
    "          } catch (NoSuchMethodException e) {",
    "            // not a member of this supertype",
    "          }",
    "        }",
    "      }",
    "      // A method the class inherits from a superclass implements its interfaces' methods here.",
    "      if (!c.isInterface()) {",
    "        for (Class<?> i : c.getInterfaces()) {",
    "          for (Method im : i.getMethods()) {",
    "            Class<?> d = c.getMethod(im.getName()).getDeclaringClass();",
    '            if (d != c && !d.isInterface()) System.out.println("inherited " + d.getSimpleName() + " " + im.getName() + " " + i.getMethod(im.getName()).getDeclaringClass().getSimpleName());',
    "          }",
    "        }",
    "      }",
    "      if (c.isInterface() || Modifier.isAbstract(c.getModifiers())) continue;",
    "      Object o = c.getDeclaredConstructor().newInstance();",
    "      for (Class<?> s : closure(c, new LinkedHashSet<>())) {",
    "        for (String x : METHODS) {",
    "          Method d;",
    "          try {",
    "            d = s.getMethod(x);",
    "          } catch (NoSuchMethodException e) {",
    "            continue;",
    "          }",
    '          System.out.println("call " + name + " " + s.getSimpleName() + " " + x + " " + d.getDeclaringClass().getSimpleName());',
    "          d.invoke(o);",
    "        }",
    "      }",
    "    }",
    "  }",
    "",
    "  static Set<Class<?>> closure(Class<?> c, Set<Class<?>> out) {",
    "    if (c == null || c == Object.class || !out.add(c)) return out;",
    "    closure(c.getSuperclass(), out);",
    "    for (Class<?> i : c.getInterfaces()) closure(i, out);",
    "    return out;",
    "  }",
    "}",
    "",
  ].join("\n");
  return files;
}

/** Compiles the model, letting javac name what a concrete class or an interface must declare; the declarations added. */
function compile(dir: string, m: Model): number {
  let added = 0;
  for (let attempt = 0; attempt < 16; attempt++) {
    const files = render(m);
    fs.rmSync(path.join(dir, "p"), { recursive: true, force: true });
    writeFiles(dir, files);
    const r = spawnSync("javac", ["-d", path.join(dir, "out"), ...Object.keys(files).map((f) => path.join(dir, f))], { encoding: "utf8" });
    if (r.status === 0) return added;
    let fixed = false;
    for (const e of r.stderr.matchAll(/\b(T\d+) is not abstract and does not override abstract method (M\d)\(\)/g)) {
      m.classes[Number((e[1] ?? "T0").slice(1))]?.declares.set(e[2] ?? "M0", false);
      fixed = true;
    }
    for (const e of r.stderr.matchAll(/\b(class|interface) ([TI]\d+) inherits (?:unrelated defaults|abstract and default) for (M\d)\(\)/g)) {
      const name = e[2] ?? "";
      const x = e[3] ?? "M0";
      if (name.startsWith("T")) m.classes[Number(name.slice(1))]?.declares.set(x, false);
      else m.ifaces[Number(name.slice(1))]?.own.set(x, false);
      fixed = true;
    }
    if (!fixed) throw new Error(`javac rejected a model it named no repair for:\n${r.stderr}`);
    added++;
  }
  throw new Error("javac repairs did not converge");
}

interface Observed {
  extends: string[][];
  implements: string[][];
  overrides: string[][];
  /** [class, supertype called through, method, declaration resolved, declaration that ran] */
  calls: string[][];
  /** an override reported for a method a class inherits, not one it declares */
  inherited: boolean;
}

function observe(dir: string): Observed {
  const r = spawnSync("java", ["-cp", path.join(dir, "out"), "oracle.Oracle"], { encoding: "utf8" });
  assert.equal(r.status, 0, r.stderr);
  const id = (name: string) => `p/${name}.java#${name}`;
  const out: Observed = { extends: [], implements: [], overrides: [], calls: [], inherited: false };
  const lines = r.stdout.split("\n");
  for (let n = 0; n < lines.length; n++) {
    const p = (lines[n] ?? "").split(" ");
    if (p[0] === "extends") out.extends.push([id(p[1] ?? ""), id(p[2] ?? "")]);
    if (p[0] === "implements") out.implements.push([id(p[1] ?? ""), id(p[2] ?? "")]);
    if (p[0] === "overrides") out.overrides.push([`${id(p[1] ?? "")}.${p[2]}`, `${id(p[3] ?? "")}.${p[2]}`]);
    if (p[0] === "inherited") {
      out.overrides.push([`${id(p[1] ?? "")}.${p[2]}`, `${id(p[3] ?? "")}.${p[2]}`]);
      out.inherited = true;
    }
    if (p[0] === "call") {
      const ran = (lines[n + 1] ?? "").split(" ");
      out.calls.push([id(p[1] ?? ""), id(p[2] ?? ""), p[3] ?? "", `${id(p[4] ?? "")}.${p[3]}`, `${id(ran[1] ?? "")}.${ran[2]}`]);
      n++;
    }
  }
  return out;
}

function sortedUnique(rows: unknown[][]): string[] {
  return [...new Set(rows.map((r) => JSON.stringify(r)))].sort();
}

test("P4-java: extends, implements and overrides are what the JVM says; every method that runs is reached through overrides", { skip: NO_JAVA }, () => {
  const guards = { implemented: 0, interfaceExtends: 0, classOverride: 0, defaultOverride: 0, dispatched: 0, inherited: 0, repaired: 0 };
  fc.assert(
    fc.property(arbModel, (m) => {
      const dir = tempDir("p4-java");
      const added = compile(dir, m);
      const seen = observe(dir);
      const { tables } = extractJava(dir, { layers: ["refs"] });
      const inModel = (id: unknown) => String(id).startsWith("p/");

      assert.deepEqual(sortedUnique(tables.rows("extends").filter((r) => inModel(r.child)).map((r) => [r.child, r.parent])), sortedUnique(seen.extends));
      assert.deepEqual(sortedUnique(tables.rows("implements").filter((r) => inModel(r.class)).map((r) => [r.class, r.interface])), sortedUnique(seen.implements));
      const overrides = tables.rows("overrides").filter((r) => inModel(r.member)).map((r): [string, string] => [String(r.member), String(r.base)]);
      assert.deepEqual(sortedUnique(overrides), sortedUnique(seen.overrides));

      // Class-hierarchy analysis is sound here: the declaration that ran is the
      // resolved one, or reached from it through overrides.
      const overriddenBy = new Map<string, string[]>();
      for (const [member, base] of overrides) overriddenBy.set(base, [...(overriddenBy.get(base) ?? []), member]);
      const reach = (from: string): Set<string> => {
        const out = new Set([from]);
        const work = [from];
        for (let x = work.pop(); x !== undefined; x = work.pop()) for (const y of overriddenBy.get(x) ?? []) if (!out.has(y)) out.add(y), work.push(y);
        return out;
      };
      let dispatched = false;
      for (const [cls, via, x, declared, ran] of seen.calls) {
        assert.ok(reach(declared ?? "").has(ran ?? ""), `${cls} through ${via}: ${x} ran ${ran}, not reached from ${declared}`);
        if (ran !== declared) dispatched = true;
      }
      guards.dispatched += dispatched ? 1 : 0;

      guards.implemented += seen.implements.length > 0 ? 1 : 0;
      guards.interfaceExtends += seen.extends.some(([c]) => (c ?? "").includes("#I")) ? 1 : 0;
      guards.classOverride += seen.overrides.some(([a, b]) => (a ?? "").includes("#T") && (b ?? "").includes("#T")) ? 1 : 0;
      guards.defaultOverride += seen.overrides.some(([, b]) => {
        const name = (b ?? "").split("#")[1] ?? "";
        const [type, x] = name.split(".");
        return (type ?? "").startsWith("I") && m.ifaces[Number((type ?? "I0").slice(1))]?.own.get(x ?? "") === true;
      })
        ? 1
        : 0;
      guards.repaired += added > 0 ? 1 : 0;
      guards.inherited += seen.inherited ? 1 : 0;
      fs.rmSync(dir, { recursive: true, force: true });
    }),
    { numRuns: RUNS },
  );
  if (process.env.CODE_FACTS_GUARDS !== undefined) console.log(`P4-java guards over ${RUNS} runs: ${JSON.stringify(guards)}`);
  assert.ok(guards.implemented >= 1, "no class implemented an interface");
  assert.ok(guards.interfaceExtends >= 1, "no interface extended another");
  assert.ok(guards.classOverride >= 1, "no method overrode a superclass's");
  assert.ok(guards.defaultOverride >= 1, "no method overrode a default method");
  assert.ok(guards.dispatched >= 1, "no call ran a declaration other than the one it resolved");
  assert.ok(guards.inherited >= 1, "no class implemented an interface's method with one it inherits");
});
