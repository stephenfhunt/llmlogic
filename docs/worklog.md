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

## 2026-09-14 (small hours) — the Go frontend's refs layer

Asked to do the Go reference layer.

**Done**
- `9567c80` refs:
  - `ref` kinds (locals left to flow; basic types, `nil`, `iota` are no refs);
  - `call_site` dispatch (concrete methods static; interface and type-parameter
    methods virtual; function values indirect; adopted literals and builtins
    static);
  - `member_access` with `via_this` through the receiver, `type_ref` positions,
    `symbol_type`, `unresolved_ref`;
  - external ids down to outside struct fields (`ext:net/http#Server.Addr`).
- In the same commit:
  - `implements`/`overrides` from `types.Implements`, over project interfaces
    and the outside ones the project names, compared in a package view that
    reaches both;
  - interface embedding as `extends`; a new `embeds` relation, with `overrides`
    for hidden methods.
- **P4-go**: the running program is the oracle (reflect's `Implements`, and
  which declaration a reflect call runs). Rarest guard 140/400 (an embedded
  interface). Mutations red: no pointer method set, promoted methods skipped,
  no hiding overrides, no `extends`. **P2-go** gains `call_site` (calls
  dispatched `virtual` → red).
- Probes found two defects no test had yet: `is_any` true for a type parameter
  (its underlying type is the constraint), and a call of `f := func…` classed
  `indirect`. Seven new tests; `checks.dl` clean.

**Decided** — `notes/go-java-frontends.md` § The Go refs layer, as built:
conversions and composite literals are no calls, builtins are; `implements`
only over interfaces the project names; generic types skipped.

**Removed** — nothing in code; the oldest worklog entry.

**Next up**
- Go flow: a statement-level CFG over `go/ast` (`defer` as `finally`,
  `panic`/`recover`, `goto`, `fallthrough`, `select`, range-over-func), `fn`
  metrics, def/use, `concurrency_site`; P1-go, P3-go.
- Then quality, dataflow (P5-go), the library pass, `reference/go.md`,
  vendoring, a dogfood; Java after.
- Carried: P3/P5 static blocks; rebuild `dist/`; code-analysis's
  `reference/datalog.md` additions; the ablation control; push `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-14 (late night) — Go and Java frontends planned; Go's structure layer built

Asked to plan Go and Java extractors: the project's toolchain assumed present,
the facts maximalist. Planned; the user chose Go first, x/tools vendored, Java's
project model asked of Maven/Gradle, and every layer including dataflow.

**Done**
- `2fb17db` design: code-analysis `decisions.md` 2026-09-14,
  `notes/go-java-frontends.md`, ROADMAP items.
- `8a575f3` frontends write rows to a file (no 1 GiB pipe cap) and chain id
  counters; Python output sha256-identical on its fixture and `experiments/`.
- `e0664cb` schema: `file.namespace` (Python fills it), `symbol.form`,
  `imports.target_dir` and kinds `dot`/`cgo`/`implicit`, `package_dep.scope`,
  `visibility: package`, `excluded_file`, `extraction.go_version`. TypeScript on
  six fixtures and tsdl, Python on two subjects, equal the previous extraction
  minus the new columns. **It went in red**: a test pins a whole `package_dep`
  row, and the gate read `npm test | tail`, whose exit code is tail's. `41a498c`
  fixed it.
- `542f3a5` the Go frontend's structure layer (`src/frontends/go`, built on first
  use with the project's toolchain into `build/`): modules, imports as a row per
  referenced file plus `implicit` rows, package symbols, symbols on the shared
  kinds with `form`, params, docs, `Deprecated:`, build-constraint exclusions.
  A probe found cgo files missing; they are now read as written. 11 tests.
- `9ac4b51` P2-go and P6-go. Guards sized at 400 runs (a two-file import 73/400,
  so 40 runs). Mutations red: no `implicit` rows, one row per spec, sources
  unsorted.

**Decided**
- Go ids: a method is `TypeFile#T.M` wherever it is declared; a package is
  `dir#<package>` (`<package_test>` for an external test package); outside the
  root `ext:<import path>#…`, universe names `lib#…`, as Python splits them.
- x/tools v0.50.0 needs Go ≥ 1.26; the build error says so.

**Removed** — `runPython`'s pipe reading (`runFrontend`), `isPythonTarget`
(`detectLang`); the oldest worklog entry.

**Next up**
- Go refs: `ref`, `call_site`, `implements`/`overrides` from `types.Implements`,
  `embeds` (not yet in the schema), `member_access`, `type_ref`, `symbol_type`,
  `unresolved_ref`, external object ids; P4-go and P2-go's `call_site` half.
- Then Go flow (P1-go, P3-go), quality, dataflow (P5-go), the library pass,
  `reference/go.md`, `package.sh` vendoring, a dogfood. Java after; Maven and
  Gradle are not installed here.
- Carried: P3/P5 static blocks; rebuild `dist/`; code-analysis's
  `reference/datalog.md` additions; the ablation control; push `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-14 (night) — GitHub issue #1: a class static block crashed code-facts

Asked to reproduce and fix issue #1. Planned; the user chose a fix and a
regression test with widening P1 deferred, then asked for the widening too.

**Done**
- `31dd7d7`: reproduced with a new `test/structure.test.ts` case, failing on the
  issue's own frame (`structure.ts:473`). `hasBodyOrSignature` is now
  `hasParameters` and excludes static blocks, so its `SignatureDeclaration`
  guard is true. The test extracts every function-like kind through refs, flow,
  dataflow and quality, and checks the static block's `fn`/`flow_node` rows and
  every kind's `param` rows. Mutation: the old predicate turns it red.
  `npm test` and typecheck green.
- `code-analysis/bugs/resolved/006` records it. The commit says `Fixes #1`, so the
  issue closes when `trunk` is pushed.
- `5e72020`: **P1 widened.** Its statements also run as a class static block
  (a `return` renders as a probe). The acceptance half: Node compiles each block
  and one `static_block` fn is extracted. The in-block guard is sized from ten
  runs; `continue` (fewest 2) and `catch` (7) stay unguarded there. The widening
  found no CFG defect.
  - *Mutations* redden it: `isOwner` skipping static blocks, and `extractFlow`
    skipping them. `executorOf` skipping them stays green: `call_site.caller`
    comes from `ownerOf`.

**Decided** — nothing new: the issue's own "narrower predicate" alternative.

**Removed** — `hasBodyOrSignature` (renamed); the oldest worklog entry.

**Next up**
- P3 and P5 still generate function bodies only; static blocks' cyclomatic
  counts and points-to facts have no property.
- Rebuild `dist/` (`./package.sh`) before re-running on the reporter's project.
- Carried: code-analysis's `reference/datalog.md` lacks the datalog skill's
  additions; rerun the ablation control; measure the code-analysis playbook.
- Push `trunk` when the user says so.
- **Open**: `datalog/bugs/015`, `016`.
