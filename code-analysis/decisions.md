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

- **2026-09-11 (later)** — **The library re-keys the relations it joins on, and
  says so in one place: `lib/keys.dl`.** The million-fact problem was not the
  aggregates the sizing spike blamed. The engine seeks a **leading** prefix and
  stops at the first unbound column (`../datalog/spec.md` §17 2026-08-21), so
  every rule binding a non-leading column scanned its whole relation once per
  outer row — `count { E | flow_node(id: E, fn: F, kind: entry) }` is 108,597
  rows × 13,983 functions. Numbers and the full audit:
  [`notes/code-facts.md`](notes/code-facts.md) § At a million facts.
  - **The fix is one rule per re-keying**, in the library, not an index in the
    engine: `child(P, S)`, `file_member(F, G, C)`, `called_by(B, A)`,
    `entry_node`, `alloc_of`, `decl_file`, `access_of`, `used_at`, `touched_by`.
    `checks.dl` **199.6 s → 13.7 s** on two of them, answers byte-identical.
  - **A re-keying is not free** — it materializes a copy — so it is worth it only
    where the scan it replaces is quadratic. `comp_edge_to` was written, measured
    at no gain against a scan of ~10⁷, and removed.
  - **`callreach.dl` is the one that cannot be re-keyed**: a whole-project call
    closure over 14k functions and 178k edges is quadratic in the graph's
    density, and at **38 s / 4.1 GB** it is the only library over 30 s.
    `callreach_seeded.dl` is the answer for a question about particular
    functions — `taint.dl`'s idiom, a `seed/1` the caller supplies — and
    `callreach.dl` now states what it costs. (It was first recorded as not
    finishing at all; that was a run stopped at 30 s on a wrong guess, and the
    bench now has a `--timeout` so a stop is reported as a stop.)
  - **The engine's share is `Provenance::Reports`** (`../datalog/spec.md` §17
    2026-09-11), which buys memory rather than time: `cohesion.dl` 35.3 → 28.2 s
    and 4.2 → 2.3 GB, but `coupling.dl` 23.0 → 27.5 s for half the residency. A
    900 s, 4 GB cell is short of the memory, so the trade is the right way round.
  - **The guard is the bench** (`tools/code-facts/bench/`): every library's
    answers digest, so a speed-up that moves a row is not a speed-up.

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
