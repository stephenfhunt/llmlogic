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

## 2026-09-13 (later that night) — the recorder cut by constants: `pointsto.dl` `?why` 1,428 → 848 MB, every derivation kept

Asked where the derivation-tracking changes stood: references rather than copies
a clear win, one derivation per fact held back unless it is what code-analysis
hits. It is not. Planned, approved, and steps 1–3 built.

**Done** — datalog `cargo test` (every binary), clippy, fmt; each commit green
- **The answer.** Library programs store 1.11–1.12 derivations per fact, 95–97%
  of them first-round. They pay for representation and base-fact bookkeeping,
  not count, and the base set does not need the one-derivation direction.
- `91b730a` — no base set: a held fact with no round stamp is base. **E11**,
  stated from `program.facts`. Mutations killed: `insert_base` stamps round 0; a
  rediscovered fact is stamped. Guard: 4 of 48.
- `ef82398` — a premise is a fact wide (80 → 32 bytes). Every kind but `Fact` is
  boxed, `NoMatch` too, since two inline variants of one shape leave no niche for
  the tag (40). Pinned by `a_premise_is_a_fact_wide`.
- Against a frozen `8c6bc91` release binary:
  - `?why` is byte-identical on 43 §16-corpus goals and both `@grafana/ui` goals;
  - `pointsto.dl` 1,428 MB / 14.7 s → 848 MB / 9.1 s;
  - `callreach.dl` 466 → 389 MB.
- `Deep` draws, with the note's harness rebased in `~/.cache/recorder-wt`:
  idx 129 6.96 → 5.43 GB, idx 119 3.58 → 2.78 GB. Most of what remains is cloned
  premise tuples (3.06 GB on idx 129).
- **The deep run still does not finish**: it failed an allocation at 372 s and
  18.1 GB under a 20 GB cap. In `bugs/015`, with a 16 GB per-commit-tier draw
  seen once and not reproduced.

**Decided**
- datalog §17 2026-07-19 ***Consequences 2026-09-13 (later)***: the reopening
  stands, narrowed to what all-derivations costs a dense program.
- One derivation per fact is not built; the plan was the user's.

**Removed** — `Model::base`; base facts' round-0 stamps; `Premise`'s inline
payloads; the note's unverified `base` direction; the oldest worklog entry.

**Next up**
- **The user's calls** (note § After the first two cuts):
  - premises as fact references — stable ids into evaluator storage, the
    remaining constant;
  - dropping `api::run`'s fact copy for a `?why` (233 MB on `pointsto.dl`);
  - whether `bugs/015` needs the count question after all.
- Re-learned: `ps -C cc,c++` misses DuckDB's `clang++` workers, so a working
  release build looked hung and was killed once.
- **Open**: `datalog/bugs/015`.

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
