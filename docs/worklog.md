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

## 2026-09-15 (night iv) — the Java quality layer

Asked to pick back up on the Java quality layer; planned first. The user's calls:
a discarded `Future` is a `floating_promise`; casts alone are `assertion`s.

**Done**
- `e4f5801` `diagnostic` from javac: every report as a set is parsed and
  analysed, once each, keyed by javac's name in a new `diagnostic.key`. Lint is
  the build's now — the frontend's own `-nowarn -Xlint:none` went — with no
  report cap. The second processor pass's Filer error ("attempt to recreate") is
  dropped; the Maven and Gradle processor tests show no diagnostic.
- `0593867` the quality layer (`notes/go-java-frontends.md` § The Java quality
  layer): `@SuppressWarnings` a row per tool its rules name, `@SuppressFBWarnings`,
  NOSONAR/NOPMD/checkstyle/spotless comments and markers from a comment lexer;
  raw types as `any_site` `raw` and `call_result`; casts; literals; typed throw
  sites; catch sites (`_` binds nothing); futures as `floating_promise`.
  Fixture `java-quality`, `checks.dl` clean on it.
- **javac's `-Xlint:rawtypes` is the raw-type test's oracle.** It found `var`'s
  inferred type counted (a type tree with no end) and an anonymous class's
  supertype counted twice (its `extends` is the `new`'s own tree). A probe of
  javac, not a third guess, showed the "missing" `new ArrayList()` row was there:
  two raw types on one line are one fact.
- Not reported, unlike the build: the notes javac prints only as a compile ends
  (deprecation, unchecked summaries) — analysis never ends one.

**Decided** — nothing new: facts from javac and compiler arguments from the
build held (`decisions.md` 2026-09-14 and 2026-09-15 (night ii),
***Consequences 2026-09-15 (night iv)***).

**Removed** — the frontend's `-nowarn -Xlint:none`; `Refs`' private `owner` and
`promise` (now `Names`', shared with `Quality`); the oldest worklog entry.

**Next up**
- Java dataflow layer and P5-java; then `reference/java.md` (the quality layer's
  traps: raw rows are per line, no end-of-compile notes).
- A Spring/Lombok dogfood; the module path.
- The test suite's leaked temp dirs.
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-15 (night iii) — the Java flow layer, P1-java and P3-java

Asked to continue: the Java flow layer, next on the worklog.

**Done**
- `0e63482` the flow layer (`notes/go-java-frontends.md` § The Java flow
  layer): a graph per method, constructor, lambda and initializer block in the
  TypeScript model; catches tested in order; try-with-resources and
  `synchronized` as implicit finallys; colon cases fall through, arrow cases do
  not; switch expressions lowered, `yield` a break. Decisions, cognitive
  complexity, Halstead (a lexer: javac's tokenizer is not public), def/use,
  captures, closures, `call_at` on the refs layer's ids. Fixture `java-flow`.
- **P1-java**, the JVM running generated methods: guarded jumps, four loop
  forms, both switch forms, switch expressions, try, resources, synchronized,
  labelled nests. 50 runs; its rarest guard, an exception caught, 41 of 200.
  **P3-java** (cyclomatic = E − N + 2) passed from its first run.
- P1-java's first failure was the harness: it followed a throw to `throw_exit`,
  which the model gives no edge outside a `try` — as P1 and P1-go do not.
- P1-java's mutations, all red: an arrow case falling through; `yield` finding no
  switch expression; the last catch passing nothing on (green until the
  generator wrote catches that do not match); a finally, or closing resources,
  re-issuing no jump; no implicit throws; a colon case not falling through; a
  labelled `continue` ignoring its label.
- `21f1c13` bugs/resolved/008: a Python or Java target that does not exist was
  read as its parent directory; now an error. Found when a P1-java measurement
  lost its temp project mid-run (the remover was not found).

**Decided** — nothing new: the flow model is the notes' 2026-09-14 plan, and
held (`decisions.md` 2026-09-14 ***Consequences 2026-09-15 (night iii)***).

**Removed** — the oldest worklog entry.

**Next up**
- Java quality layer: diagnostics (javac's, keyed by name), suppressions
  (`@SuppressWarnings`, `// NOSONAR`, checkstyle), markers, casts and raw types,
  literals, throws and catches.
- Then the dataflow layer and P5-java; `reference/java.md`; a Spring/Lombok
  dogfood; the module path.
- The test suite's leaked temp dirs.
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-15 (night ii) — javac runs as the build configures it

Asked to keep going; the two choices with side effects were the user's —
processors run and their output is extracted, plugin output is read as left.

**Done**
- `84c6a04` P4-java's test did not typecheck: `npm test` type-strips and
  never checks, so the gate is now `npm run typecheck && npm test`.
- `4bc2052` the model carries each source set's compiler settings (Maven's
  compiler plugin, with `annotationProcessorPaths` resolved by Maven itself;
  Gradle's compile task). Extraction runs a `-proc:only` pass into the build's
  generated-sources directory before ids are claimed, extracts what it wrote as
  `is_generated`, and analyses with processors on (Lombok). Plugin-generated
  roots are read as the last build left them; encoding and compiler arguments
  are the build's. Tested with a processor the tests build, through Maven and
  Gradle, a Latin-1 source, and `target/generated-sources`.
- JDK 26 javac, probed: runs no processor it only discovers on the classpath.
- A memory kill mid-measurement: 2,264 test temp dirs (411 MB) in the RAM-backed
  `/tmp` and a Gradle daemon; cleared. The suite leaks temp dirs — not fixed.

**Decided** — `decisions.md` 2026-09-15 (night ii): processors run, output
extracted; plugin output read as left. *Rejected:* resolve-only processing,
running generate-sources, `-proc:none`.

**Removed** — the frontend's fixed `-proc:none` and `-encoding UTF-8`; the
oldest worklog entry.

**Next up**
- Java flow layer, P1-java and P3-java; then quality and dataflow.
- A Java dogfood on a Spring/Lombok project (processors on a real subject).
- The module path for `module-info.java` projects.
- The test suite's leaked temp dirs.
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.
