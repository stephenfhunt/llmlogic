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

## 2026-07-29 — `absent` × negation: the anti-join is not a join

ROADMAP negation item 1, open since 2026-07-25 — design and implementation in
one sitting, since §17 had already argued the direction. 407 tests pass (401 +
6), clippy and rustfmt clean, `--ignored` now **zero** known failures, not two.

**Done**
- **`q(X) :- p(X), not p(X).` derives nothing.** `AbsentPattern::matches`
  compares closed slots structurally — one line, one choke point, since the
  anti-join is its only caller. `engine::naive`'s `refutes` re-expresses the rule
  independently, per that module's standing rule.
- **§4 gained the four-site table**, the real deliverable: join and comparison
  semantic, group key semantic, anti-join structural, dedup and `Ord` structural.
  The split had lived only in `ir.rs`'s doc comment, whose "every join, anti-join
  and unification site must use `unifies_with`" was about to become false.
- **C9** (`c9_a_body_and_its_negation_derive_nothing`), a `contra` rule in
  `absent_ir`, the non-vacuity guard — all three plus `b1_absent_programs_agree`
  confirmed red with the fix reverted.

**Decided**
- **Only one of the two "broken laws" was a defect.** The open question paired
  non-contradiction with idempotence of conjunction; not one phenomenon.
  Idempotence stays broken over `absent` **by design** — a *join* property, and
  restoring it means giving up `NULL ≠ NULL`. The proof is mechanical rather than
  argued: `repeating_a_body_literal_drops_absent_rows` passes identically with the
  fix and without it, while every negation property flips.
- **The price, measured before it was accepted.** On the §16.8 sparse shape a food
  whose id is `absent` used to survive `not measurement(F, _)` even with
  `measurement(absent, 3)` stored. It no longer does. SQL's answer.
- **§6 was deliberately *not* folded in**, against the 2026-07-25 review: it
  should describe a ratified semantics, not one being decided as it is written.
  Amended there rather than here.
- **A four-day-old scope note earned its keep.** That same review predicted this
  session would ship a three-site table omitting the aggregate group key — which
  is exactly what the plan had. Re-verified `g(absent, 0)` and pinned it.

**Removed**
- Both `#[ignore]`d tests: one un-ignored, one converted. With them, ROADMAP's
  "`--ignored` should report exactly **two** known failures" and testing.md's
  "two logical laws absent still breaks" paragraph. Nothing else deleted — but
  `SKILL.md`'s "Missing data" bullet was *incomplete*, the source-analysis
  recipe's `not used(F)` being exactly the shape that changed, so it gained the
  negation case.
- The 2026-07-27 `bugs/003` entry rotated verbatim to `worklog-archive/2026-07.md`.

**Next up**
- **Termination & value-creating recursion** — the remaining design session and
  the one that unblocks `004`. Direction decided (static error, not fuel); open is
  whether cost-accumulating transitive closure gets an escape hatch.
- **§6's extension** is now the better-prepared of the two: one unknown lighter,
  with §4's table to describe.
- Unchanged: **parenthesized expressions** (cheapest non-design item), the **§17
  restructure**, **CI**, the GitHub *About* panel.

## 2026-07-27 — `bugs/006`: `<` orders every primitive, and B1 could not have known

The last unblocked defect. Three lines deleted from the type checker; no spec or
evaluator change. 401 tests pass (395 + 6), clippy and rustfmt clean, `--ignored`
still exactly two.

**Done**
- **`p(X), p(Y), X < Y` answers over strings, symbols and bools**, verified
  against the release binary; `min`/`max` unchanged, `1 < "a"` still a type error
  because `union(l, r)` was never the numeric constraint. §8 already specified
  this, so the fix edited **no normative text** — the checker was wrong.
- **A third site the bug file never listed:** `finish`'s error message said "used
  in arithmetic **or an ordered comparison**", which after the fix is
  *unreachable*, not merely stale. Found by grepping every `numeric.push`.
- **The property the bug file identified, in the same sitting** —
  `ordered_comparison_and_minmax_agree_on_every_type` (C8), over a purpose-built
  generator across all five primitives and both ends of the order, plus its
  coverage guard. `arb_comparison_program` now orders the string key it always
  had, counted in `generator_emits_comparison_shapes`.

**Decided**
- **A differential property cannot catch a type-checker defect, and B1 is one.**
  The bug file called extending B1's generator "the whole job". Measured: with
  the fix reverted the extended `b1_comparison_programs_agree` stays **green**,
  because `eval` is type-blind by design (§17, 2026-07-21) and a differential
  between two evaluators never asks what `typecheck` accepts. The property that
  fails is a new `comparison_generator_is_well_typed`. **Widening a generator
  needs a matching acceptance property.** In §17, testing.md C8, and the bug's
  Resolution.
- **The 2026-07-21 type-inference entry is amended, not falsified.** "arithmetic/
  ordered comparison ⇒ numeric" was derived one step too far; no coercion — the
  other half of that sentence — stands. Its type-blind `eval` held too, and this
  is what it cost. Both new properties and the coverage guard were confirmed red
  with the fix reverted before being kept, per `bugs/005`'s discipline.

**Removed**
- The skill recipe's **trap #3, which told readers to work around this defect**
  (emit a numeric `file_id` and sort by that) — deleted, renumbered to four. The
  only doc content this made false rather than incomplete.
- ROADMAP's claim that the bug queue held "**only `004`**" and "has nothing
  unblocked in it: the next work is a design session, not a defect" — written
  while `006` was open, unblocked, and named *Next up* in that same session's
  worklog. Stale on arrival; the ROADMAP contradicting the worklog is new.
- The 2026-07-27 `bugs/005` entry rotated verbatim to `worklog-archive/2026-07.md`.

**Next up**
- **The `bugs/` queue has nothing unblocked in it** — now actually true. `004`
  waits on Termination, so the next work is a **design session**: `absent` ×
  negation (which also unblocks §6) or Termination & value-creating recursion.
- Cheapest non-design item, unchanged: **parenthesized expressions**
  (`src/parser.rs:619-628`, `src/print.rs:169` — the printer is the real work).
  Also unchanged: the **§17 restructure**, **CI**, the GitHub *About* panel.

## 2026-07-27 — Source-analysis dogfood, second run: the engine on 28k facts

Re-ran the 2026-07-23 use case now that §13 imports and §9 aggregation exist.
Facts from `syn` (a throwaway scratchpad extractor, not committed), not regex.
Docs only in the engine; one build-tooling line. 395 tests pass, clippy and
rustfmt clean, `cargo package-skill` green with `recipes/` in the bundle.

**Done**
- **`skill/recipes/source-analysis.md`** — the recipe deferred in 2026-07-23
  pending §13/§9. `cargo package-skill` now bundles `recipes/`, so it ships.
- **`bugs/006`** — `<` rejects strings while `min`/`max` order them, against §8's
  own text; `src/typecheck.rs` cites §8 for both halves. Found by needing `A < B`
  to canonicalise a pair, not by reading the spec.
- **Findings, each source-verified before being written down:** `engine ↔
  provenance` is a genuine import cycle; three mutual-recursion clusters, all
  reached through the **aggregate** arm, including `parse_primary →
  parse_aggregate → parse_expr`; `print_fact_lines` is public with zero callers
  and zero tests; `lower.rs ↔ provenance.rs` co-change 6× with no `use` edge,
  coupled through `ir::BodyIdx`'s meaning. No dead private function, no
  unconstructed variant — the negatives prose cannot claim credibly.
- **First evaluation numbers**, in `notes/performance-baseline.md`: importing 28k
  facts is 0.32 s, so the cost is evaluation — 173k derived tuples in 11.6 s and
  505 MB, and a **35x cliff** from an unused closure merely being in scope.

**Decided**
- **The engine holds the uncertainty, not the extractor.** 2026-07-23 said the
  fix for bad facts was a better parser. `syn` removed the regex error class and
  added its own — macro bodies opaque (7% of functions invisible), bare callee
  names merging `Model::new` with `Lexer::new`. What worked was resolving in
  Datalog in confidence tiers, leftovers kept as a *counted* relation: 1,504
  certain edges, 2,458 likely, conclusion stable across both.
- **Two language limits push work back into the fact producer** — no string
  ordering (`bugs/006`), no string operations at all. Both forced extractor
  columns that exist only to serve the query. Their resolutions diverge, and the
  line between them is *filters yes, constructors no*: **widen `<`**, and
  **reject string operations outright** rather than defer them (§17) — `concat`
  fails the finite-value-set test §5 already applies to casts.
- **The count-distinct trap is worse than §9 says**, and §13 is why: over a wide
  imported table the wildcard is never written — 36 call sites where the question
  wanted 20 callers. §17 and the ROADMAP item carry the measurement.

**Removed**
- The 2026-07-27 GitHub-publication entry rotated verbatim to
  `worklog-archive/2026-07.md`. Nothing else: no doc claim was found stale.

**Next up**
- **`bugs/006` is decided and specified, not done** — widen the typechecker. The
  file carries the exact three lines, the finding that *both evaluators already
  handle it*, and the warning that `b1_comparison_programs_agree`'s generator is
  integer-only and cannot cover the widened surface. `004` blocked on Termination.
- Unchanged: the two design sessions (**`absent` × negation**, **Termination**),
  the **§17 restructure**, and **CI**.
