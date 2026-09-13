// The git layer: commits and the paths each touched, with renames followed to
// today's path so co-change history joins against `file.path`. Every path the
// history touches is emitted — not only TypeScript — since a config file or a doc
// that always changes with a module is coupling too.

import { execFileSync, spawnSync } from "node:child_process";
import { availableParallelism } from "node:os";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { truncate } from "../context.ts";
import { relTo } from "../program.ts";
import type { Tables } from "../writer.ts";
import type { NumstatJob } from "./git-numstat.ts";

const MAX_BUFFER = 1 << 30;
const NUMSTAT = fileURLToPath(new URL("./git-numstat.ts", import.meta.url));

function git(cwd: string, args: string[]): string | undefined {
  try {
    return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8", maxBuffer: MAX_BUFFER, stdio: ["ignore", "pipe", "ignore"] });
  } catch {
    return undefined;
  }
}

export function gitHead(root: string): string | null {
  return git(root, ["rev-parse", "HEAD"])?.trim() ?? null;
}

interface Entry {
  status: string;
  path: string;
  oldPath: string | null;
}

interface Commit {
  sha: string;
  author: string;
  email: string;
  time: string;
  parents: number;
  subject: string;
  entries: Entry[];
}

function splitRecords(out: string): { header: string[]; tokens: string[] }[] {
  const records: { header: string[]; tokens: string[] }[] = [];
  for (const chunk of out.split("\x01")) {
    if (chunk === "") continue;
    const nul = chunk.indexOf("\0");
    const header = (nul < 0 ? chunk : chunk.slice(0, nul)).split("\x02");
    let rest = nul < 0 ? "" : chunk.slice(nul + 1);
    if (rest.startsWith("\n")) rest = rest.slice(1);
    const tokens = rest.split("\0");
    while (tokens.length > 0 && tokens[tokens.length - 1] === "") tokens.pop();
    records.push({ header, tokens });
  }
  return records;
}

export interface GitOptions {
  since?: string | undefined;
  maxCommits: number;
  /** Concurrent `git log --numstat` processes (default: the machine's parallelism). */
  numstatJobs?: number | undefined;
}

/** Each commit's numstat, from `git-numstat.ts` over `jobs` chunks of `shas`, or
 * none if it failed — line counts go missing, as a failed `git log` always left
 * them, rather than the history. */
function numstat(top: string, shas: string[], pathspec: string | null, jobs: number): string[] {
  if (shas.length === 0) return [];
  const size = Math.ceil(shas.length / Math.max(1, Math.min(jobs, shas.length)));
  const chunks: string[][] = [];
  for (let i = 0; i < shas.length; i += size) chunks.push(shas.slice(i, i + size));
  const job: NumstatJob = { top, chunks, pathspec };
  const r = spawnSync(process.execPath, ["--disable-warning=ExperimentalWarning", NUMSTAT], {
    input: JSON.stringify(job),
    encoding: "utf8",
    maxBuffer: MAX_BUFFER,
    stdio: ["pipe", "pipe", "ignore"],
  });
  if (r.error !== undefined || r.status !== 0) return [];
  return JSON.parse(r.stdout) as string[];
}

export function extractGit(ctx: { root: string; tables: Tables }, opts: GitOptions): void {
  const top = git(ctx.root, ["rev-parse", "--show-toplevel"])?.trim();
  if (top === undefined || top === "") return;
  const toRoot = (p: string) => relTo(ctx.root, path.join(top, p));
  const range: string[] = [];
  if (opts.maxCommits > 0) range.push(`-n${opts.maxCommits}`);
  if (opts.since !== undefined) range.push(`--since=${opts.since}`);
  // A root below the repository's top level means that subtree's history only.
  const sub = relTo(top, ctx.root);
  if (sub !== ".") range.push("--", sub);

  const statusOut = git(top, ["log", "-z", "-M", "--name-status", "--format=%x01%H%x02%an%x02%ae%x02%at%x02%P%x02%s", ...range]);
  if (statusOut === undefined) return;

  const commits: Commit[] = [];
  for (const { header, tokens } of splitRecords(statusOut)) {
    const [sha = "", author = "", email = "", at = "0", parents = "", subject = ""] = header;
    const entries: Entry[] = [];
    for (let i = 0; i < tokens.length; i++) {
      const status = tokens[i] ?? "";
      if (status.startsWith("R") || status.startsWith("C")) {
        entries.push({ status: status.slice(0, 1), oldPath: tokens[i + 1] ?? "", path: tokens[i + 2] ?? "" });
        i += 2;
      } else {
        entries.push({ status: status.slice(0, 1), oldPath: null, path: tokens[i + 1] ?? "" });
        i += 1;
      }
    }
    commits.push({
      sha,
      author,
      email,
      time: new Date(Number.parseInt(at, 10) * 1000).toISOString().slice(0, 19),
      parents: parents.trim() === "" ? 0 : parents.trim().split(" ").length,
      subject: truncate(subject, 200),
      entries,
    });
  }

  // (sha, new path) → [added, deleted]; binary files report "-".
  const lines = new Map<string, [number | null, number | null]>();
  const shas = commits.map((c) => c.sha);
  for (const out of numstat(top, shas, sub !== "." ? sub : null, opts.numstatJobs ?? availableParallelism())) {
    for (const { header, tokens } of splitRecords(out)) {
      const sha = header[0] ?? "";
      for (let i = 0; i < tokens.length; i++) {
        const parts = (tokens[i] ?? "").split("\t");
        const count = (s: string | undefined) => (s === undefined || s === "-" ? null : Number.parseInt(s, 10));
        const added = count(parts[0]);
        const deleted = count(parts[1]);
        let p = parts[2] ?? "";
        if (p === "") {
          // A rename: "a\td\t" then old, new as their own tokens.
          i += 2;
          p = tokens[i] ?? "";
        }
        lines.set(`${sha}\0${p}`, [added, deleted]);
      }
    }
  }

  // Walk newest → oldest, carrying each historical path forward to today's.
  const now = new Map<string, string | null>();
  for (const p of (git(top, ["ls-files", "-z", ...(sub !== "." ? ["--", sub] : [])]) ?? "").split("\0")) if (p !== "") now.set(p, p);
  const nowOf = (p: string) => now.get(p) ?? null;
  const change: Record<string, string> = { A: "added", M: "modified", D: "deleted", R: "renamed", C: "copied", T: "type_changed" };

  for (const c of commits) {
    ctx.tables.add("commit", {
      sha: c.sha,
      author: c.author,
      email: c.email,
      time: c.time,
      parents: c.parents,
      files: c.entries.length,
      subject: c.subject,
    });
    const updates: [string, string | null][] = [];
    for (const e of c.entries) {
      const kind = change[e.status];
      if (kind === undefined) continue;
      const current = e.status === "D" ? null : nowOf(e.path);
      const [added, deleted] = lines.get(`${c.sha}\0${e.path}`) ?? [null, null];
      ctx.tables.add("touch", {
        sha: c.sha,
        path: toRoot(e.path),
        path_now: current !== null ? toRoot(current) : null,
        old_path: e.oldPath !== null ? toRoot(e.oldPath) : null,
        change: kind,
        added,
        deleted,
      });
      if (e.status === "R" && e.oldPath !== null) updates.push([e.oldPath, current]);
      if (e.status === "A" || e.status === "D") updates.push([e.path, null]);
    }
    for (const [p, v] of updates) now.set(p, v);
  }
}
