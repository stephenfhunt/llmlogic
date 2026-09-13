import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "node:test";
import { datalog, engineAvailable, extract, tempDir, writeProject } from "./helpers.ts";
import { extractGit } from "../src/layers/git.ts";
import { Tables } from "../src/writer.ts";

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

// Numstat runs as parallel `git log --no-walk` jobs over the commit list; how the
// commits are split must not change a row. The history has what numstat treats
// specially — a binary file, a merge, a rename — and more commits than jobs, and
// is read from the top level and from a root below it.
test("git: splitting numstat across jobs changes no commit or touch row", () => {
  const dir = tempDir("gitjobs");
  writeProject(dir, { "app/src/a.ts": "export const a = 1;\n", "app/src/b.ts": "export const b = 2;\n", "lib/c.ts": "export const c = 3;\n" });
  sh(dir, "init", "-q", "-b", "main");
  sh(dir, "add", ".");
  sh(dir, "commit", "-q", "-m", "first");
  fs.writeFileSync(path.join(dir, "app", "logo.bin"), Buffer.from([0, 1, 2, 255, 0, 7]));
  sh(dir, "add", ".");
  sh(dir, "commit", "-q", "-m", "binary");
  sh(dir, "checkout", "-q", "-b", "side");
  fs.appendFileSync(path.join(dir, "app", "src", "a.ts"), "export const a2 = 2;\n");
  sh(dir, "commit", "-q", "-am", "side edit");
  sh(dir, "checkout", "-q", "main");
  // Both sides touch app/, so the merge survives history simplification under that root.
  fs.appendFileSync(path.join(dir, "lib", "c.ts"), "export const c2 = 4;\n");
  fs.appendFileSync(path.join(dir, "app", "src", "b.ts"), "export const b2 = 5;\n");
  sh(dir, "commit", "-q", "-am", "main edit");
  sh(dir, "merge", "-q", "--no-ff", "-m", "merge side", "side");
  sh(dir, "mv", "app/src/b.ts", "app/src/bee.ts");
  sh(dir, "commit", "-q", "-m", "rename b");
  for (let i = 0; i < 6; i++) {
    fs.appendFileSync(path.join(dir, i % 2 === 0 ? "app/src/a.ts" : "lib/c.ts"), `export const x${i} = ${i};\n`);
    sh(dir, "commit", "-q", "-am", `edit ${i}`);
  }

  const history = (root: string, numstatJobs: number) => {
    const tables = new Tables();
    extractGit({ root, tables }, { maxCommits: 0, numstatJobs });
    return { commits: tables.rows("commit"), touches: tables.rows("touch") };
  };
  for (const root of [dir, path.join(dir, "app")]) {
    const one = history(root, 1);
    assert.ok(one.commits.some((c) => c.parents === 2), `${root}: a merge`);
    assert.ok(one.touches.some((t) => t.change === "renamed"), `${root}: a rename`);
    assert.ok(one.touches.some((t) => t.change === "added" && t.added === null), `${root}: a binary file`);
    assert.ok(one.touches.some((t) => typeof t.added === "number" && t.added > 0), `${root}: line counts`);
    for (const jobs of [2, 3, 64]) assert.deepEqual(history(root, jobs), one, `${root}: ${jobs} jobs`);
  }
  fs.rmSync(dir, { recursive: true, force: true });
});

test("git: a directory that is not a repository has no history, and says so by being empty", () => {
  const dir = tempDir("nogit");
  writeProject(dir, { "src/a.ts": "export const a = 1;\n" });
  const { tables } = extract(dir, { layers: ["git"] });
  assert.equal(tables.count("commit"), 0);
  assert.equal(tables.rows("extraction")[0]?.git_head, null);
});

test("cochange.dl: co-change counts, confidence, hidden coupling, churn and ownership", { skip: !engineAvailable() }, () => {
  const dir = tempDir("cochange");
  writeProject(dir, {
    "src/a.ts": "export const a = 1;\n",
    "src/b.ts": 'import { a } from "./a.js";\nexport const b = a;\n',
    "src/c.ts": "export const c = 3;\n",
  });
  sh(dir, "init", "-q", "-b", "main");
  sh(dir, "add", ".");
  sh(dir, "commit", "-q", "-m", "start");
  // a and c change together three times — and nothing imports between them.
  for (let i = 0; i < 3; i++) {
    fs.appendFileSync(path.join(dir, "src", "a.ts"), `export const a${i} = ${i};\n`);
    fs.appendFileSync(path.join(dir, "src", "c.ts"), `export const c${i} = ${i};\n`);
    sh(dir, "commit", "-q", "-am", `a and c ${i}`);
  }
  // a and b once, by someone else.
  fs.appendFileSync(path.join(dir, "src", "a.ts"), "export const z = 0;\n");
  fs.appendFileSync(path.join(dir, "src", "b.ts"), "export const y = 0;\n");
  execFileSync("git", ["commit", "-q", "-am", "a and b"], {
    cwd: dir,
    stdio: "ignore",
    env: { ...process.env, GIT_AUTHOR_NAME: "Bo", GIT_AUTHOR_EMAIL: "bo@example.com", GIT_COMMITTER_NAME: "Bo", GIT_COMMITTER_EMAIL: "bo@example.com" },
  });
  const out = path.join(dir, "out");
  extract(dir, { out, layers: ["refs", "git"] });
  const ask = (q: string) => datalog(path.join(out, "lib", "cochange.dl"), [q]).stdout.split("\n").filter((l) => l !== "");
  assert.deepEqual(ask("cochange(A, B, N)"), [
    'cochange("src/a.ts", "src/b.ts", 2).',
    'cochange("src/a.ts", "src/c.ts", 4).',
    'cochange("src/b.ts", "src/c.ts", 1).',
  ]);
  // The first commit added all three; b imports (and uses) a, so only a–c and b–c are hidden.
  assert.deepEqual(ask("hidden_coupling(A, B, N)"), [
    'hidden_coupling("src/a.ts", "src/c.ts", 4).',
    'hidden_coupling("src/b.ts", "src/c.ts", 1).',
  ]);
  assert.deepEqual(ask('confidence("src/c.ts", B, X)'), ['confidence("src/c.ts", "src/a.ts", 1.0).', 'confidence("src/c.ts", "src/b.ts", 0.25).']);
  assert.deepEqual(ask('revisions("src/a.ts", N)'), ['revisions("src/a.ts", 5).']);
  assert.deepEqual(ask('churn("src/a.ts", A, D)'), ['churn("src/a.ts", 5, 0).']);
  assert.deepEqual(ask('main_author("src/a.ts", E, S)'), ['main_author("src/a.ts", "ada@example.com", 0.8).']);
  assert.deepEqual(ask('author_commits("src/b.ts", E, N)'), [
    'author_commits("src/b.ts", "ada@example.com", 1).',
    'author_commits("src/b.ts", "bo@example.com", 1).',
  ]);
  fs.rmSync(dir, { recursive: true, force: true });
});
