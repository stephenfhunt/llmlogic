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

- **2026-09-12 (later)** — **A library pays for what it imports, so a closure
  does not travel with the cheap rules.** Found by analysing a 1.48M-line
  repository nobody here wrote; long form in `notes/code-facts.md` § Dogfooding —
  Grafana.
  - **`file_reaches` / `in_cycle` / `cycle_edge` move to `reach.dl`.**
    `modgraph.dl` computed the import graph's transitive closure — 17.45M pairs
    on this subject — for every importer, and its two importers (`orient.dl`,
    `cochange.dl`) read only `dep`. Isolated, same answer: **306 s / 12.6 GB with
    the closure, 70 s / 3.3 GB without**. `orient.dl` — the playbook's *step 3* —
    went from OOM-killed at 21 GB to completing in 207 s. Same split, same
    reason, as `callreach.dl`; the precedent was there and this file had not
    taken it.
  - **Why no test caught it:** the calibration corpus is VS Code's `vs/base`,
    which has **zero** import cycles, so the one superlinear rule in the library
    was never exercised. Grafana's frontend has 915 files in 27 cycles, the
    largest a 796-file SCC. **A corpus chosen for size does not exercise shape.**
  - **`packages.dl` reads a workspace dependency off `file.package`.** A monorepo
    sibling resolves *inside* the root, so `imports.target_package` is absent on
    every one — 44,908 of 55,762 imports here — and `unused` named four packages
    that 540 statements import. `imported_workspace` fixes `used`; it deliberately
    does **not** feed `undeclared`, because `imports` cannot say whether a
    specifier was written bare (a package request) or as a path (reaching across
    a directory), and only the first is a missing declaration.
  - ***Rejected:* making the new edge feed `undeclared` too.** Tried, and it
    reported a relative import into another directory as an undeclared dependency
    — on Grafana, a phantom dependency on a package called `scripts/cli`. A
    relation that cannot tell the two apart should claim the weaker thing.
  - **An unnamed `package.json` is not a package** — `{"type": "module"}` is a
    module-system marker; npm cannot install it and nothing can declare a
    dependency on it, so its files belong to the nearest *named* package. Naming
    it after its directory is what produced the phantom above.

- **2026-09-12** — **Three defects, found by running the playbook as a user and
  not by any test.** The dogfood was a large repository end to end; each of
  these sat in front of a first result, and none was reachable from a fixture of
  a few dozen files.
  - **The wrapper never raised node's heap.** V8 caps the old space near 4 GB
    whatever the machine has, so `code-facts src/tsconfig.json` on VS Code died
    at 41 s with a native V8 trace and no output, on a box with 20 GB free. It
    now takes three quarters of total RAM. **A tool that routinely needs more
    than a default has to ask for it**, and a fatal trace is not a diagnosis.
  - **Extraction said nothing about scale until it was too late.** It now prints
    the file and line count as soon as the program loads — before the phase that
    dies — and the warning quotes **measured anchors** rather than a rate: cost
    is sublinear (156k lines is 1.6 GB, 2.9M is 13 GB), and the first version
    extrapolated to 29 GB and was wrong by 2.2× exactly where it mattered.
  - **A layer switched off had no schema, so `checks.dl` could not run.** 40
    semantic errors and exit 2 — and the file's own header claimed the opposite.
    **This made the playbook contradict itself**: step 1 tells a large repository
    to drop layers, step 2 then fails. Unextracted layers are now `declare`d and
    empty. A test had *pinned* the wrong behaviour, which is how it survived.
  - **What this says about the test suite.** Every one of these is a property of
    the tool at a scale and in a configuration the fixtures never take. The
    property catalog tests what the extractor *derives*; nothing tested what a
    user *encounters*. The end-to-end check added here (`checks.dl` on a
    layer-less extraction) is the first of that kind.
  - **Whole-repository analysis is viable, with layers dropped.** VS Code's
    `src/` — 2.87M lines, 9,007 files — is **8.9M facts in 276 s and 13 GB**
    under `--layers refs,quality`; with every layer it exhausts a 12 GB heap.

- **2026-09-11 (later ii)** — **An import that resolves through a wildcard
  `declare module` names a pattern, not a file or a package**
  (`imports.target_ambient`). `import './actionbar.css'` was `resolved: true`
  with both targets absent — a fact base contradicting itself, which `checks.dl`
  reported on 42 files of `vs/base`.
  - **The declaring `.d.ts` must not go in `target_file`.** It is the obvious
    fix and it is wrong: every stylesheet-importing file would gain an import
    edge to `src/typings/css.d.ts`, and `modgraph`'s cycles, `coupling` and
    `cohesion` would all inherit a dependency that does not exist at run time.
  - ***Rejected:* a new `imports.kind`.** `kind` is the syntactic form
    (`static`, `side_effect`, `dynamic`, …); how a specifier *resolved* is a
    different axis, and collapsing them would make "a side-effect import" and
    "an asset import" unaskable apart.
  - ***Rejected:* `resolved: false`.** The checker did resolve it, and making it
    indistinguishable from a broken import would inflate every blind-spot count
    `orient.dl` prints.
  - **A wildcard match is not a package.** `bundler!./widget.css` and
    `vs/css!./x.css` are bare by the leading-character test and were becoming
    `target_package` `"bundler!."`. Found by the new `assets` fixture, not by
    reasoning.

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
