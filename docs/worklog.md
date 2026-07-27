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

## 2026-07-27 — `bugs/005`: a query folds a ground computed argument

The cheapest open defect, taken after making the design call the ROADMAP held it
on. 393 tests pass (385 + 8), clippy clean, `--ignored` still exactly two.

**Done**
- **`?- p("a", 1 + 1).` answers `p("a", 2).`**, identical to the folded spelling
  and verified against the release binary. A query lowers a compound atom
  argument with a new `ArgMode::FoldGround` — fold when ground, hoist otherwise —
  so it stays the single atom §14 reads its output shape from. `api.rs` did not
  change at all.
- **One case more than the report described:** `?- p(X, 1 + 1).` printed the
  weaker `answer("a")` and now prints `p("a", 2).` — the same defect one variable
  short of ground.
- Four unit tests, two properties, two non-vacuity guards; §5/§14/§17,
  testing.md C8, `bugs/005` → `resolved/`, ROADMAP blockquote.

**Decided**
- **Fold, not plumb — the bug file's own fix sketch was rejected.** Plumbing
  hoist-origin into the IR costs a new field *and* A15's claim that inline and
  hand-hoisted arguments lower to the same program. Its "cheaper alternative" (an
  existence-check case in `api.rs`) is not cheaper either: `V` is unprojected, so
  the row must be reconstructed. Scoped to queries, so A15 needs no weakening.
  Both rejections and what they would have cost are in §17.
- **A non-vacuity guard can be green while the property is useless.** Written
  first as a rewrite over `arb_ast_program`, the property **passed with the bug
  present**: two spellings of a query that matches nothing both print nothing,
  and over arbitrary programs a query that computes *and matches* is vanishingly
  rare. The guard counted ground compound arguments in queries — never the
  binding constraint.
- **So: revert the fix and watch the property fail, every time.** That is the
  only direct evidence a property tests what its name says, and it is how this
  one was caught. C8's two earlier properties were written against a live defect,
  which supplied that evidence for free; this one was not.
- **The targeted generator is the pattern, not the exception.** All three C8
  properties now build both spellings from a purpose-built generator
  (`arb_ground_query_spellings` builds the expression backwards from a value the
  EDB contains). It failed on its first generated case; the seed is recorded.

**Removed**
- ROADMAP's "`005` is now the cheapest to close" paragraph — its whole content was
  instructions for work now done — and §14's "a pure existence check", which
  overstated a gap that is now only the *multi-atom* case.
- The 2026-07-26 agent-context entry rotated verbatim to `worklog-archive/2026-07.md`.

**Next up**
- **`bugs/003`** is now the cheapest: three normative spec errors, one sitting,
  no code change. `004` stays blocked on Termination.
- The two design sessions are unchanged and still want their own: **`absent` ×
  negation** (with §6) and **Termination & value-creating recursion**.
- Unchanged: the **§17 restructure**, and **CI** (deferred, not rejected).
- Still browser-only from the last session: the GitHub *About* panel and a look
  at the rendered README. `gh` is not installed in this checkout.

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
