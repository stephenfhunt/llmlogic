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

- **2026-09-16** — **A Java source set that declares a module is read as that
  module**: its dependencies go on the module path, not the class path. Long
  form: `notes/go-java-frontends.md` § Javac as the build runs it.
  - *Why it had to change:* on the class path a dependency is in the unnamed
    module, which a named module cannot read, so every `requires` failed with
    "module not found" and nothing a dependency held resolved. The first reading
    of the gap — that module boundaries went unenforced — was wrong in the other
    direction, and JDK boundaries were never affected at all.
  - **Every dependency goes on the module path, and javac classifies them.**
    Which entries are named modules, which are automatic, and what each exports
    is the compiler's answer; the extractor chooses only the location. *Rejected:*
    splitting the path ourselves by reading each jar for a `module-info.class` or
    an `Automatic-Module-Name` — a second implementation of a rule the compiler
    already owns (`facts from the language tooling`, 2026-09-14).
  - *Consequence:* `symbol.package` now prefers a jar's coordinate over the
    module name, since a modular project's dependency is both and the coordinate
    is what the build declared and what `packages.dl` joins on. A module with no
    jar — the JDK's — still reports its name.
  - *Open:* a sibling module of the same build is read from source, so it is not
    on the module path and `requires` naming it is reported not found. A modular
    multi-module build is not read exactly.

- **2026-09-15 (night v)** — **The Java dataflow layer**, lowered from javac's
  trees as `layers/dataflow.ts` lowers TypeScript's. Long form:
  `notes/go-java-frontends.md` § The Java dataflow layer. Three calls were the
  user's:
  - **A record is modelled at its use sites** — `new R(a, b)` stores each
    component, `r.x()` loads it back — rather than left opaque. No `fn`, `formal`
    or `ref` row is invented, so *what the compiler writes is no one's*
    (2026-09-14) still holds; only stores and loads at sites written in source.
    *Rejected:* strict parity with refs, which costs a record-heavy codebase all
    of its field flow.
  - **Allocation-site ids chain across frontends** (`bugs/resolved/009`), fixed
    before the layer rather than filed: a third dataflow emitter was about to
    make a latent collision a real one.
  - **Deconstruction patterns bind by a load per component**, not just type
    patterns.
  - **A class holding static state is a `cell`** — Go's package-level variable in
    Java's spelling. Without it every static field carries nothing. A class
    outside the project gets none, so `System.out` is opaque.
  - *Open:* `this_var` is unfalsifiable by P5-java and by any shape it could
    generate — `pointsto.dl` reaches `this` by the allocation's type as well.
    Whether the receiver rule earns its place is a question for the library, not
    the frontend.
  - ***Consequences*** (2026-09-16, OpenRefine): the layer held — `checks.dl`
    clean on 968k facts, and a real getter's points-to is exact. The record rule
    earned its place; the **functional-interface rule earned nothing**, because
    every one of the subject's 305 lambdas is handed to the standard library and
    none is ever called through a value the project holds. Not wrong, and not a
    reason to widen it: an ungated `callee_var` was measured to resolve nothing
    either. What the dogfood cost was two *model* defects, not layer ones
    (`bugs/resolved/011`), and what it exposed is that `taint.dl` cannot run on a
    base this size.

- **2026-09-15 (night ii)** — **Java extraction runs javac as the build
  configures it** (the user's choices). Long form: `notes/go-java-frontends.md`
  § Javac as the build runs it.
  - **Annotation processors run** from the build's processor path, processors
    and `-proc`. A generate-only pass writes into the build's own
    generated-sources directory before ids are claimed, and what it wrote is
    extracted, `is_generated`. Uses from generated code count: a generated
    factory calling a constructor is a call.
  - **Plugin-generated sources are read as the last build left them**, marked
    generated; a build that generates and left nothing says so.
  - Encoding and compiler arguments are the build's; options choosing output,
    paths, release and encoding stay code-facts'.
  - *Rejected:* running processors only to resolve names, which hides every use
    from generated code; running the build's generate-sources step, which runs
    plugins, may download, and writes build output; keeping `-proc:none`.
  - ***Consequences 2026-09-15 (night iv)*** — "compiler arguments are the
    build's" reached lint: the frontend's own `-nowarn -Xlint:none`, set before
    diagnostics were recorded, went, so `diagnostic` is what the build's javac
    reports. It cannot have the notes javac prints only as a compile ends. The
    second processor pass's Filer error surfaced, and is dropped by its message.

- **2026-09-14** — **Go and Java frontends, in Python's shape, with the library's
  vocabulary kept** (the user's choices: Go first; x/tools vendored; Java's
  project model asked of Maven and Gradle; every layer including dataflow). Long
  form: [`notes/go-java-frontends.md`](notes/go-java-frontends.md).
  - **The project's toolchain is assumed present**; the skill bundles none of it.
    Go resolves through `go/packages` + `go/types` + SSA, Java through the JDK's
    compiler API — the checker that builds the project, as for TypeScript.
  - **Constructs map onto existing `symbol.kind`s**, with the language's own name
    in a new `symbol.form`, so `lib/` needs no per-language branch.
  - **The module graph stays file-to-file**: a package or type import is one
    `imports` row per file referenced, and a reference with no import is kind
    `implicit`. *Rejected:* a `ref`-derived edge gated on `lang` in `modgraph.dl`,
    and imports as written only — which misses same-package dependents in every
    impact closure.
  - Go's structural interfaces become `implements`/`overrides` rows via
    `types.Implements`, so CHA needs no change.
  - ***Consequences 2026-09-15*** — on a real module (caddy, `notes/go-java-frontends.md`
    § The Go dogfood) the kept vocabulary held: `checks.dl` clean, every library
    run unchanged. The file-to-file graph cost what was predicted — `in_cycle`
    lists one package's files — and namespace granularity answers it. Asked
    `implements` found every `caddy.Module` implementation once types in test
    files were matched in their own view, a case P4-go had not generated.
  - ***Amended 2026-09-15*** — Maven's project model comes from a **core
    extension** compiled against the user's own Maven, not `dependency:build-classpath`
    (the user's choice). It is one `mvn validate` that stops once the reactor is
    written, so it runs no plugin and downloads none. Its output has the Gradle init script's shape.
    *Rejected:* plugin goals, whose output would be parsed per module, and which fetch two plugins on first use.
  - ***Consequences 2026-09-15 (night)*** — "the checker resolves the names" held
    for references but not at first for declarations: the structure layer copied
    Java's modifier rules, Javadoc and test detection off syntax. It now claims
    ids from syntax and takes every fact from javac's elements (the user's
    question). Not yet held: javac runs with its own settings, not the build's
    (processors, encoding, arguments, module path). P4-java made an inherited
    method implementing an interface's an `overrides` row, as Go's promoted ones are.
  - ***Consequences 2026-09-15 (night iii)*** — the flow plan (TypeScript's model;
    catches tested in order as Python's; resources and `synchronized` as implicit
    finallys) held: P1-java found no model defect, only its own harness and a
    generator whose catches always matched, which hid a mutant.
  - ***Consequences 2026-09-15 (night iv)*** — the quality layer held to javac:
    diagnostic keys, resolved annotation types, types for raw-ness and throws,
    with javac's rawtypes lint as the test's oracle. Only comments come from a
    lexer. A first reading of raw positions counted `var`'s inferred type and an
    anonymous class's supertype twice; the oracle and a probe of javac found both.

- **2026-09-13 (night ii)** — **The skill's texts are written for a stranger's
  project, and the skill owns every one of them** (the user's ruling).
  - **What ships is published, not project documentation.** That covers
    `SKILL.md`, `reference/`, the library headers and the text the extractor
    writes or prints. Its reader is an agent in a novel project with its own
    setup, conventions and tooling. None of it carries provenance: no defect ids,
    dates, subject codebases, session narrative or repo paths.
  - **`reference/datalog.md` and `bring-your-own.md` are this skill's own
    files**, not symlinks. The two skills serve different purposes, and dual
    maintenance is accepted.
  - Swept: "the project above" (nine times), sqlparse and the experiments'
    answer key, bug ids and "found dogfooding" in seven library headers,
    `keys.dl`'s engine-workaround lesson, the extractor's call-site size note
    (against 2026-09-12 evening), and links to files the bundle lacks.
  - Guarded by `test/published-text.test.ts`, which cannot see context-dependent
    prose; that stays a reading discipline (`AGENTS.md`).
  - ***Consequences*** (2026-09-14): dual maintenance's cost showed within a day.
    This fork had introduced named arguments, defined its recipe's relations,
    stated `;` in queries and run the engine by path; none of it reached the
    datalog skill until a stranger's read found them there
    (`../datalog/spec.md` §17 2026-09-14 (evening)). The accepted cost is real
    and silent, so a fix to one copy wants a look at the other.

- **2026-09-13 (night)** — **The playbook teaches investigation, not only
  measurement.** It read as a menu of libraries. Yet the best Grafana result came
  from the maintainers' own question, and the one false result was a negative
  reported unprobed (`notes/code-facts.md` § Dogfooding — Grafana).
  - `SKILL.md` gains five things:
    - a first step that finds the question: the project's own lint rules and
      owners, or the user;
    - going down rather than across, and asking the query that refutes a claim;
    - probing every negative with `?whynot` and the blind spots, and sampling
      for precision;
    - a synthesis step;
    - impact and two-commit-diff recipes, and a hazards row naming quality facts
      no library reads.
  - **Every recipe was run before it was written**: on `@grafana/ui` impact takes
    0.1–1.6 s and the drill 11 s; the diff was run on code-facts itself.
  - ***Rejected:*** an impact library, since a seeded closure is four rules over
    the caller's own change set. Also rejected: a diff tool, since sorted
    canonical answers make `diff` enough.
  - Open: whether it changes what an agent does. H-CA1 grades questions we
    specify and cannot see it; a Grafana re-run with only the bundle can.

- **2026-09-13 (later)** — **An import the compiler could not resolve claims no
  package; a declaration can confirm the guess** (`bugs/002`, the user's choice).
  - `imports.target_package` is set only for a resolved target. An unresolved bare
    specifier's first segment goes in `unresolved_package` — the unresolved-case
    counterpart of 2026-09-11 (later ii), which kept a wildcard ambient match
    out of `target_package`.
  - **A declaration confirms a guess**: `packages.dl` counts a confirmed guess as
    `imported`. An unconfirmed one is `unresolved_bare`, never `undeclared`,
    because a bundler alias and an uninstalled package look the same. Same
    principle as 2026-09-12 (later): a relation that cannot tell two cases apart
    claims the weaker thing.
  - ***Rejected:*** reading bundler aliases, since the extractor mirrors tsconfig
    and nothing else. Also rejected: feeding the guess to `used` alone, which
    silenced three relations on an uninstalled extraction.

- **2026-09-13** — **Several tsconfigs parse each file once; the extractor's
  output does not move.** Profiled by phase on Grafana (`notes/code-facts.md`
  § The extractor's own cost): 7.0 GB of the 16-tsconfig load was 44,954 parsed
  files for 13,768 paths.
  - **The share key is TypeScript's own `DocumentRegistry` bucket key without
    `pathsBasePath`**, plus the call's parse options. `pathsBasePath` is read only
    by module resolution and specifier generation (TS 6.0.3), and it alone kept the
    root config apart from its packages. Settings stay each tsconfig's — a file
    is shared only where they would parse and bind it identically (P9).
  - **Declaration keys name a file by integer** — kept because it measured
    (−213 MB after `ids`), the bar set before building it.
  - **Numstat runs in parallel through a child process**, not a worker thread:
    a synchronous caller blocked on a worker that fails to load hangs forever,
    while a failed child is an exit code. 38 → 7.5 s on Grafana, output identical.
  - Not taken: the emit's full type check and the per-program checkers — the
    peak now — need more than a cache.
    ***Amended 2026-09-13*** — and not queued. The user's ruling: the extractor
    does not work around TypeScript's own cost. Anyone running it compiles the
    project too, so similar cost is no surprise; what code-facts adds on top of
    `tsc` is what gets optimised.

- **2026-09-12 (evening)** — **The tool is made usable at size, not documented
  around it.** The user's rulings, on the engine's memory work
  (`../datalog/notes/memory-profile-2026-09-12.md`):
  - **No size gate on `orient.dl`.** It asks for its runtime closure at any size;
    what that costs is the engine's to reduce — 4.94 GB on Grafana's frontend.
  - **The skill carries no size or cost guidance** — no timing columns, no "does
    not fit", no "narrow before you close over the graph", no join-key rule. An
    agent asks the question the analysis needs; one too slow to answer is a defect
    in the engine or the extractor, and is profiled as one.
  - **The engine stays generic.** `pointsto.dl` is profiled as a vehicle for
    engine work, never given behaviour of its own; the extractor gets its own
    profiling session.
  - **The library reads naturally**, so a workaround in it is engine debt:
    `lib/keys.dl` is the one left (2026-09-11 (later), reopened).
  ***Consequences 2026-09-13*** — it held for correctness traps too, not only
  size. Of five dogfood defects filed as "document the trap", four closed in
  the tool instead: a file-name test for `is_generated`, a weaker relation for
  an unresolved import, `lib/exports.dl`, and `uncounted_dependent` beside a
  doc line. Only `hidden_coupling` stayed a doc fix, because deriving it needs a
  history of the graph that nothing else wants (`bugs/resolved/001`–`005`).
  ***Consequences 2026-09-13 (night ii)*** — the ruling reaches everything an agent
  reads, not only `SKILL.md`. `code-facts` still printed "will be slow; anchor on
  edges or narrow first", and `keys.dl`'s header still taught re-keying; both
  went in the leak sweep.

- **2026-09-12 (later still)** — **The import-graph closure is back in
  `modgraph.dl`, because the engine now pays for a rule only when a goal reaches
  it.** `reach.dl` is deleted; `file_reaches` / `in_cycle` / `cycle_edge` close out
  `modgraph.dl` again (`../datalog/spec.md` §17 2026-09-12, rule pruning).
  - **Same answers**: on `vs/base` the folded `modgraph.dl`, asked all five of its
    questions, prints what the old `modgraph.dl` and `reach.dl` printed — 670
    lines, byte for byte — and `orient.dl`'s bench digest is unchanged.
  - **Checked on shape, not size** — the lesson below applied. `vs/base` has no
    import cycle, so a synthetic 2,000-file cycle is the check: `dep` alone
    through the folded file is **32.7 s / 778 MB** on an engine without pruning
    and **0.03 s / 34 MB** with it.
  - **`lib/keys.dl` stays**: its re-keyings answer the leading-prefix seek, which
    pruning does not touch. **`callreach.dl` stays split** — the same shape, but
    nothing imports `callgraph.dl` expecting to avoid it, and the seeded variant
    beside it is a different closure.

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
    taken it. ***Superseded by 2026-09-12 (later still)*** — the engine now
    evaluates only the rules a goal reaches, so the closure is back in
    `modgraph.dl`.
  - **Why no test caught it:** the calibration corpus is VS Code's `vs/base`,
    which has **zero** import cycles, so the one superlinear rule in the library
    was never exercised. Grafana's frontend has 915 files in 27 cycles, the
    largest a 796-file SCC. **A corpus chosen for size does not exercise shape.**
    ***Consequences 2026-09-12 (evening)*** — it held for the engine's memory
    work too: the fix worth 3.3 GB on Grafana's runtime closure is worth 2 MB on
    `vs/base` `orient.dl`, whose saving came from the base facts instead. Measured
    on both before either was believed
    (`../datalog/notes/memory-profile-2026-09-12.md`).
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
  ***Reopened 2026-09-12 (evening)*** — a library free of engine workarounds is the
  direction (above): the skill no longer teaches re-keying, and `lib/keys.dl` stays
  only until the engine seeks a non-leading column.

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
    ***Superseded by 2026-09-13 (night ii)*** — each skill owns its texts; one
    reader's guide written into the other skill leaked its history.
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
