# Worklog

A running handoff log for chaining agentic coding sessions. Each session ends by
adding an entry so the next session can get oriented in seconds — without re-reading
raw transcripts (Claude Code auto-saves those under
`~/.claude/projects/<repo-slug>/*.jsonl`; resume with `claude --resume`).

**Conventions**
- Newest entry on top (reverse-chronological).
- Keep each entry short and high-signal. Four fields:
  - **Done** — what changed this session (link commits/PRs where useful).
  - **Decided** — key decisions made (design decisions also go in `datalog/spec.md`
    §17; note them here too so the timeline is complete).
  - **Removed** — what you deleted, merged, or replaced.
  - **Next up** — the concrete next threads, so the following session starts oriented.
- This is a curated summary, not a transcript. Don't paste raw output here.
- **Entries stay under ~50 lines**, and **this file keeps the most recent three.**
  Older entries rotate verbatim into [`worklog-archive/`](worklog-archive/) by
  month, so session-start orientation stays a fixed cost instead of a growing one.
  A session needing more room than that is describing work that wants its own
  document — put the long form in `datalog/notes/` and link it from the entry.
  (50 rather than 40 because the entry that set this rule landed at 48, and a cap
  nobody meets gets ignored — cf. the `Stable` rung, deleted for the same reason.)

---

## 2026-09-13 (night) — code-analysis from the user's chair: the playbook investigates, and its texts stop leaking

Asked what the skill tells a user, what it won't, and whether agents dig or only
run the table. Assessed in plan mode; the user chose the playbook follow-up, then
asked for an audit of what in the skill's texts leaks this project's background.

**Done** — code-facts `npm test`, typecheck; bundle rebuilt (`dist/` untracked)
- **Assessment**: the facts and libraries outrun the playbook, which taught
  measuring, not investigating or synthesising; no library reads the quality
  layer, and nothing diffs commits, recovers structure or measures digging.
- **`SKILL.md`**:
  - find the question first; *How to investigate* (go down, refute, `?whynot`
    on negatives, sample precision);
  - impact and two-commit `diff` recipes, and a hazards row; *Synthesise, then
    report*.
  - Each recipe ran on `~/.cache/code-facts/grafana-ui` first (impact 1.6 s,
    drill 11 s); the diff ran on code-facts itself.
- **The leak sweep.** The bundle read like this repo's notebook:
  - "the project above" nine times, and sqlparse with the experiments' answer
    key;
  - bug ids and "found dogfooding" in seven library headers;
  - the extractor's size note and dated schema comment;
  - `datalog.md`'s links out of the bundle, and `bring-your-own.md`'s dated
    crate narrative.
  - All rewritten as cases. `SKILL.md` now reads for a novel project: its own
    scratch directory, a user it may not reach, worktree cleanup, its report
    conventions.
- `reference/datalog.md` and `bring-your-own.md` are this skill's own files, not
  symlinks. `test/published-text.test.ts` fails on provenance in what ships.

**Decided** (code-analysis `decisions.md`)
- 2026-09-13 (night): no impact library, no diff tool.
- 2026-09-13 (night ii), the user's: **skill texts are published for a stranger's
  project**; each skill owns its texts. This supersedes 2026-09-11's
  single-homing.

**Removed** — the two reference symlinks and `package.sh`'s frontmatter
stripping; every provenance line above; the extractor's size note; the probe
files and scratch worktree; the oldest worklog entry.

**Next up**
- **Measure the playbook**: re-run the `@grafana/ui` dogfood with a fresh agent
  and only the bundle.
- **The datalog skill has the same leaks** (`More` links, dated recipe
  narrative) — its own session, since the experiments measure it.
- The bundle still ships `tools/code-facts/src`, whose comments name subjects and
  bugs.
- Queued on code-analysis ROADMAP: quality-layer libraries, structure recovery,
  an eval of open-ended analysis. **Open**: `datalog/bugs/015`.

## 2026-09-13 (evening) — the dogfood bugs: eight closed with their properties; the ninth found the recorder

Asked to work the bugs Grafana and VS Code filed and to build quality into the
tooling. Planned. The user ruled on four choices: 003 as a library, 009 and 014
designed then built, 002's weaker relation, and 015's option. The session then
withdrew 015's option.

**Done**
- **code-analysis** (`npm test` 126/126): `dd72fef` 001 file-name
  `is_generated`; `3735f44` 002 `unresolved_package` / `unresolved_bare`;
  `3d045a5` 003 `lib/exports.dl`; `c4ed45d` + `687163f` 004
  `uncounted_dependent`; `552e11c` 005 docs only.
- **datalog** (pinned `.err` byte-identical throughout):
  - `2e05fb3` 013: one error per unsafe `;`-alternative; **A16**.
  - `f793822` 014: declared types seed untyped classes; **C17**.
  - `597117d` 009: `Error.related`, `fact_spans`; **C18**.
  - `e678579` B13 compares proofs one step per live fact (`ProofTree::step`),
    exactly as strong as trees by induction on first round. 015 stays open.
  - `c4e27d8` `datalog/notes/recorder-at-scale.md`, measured in a scratch
    worktree (`~/.cache/recorder-wt`, kept for the design session).
- **Caught on the subject, not the fixture:** 004's first fix was green and empty
  on `@grafana/ui`. A re-export makes no `ref`.

**Decided**
- datalog §17 2026-09-13 (later): declarations constrain inference, seeded after
  `gather`.
- datalog §17 2026-09-13 (later ii): related locations, on the column form only.
- code-analysis 2026-09-13 (later): an unresolved import claims no package.
- **015 gets no test budget** (the user's): the recorder is the defect. §17
  2026-07-19 all-derivations is ***Reopened***.
- Withdrawn: B13 unrecorded at `Deep`. Its justification, E9, is not tiered.

**Removed** — `unplaced_dependent`; the `TypeEnv` declared-type fallback; the
hand-written dead-export recipe; ROADMAP's hand count of resolved bugs; 009's
`#[ignore]`s; the oldest worklog entry.

**Next up**
- **The recorder design session** (datalog ROADMAP § Provenance surface; the
  note's five questions). What it has to work from:
  - real library programs store about 1.1 derivations per fact but pay 4.5–5.5×
    in copies and base-fact bookkeeping (a `?why` holds base facts four times);
  - dense `Deep` programs are 93–99% later-round rediscoveries;
  - the directions raised are one derivation per fact chosen at establishment,
    and premises as references.
- Until then the deep run does not finish, and § Performance's gate waits.
- Move the early check past lowering for fully declared programs; tier the rest.
- Re-learned: `systemd-run --user -p MemoryMax` is not enforced here; use
  `prlimit --as`.
- **Open**: `datalog/bugs/015`.

## 2026-09-13 (later) — the TypeScript extractor profiled: 19.35 → 16.37 GB on Grafana, facts identical

Asked to profile the TypeScript extractor for memory, giving up no fact data,
and take any low-hanging fruit. Planned from a by-phase profile (forced GC per
phase), then shipped in three commits, each checked against a frozen trunk
extraction by `sha256sum facts/*.jsonl`.

**Done** — code-facts `npm test`, typecheck; each intermediate tree tested on its own
- **`8adb540`** — programs share each parsed file where their settings would parse
  and bind it the same. On Grafana's 16 tsconfigs, 44,954 `SourceFile`s had held
  13,768 paths: load **7,043 → 2,710 MB** retained, 36.5 → 15 s. **P9**: output
  identical with sharing on and off.
- **`2c76fe6`** — declaration keys name a file by an integer: `ids` +742 → +529 MB.
- **`444c97b`** — numstat as parallel `git log --no-walk` jobs via a child
  process: git layer 43 → 11 s. A test compares rows across job counts.
- **End to end** (single runs): frontend `refs,quality,git` **382 s / 19.35 GB →
  361 s / 16.37 GB**; `@grafana/ui` **67.3 s / 2.26 GB → 37.0 s / 2.05 GB**. Both
  digests identical. Measurements: `code-analysis/notes/code-facts.md` § The
  extractor's own cost.
- **Mutations**: keying the share on file name alone was **green at first**. Under
  `nodenext` the default `moduleDetection` binds every `.ts` file as a module, so
  only `legacy` differs, and a randomly placed witness hit 4% of runs. Pinned, it
  is 49%, and the mutant is red 3 of 3. Dropping a chunk's last commit → red.
  Ignoring the pathspec in numstat stayed green; it is output-invisible.

**Decided** (`code-analysis/decisions.md` 2026-09-13; the first the user's)
- **The extractor represents the project as its tsconfigs configure it** — no
  setting of its own; an optimisation must equal per-tsconfig output.
- The share key is TypeScript's `DocumentRegistry` key without `pathsBasePath`.
- Numstat uses a child process, not a worker thread, because a synchronous
  caller blocked on a worker that fails to load hangs.
- Compact keys only if they measured ≥100 MB — they did.

**Removed**
- The single-pass `git log --numstat`; a worker-thread draft of it, replaced
  before commit; the oldest worklog entry.

**Next up**
- **The extractor's remaining peak is TypeScript's checkers — not queued.** The
  user's ruling: no working around TypeScript's cost, since users compile the
  project anyway (`code-analysis/decisions.md` 2026-09-13, amended).
- **`datalog/bugs/015`** — still the user's call; the § Performance gate waits on it.
- **Open**: `datalog/bugs/009`, `013`, `014`, `015`; `code-analysis/bugs/001`–`005`.
