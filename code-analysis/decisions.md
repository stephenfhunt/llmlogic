# Decisions log & open questions — `code-analysis/`

An **append-only record**: history is what it is for. Amend entries in place,
never rewrite them. Everything outside this file states present truth and points
here (`../docs/rules/editing-docs.md`). **Newest first.** The amendment markers
are the repo's one vocabulary, defined in [`../datalog/spec.md`](../datalog/spec.md)
§17. Entries cap at ~15 lines; long-form goes to `notes/`.

The extractor's founding decisions — TypeScript 6.0 in-process, `schema.ts` as the
schema's one home, position-keyed ids, primitives rather than verdicts, the
emitter deciding import elision — were made while it lived in the datalog skill
and are recorded there: `../datalog/spec.md` §17 2026-09-10 (and its amendment),
with the long form in [`notes/code-facts.md`](notes/code-facts.md).

## Decisions

- **2026-09-11** — **Code analysis is its own project and its own skill**, not a
  second job of the `datalog` skill. Three reasons, the first decisive:
  - **Discovery.** A skill is reached for by its description, and "reasoning
    problems … transitive relationships" names nothing a user asking for an
    architecture review would say. Widening it dilutes it for every other use.
  - **Measurement.** `../datalog/skill/SKILL.md` and `recipes/` are what the
    experiments measure; code-analysis guidance changing there changed S1's
    briefing. Moving it out restores that artifact byte-for-byte to `7f3e998`.
  - **Guidance.** The facts support dozens of analyses; a domain skill can be a
    playbook for which to run and how to read them, where a general one cannot.
  - *Shape:* the extractor is renamed `ts-facts` → `code-facts` (it gains Python);
    the Datalog guide and the bring-your-own recipe stay single-homed in the
    datalog skill, reached by symlink, resolved at package time.
  - *Rejected:* a second skill inside `datalog/` (a domain application owned by
    the engine's project), and one widened skill (above).
  - *Open:* whether a domain skill beats the general one plus the same tools is
    exactly the experiments' kind of question — pre-registered as H-CA1 in
    `../experiments/hypotheses.md`, to be built as its own pack.
  - ***Consequences*** (2026-09-11, later): the measurement reason held — the
    restore was byte-exact and experiments stayed at 1,661 green. The shared
    schema held for a second language with no change to `lib/` beyond Python's
    primitive type names and its `_x` content-coupling rule, and the one
    schema home paid off: every Python row is validated by the TypeScript
    writer. Not yet known: whether the playbook, rather than the tools, is what
    helps — H-CA1 is still unbuilt.
