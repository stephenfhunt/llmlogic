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

## 2026-09-15 (morning) — Go's own facts, and namespaces in the library

Asked how well language-specific considerations are covered ("TypeScript
module ~= Go package?" — no: a module is a file, a package a directory), then to
do the Go-specific pieces before building further.

**Done**
- `2e6268e` facts:
  - `entry_point` by `go test`'s naming and signature rules;
  - `field_tag` per `key:"value"`, and `typed_const` (value, `iota`);
  - `module_directive` (go, toolchain, replace, exclude, retract, godebug);
  - receiver forms, and `implements.pointer`;
  - generic types in `implements` and embedding, via self-instantiation.
- P4-go widened to generic types and `pointer`; mutations (generics skipped,
  `pointer` always false) red. A TypeScript test pinned a whole `implements`
  row and needed the new absent column.
- `7e8f05c` library: granularity `-3` (namespace) in `units.dl`;
  `namespace_dep` and namespace cycles in `modgraph.dl`; Go's basic types in
  `coupling_kinds.dl`. Bench on fresh tsdl facts: five digests identical.

**Decided** — `notes/go-java-frontends.md` § Go's own facts: `typed_const` is
a shape, not an enum verdict; the traps for `reference/go.md` are listed there
(uncalled entry points, intra-package file cycles, `dit`/`noc` zero for
structs, promoted access, per-file `module_lcom4`).

**Removed** — nothing; the oldest worklog entry.

**Next up**
- Go quality layer: `diagnostic`, `lint_directive` (`//nolint`,
  `//lint:ignore`), `compiler_directive` (incl. `//go:embed`),
  `comment_marker`, `literal`, `assertion` (`type_assert`), `any_site`,
  `throw_site`/`catch_site`, `ignored_error`.
- Then dataflow (P5-go), `reference/go.md` with the traps above, vendoring, a
  dogfood; Java after (the Gradle choice pending with the user).
- Carried: P3/P5 static blocks; rebuild `dist/`; code-analysis's
  `reference/datalog.md` additions; the ablation control; push `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-14 (dawn) — the Go frontend's flow layer

Asked to carry on; the user installed Maven, and found no Gradle package.

**Done**
- `4110d3d` flow: a statement-level CFG per function over `go/ast`.
  - `defer` is one `finally` node per deferring function. Returns, the body's
    end and panics enter it; it loops when several calls may be deferred, and
    resumes at `exit` when a deferred call may `recover`.
  - Implicit panics only where a function defers. `goto`/`fallthrough` node and
    edge kinds. `select` evaluates its operands on entry, its cases chained.
  - A type switch's variable is defined at the switch; package initializers
    run in `<module>`.
  - def/use, `captures`, `closure`, `call_at` (a deferred call at `finally`),
    decisions, `fn` metrics, and a new `concurrency_site`; `flow.dl` counts the
    new statement kinds.
- **P1-go** runs generated functions against their graphs. It found
  `fallthrough` carried through an empty clause into the next. Two harness
  rules were needed: a `finally` may run no deferred call, and a select's
  operand probes share a node.
- Guards were too rare at first (a labeled jump in 3% of runs, a recovery in
  6.5%). Two generator shapes (`nest`, `risky`) and probes before labeled
  jumps now observe them directly. Rarest guard ~10% → 100 runs.
- The mutation "no resume after recover" stayed **green**: the edge is
  redundant wherever a function can end normally. A doomed last function (a
  recovering defer, then `panic(0)`) made it red. The other six mutations are
  red; **P3-go** red on `range` uncounted.
- A flow test over a module with every construct, whose edge multisets were
  checked by hand; `flow.dl` finds exactly its one dead store.

**Decided** — `notes/go-java-frontends.md` § The Go flow layer, as built.

**Removed** — nothing in code; the oldest worklog entry.

**Next up**
- Go quality: `diagnostic` (go/types errors), `lint_directive` (`//nolint`,
  `//lint:ignore`), `compiler_directive`, `comment_marker`, `literal`,
  `assertion` (`type_assert`), `any_site`, `throw_site`/`catch_site`
  (panic/recover), `ignored_error`.
- Then dataflow (P5-go), the library pass, `reference/go.md`, vendoring, a
  dogfood; Java after. Gradle: the user to choose SDKMAN, a Gradle zip, or a
  Gradle-wrapper-only fixture before Java's project model.
- Carried: P3/P5 static blocks; rebuild `dist/`; code-analysis's
  `reference/datalog.md` additions; the ablation control; push `trunk` when asked.
- **Open**: `datalog/bugs/015`, `016`.

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
