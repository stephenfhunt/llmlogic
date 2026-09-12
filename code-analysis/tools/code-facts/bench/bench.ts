// Times the rule library over an extracted fact directory.
//
//   npm run bench -- --facts <dir> [--only coupling,cohesion] [--repeat 3]
//
// Prints a markdown table: wall clock (min of N), peak RSS, rows answered, and
// a digest of the answers. **The digest is the point.** A performance change
// that moves an answer is not a performance change, and the only way to know is
// to ask each library the same questions before and after and compare bytes —
// the discipline `../../../datalog/notes/profile-2026-08-20.md` used for its
// cut builds ("a timing delta with a changed digest was never reported").
//
// Fact directories live outside the checkout; extract one with
//
//   ./skill/code-facts <tsconfig> -o ~/.cache/code-facts-bench/<name> --no-git

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const TOOL_DIR = path.join(HERE, "..");

export const DATALOG =
  process.env.DATALOG_BIN ?? path.resolve(TOOL_DIR, "..", "..", "..", "datalog", "target", "release", "datalog");

/** A library, and the questions the digest is taken over.
 *
 * The queries are the relations the library's own header documents as its
 * output — never its intermediates. `coupling.dl`'s `crossing` has millions of
 * rows and is derived whether or not anything asks for it, so printing it would
 * measure the writer; asking for `comp_edge_weight` forces exactly the same
 * fixpoint and prints a table a human could read.
 *
 * `checks.dl` and `orient.dl` carry their own `?-` goals and take none. */
export interface Library {
  name: string;
  queries: string[];
}

export const LIBRARIES: Library[] = [
  { name: "checks", queries: [] },
  { name: "orient", queries: [] },
  {
    name: "modgraph",
    queries: ["unit_dep(D, A, B)", "package_edge(A, B)", "external_dep(F, P)", "in_cycle(F)", "cycle_edge(A, B)"],
  },
  {
    name: "callgraph",
    queries: ["call_edge(A, B)", "call_edge_declared(A, B)", "call_edge_lexical(A, B)", "called(B)"],
  },
  { name: "callreach", queries: ["recursive(F)", "mutual(A, B)"] },
  {
    name: "coupling",
    queries: [
      "comp_edge_weight(G, A, B, N)",
      "instability(G, C, I)",
      "abstractness(G, C, A)",
      "distance(G, C, D)",
      "sdp_violation(G, A, B)",
      "cbo(T, N)",
    ],
  },
  {
    name: "cohesion",
    queries: ["lcom4(C, N)", "tcc(C, T)", "lcom_hs(C, L)", "module_lcom4(F, N)", "relational_cohesion(G, C, H)"],
  },
  {
    name: "coupling_kinds",
    queries: [
      "module_coupling(A, B, K)",
      "worst_coupling(A, B, K)",
      "content_access(F, M, C)",
      "common_state(A, B, V)",
      "shared_literal(A, B, V)",
      "control_call(A, B, P)",
      "stamp_call(A, B, T)",
      "data_call(A, B)",
    ],
  },
  {
    name: "metrics",
    queries: ["dit(C, N)", "noc(C, N)", "wmc(C, W)", "rfc(C, N)", "fan_out(F, N)", "fan_in(F, N)"],
  },
  // Beyond the module/design libraries: the flow-layer ones the playbook also
  // sends an agent to. Their documented *findings* only — `pts`, `dominates`,
  // `def_use` and `live_out` run to millions of rows, and printing them would
  // measure the writer.
  { name: "flow", queries: ["unreachable(F, N, L)", "undefined_use(N, V)", "dead_store(D, V, L)"] },
  { name: "dominators", queries: ["back_edge(N, H)", "loop_header(F, H)"] },
  { name: "pointsto", queries: ["call_edge_pt(A, B)", "unresolved_call(S)"] },
  {
    name: "packages",
    queries: [
      "imported(P, D, F)",
      "undeclared(P, D, F)",
      "unused(P, D, K)",
      "dev_in_production(P, D, F)",
      "only_in_tests(P, D)",
      "types_only(P, D)",
    ],
  },
];

export interface Measurement {
  name: string;
  /** Wall clock, seconds — the minimum over the repeats. */
  seconds: number;
  /** Peak resident set, MB — the maximum over the repeats. */
  peakMb: number;
  /** Answer rows printed across the library's queries. */
  rows: number;
  /** sha256 of the answers, first 12 hex. Empty when the run did not answer. */
  digest: string;
  /** Engine exit code: 0 answered, 1 ran with no rows, ≥2 did not answer.
   * A run the kernel killed (out of memory, or an operator's interrupt) reports
   * -1, which is the same claim: it produced no answer, and the seconds beside
   * it are when it stopped rather than how long it takes. */
  code: number;
  /** stderr, kept when the run did not answer. */
  stderr: string;
  /** The bench stopped it rather than the engine finishing. */
  timedOut: boolean;
}

/** One timed run. `/usr/bin/time` supplies peak RSS; the elapsed time is taken
 * here, around the spawn, so a machine without GNU time still reports one. */
function once(
  file: string,
  queries: string[],
  timeoutMs: number,
): { seconds: number; peakMb: number; stdout: string; stderr: string; code: number; timedOut: boolean } {
  const args: string[] = [file];
  for (const q of queries) args.push("-q", q);

  const rssFile = path.join(os.tmpdir(), `code-facts-bench-${process.pid}.rss`);
  const gnuTime = fs.existsSync("/usr/bin/time");
  const command = gnuTime ? "/usr/bin/time" : DATALOG;
  const argv = gnuTime ? ["-f", "%M", "-o", rssFile, DATALOG, ...args] : args;

  const start = performance.now();
  // A library whose closure does not fit takes the machine down rather than
  // taking a long time — `callreach.dl` on vs/base is past 4 GB in 15 s — so the
  // bench stops it and says so. A killed entry's seconds are when it was
  // stopped, not how long it takes.
  const r = spawnSync(command, argv, { encoding: "utf8", maxBuffer: 1 << 30, timeout: timeoutMs });
  const seconds = (performance.now() - start) / 1000;
  if (r.error !== undefined) throw r.error;

  let peakMb = 0;
  if (gnuTime && fs.existsSync(rssFile)) {
    peakMb = Number(fs.readFileSync(rssFile, "utf8").trim().split("\n").pop()) / 1024;
    fs.unlinkSync(rssFile);
  }
  return {
    seconds,
    peakMb,
    stdout: r.stdout,
    stderr: r.stderr,
    code: r.status ?? -1,
    timedOut: r.signal !== null && r.status === null,
  };
}

export function measure(
  factsDir: string,
  library: Library,
  repeat: number,
  libDir = "lib",
  timeoutMs = 300_000,
): Measurement {
  const file = path.join(factsDir, libDir, `${library.name}.dl`);
  let best: ReturnType<typeof once> | undefined;
  let peakMb = 0;
  for (let i = 0; i < repeat; i++) {
    const run = once(file, library.queries, timeoutMs);
    peakMb = Math.max(peakMb, run.peakMb);
    if (best === undefined || run.seconds < best.seconds) best = run;
  }
  const run = best!;
  const lines = run.stdout.split("\n").filter((l) => l.trim() !== "");
  return {
    name: library.name,
    seconds: run.seconds,
    peakMb,
    rows: lines.length,
    digest: run.stdout === "" ? "" : createHash("sha256").update(run.stdout).digest("hex").slice(0, 12),
    code: run.code,
    // An engine that did not answer has to say so in the table, not silently
    // post a fast time: a program that fails to parse is the fastest of all.
    stderr: answered(run.code) ? "" : run.stderr.trim(),
    timedOut: run.timedOut,
  };
}

/** Did the engine answer? 0 is rows, 1 is a clean empty answer; everything
 * else — a structured error, or a signal — is not an answer. */
export function answered(code: number): boolean {
  return code === 0 || code === 1;
}

export function table(rows: Measurement[]): string {
  const out = [
    "| library | time | peak RSS | rows | digest |",
    "|---|---:|---:|---:|---|",
  ];
  for (const m of rows) {
    const note = answered(m.code)
      ? ""
      : m.timedOut
        ? " — **stopped**, still running"
        : ` — **did not answer** (exit ${m.code})`;
    out.push(
      `| \`${m.name}.dl\` | ${m.timedOut ? "> " : ""}${m.seconds.toFixed(1)} s | ${m.peakMb.toFixed(0)} MB | ${m.rows} | \`${m.digest}\`${note} |`,
    );
  }
  return out.join("\n");
}

function main(argv: string[]): number {
  let factsDir = process.env.CODE_FACTS_BENCH_FACTS ?? "";
  let only: string[] | undefined;
  let repeat = 1;
  let libDir = "lib";
  let timeoutMs = 300_000;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--facts") factsDir = argv[++i] ?? "";
    else if (arg === "--only") only = (argv[++i] ?? "").split(",").filter((s) => s !== "");
    else if (arg === "--repeat") repeat = Number(argv[++i] ?? "1");
    // A second library directory beside the facts, so a before/after pair is one
    // extraction and not two — `lib-stock` against `lib`, same fact base.
    else if (arg === "--lib") libDir = argv[++i] ?? "lib";
    else if (arg === "--timeout") timeoutMs = Number(argv[++i] ?? "300") * 1000;
    else {
      process.stderr.write(`bench: unknown argument ${arg}\n`);
      return 2;
    }
  }
  if (factsDir === "") {
    process.stderr.write("bench: --facts <dir> (an extracted fact directory) is required\n");
    return 2;
  }
  if (!fs.existsSync(path.join(factsDir, libDir))) {
    process.stderr.write(`bench: ${factsDir} has no ${libDir}/ — is it a code-facts output directory?\n`);
    return 2;
  }
  if (!fs.existsSync(DATALOG)) {
    process.stderr.write(`bench: no datalog binary at ${DATALOG} — build it with \`cargo build --release --offline\`\n`);
    return 2;
  }

  const chosen = only === undefined ? LIBRARIES : LIBRARIES.filter((l) => only.includes(l.name));
  const unknown = (only ?? []).filter((n) => !LIBRARIES.some((l) => l.name === n));
  if (unknown.length > 0) {
    process.stderr.write(`bench: unknown library ${unknown.join(", ")}\n`);
    return 2;
  }

  process.stderr.write(`bench: ${factsDir}/${libDir}, ${DATALOG}, min of ${repeat}\n`);
  const rows: Measurement[] = [];
  for (const library of chosen) {
    const m = measure(factsDir, library, repeat, libDir, timeoutMs);
    rows.push(m);
    const stopped = m.timedOut ? " (stopped)" : "";
    process.stderr.write(`  ${m.name}: ${m.seconds.toFixed(1)} s, ${m.peakMb.toFixed(0)} MB${stopped}\n`);
    if (m.stderr !== "") process.stderr.write(`${m.stderr}\n`);
  }
  process.stdout.write(`${table(rows)}\n`);
  return rows.some((m) => !answered(m.code)) ? 1 : 0;
}

if (process.argv[1] !== undefined && import.meta.filename === path.resolve(process.argv[1])) {
  process.exit(main(process.argv.slice(2)));
}
