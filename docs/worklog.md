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

## 2026-09-16 (later iii) — a modular Java project is read as its module

Asked for the module path, after the completeness review named it the one place
the facts could be quietly wrong rather than merely absent.

**Done**
- **The gap was the opposite of what I had written.** Probing javac first: a
  `module-info.java` has always been in the file list, so *JDK* boundaries were
  already enforced — a module a declaration does not `require` was never visible.
  What failed was dependencies. On the class path they are in the unnamed module,
  which a named module cannot read, so **every `requires` failed with "module not
  found"** and nothing a dependency held resolved. Not too much visible: almost
  nothing.
- A source set declaring a module now has its dependencies on `MODULE_PATH`.
  Which entries are modules and what each exports is javac's answer; the
  extractor chooses only the location. Splitting the path ourselves — reading
  each jar for a `module-info.class` — was rejected as a second implementation of
  a rule the compiler owns.
- **Two knock-ons the fixture caught**, both from a dependency now being a named
  module as well as a jar: `symbol.package` returned the module name where
  `packages.dl` joins on the build's coordinate, and `jdk` called every named
  module the platform's. `packageOf` prefers the jar's coordinate now, and `jdk`
  means *in a module with no jar behind it*. Byte-identical `symbol` and
  `import_name` on a non-modular real subject, which is the whole blast radius.
- Fixture `java-modular` with a modular dependency built into the test
  repository: a `requires` that resolves, an exported package that does, and two
  the boundary hides — a non-exported package and an unrequired JDK module. The
  class-path mutation is red with exactly `compiler.err.module.not.found`.

**Decided** — `decisions.md` 2026-09-16: a modular source set is read as its
module; javac classifies the path; the coordinate wins over the module name.
*Rejected:* splitting the module path ourselves.

**Removed** — the "javac is never given a module path" note, written earlier the
same day and wrong in both directions; the oldest worklog entry.

**Next up**
- **A modular multi-module build**: a sibling of the same reactor is read from
  source, so it is not on the module path and a `requires` naming it is reported
  not found. `--module-source-path` is the answer.
- `module-info.java` as a `symbol`, so a package joins to the module exporting it.
- A Lombok project on a real subject; a Gradle one — both fixture-tested only.
- Go's own equal gap: a subject with `go.work` and cgo.
- `taint.dl`'s cost; the suite's leaked temp dirs.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-16 (later ii) — the module path, and taint.dl measured at last

Asked to knock out the two items the last entry left queued.

**Done**
- `806c06f` **Java's module path.** A `module-info.java` becomes
  `module_directive` rows — the declaration, each `requires` with its modifier,
  each `exports` and `opens` (a row per module a qualified one names), `uses` and
  `provides`. The relation was go.mod's and is now what a module declares about
  itself in either language; `package` is nullable, since plain sources have no
  build module.
  - The *types* a `uses` or `provides` names already resolved as `ref` rows —
    the one reflective edge that is a fact. The module and package names around
    them were not, and produced **a bogus `unresolved_ref` per directive**, five
    on a nine-line declaration. Both scanners now walk a module tree for its
    service names only.
- `806c06f` **The build's own includes and excludes.** A file Maven's
  `<excludes>` or Gradle's pattern set leaves out was being extracted *and
  compiled* — code the product does not contain. It is an `excluded_file` with
  reason `build_excluded` now, and in no other relation. Patterns are matched as
  globs. Fixtures `java-module` and an exclusion in `java-maven-gen`; each of the
  three fixes is red without it, and a real subject using neither is unchanged.
- `a47d1c3` **`taint.dl` is in the bench.** A `Library` may carry a `driver` the
  bench runs in its place, since taint's question comes from its caller. The
  first seed pair tried came out empty on a fixture — a seed that can be empty
  digests nothing — so the pair is now external-in to external-out, populated
  wherever the dataflow layer is.
- **The measurement corrected me.** I had called taint's cost a Java-and-size
  problem; it is neither. On `@grafana/ui` — TypeScript, 1.17M facts, *larger*
  than OpenRefine's 968k — it finishes in **475 s against `pointsto.dl`'s 6.2 s**,
  while on OpenRefine it is stopped at 1800 s. `datalog/ROADMAP.md` and
  `notes/code-facts.md` carry it.
- **Being first to reach `--timeout` found two bench defects**: a stop arrived as
  a thrown ETIMEDOUT and took the whole run down, and the engine outlived the
  stop — Node signals only the process it spawned — keeping a core busy through
  every library after it. Both fixed; a stop is reported as a stop.

**Decided** — nothing new. ROADMAP: the module path is _shipped_; `taint.dl`'s
cost is its own _open_ item.

**Removed** — the "`module_directive` is empty for Java" line in
`reference/java.md`; the oldest worklog entry.

**Next up**
- `taint.dl`'s cost: the engine item it belongs to is the non-leading-column
  seek, and a program cannot re-key its way out of binding `pts` both ways.
- The suite's leaked temp dirs (a RAM-backed `/tmp` stalled a full run once).
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

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
