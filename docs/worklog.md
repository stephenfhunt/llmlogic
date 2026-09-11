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

## 2026-09-10 — `ts-facts`: a TypeScript project, extracted for the skill

Asked for a maximalist tool in the skill's static-analysis section: point it at a
tsconfig, get facts about everything from module relationships to code flow,
useful for coupling, cohesion and questions no linter asks. Built as planned, in
eleven commits, and it found two engine defects, which were fixed here when asked.

**Done** — `skill/tools/ts-facts` (`./ts-facts`), **72 tool tests**, engine suite
and clippy clean, experiments **1,661** green
- **61 relations over seven layers**: structure, refs (checker-resolved call
  sites with dispatch kinds), flow (a CFG per function, def/use, metrics),
  dataflow (Doop-style), quality, git (renames followed to today's path).
- **`lib/`, split by cost**: checks, modgraph, callgraph/callreach, coupling,
  cohesion, metrics, flow, dominators, pointsto, taint, cochange.
- **Properties P1–P8**, each mutation-verified (`testing.md`); P1/P5 run in Node.
- **Engine:** `bugs/resolved/010` (empty JSONL import), `011` (clash message).
- **Harness:** the local subject's skill listing named every file under `skill/` —
  thousands, once the tool landed; it now names what the workspace carries.
- **tsdl** (24.5k lines, TS 7): 205k facts in 5.7 s, libraries 0.3–9.4 s, clean.

**Decided** (`spec.md` §17 2026-09-10; `notes/ts-facts.md`; experiments
`decisions.md` 2026-09-11)
- **TypeScript 6.0 pinned, in-process** — 7.0 has only an unstable IPC API.
- **`src/schema.ts` is the schema's one home**; ids keyed by declaration position.
- **Emit primitives, derive measures in Datalog**; a library's function-typed
  callee is named as the target (2,843 of tsdl's 2,870 "indirect" calls were
  vitest).
- **Checking answers against the source found every call-graph trap.** Untested
  exports on tsdl went 5 → 0 across library-invoked callbacks, callbacks in object
  literals, and structural implementations CHA cannot see.
  `hidden_coupling`'s top tsdl pair was real.

**Removed**
- The writer's header-only-CSV workaround for empty tables, once `010` was fixed.
- `skill_body`'s directory-wide listing, including the wrapper it always listed
  and never copied.

**Decided — *added 2026-09-11***
- **`imports.runtime` comes from the emitter** (§17 amendment; P8). The user
  asked whether import facts separate type from runtime imports: they did only
  syntactically, and TypeScript elides any import whose bindings only annotate.
  `runtime_dep` was wrong on every project not under `verbatimModuleSyntax`.

**Next up**
- **Point it at a real, larger codebase** than tsdl — a class-heavy one, since
  tsdl has no classes and the cohesion library has only fixture evidence.
- **One unexplained `npm test` failure**, seen once in ~25 full runs; capture the
  next with the default reporter.
- A TypeScript 7 backend when its API stabilizes; accessors, spread and instance
  method values in the dataflow layer.
- Unchanged from before: size the provenance run with `--repeats`; the
  `SKILL.md` exit-code gap.

## 2026-08-31 (later ii) — Provenance-core questions, and a reach that is still zero

Asked whether Haiku needs its own provenance test, whether we are set up for one,
and for a small sample to explore it. Yes; half; and it ran — **twice**, because
the first run's only discordant pair turned out to be a defect in my own question.

**Done** — **1,660 tests** (+135), ruff clean, the 310-cell offline grid renders
- **The answer to the question asked.** The repair route to provenance cannot
  fire on Haiku: its `engine-briefed` arm is **44/44 correct**, ran `datalog`
  with **visible stdout in 44/44 transcripts**, and never derives nothing. There
  is no empty result to interrogate, so the only route left is a question whose
  *answer* is the derivation.
- **`domains/provenance`** — `critical-grant`, `minimal-repair`, `access-path`
  over `access_control`'s graph, its own deeper pinned graph, `at-scale` refused
  with a reason. Its own pack rather than three more `access_control` tasks: a
  two-level hierarchy answers a derivation question with one role, and the local
  subject's pre-registration names **the 16 pinned items**.
- **`reference/correct/provenance.dl`** — the engine answers all three and agrees
  with the plain-Python oracle on all 107 rows, **using neither sigil**. That is
  recorded as the pack's honest limit: it makes provenance *applicable*, not
  necessary.
- **Two runs, 40 cells, $4.72**, the second halted by the session limit at 5 and
  resumed to 20. The pilot was pre-registered before either.

**Decided** (`decisions.md`, two entries; `hypotheses.md` addendum)
- **Reach 0 of 20, zero goals run**, across both runs. Compliance was **20/20
  `answered-from`** and controls **8/8** in both arms, so this is not the mandate
  failing to take. **The subject uses the engine and never interrogates it**, now
  measured on questions where `?why` answers what was asked.
- **The first run's accuracy delta is void, and the defect was mine.**
  `critical-grant` carried *"and every other user keeps whatever they had"*;
  under that reading the answer is **empty on both rungs** — computed — while
  `truth.py` grades per user. `prose` read it literally and was graded wrong for
  being right. Repaired, and the item now answers **12/12 exactly**.
- **The API subject is not deterministic across runs.** One item flipped
  correct → missing 11 of 33, same prompt, same arm. 2026-08-27's *"`--repeats`
  buys nothing"* was about the **local** subject and does not carry here.
- **The class costs ~3x the ladder's unit** — ~$0.20 and 109–205s per cell
  against $0.08 and 40s.

**Decided — *added 2026-09-01***
- **`ScriptUse` over all 558 archived `prose` cells**, not just the ladder's 44:
  every paired comparison on disk is **18 wins / 38 losses / 382 ties**, and
  within a subject **scripting does not predict correctness** (haiku 84% with
  against 88% without). *"Prose is really code"* explains the ladder's mechanism
  and not the arm's accuracy — the engine's deficit is the **encoding step**.
  Opens the framing question: four grids of nulls may be bracketing the models'
  reasoning ceilings rather than testing S1. — `notes/what-prose-is-doing.md`,
  `decisions.md` 2026-09-01.

**Removed**
- The `critical-grant` clause that contradicted its own oracle. Nothing else: the
  voided run stays on disk, as `results/` requires.

**Next up**
- **The two runs disagree in sign** — engine +1 cell voided, `prose` +2 cells on
  the repair — which is what n = 6 against a stochastic subject buys. **Size the
  next one with `--repeats`** before reading any delta on this class.
- **Then the question the zero raises**: reach is 0 with the manual, 0 with an
  instruction the local subject never reached, and 0 where provenance *is* the
  question. The next lever is the `SKILL.md` exit-code gap, not a sixth arm.
- Unchanged: `at-scale`, the mandate arm's `invoked` split, `scheduling`.

## 2026-08-31 (later) — The provenance arm, and a gate that stopped it one step early

Asked to run a local-model experiment on **briefed provenance** — does
*instructing* the subject to interrogate its own failures convert them — and to
build cases where provenance is core rather than incidental. **The gate stopped
the run, and the reason is more useful than the run would have been.**

**Done** — **1,525 tests** (+17), ruff clean, the 280-cell offline grid renders
- **The zero, measured twice**: **no `?why` / `?whynot` in 1,812 transcripts,
  23,430 tool calls** — every real run on disk — by `grep` and by the parser.
- **Triaged the 12 failing briefed cells first**: **nine derived nothing at all**
  (`missing == truth_size`), the exact `?whynot` case, burned over 35 and 28
  turns rewriting the program instead.
- **`engine-briefed-provenance`** — a fifth arm, an *instruction* not more
  documentation, appended after the briefing so briefed stays a strict prefix.
  Pre-registered with its endpoint, prediction, and the rule that then fired.
- **`signals.ScriptUse`**, and **`ROADMAP.md` was wrong that it could not be
  backfilled** — transcripts *are* persisted, so it was, over the ladder.
- **Two defects, opposite directions** — a one-call `datalog … > answer.txt`
  read as *non-compliance* (`>` wanted `>=`; `classify_script` had it worse, 11
  of 14 cells); and `ran_engine`'s comment asserting an invariant the code never
  had, which makes 188 legitimate records look broken.

**Decided** (`decisions.md`, three entries; `hypotheses.md` addendum)
- **The gate: provenance reach 0/2, zero goals.** Stopped, per the rule.
- **But not because the instruction was ignored.** Both cells complied fully and
  **both piped `datalog … | sed … > answer.txt` in one command**; the second ran
  the identical command twice and stopped. **The subject never sees the output**,
  so it cannot notice the answer is empty, so the block's opening condition is
  never evaluated. Provenance is untested, not refuted.
- **Observation is upstream of provenance**, as reach is upstream of capability.
  The `SKILL.md` exit-code gap now *blocks a feature* rather than costing rounds,
  and needs its own block and arm or the delta gets two causes.
- **The ladder's `+0 at every rung` decomposes**: `prose` answered from a script
  **33/44**, `engine-briefed` **0/44**. It was never prose against the engine.

**Removed**
- The "transcripts are not persisted" claim, load-bearing twice in `ROADMAP.md`.
  The gate keeps its `engine_use` **as recorded**: backfilling a field a run
  never had is additive, correcting one it did record is not.

**Next up**
- **Teach the subject to read its own output** — its own block, its own arm, and
  now the blocker; nothing downstream of it is measurable.
- **Then re-run the provenance gate**, 2 cells, which settles the question the
  pre-registration actually asked.
- **Part 2 unstarted**: provenance-core tasks (`critical-grant`,
  `minimal-repair`, `access-path`), keeping the set-equality answer contract.
- Unchanged: `at-scale`, the mandate arm's `invoked` split, `scheduling`.
