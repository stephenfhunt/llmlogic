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

## 2026-07-25/26 — Practice: stop building on doc claims that stopped being true

Same day, after `bugs/001`. Prompted by asking why this week's defects kept
having the same shape. They do, and the diagnosis was already written down — four
bug files independently prescribed the same fix and none of them promoted it to
practice.

**Done**
- **The evidence.** Every "surface form A means the same as form B" claim carrying
  a *property* has held (named ≡ positional, body order); both carrying only a
  *unit test* became defects (`bugs/001`, `bugs/002`). `bugs/003` is the doc-side
  variant — one rule stated in four sections, "updating three was enough to look
  done". Fixing `bugs/001` meant editing that rule in nine places.
- **`AGENTS.md` gained a "Changing what already exists" section**: the
  current-state vs append-only document split, one normative home per rule, sweep
  §17 when a rule moves, annotate a decision when its consequences land. Folded
  into the existing list rather than bolted on.
- **Session-end checkpoint** gained *Removed* and *annotate §17* — the annotation
  rule needed a trigger that already fires, and every other worklog field rewards
  adding.
- **§17 preamble**: one amendment vocabulary (Falsified / Superseded / Amended /
  **Consequences**) replacing the six in use. The fourth is new and is the only
  marker that can record a decision working out *well*.
- **Three §17 entries annotated** — §8 builtins, Phase D, the `-q` classifier —
  each saying what it cost and whether the rationale held.
- **testing.md C8**: `A15` (inline ≡ hand-hoisted, via `testgen::hoist_atom_args`
  and a new `alpha_eq`), disjunction ≡ separate rules, and `-q` ≡ file program.
  The last **fails and is `#[ignore]`d** as `bugs/002`'s acceptance criterion; it
  shrinks to `d(K) :- n(K,V), V = 0 ; n(K,V), V = 0` and the seed is recorded.
  Known failures are now three, each an open defect with a test saying what
  correct looks like.

**Decided**
- **Widen the guard, don't just write the rule.** The prescription existed in four
  places already; what was missing was anyone executing it. So the practice change
  ships with the properties it calls for, and its first act is a *failing* test for
  an open defect rather than another prose instruction.
- **`alpha_eq` over `==` for A15.** The hand-written variable is named where
  lowering mints an anonymous slot, and slot numbering differs. Comparing modulo
  renaming is not a weakening — "engine-identical" is exactly the claim, since the
  evaluator never reads `var_names`.
- **The §17 split is a rewrite, not a move.** Sized as a move it will be done as
  one, and the residue — §1–§16 narrating changes and restating rules — is the
  actual problem. ROADMAP item restated with that acceptance criterion.

**Removed**
- Five lines of change-narration from `src/lower.rs`'s negation-safety comment, a
  story already told in §17 *and* `bugs/resolved/001` — written three times,
  needed once.
- §16.2's "relaxed from *positively* bound" and §8's bare date parenthetical.
- The `TBD → Draft → Stable` ladder and "before marking a section *Stable*" from
  `AGENTS.md` — nothing has ever reached `Stable`, so the instruction was
  unfollowable. The dead rung itself stays a queued ROADMAP item.
- `engine::positively_bound` (earlier in the day, superseded by the shared
  scheduler binding set).

**The suite earned itself immediately.** `a15` failed on a generated case within
minutes of landing — not an engine bug but a flaw in the property: it rewrote
*queries*, where the two spellings genuinely differ (a hand-written variable is
an answer variable per §14; lowering's anonymous slot is not). Restricted to rule
bodies, with the reason recorded rather than silently narrowed. Chasing it
surfaced **`bugs/005`**: `?- p("a", 1 + 1).` prints nothing where `?- p("a", 2).`
prints the fact — the third instance of this session's class, found by the thing
built for it. §14's "one shape the closure does not cover yet" is reachable from
an ordinary-looking query.

**`CLAUDE.md` → `AGENTS.md` symlink.** Checked the docs rather than assuming:
Claude Code reads `CLAUDE.md`, *not* `AGENTS.md`, and has a documented section on
exactly this bridge. So until now the practice rules had no trigger — this session
read `AGENTS.md` on its own initiative, not because anything loaded it. Verify
with `/context` under **Memory files**.

**Next up**
- **`absent` × negation** (negation item 1) — the only open negation thread, and
  unblocked. Two `#[ignore]`d tests are the acceptance criterion.
- **`bugs/002`** is the cheapest defect to close: the fix sketch is written, and
  the property already exists and fails. Delete the `#[ignore]` to finish.
- **`bugs/005`** is new and small — an output-shape defect in `api.rs`, no engine
  or lowering change implied.
- `bugs/003` (three false spec assertions) and `bugs/004` (blocked on the
  termination design item).
- **Agent-context pass, deferred deliberately** (user call): `AGENTS.md` is now
  238 lines against a documented target of under 200 for a loaded instruction
  file, and longer files measurably reduce adherence. The working-style rules are
  the natural thing to move into `.claude/rules/` — same load behaviour, scopable,
  and it keeps the root file short. Do this before adding more to `AGENTS.md`.
- **Known coverage gap, recorded not fixed:** E3's `replay` returns `None` on any
  `Premise::Builtin`, so derivation replay has never covered §8 builtins at all.
  Widening it means folding builtin premises into the replayed environment.
