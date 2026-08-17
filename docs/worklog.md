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

## 2026-08-03 — The query answer shape: one atom, not one literal

Design session, prompted by a user question about §14's synthesized `answer/N`
output. Docs only — no engine or test change; 407 pass, 1 ignored, unchanged. The
decision ships as a ROADMAP item with its property specified first, per the
2026-07-22 entry's own *Consequences* note about untested output-shape widenings.

**Done**
- **`spec.md` §14** — a paragraph stating that substituted-atom output is a
  **projection, not the relation**: it prints a real predicate's name over a
  subset of its rows with nothing marking it as one, so composing it onward
  narrows that predicate. Present truth, undocumented since 2026-07-22.
- **`notes/query-answer-shape.md`** (new) — the overflow: the measured boundary
  table, the Soufflé survey, both rejected alternatives, an implementation brief.
- **`spec.md` §17** — the decision, pointing at the note; plus an ***Amended
  2026-08-03*** marker on the 2026-07-22 entry whose output-shape rule this moves
  for the third time.
- **`ROADMAP.md`** — the widening as its own item; **`testing.md`** — a C8
  sub-claim specified, `#[ignore]`d-and-failing, generator built backwards from
  an EDB fact.

**Decided**
- **The atom form keys on one positive *atom*, not one *literal*** — plus any
  number of non-binding literals, and the atom's variables must **equal** the
  answer variables. The trigger was not the opaque name: `?- person("bob", 17).`
  prints the fact and `?- person("bob", 17), 1 < 2.` prints **nothing**, which is
  `bugs/005`'s shape reached by a filter instead of by hoisting. Variable-set
  equality is a genuinely new condition — today's check is one-directional and
  survives only because a one-literal body has no other binder.
- **Inference is for reading one query's output; naming is for composing it.**
  Soufflé settled this by having no `?-` at all — goals are `.output` directives,
  so the user always names the relation, which is define-and-select, already
  shipped. An inferred name is wrong for composition either way: `person`
  collides with the relation it was projected from, `answer` with every query.
- **The narrowing hazard cut in favour of widening**, because `answer/N` marks
  *unnameable*, not *projection* — the atom form has carried the same hazard
  since 2026-07-22. The answer was §14 prose, not more anonymous queries.

**Removed**
- The 2026-07-27 *Source-analysis dogfood* entry rotated verbatim to
  `worklog-archive/2026-07.md`.
- The existence-check ROADMAP item trimmed 8 lines → 5: its `bugs/005` history was
  narration a current-state document should not carry, and §17 has it. Its section
  intro lost a hardcoded "all three" that had already stopped counting correctly.

**Next up**
- **Implement the widening**: `answer_lines` (`src/api.rs`), syntactic over
  `ir::Query`, no evaluator or IR work; land C8 by deleting its `#[ignore]`. Sweep
  `skill/SKILL.md` and `docs/agent-skill.md`, which each restate the rule.
- Unchanged: the **Termination** design session, the **§17 restructure**, **CI**.
  `bugs/004` still blocked on Termination.

## 2026-07-29 — `absent` × negation: the anti-join is not a join

The last open item of ROADMAP's Negation section, there since 2026-07-25 — design
and implementation in one sitting, since §17 had already argued the direction. 407
tests pass (401 + 6), clippy and rustfmt clean, `--ignored` now **zero** known
failures rather than two.

**Done**
- **`q(X) :- p(X), not p(X).` derives nothing.** `AbsentPattern::matches` compares
  closed slots structurally — one line, one choke point, the anti-join being its
  only caller. `engine::naive`'s `refutes` re-expresses the rule independently.
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
  restoring it means giving up `NULL ≠ NULL`. Proof is mechanical, not argued:
  `repeating_a_body_literal_drops_absent_rows` passes identically with the fix and
  without, while every negation property flips.
- **The price, measured before it was accepted.** On the §16.8 sparse shape a food
  whose id is `absent` used to survive `not measurement(F, _)` even with
  `measurement(absent, 3)` stored. It no longer does. SQL's answer.
- **§6 was deliberately *not* folded in**, against the 2026-07-25 review: it should
  describe a ratified semantics, not one being decided as written. Amended there.
- **A four-day-old scope note earned its keep.** That same review predicted this
  session would ship a three-site table omitting the aggregate group key — exactly
  what the plan had. Re-verified `g(absent, 0)` and pinned it.

**Removed**
- Both `#[ignore]`d tests: one un-ignored, one converted. With them, ROADMAP's
  "`--ignored` should report exactly **two** known failures" and testing.md's
  "two logical laws absent still breaks" paragraph. `SKILL.md`'s "Missing data"
  bullet was *incomplete* rather than wrong — the recipe's `not used(F)` is
  exactly the shape that changed — so it gained the negation case.
- **ROADMAP's whole `### Negation (§7)` section**, ~30 lines. Closing this item
  emptied it, so an open-backlog index held two ✅ items plus a preamble narrating
  a resequencing it then said was "recorded in §17". Both are now one pointer
  under milestone 4. Four current-state "negation item 1/2" references repointed;
  those in `bugs/resolved/` and §17 stay, being frozen.
- The 2026-07-27 `bugs/003` entry rotated verbatim to `worklog-archive/2026-07.md`.

**Next up**
- **Termination & value-creating recursion** — the remaining design session and the
  one that unblocks `004`. Direction decided (static error, not fuel); open is
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
