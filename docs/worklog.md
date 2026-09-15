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

## 2026-09-15 (evening) — the Java frontend's structure layer

Asked to get started on the Java extractor; planned first (Maven through a core
extension, and structure + P2/P6 this session — the user's calls).

**Done**
- `1cb1351` schema columns for Java (`lang java`, forms, `static_import`,
  `on_demand`, `other_language`, `decorator.text`, `java_version`); TS, Python
  and Go fixture output byte-identical once the new absent columns are dropped.
- `e020aed` that byte-compare found **Go dataflow allocation sites varying run to
  run** (`bugs/resolved/007`): globals from maps, and an initializer tied with
  its test variant's. P6-go now extracts every layer; red on the unfixed code.
- `36ab69a` the Java frontend, structure layer (`notes/go-java-frontends.md` §
  The Java structure layer):
  - Maven's reactor from an extension compiled against the user's Maven, which
    stops the build before any plugin; Gradle from an init script run by the
    fixture's committed 9.7.1 wrapper; a plain directory; a failing build
    degrades to conventional roots and says so;
  - one javac task per source set, ids by declaration offset in path order;
  - imports as a row per file each statement reaches, `implicit` rows, runtime
    false for inlined constants and Javadoc links, coordinates from the build;
  - fixtures: a Maven reactor resolving from a local repository the tests build,
    a Gradle build, a plain directory; `checks.dl` clean on all three.
  - Frontends' stderr now reaches the user (Go's warnings were dropped).
- `686aca7` **P2-java** (the model rendered as packages and static imports,
  plain sources; 40 runs, guards from 200) and **P6-java** (reactor order); four
  mutations red, recorded in `testing.md`.
- `65e3d8d` the gate went red once on **P4-go's promoted-method guard** (equalities
  held): since its widenings it fires in 33 of 200 runs, so 25 runs missed it
  about one suite in 90; 50 by default, rates in `testing.md`.

**Decided** — `decisions.md` 2026-09-14 ***Amended 2026-09-15*** (the Maven
extension); `-proc:none` until the refs layer shows what processors cost.

**Removed** — the notes' `dependency:build-classpath` plan; the oldest worklog entry.

**Next up**
- Java refs layer and P4-java (the JVM as oracle): `ref`, `call_site`,
  `extends`/`implements`/`overrides`, `throws_decl`, `member_access`.
- Annotation processors: run the build's (Lombok, generated sources) — decide
  in the refs session with a Spring/Lombok module in hand.
- Carried: a Go subject with go.work and cgo; P3/P5 static blocks;
  code-analysis's `reference/datalog.md` additions; the ablation control; push
  `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-15 (afternoon) — `reference/go.md`, the Go bundle, and a caddy dogfood

Asked to write the skill texts for Go, vendor it, then dogfood.

**Done**
- `72090e4` `skill/reference/go.md` in `python.md`'s shape (the mapping, Go's
  own facts, the library over Go, ten traps), each claim run on a small module
  first. `SKILL.md`, `typescript.md` (granularity `-3`), `bring-your-own.md` and
  the wrapper name Go. `package.sh` runs `go mod vendor`; the bundle built its
  frontend with an empty module cache and `GOPROXY=off`.
- `6179073` `GOOS`/`GOARCH` in the environment built a frontend that could not
  run here; they are cleared for the build.
- Dogfood on caddy (107k lines, 2,684 commits) from the bundle: extraction
  23 s / 723 MB, `checks.dl` clean, `pointsto.dl` 54 s / 840 MB, the rest
  ≤ 10 s. Probes: cycles all within packages, `Module` recall 144/144, 504
  unexported functions with none dead (98 need points-to or value refs).
- `88c1a7f` `unsafe.Sizeof`/`Add` resolve to `ext:unsafe#…`; go.md corrected
  where caddy disagreed (interface embedding is `extends`; name-only
  constraints carry no detail; computed function values stay `unresolved`,
  since `checks.dl` wants a callee on `indirect`).
- `3106935` `implements` for types in `_test.go` (5 of 144 were missing): matched
  in the type's own view. **P4-go** widened to test-file types — red on the
  unfixed code at its first case. `5bfe0af` gofmt.
- `dist/` rebuilt.

**Decided** — `notes/go-java-frontends.md` § The Go dogfood: caddy; a
consequences note on `decisions.md` 2026-09-14.

**Removed** — the notes' stale P4-go oracle line; the oldest worklog entry.

**Next up**
- Java frontend, in its own session. Gradle comes from the wrapper (the user's
  call): generate `gradlew` once from a downloaded distribution (9.4.0 or later
  runs on JDK 26), and commit it with the fixture. JDK 26 has the compiler;
  `java` must resolve to it too.
- A Go subject with go.work and cgo, which caddy has neither of.
- Carried: P3/P5 static blocks; code-analysis's `reference/datalog.md`
  additions; the ablation control; push `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.
