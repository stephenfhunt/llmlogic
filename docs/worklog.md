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

## 2026-09-15 (night v) — the Java dataflow layer and P5-java

Asked for the Java dataflow layer; planned first. The user's calls: a record is
modelled at its use sites, allocation ids are fixed now rather than filed, and
deconstruction patterns bind by component.

**Done**
- `e5a54bf` `bugs/resolved/009`: allocation-site ids chain across frontends
  (`__counters__` carries `alloc_site`). A tsconfig and a Go module in one run
  gave two variables one site; no test extracted two dataflow languages before.
- `82da3d0` the dataflow layer (`notes/go-java-frontends.md` § The Java dataflow
  layer): a field with no receiver loads off `this`, a static off its class,
  which is a `cell` so statics have a home; lambdas and method references are
  `function` allocations, and a functional interface's own call names its receiver
  `callee_var` — the half the refs layer left to points-to; records modelled at
  `new` and at the accessor; patterns by copy and by component. Fixture
  `java-dataflow`, `checks.dl` clean, points-to resolving all four end to end.
- **javac's own answers settled two guesses**: a record accessor's origin is
  `EXPLICIT` and it has no declaration tree, so *a declaration of one's own* is
  the test for what the compiler wrote; and the deconstruction pattern carries
  two interface names across JDKs, both of which are tried.
- `4d11e04` `bugs/resolved/010`: every declarator of `int a = 1, b = 2` shared
  one symbol — javac gives them all the declaration's position and kind. Its
  references, def/use and dataflow variable were all another variable's.
- `26db1b0` **P5-java**: javac and the JVM as the oracle, with a marker a
  functional value answers with its own name. Five forced shapes; six of seven
  mutations red. It found `010`: its probes all reported their method's first
  local, and four of those mutations survived until that was fixed.
- **`this_var` is unfalsifiable and no shape would help**: `pointsto.dl` also
  binds `this` from the allocation's type. Recorded as the library's question.

**Decided** — `decisions.md` 2026-09-15 (night v): records at their use sites;
allocation ids chained; deconstruction patterns by component; a class holding
statics is a `cell`.

**Removed** — `Refs`' private functional-interface test (now `Names`', shared);
the oldest worklog entry.

**Next up**
- `reference/java.md` — the dataflow layer's traps: a bound method reference's
  receiver is not modelled, an outside class's statics are opaque, the
  compiler-written record rule.
- A Spring/Lombok dogfood — the dataflow layer's first real subject, and the only
  place its cost is known: there is no Java repository on this machine, so the
  bench has never run over Java facts. The module path.
- **The suite's leaked temp dirs** — 509 dirs / 1.5 GB in a RAM-backed `/tmp`
  stalled a full run for 40 minutes. A gate problem now, not tidying.
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

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
