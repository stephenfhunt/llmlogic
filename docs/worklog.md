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

## 2026-09-15 (night) — the Java refs layer, P4-java, and facts from javac

Asked to keep going with the next step (refs + P4-java); mid-way, asked whether
the Java frontend really uses the language's tooling — it did not everywhere.

**Done**
- `5fd77da` `throws_decl`; `556b460` `Names`, one home for element ids;
  `8d10949` a lambda and its first parameter shared an id — keys carry the tree kind.
- `5ed9d8a` the refs layer (`notes/go-java-frontends.md` § The Java refs layer):
  refs with kinds, call sites (static / virtual at javac's declaration),
  written supertypes, overrides against direct supertypes (`Object` too),
  implicit-`this` member access, type positions, types, unresolved names.
- `ad6e82f` **P4-java**, the JVM as oracle (reflection's supertypes and
  `getMethod`, and the method each call runs): found three `overrides` gaps —
  an inherited method implementing an interface's had no row (CHA missed code
  that runs), a supertype's members included an interface method it does not
  inherit, and javac's `Elements.overrides` skips inherited *abstract* methods.
  40 runs from 200-run guards; five mutations red.
- `f180d21` the structure layer takes what javac knows from its elements
  and `DocTrees` — implied modifiers, enum constants, record components,
  varargs, Javadoc, `main`, test methods by resolved annotation — where it had
  copied the language's rules off syntax. Fixture output unchanged.
- `65e3d8d`-style fix for **P4-py** (`92981b1`): its C3 guard fires in 38 of 200
  runs under Python 3.13, not 54; 50 runs by default.

**Decided** — `decisions.md` 2026-09-14 ***Consequences 2026-09-15 (night)***:
ids from syntax, facts from javac; inherited implementations are `overrides`.

**Removed** — `Declarations`' copy of Java's modifier rules, its Javadoc
scanner, name-matched tests and varargs regex; the oldest worklog entry.

**Next up**
- **Run javac as the build does**: annotation processors (Lombok, generated
  sources), encoding, compiler arguments, module path, plugin-added roots — the
  extension and init script report them.
- Java flow layer, P1-java and P3-java; then quality and dataflow.
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.
