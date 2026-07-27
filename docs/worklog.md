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

## 2026-07-27 — Public on GitHub: a top-level README, private remote out of the tree

**Docs and metadata only;** 385 tests pass, clippy clean. Nothing pushed yet.

**Done**
- **A top-level `README.md`** — the repo had none, so a visitor landed on
  `AGENTS.md`, which is agent guidance, not an introduction. Every console block
  in it is verbatim output from a release build, re-run from an empty directory
  with the binary on `PATH` to confirm a reader pasting it gets those bytes.
- **`vault` gone from `AGENTS.md`** (its only occurrence in tracked content),
  `repository` set in `Cargo.toml`, and `lib.rs`'s crate doc corrected — it had
  opened with "an early scaffold … most modules are stubs pending the language
  specification" since before the parser landed.
- Checked before shipping: all nine relative README links resolve, and a grep for
  the private remote, absolute paths, and the author's address across tracked
  files is empty.

**Decided**
- **Public and unadvertised** (user call), not private-with-collaborators: link-shareable
  now, portfolio-usable later, at the cost of being indexed and forkable.
- **Licensing stays deferred even so** — all rights reserved by default. Because
  the repo is public the README says this outright, so the absence reads as a
  decision rather than an oversight. `Cargo.toml` still carries no `license`.
- **The root README's subject is `llmlogic`, not `datalog`** (user call): the goal
  is a holistic set of tools and skills for formal reasoning with LLMs, of which
  the engine is the first instance. Written so a second project costs one table
  row — the same constraint `AGENTS.md` already lives under.
- **The process docs are the portfolio piece, not noise.** The README's *How it's
  built* section foregrounds spec-first design, §17, this worklog, `bugs/`,
  properties-over-unit-tests, and `editing-docs.md`. Publishing them is the choice
  being made; they were written for an audience of one until now.
- **No CI this pass, no history rewrite** — DuckDB builds bundled from source, so
  cold runs are slow enough to want their own session; and the `Claude-Session:`
  trailers and author address on all 84 commits go public knowingly.

**Removed**
- `AGENTS.md`'s `The remote is `vault`.`; `lib.rs`'s scaffold paragraph;
  `Cargo.toml`'s "no repository URL until a remote exists" comment, false the
  moment the remote existed. All three are the same failure the *Practice* entry
  (now archived) named: a current-state claim nothing routes you back to.
- The 2026-07-25/26 entry rotated verbatim to `worklog-archive/2026-07.md`.

**Next up**
- **Push**: `git push -u github trunk` — the remote is `github`; `vault` stays as
  the local backup. Then the GitHub *About* panel (description, topics), which is
  not stored in-repo, and a look at the rendered README.
- **CI** (`cargo test`/`clippy`/`fmt`) deferred, not rejected. Nothing in §17 to
  annotate — this session touched no language decision.
- Unchanged: **`bugs/005`** pending the fold-vs-plumb call, **`absent` × negation**,
  the **§17 restructure**, `003`, `004`.

## 2026-07-26 — `bugs/002`: a disjunctive rule survives `-q`

The cheapest open defect, taken from the `bugs/` queue and closed in `0c86ab1`.
385 tests pass, clippy clean.

**Done**
- **`-q 'r(X) :- p(X), X < 5 ; p(X), s(X)'` answers `r(1). r(9).`**, byte-identical
  to the file form. The classifier matched a *single* statement; the parser
  expands a top-level `;` into one clause per disjunct sharing the head, so a
  disjunctive rule fell through to the query path and reported a syntax error
  about a grammar the user never wrote. Now: N clauses, non-empty bodies, same
  printed head, query taken from the first.
  Heads compared as *printed text*, not as ASTs — span-free without a
  span-zeroing helper, and it keeps `a(X) :- p(X). b(X) :- p(X)` a parse error
  rather than a silent partial answer.
- **`dash_q_rule_equals_the_same_rule_in_a_file` un-`#[ignore]`d** (testing.md
  C8). Known failures three → two, both absent × negation.
- §14 prose, §17 *Amended*, `bugs/002` → `bugs/resolved/` with a resolution note,
  ROADMAP blockquote, testing.md C8.

**Decided**
- **The prose was the carrier, not the code.** §14 said "a single clause with a
  non-empty body is a **rule**" — a description of a Rust `match` arm that read
  as a language rule, so nothing flagged it when the parser started desugaring
  one clause into several. Restated as "one rule, however many clauses it
  desugars to". This is `bugs/003`'s drift mechanism again, one layer down: not a
  rule stated in four places, but a rule stated once *about the implementation*.
- **The property paid for itself completely.** First time here a property was
  written before the fix it specified: closing the defect was deleting one
  `#[ignore]`, the recorded seed replayed the shrunk case first run, and seven
  lines of `api.rs` changed. C8 has now *closed* a defect, not just found them.
- **`bugs/005` needs a design call before anyone codes it.** It was the natural
  companion (same §14 family, same `api.rs`), but its own fix sketch proposes
  plumbing hoist-origin into the IR, which would collide with A15's inline ≡
  hand-hoisted claim. Constant-folding a ground compound argument in a *query* —
  the rule facts already use (`ArgMode::Fold`) — reaches the same output with no
  IR change. Pick between them first; the sketch in the bug file is not the
  obvious answer it looks like.

**Removed**
- The "`002` is the cheapest to close" paragraph from ROADMAP (its whole content
  was instructions for work now done), the stale `#[ignore]` and its four-line
  justification, and §14's `match`-arm sentence.

**Next up**
- **`bugs/005`** is now the cheapest defect, pending the fold-vs-plumb call above.
- Unchanged: **`absent` × negation** (with §6, per ROADMAP), the **§17
  restructure** (`notes/decisions-log-restructure.md`), `003`, `004`.
- Path-scoped loading **verified live**: root `CLAUDE.md` loads at launch,
  `editing-docs.md` and `datalog/CLAUDE.md` only after reading a file each scopes
  to. Closes the previous entry's first *Next up*.

## 2026-07-26 — Agent-context pass: session-start load 2,149 → 301 lines

Prompted with two references on context engineering for Claude 5 models; the
useful half was checking their claims, and the previous *Next up*, against Claude
Code's documented loading behaviour first. **Docs only;** 382 tests pass, clean.

**Done**
- **Root `AGENTS.md` 238 → 77, repo-wide only.** 60% of it was datalog-specific
  and now lives in `datalog/AGENTS.md` (109 lines) with a `CLAUDE.md` symlink, so
  it loads only when reading files in that directory. A new project costs the root
  file one row in its project table, not a section.
- **`docs/rules/editing-docs.md`** — the doc-editing discipline, path-scoped, plus
  new **length caps** (worklog entry ~50, ROADMAP item ~3, decision ~15) with each
  project's `notes/` as the overflow. Stated generically: which documents a project
  has is that project's business, not the repo rule's.
- **Worklog 1,613 → 224**, 24 entries rotated verbatim to `worklog-archive/`.
  `.gitignore` now splits per-checkout *state* from shared *config*; a fresh clone
  built from scratch gave 382 passed, identical to the working tree.
- **`spec.md` §17 restructure written up** in `datalog/notes/` with the
  measurements; its ROADMAP item collapsed 23 lines → a pointer, the caps' first use.

**Decided**
- **`.claude/rules/` without `paths:` buys no context** — it loads at launch like
  `.claude/CLAUDE.md`; `@path` imports likewise. Only path-scoping defers. The
  previous *Next up* proposed it as a budget fix; it isn't one.
- **Deletion was the fix, extraction the sideshow** — 97 of 107 lines were deleted.
  The working-style rules are the file's *best* content; history and derivable
  layout were the bloat.
- **Repo-wide vs per-project is the axis that scales** (user call). Encoding one
  project's document names in a repo-level rule was the tell — `*/spec.md` assumes
  every project has a spec. The repo rule owns the *discipline*; each project's
  `AGENTS.md` owns its *document map*.
- **A project's skill is a deliverable, not development infrastructure** (user
  call) — `.claude/skills/` stays gitignored; one always-loaded description per
  project does not scale. Not a contradiction of availability-over-gating: that
  trades build time, this trades context.
- **Caps are the only lever that bends the curve.** Cleanup without them is
  re-accreted within a month; this entry was 130 lines before its own cap applied.

**Removed**
- 161 lines from root `AGENTS.md`, 1,389 from this file, 20 from the ROADMAP item,
  and the blanket `.claude/` ignore that left guidance unable to load itself.

**Next up**
- **Verify loading live** — unverifiable from inside this session. `/context`
  should show root `CLAUDE.md` but not `editing-docs.md` or `datalog/CLAUDE.md`;
  both should appear after reading a matching file. `/doctor` is a second opinion.
- **§17 restructure** and **`ROADMAP.md`'s 9 long items** (299 of 435 lines) are
  the remaining half — see `datalog/notes/decisions-log-restructure.md`.
- Unchanged: **`absent` × negation**, **`bugs/002`** (cheapest), **`005`**, `003`/`004`.
