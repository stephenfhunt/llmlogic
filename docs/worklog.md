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

## 2026-08-16 — The `as` cast ships, and its deferred question splits in two

Expressions is now empty: the last queued non-design item is built. 421 tests
pass (was 410), 1 ignored, clippy and rustfmt clean, `--no-default-features`
still builds.

**Done**
- **`Expr as type` end to end** — a `cast` precedence level between `mul` and
  `primary`, `ast::ExprKind::Cast` / `ir::Expr::Cast` (it survives lowering; a
  per-row conversion has nothing to hoist), a typecheck arm fixing the result
  type *without* unioning the operand, `apply_cast`, and a printer that wraps a
  cast's **operand** — the parenthesization case arithmetic alone cannot reach.
- **The literal-grammar classifier moved to `lexer.rs`** (its own commit) so
  `"30" as int` and an imported `30` cell are the same function — §13's "no
  second classifier exists" is now structural rather than a convention.
- **§16.9, the first example that names its test** and pins its output in a fence
  compared byte-for-byte — the shape the other eight should be retrofitted to.
- **Tests**: B9 (§8's table as an *independent* oracle — `naive.rs` deliberately
  calls `apply_cast`, so a differential would agree with a wrong table forever),
  three algebraic laws incl. the `as string as T` round trip, B10 as a
  biconditional, the printer's acceptance partner, and a non-vacuity guard. Seven
  mutations run and recorded.

**Decided**
- **A failed conversion splits, which the question's either/or did not admit**:
  `absent` where there is *no value to represent* (`"abc" as int`), an error
  where representing it would be **lossy** (`2.5 as int`, `as float` above 2⁵³).
  The second half extends §8's ratified widening rule to narrowing unchanged.
- **The deciding argument was expressibility, not reporting.** With an error rule
  a dirty column has *no writable query* — this language has no convertibility
  predicate, string ops having been rejected rather than deferred — so `absent`
  is what puts the guard back in existing vocabulary. Support: `eval_expr` is
  `?`-propagated, so an error emits nothing, not even facts already derived. The
  accepted cost, malformed reclassified as missing, is a new ROADMAP item.
- **The conversion table was as open as the failure mode**, and needed settling
  in the same sitting: text is the universal intermediary, and `bool as int` and
  friends have *no* conversion rather than a debatable one.

**Removed**
- **ROADMAP's Expressions essay**, 55 lines to 30. Its conclusion — "so the fix
  is an *explicit* `float(X)`" — had stopped being true the moment `as` was
  ratified: the exact stale-claim failure `docs/rules/editing-docs.md` exists for.
- §8's *Still open* bullet and §4's "deliberately still open"; two copies of the
  i128 exactness test, now one shared function; one of three `type_label`s.
- The 2026-08-03 entry rotated verbatim to `worklog-archive/2026-08.md` (new).

**Next up**
- Unchanged and still the user call: **the answer shape** (blocks the widening),
  then **Termination** (blocks `bugs/004`), then **§6's extension**.
- New here, both small: making the malformed/missing reclassification visible,
  and a type-clash diagnostic that names `as` — there is a concrete fix to
  suggest now, which there was not before. The **§16 retrofit** also got cheaper:
  §16.9 is a pattern to copy rather than a shape to invent.

## 2026-08-16 — Building the decided backlog: one shipped, one renamed, one falsified

Three sessions of design had left five items *decided, not built*, and `src/` had
not changed since 2026-07-29. This session took the three carrying no open design
question. 410 tests pass (was 407), 1 ignored, clippy and rustfmt clean.

**Done**
- **Parenthesized expressions ship** — `primary → "(" expr ")"`, and printing
  became precedence-aware: parens go on a child binding looser than its parent,
  and on an equal-precedence child on the **right**, so `A - (B - C)` survives.
  `arb_printable_expr` generates arbitrarily shaped trees now, which is what puts
  D2/D3 on the new code.
- **The rename landed** — `AbsentPattern` → `NoMatchPattern`, `Premise::Absent` →
  `NoMatch`, "absence pattern" → "no-match pattern". §4's and §11's two standing
  "shares a word, not a concept" disclaimers are gone, which was the point.
- **§17's open questions were swept**: four contradicted decisions above them in
  the same file. ***Answered*** joined the marker table, having been in use since
  2026-07-29 without being in the vocabulary that table calls a grep.

**Decided**
- **First-appearance stamping is closed without being built: its premise is false
  here.** It was adopted from tsdl on the reasoning that "the solver reads the
  store live". Ours does not — `eval_stratum` collects a round's matches against
  the previous round's model and applies them in a **batch**, so the
  first-producing derivation always has strictly-earlier premises. Measured over
  400 generated programs: `explain` returned `None` **zero** times, and same-round
  premises sat only on redundant rediscoveries (254 of 659 derivations). A sequence
  number would admit those and *change which proof is printed*, for no correctness
  gain. The forward risk is now recorded at the apply loop: interleaving collection
  with insertion breaks the bound silently, and E1 is what fires.
- **D2 is the property that bites, not D3.** Mutation-verified: flattening the
  printer leaves **D3 green**, dropping parens being still a *fixpoint* — it just
  re-parses to a different tree. Only the round trip sees a lossy canonicalization.
- **The rename's scope claim was wrong**, amended in place: ~40 non-frozen lines
  over 9 files, not 31 over 8, and six §17 entries hold the old name and keep it.
  The carve-out that entry called "nothing to argue about" is what the sweep needed.

**Removed**
- The parens ROADMAP item and its §8 *Not covered* paragraph;
  `grouped_expression_is_rejected_with_the_decomposition_hint` and the message it
  pinned; `arb_chain`, whose whole job was the left-leaning restriction. The
  Expressions preamble lost a "two findings" that had stopped counting.
- **testing.md E5 restated, not deleted** — it waited on provenance-as-facts,
  decided in the *negative*, so it now pins that decision's own guard: stripping
  `%` comments leaves byte-for-byte what the program prints without its goals.
- The 2026-07-29 entry rotated verbatim to `worklog-archive/2026-07.md`.

**Next up**
- **The answer shape** is still the user call, and still blocks the widening.
  Unchanged: **Termination** (blocks `bugs/004`), then **§6's extension**.
- The `as` cast is the cheapest remaining non-design item; Expressions holds
  nothing else. **Parallelism** is now the change most likely to break E1's
  batching assumption — written down where it would be violated.

## 2026-08-16 — Reading a sibling engine: three decisions taken, one question reopened

Cross-project review of `~/code/tsdl`, a Datalog engine in TypeScript that names
this project as its prior art and derives its testing rules from our `bugs/` files.
Docs only — 407 pass, 1 ignored, unchanged; nothing under `src/` or `tests/` moved.

**Done**
- **`notes/tsdl-cross-project-review.md`** (new) — the survey and the overflow
  target: what was adopted with its evidence, what was declined with its reason.
- **§17 gained four decisions**: the **truncation contract** (an incomplete model
  does not answer; the whole-model surface survives it), the **provenance query
  surface** (`?why`/`?whynot` as one union, a bounded why-not), the **three names**,
  and the **declines**. Four existing entries amended — 2026-07-25 Termination,
  2026-07-19 first-round stamping and all-derivations recording, 2026-08-03 answer
  shape. §17's marker table gained ***Reopened***.
- **The spec hygiene pass**, closing three ROADMAP items: **every status marker
  deleted** from §§1–16, each section now ending with one ***Not covered*** footer
  — which is also where six different deferral labels went — and the audit trail
  moved to **`notes/spec-traceability.md`** (new).
- **`testing.md` gained its Four rules section**, the normative home, with rules 2
  and 3 stated for the first time plus two corollaries: an oracle calling the
  engine's own function agrees with a wrong engine forever, and a rejection claim
  wants a **biconditional** property. `AGENTS.md`'s item 5 became a pointer.

**Decided**
- **No budget and no fuel — but truncation gets a contract.** 2026-07-25 stays the
  whole guarantee (a hung browser tab is their forcing case; a CLI has `^C`). What
  it never asked is what the engine owes when the model is short *anyway*: the store
  only grows, so an incomplete fixpoint holds missing facts and never false ones —
  but a **query** solved against it can be *wrong* rather than missing, since
  `not p(X)` over an incomplete `p` succeeds. Three live instances, none a budget.
- **Proof trees are not facts**, closing "provenance as facts" in the negative; they
  ride in `%` comments, keeping Datalog-out-is-Datalog-in byte-for-byte.
- **First appearance beats first round** for proof extraction — a round cannot
  separate two facts derived in the same one, and a sequence number costs the same
  `u32`. Whether the recorder earns its keep at all is a question for *after* the
  profile, not before.

**Removed**
- All 16 `*Status:` lines in `spec.md` and the paragraph declaring the vocabulary;
  §16's contradiction with §16.4 over "provisional"; §16.7's present-tense
  workaround, which dissolved when step 7 landed.
- Two ROADMAP items absorbed into the answer-shape question (the 2026-08-03 widening
  and the multi-atom existence check — one question with five axes, not three
  items), and three hygiene items closed outright.
- The 2026-07-27 `bugs/006` entry rotated verbatim to `worklog-archive/2026-07.md`.

**Next up**
- **The answer shape is a design session, and the widening is not built until it
  happens** — user call. Five axes in §17's open question, long form in
  `notes/query-answer-shape.md`; axis 4 (yes/no) can be settled early.
- **Termination** still blocks `bugs/004`, now with three of its five acceptance
  criteria answerable. Then §6's extension, two unknowns lighter.
- New, both from the review: **temporal types**, and **making `EXPERIMENTS.md` a
  measuring instrument** — which the parked "other agent-exposure forms" decision
  is explicitly waiting on.
