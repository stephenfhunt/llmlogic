import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import { extract, tempDir, writeProject } from "./helpers.ts";

function sh(cwd: string, ...args: string[]): void {
  execFileSync("git", args, {
    cwd,
    stdio: "ignore",
    env: {
      ...process.env,
      GIT_AUTHOR_NAME: "Ada",
      GIT_AUTHOR_EMAIL: "ada@example.com",
      GIT_COMMITTER_NAME: "Ada",
      GIT_COMMITTER_EMAIL: "ada@example.com",
      GIT_AUTHOR_DATE: "2026-02-03T04:05:06Z",
      GIT_COMMITTER_DATE: "2026-02-03T04:05:06Z",
    },
  });
}

test("git: commits, touches, and renames followed to today's path", () => {
  const dir = tempDir("git");
  writeProject(dir, { "src/a.ts": "export const a = 1;\n", "src/b.ts": "export const b = 2;\n", "README.md": "hi\n" });
  sh(dir, "init", "-q", "-b", "main");
  sh(dir, "add", ".");
  sh(dir, "commit", "-q", "-m", "first");
  // a.ts → old.ts → z/new.ts across two commits; b.ts edited alongside.
  fs.mkdirSync(path.join(dir, "src", "z"));
  sh(dir, "mv", "src/a.ts", "src/old.ts");
  fs.appendFileSync(path.join(dir, "src", "b.ts"), "export const c = 3;\n");
  sh(dir, "add", ".");
  sh(dir, "commit", "-q", "-m", "rename a, grow b");
  sh(dir, "mv", "src/old.ts", "src/z/new.ts");
  sh(dir, "commit", "-q", "-m", "move again");
  fs.rmSync(path.join(dir, "README.md"));
  sh(dir, "commit", "-q", "-am", "drop readme");

  const { tables } = extract(dir, { layers: ["git"] });
  const commits = tables.rows("commit");
  assert.deepEqual(
    commits.map((c) => [c.subject, c.author, c.email, c.time, c.parents, c.files]),
    [
      ["drop readme", "Ada", "ada@example.com", "2026-02-03T04:05:06", 1, 1],
      ["move again", "Ada", "ada@example.com", "2026-02-03T04:05:06", 1, 1],
      ["rename a, grow b", "Ada", "ada@example.com", "2026-02-03T04:05:06", 1, 2],
      ["first", "Ada", "ada@example.com", "2026-02-03T04:05:06", 0, 4],
    ],
  );
  const subject = new Map(commits.map((c) => [c.sha, c.subject]));
  const touches = tables.rows("touch").map((t) => [subject.get(t.sha as string), t.path, t.path_now, t.old_path, t.change, t.added, t.deleted]);
  assert.deepEqual(touches, [
    ["drop readme", "README.md", null, null, "deleted", 0, 1],
    ["move again", "src/z/new.ts", "src/z/new.ts", "src/old.ts", "renamed", 0, 0],
    ["rename a, grow b", "src/b.ts", "src/b.ts", null, "modified", 1, 0],
    ["rename a, grow b", "src/old.ts", "src/z/new.ts", "src/a.ts", "renamed", 0, 0],
    ["first", "README.md", null, null, "added", 1, 0],
    ["first", "src/a.ts", "src/z/new.ts", null, "added", 1, 0],
    ["first", "src/b.ts", "src/b.ts", null, "added", 1, 0],
    ["first", "tsconfig.json", "tsconfig.json", null, "added", 14, 0],
  ]);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("git: a directory that is not a repository has no history, and says so by being empty", () => {
  const dir = tempDir("nogit");
  writeProject(dir, { "src/a.ts": "export const a = 1;\n" });
  const { tables } = extract(dir, { layers: ["git"] });
  assert.equal(tables.count("commit"), 0);
  assert.equal(tables.rows("extraction")[0]?.git_head, null);
});
