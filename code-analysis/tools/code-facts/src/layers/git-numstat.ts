// The git layer's line counts, in parallel: `git log --numstat` diffs every blob a
// commit touched, single-threaded — 38 of the 43 s the layer took on Grafana. Each
// chunk of commits is its own `git log --no-walk`, run concurrently; a commit's
// numstat does not depend on which other commits share its process.
//
// Run by `git.ts` as a child process, since the extractor is synchronous: reads
// `{ top, chunks, pathspec }` as JSON on stdin, writes the outputs, in chunk
// order, as a JSON array on stdout, and exits non-zero if any `git log` failed.

import { spawn } from "node:child_process";
import * as fs from "node:fs";

export interface NumstatJob {
  top: string;
  /** Commit shas, one `git log` per chunk. */
  chunks: string[][];
  /** A root below the top level: that subtree's paths only. */
  pathspec: string | null;
}

function numstat(top: string, shas: string[], pathspec: string | null): Promise<string> {
  return new Promise((resolve, reject) => {
    const args = ["-C", top, "log", "--no-walk=unsorted", "--stdin", "-z", "-M", "--numstat", "--format=%x01%H"];
    if (pathspec !== null) args.push("--", pathspec);
    const child = spawn("git", args, { stdio: ["pipe", "pipe", "ignore"] });
    const parts: string[] = [];
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (d: string) => parts.push(d));
    child.on("error", reject);
    child.on("close", (code) => (code === 0 ? resolve(parts.join("")) : reject(new Error(`git log --numstat exited ${code}`))));
    child.stdin.end(`${shas.join("\n")}\n`);
  });
}

const job = JSON.parse(fs.readFileSync(0, "utf8")) as NumstatJob;
Promise.all(job.chunks.map((shas) => numstat(job.top, shas, job.pathspec))).then(
  (outputs) => process.stdout.write(JSON.stringify(outputs)),
  (e: unknown) => {
    process.stderr.write(`${e instanceof Error ? e.message : String(e)}\n`);
    process.exitCode = 1;
  },
);
