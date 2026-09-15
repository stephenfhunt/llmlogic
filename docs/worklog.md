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

## 2026-09-15 (midday) — the Go frontend's dataflow layer

Asked to keep going: the Go dataflow layer.

**Done**
- `22bf679` dataflow, lowered from x/tools SSA built over every loaded package
  (dependencies and ill-typed packages from types; a failed build loses only
  that package's facts):
  - registers `<fn>$tN`; `DebugRef` feeds each named variable's symbol id;
  - a pointer to a struct or array is its object; others' content is `*`;
    field and element addresses resolve to object and field, with inline
    embedded structs flattened;
  - closures allocate with captures as `free0…` through the literal's `$this`;
    package variables are cells, functions function values, in `<module>`;
  - `<chan>`, `$thrown`, `append`/`copy`; `alloc` gains slice, map, chan, cell.
- `checks.dl` caught a `DebugRef`'s variable written without a `var` row
  on the first extraction.
- **P5-go**: generated programs compiled and run; every observed object or
  function in `pts`. Guards at 200 runs 78–146. Mutations red: no field
  loads, no formals, no function allocations, no interface copies, and no
  `callee_var` through function values — **green** until each function was
  made to call through a function value into a variable nothing else writes.
- A dataflow test over the flow module: closures, cells, `<chan>`, `$thrown`,
  `pts` of a captured variable; full suite green.

**Decided** — `notes/go-java-frontends.md` § The Go dataflow layer, as built,
including what is not modelled (escaping field addresses, method-value
receivers, promotion through an embedded pointer).

**Removed** — nothing; the oldest worklog entry.

**Next up**
- `reference/go.md` (traps in the notes), `SKILL.md` and `bring-your-own.md`
  naming Go, `package.sh` vendoring the Go frontend's modules and INSTALL.md.
- A Go dogfood on a real repository chosen for shape; calibration in `notes/`.
- Java after; the Gradle choice pending with the user.
- Carried: P3/P5 static blocks; rebuild `dist/`; code-analysis's
  `reference/datalog.md` additions; the ablation control; push `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-15 (late morning) — the Go frontend's quality layer

Asked to go ahead with the Go quality layer.

**Done**
- `eb69d96` quality, over the TypeScript layer's semantics:
  - `diagnostic`: go/packages' errors, once each across test variants;
  - `lint_directive`: `//nolint`, `//lint:ignore`, `#nosec`;
  - new `compiler_directive`: `//go:…`, `//export`, `//line`;
  - `comment_marker`, and `literal` (no import paths, tags or array lengths);
  - new `ignored_error`: discarded, blank, deferred, `go`;
  - `any_site`: written, but not as a constraint; call results;
  - `assertion`: `type_assert`, `type_switch`;
  - `throw_site` for `panic`, `catch_site` for `recover`.
- A probe caught two defects before the test was written. The go command's
  echo of a failed compile (`# pkg`) duplicated the type error as a
  file-less row; it is now dropped where type errors exist. And `catch_site.empty`
  was true for any one-statement function; it is now true only for bare
  `recover()`.
- A quality test over a module exercising every relation; full suite green.

**Decided** — `notes/go-java-frontends.md` § The Go quality layer: every dropped
error is a fact (`fmt.Println` included); a conversion is no assertion.

**Removed** — nothing; the oldest worklog entry.

**Next up**
- Go dataflow: three-address facts lowered from x/tools SSA (`var`,
  `assign`, `alloc`, `load`/`store`, `formal`/`actual`, `receiver`,
  `callee_var`), and P5-go (points-to soundness against execution).
- Then `reference/go.md` with the traps in the notes, `package.sh`
  vendoring, a dogfood; Java after (the Gradle choice pending with the user).
- Carried: P3/P5 static blocks; rebuild `dist/`; code-analysis's
  `reference/datalog.md` additions; the ablation control; push `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.
