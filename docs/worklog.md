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

## 2026-09-16 (later) — the Java frontend ships

Asked to wrap the stage up. The outstanding piece was the skill, not the
extractor: the Java frontend has been in the bundle since it was written, and
nothing the skill *says* mentioned Java.

**Done**
- `skill/reference/java.md` — the sibling of `go.md` and `python.md`: how Java's
  constructs map onto the shared relations, the library over Java facts, and ten
  traps. Every claim in it was run against a real fact base first, which changed
  three of them:
  - `extends` is the written clause, so implicit `Object` is not in it and a
    class extending nothing has `dit` 0 — while `overrides` *does* name
    `Object.equals`. The asymmetry is worth a trap of its own.
  - `coupling_kinds.dl`'s `common` is empty over Java: it looks for module-level
    variables, and Java's shared mutable state is `static` fields. The reference
    says what to ask instead.
  - `content` coupling does classify — as an outer class touching a nested
    class's private member, which is the Java idiom for it.
- **Reflection is trap 2**, ahead of everything else. A method reached only
  through `getMethod(…).invoke(…)`, a `ServiceLoader` or a JSON binding looks
  dead, and Java does that constantly.
- `SKILL.md` names Java in the extract step and the reference list, with *build
  the project first* stated where a reader meets it; `package.sh`'s INSTALL.md
  names the JDK requirement.
- **Verified as the bundle, not the checkout**: `./package.sh`, then the packaged
  `./code-facts` and `./datalog` over a real Maven module — 273 files, zero
  unresolved refs, zero diagnostics, `checks.dl` clean.
- Exercised the libraries on a coverage question over the dogfood base. No defect
  found; the useful part was that **static reach answers it only half-way** —
  reflection bounds it below, CHA above — and the git view (98% of production
  files have been changed in a commit that also touched a test) disagrees usefully
  with the call-graph view (66% of functions reachable from a test).

**Decided** — nothing new. ROADMAP: the Java frontend is _shipped_; the module
path is its own _queued_ item.

**Removed** — the "what is left" clause on the Java frontend item, split into the
module-path item; the oldest worklog entry.

**Next up**
- **`taint.dl` in the bench**, with a source and sink it supplies itself — the one
  library nothing measures, and the one that does not finish.
- Java's module path; the build's includes/excludes as `excluded_file`.
- The suite's leaked temp dirs (a RAM-backed `/tmp` stalled a full run once).
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-16 — the Java dogfood: OpenRefine

Asked to pull OpenRefine and use it as the test bed. Its default branch is
`master`, not `main`; built with `mvn test-compile` first, since the extractor
reads the build's output as the build left it.

**Done**
- `3c94e78` `bugs/resolved/011`, two model defects the subject found, together
  **2,942 javac errors and 2,184 unresolved references over 320 files → zero**:
  - a plugin writing generated sources into `target/generated-sources` itself
    (build-helper's `add-source`) got `generated-sources/com` as its root. A
    generated root is now the directory a file's own `package` declaration
    implies — the language's rule, not a naming convention;
  - a module depending on a sibling's `<type>test-jar</type>` saw neither the jar
    (the reactor filter drops it, rightly) nor the sibling's test sources; the
    model carries `testModules` beside `modules` now. Fixture `java-maven-gen`
    covers both layouts and the test-jar, and is red without either fix.
- **Calibration** (`notes/go-java-frontends.md` § The Java dogfood): 968k facts
  in 27.7 s at 5.0 GB; `checks.dl` clean, `unresolved_name_count(0)`;
  `pointsto.dl` 55.1 s at 1.2 GB, the only library over 30 s — caddy's profile.
- **The dataflow layer held.** `ProjectManager.getLookupCacheManager()` points to
  one site and nothing else — the field initializer's `new`, through the `this`
  store and the getter's load. All 8,113 functions' parameters have `formal` rows;
  `this_var` is on exactly the non-static methods and constructors.
- **The functional-interface rule earned nothing here, and that is the subject.**
  All 305 lambdas stay in the temporary they were allocated into: OpenRefine
  hands them to the standard library. An ungated `callee_var` would resolve
  nothing either — measured, not assumed.
- **`taint.dl` does not finish**: stopped at 20 minutes, its heap step alone
  timing out at 7. It binds `pts` on both columns in one rule, which no re-keying
  answers — the engine item's own stated reopening case, now measured
  (`datalog/ROADMAP.md`). Nothing benches `taint.dl`, which is why nothing knew.
- The decorator layer met a real subject at last: 6,008 rows over 45 annotations.

**Decided** — nothing new; `decisions.md` 2026-09-15 (night v) gains
***Consequences 2026-09-16***.

**Removed** — the directory-name heuristic for Maven generated roots; the oldest
worklog entry.

**Next up**
- `reference/java.md` — the dataflow traps, and what this dogfood adds: build the
  project first.
- **`taint.dl` in the bench**, with a source and sink it supplies itself — the
  one library nothing measures, and the one that does not finish.
- The suite's leaked temp dirs (a RAM-backed `/tmp` stalled a full run once).
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

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
