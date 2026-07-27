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

## 2026-07-27 — `bugs/003`: the safety rule had six homes, not four

The last unblocked defect. Docs plus one test; no engine change. 395 tests pass
(393 + 3 − 1), clippy clean, `--ignored` still exactly two.

**Done**
- **§10 is now the single normative statement of what "bound" means**, and says so
  in its own text. §7, §8 (×2), §9 (×2) and §14 keep their local content and refer
  to §10 rather than re-listing the three binders.
- **A sixth site the bug file never named**, and the second false one: §8's
  "Mode / safety (§10)" said `bound by a positive atom` — the same 2026-07-25
  residue as §8:521. Both falsified against the release binary: `X = A + 1, X > 2`
  and `N = count {…}, N > 1` are accepted today.
- **§3's reserved list completed** — two words behind the lexer (`is`, `absent`) —
  plus the converse it never stated: `table`, the five type names and the five
  aggregate operators are *contextual*, so `int(2).` is a legal relation. §8's
  "precedence is deferred to the parser" italic deleted; resolved 2026-07-22, 100
  lines above it in the same section.
- **The optional acceptance criterion was taken**: two table-driven parser tests
  walk §3's two lists, so a keyword added to the lexer without a §3 edit now fails
  a test. Also §17 ×2, testing.md Phase D, ROADMAP, `bugs/003` → `resolved/`, and
  `editing-docs.md`'s citation of this bug (it said "four sections").

**Decided**
- **Counting the sites was never the fix.** Opened on "four", re-verified this
  morning at "four", swept at six — and two sessions each found a *different*
  subset, including the 2026-07-25 sweep run with this bug file open, whose whole
  subject was the enumeration. So the fix had to be structural, not a better pass.
- **A decision entry must route surface consequences back to their canonical
  section.** The `is [not] absent` design (2026-07-24) added a reserved word and
  recorded it nowhere; §3 owns that list and the lexer will not tell it. Annotated
  there, since the next feature adding a keyword gets read there, not here.
- **Non-vacuity by swapping the two lists** — `table` into the reserved one, `is`
  into the contextual one, both tests fail with the right message. `bugs/005`'s
  discipline, cheap here: the two assert opposite outcomes over one shape.

**Removed**
- `absent_is_reserved_and_cannot_name_a_relation`, subsumed by the table; §8's
  precedence italic; five re-enumerations of the three binders. From ROADMAP, the
  "`003` the cheapest to close" paragraph and the `bugs/001-003` grouping; from
  the §17-restructure note, "pairs with `bugs/003`".
- The 2026-07-26 `bugs/002` entry rotated verbatim to `worklog-archive/2026-07.md`.

**Next up**
- **The `bugs/` queue has nothing unblocked in it.** `004` waits on Termination,
  so the next work is a **design session**: `absent` × negation (which also
  unblocks §6) or Termination & value-creating recursion.
- Cheapest non-design item for a short session: **parenthesized expressions**
  (`src/parser.rs:619-628`, `src/print.rs:169` — the printer is the real work).
- Unchanged: the **§17 restructure** (own session), **CI** (deferred, not
  rejected), the browser-only GitHub *About* panel.

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
