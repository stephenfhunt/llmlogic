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

## 2026-07-27 — Source-analysis dogfood, second run: the engine on 22k facts

Re-ran the 2026-07-23 use case now that §13 imports and §9 aggregation exist.
Facts from `syn` (a throwaway scratchpad extractor, not committed), not regex.
Docs only in the engine; one build-tooling line. 395 tests pass, clippy and
rustfmt clean, `cargo package-skill` green with `recipes/` in the bundle.

**Done**
- **`skill/recipes/source-analysis.md`** — the recipe deferred in 2026-07-23
  pending §13/§9. `cargo package-skill` now bundles `recipes/`, so it ships with
  the skill rather than living only in this checkout.
- **`bugs/006`** — `<` rejects strings while `min`/`max` order them, against §8's
  own text; `src/typecheck.rs` cites §8 for both halves. Found by needing `A < B`
  to canonicalise a pair, not by reading the spec.
- **Findings, each source-verified before being written down:** `engine ↔
  provenance` is a genuine import cycle; three mutual-recursion clusters, all
  reached through the **aggregate** arm, including `parse_primary →
  parse_aggregate → parse_expr`; `print_fact_lines` is public with zero callers
  and zero tests; `lower.rs ↔ provenance.rs` co-change 6× with no `use` edge,
  coupled through `ir::BodyIdx`'s meaning; §1, §2, §6 cited by no source line. No
  dead private function, no unconstructed variant — the negatives are the part
  prose cannot claim credibly.
- **First evaluation numbers**, filed under *Profile the engine*: importing 28k
  facts across 15 JSONL files is 0.4 s, so the cost is evaluation — a 5,248-edge
  closure deriving 173k pairs took ~13 s, and its self-join did not finish in 2
  minutes (25 s anchored on direct edges, 2.3 s once the edges were resolved).

**Decided**
- **The engine holds the uncertainty, not the extractor.** 2026-07-23 said the
  fix for bad facts was a better parser. `syn` removed the regex error class and
  introduced its own — macro bodies opaque (7% of functions invisible), bare
  callee names merging `Model::new` with `Lexer::new`. What worked was resolving
  in Datalog in confidence tiers, leftovers kept as a *counted* relation: 1,504
  certain edges, 2,458 likely, conclusion stable across both.
- **Two language limits push work back into the fact producer** — no string
  ordering (`bugs/006`), no string operations at all. Both forced extractor
  columns that exist only to serve the query. New ROADMAP item under *Surface
  uniformity*, where it is the fourth of a genre.
- **The count-distinct trap is worse than §9 says** and §13 is why: over a wide
  imported table the wildcard is never written. 36 call sites where the question
  wanted 20 callers. §17 and the ROADMAP item now carry the measurement.

**Removed**
- The 2026-07-27 GitHub-publication entry rotated verbatim to
  `worklog-archive/2026-07.md`. Nothing else — the ROADMAP recipe item was
  completed rather than deleted, and no doc claim was found stale this session.

**Next up**
- **`bugs/006` is decided and specified, not done** — widen the typechecker
  (owner's call this session). The file now carries the exact three lines, the
  finding that *both evaluators already handle it* so nothing below typecheck
  changes, and the warning that `b1_comparison_programs_agree`'s generator emits
  integers only and so cannot cover the widened surface. `004` stays blocked on
  Termination.
- Unchanged: the two design sessions (**`absent` × negation**, **Termination**),
  the **§17 restructure**, and **CI**.

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
