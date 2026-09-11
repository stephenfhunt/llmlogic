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

## 2026-09-11 (later) — `code-analysis/`: its own project and skill, and Python

Asked whether a domain skill should package the extractor and the engine and
guide the exploration. Decided with the user: a new top-level project, a Python
extractor now, the measurement designed now and built next session. Built so.

**Done** — **108 tool tests**, experiments **1,661**, every commit gated
- **`code-analysis/`**: `ts-facts` moved and renamed `code-facts`; the datalog
  skill restored to `7f3e998` byte for byte, so experiment workspaces hash as
  before. The skill is a playbook (extract → `checks.dl` → `orient.dl` → explore
  by concern → verify → report; architecture rules as exit codes),
  `reference/typescript.md` and `python.md`; `package.sh` builds the bundle,
  verified installed.
- **Python frontend** (stdlib; Node validates its stream against `schema.ts`):
  every layer but dataflow. P1-py–P4-py, mutation-verified; P1-py and P4-py take
  `python3` itself as the oracle.
- **Found on the way in:** breadth-first MRO; a `match` guard sharing its
  pattern's node; sqlparse's self-importing `__init__` (1,165 unresolved names);
  `x = x.next()` recursing forever; lambdas in decorators never declared;
  comprehensions in nested defs binding outward. In TypeScript: cognitive
  complexity under-counted else-if bodies.
- **Checked against independent answers:** the facts answer `static_analysis`'s
  four questions exactly as `truth.py`; on `experiments/` (21.6k lines, 4.3 s)
  1,116 of 1,117 functions' branch counts equal ruff's mccabe. And a finding:
  `anthropic`, declared in `experiments/pyproject.toml`, is imported nowhere.
- **The unexplained `npm test` failure was P5's heap guard** (32 of 400 runs);
  P2-py had a second (35 of 400). Both reshaped, their rates measured.
- **H-CA1 pre-registered** — `experiments/hypotheses.md` addendum.

**Decided** (`code-analysis/decisions.md`; experiments `decisions.md` 2026-09-11 later)
- **One schema for both languages**; a Python writer would be a second home.
- **Python cyclomatic counts like ESLint**, for parity; mccabe's difference is
  documented and measured, not hidden.
- **A guard that needs a shape once is sized from its measured rate.**
- **H-CA1's `engine` arm gets `code-facts` undocumented**; the primary endpoint
  is `code-analysis` − `engine` at haiku.

**Removed**
- From the datalog skill: `ts-facts`, `tools/`, `recipes/typescript.md`. From the
  frontend: its breadth-first `mro()`, its unscoped binding walk, the silent
  fallback when `py_flow` failed to import.

**Next up**
- **Build the H-CA1 pack**: the TypeScript corpus, oracles, `node`/`python3` on a
  scrubbed PATH; run `harness power` on the real item count before any grid.
- **A test file's process dies now and then** (`'test failed'`, no assertion):
  twice in 25 runs under heavy concurrent load, never in 35 idle ones; and one
  uncaptured failure. Capture every suite run's output until it is named.
- Parked: a Python dataflow layer; a name-tier helper in `lib/` for Python's
  unresolved calls.

## 2026-09-11 — ts-facts at module scale: cohesion, coupling kinds, package hygiene

Asked, after a brainstorm on what the facts make derivable, for the
`member_access` fix and three library additions. All four shipped; the
brainstorm's other threads are below.

**Done** — **79 tool tests**, each commit gated on the suite
- **`member_access`** covers object type aliases and inline object types
  (`class` → `owner`): tsdl 2,757 → 3,281 rows. The refs layer resolves
  `obj["secret"]`, which is how TypeScript code reaches a `private`.
- **`cohesion.dl`: `module_lcom4`** — a file's exports grouped by what they
  share. tsdl: `test/support.ts` is 9 groups; `lexer.ts`'s second is
  `CONTEXTUAL_KEYWORDS`, exported and used by nothing in `src`.
- **`coupling_kinds.dl`** — Myers' scale per file pair. tsdl's `external`
  coupling is real: Datalog tokens (`":-"`, `"?why"`) spelled in several modules.
- **`packages.dl`** — undeclared, unused, dev-in-production, test-only and
  types-only dependencies; `imports.builtin` and `package_dep.types_for` added.
- New `design` fixture, expectations worked by hand; recipe, note updated.

**Decided**
- **Classify by fact, not by count:** each coupling kind names the member,
  variable, literal or parameter that makes it so.
- **Text-typed heuristics stay in the lib, stated** — "primitive" is printed
  `symbol_type.text`; changing them is editing one rule.

**Removed**
- `member_access.class` (renamed). Nothing else.

**Next up**
- **`SKILL.md`'s description** never mentions codebases or `./ts-facts`; the
  user's call, since it loads every session and the experiments measure it.
- From the brainstorm, unbuilt: architecture rules as exit-code checks with
  `?why` for the offending chain; `--tag` name classification at extraction;
  purity and escape analysis; Lakos levelization (collapse cycles first).
- **Two broken intermediate commits** (`bb41e57`, `feb9741`: a fixture file the
  npm glob ran as a test), fixed forward in `244620e`.

## 2026-09-10 — `ts-facts`: a TypeScript project, extracted for the skill

Asked for a maximalist tool in the skill's static-analysis section: point it at a
tsconfig, get facts about everything from module relationships to code flow,
useful for coupling, cohesion and questions no linter asks. Built as planned, in
eleven commits, and it found two engine defects, which were fixed here when asked.

**Done** — `skill/tools/ts-facts` (`./ts-facts`), **72 tool tests**, engine suite
and clippy clean, experiments **1,661** green
- **61 relations over seven layers**: structure, refs (checker-resolved call
  sites with dispatch kinds), flow (a CFG per function, def/use, metrics),
  dataflow (Doop-style), quality, git (renames followed to today's path).
- **`lib/`, split by cost**: checks, modgraph, callgraph/callreach, coupling,
  cohesion, metrics, flow, dominators, pointsto, taint, cochange.
- **Properties P1–P8**, each mutation-verified (`testing.md`); P1/P5 run in Node.
- **Engine:** `bugs/resolved/010` (empty JSONL import), `011` (clash message).
- **Harness:** the local subject's skill listing named every file under `skill/` —
  thousands, once the tool landed; it now names what the workspace carries.
- **tsdl** (24.5k lines, TS 7): 205k facts in 5.7 s, libraries 0.3–9.4 s, clean.

**Decided** (`spec.md` §17 2026-09-10; `notes/ts-facts.md`; experiments
`decisions.md` 2026-09-11)
- **TypeScript 6.0 pinned, in-process** — 7.0 has only an unstable IPC API.
- **`src/schema.ts` is the schema's one home**; ids keyed by declaration position.
- **Emit primitives, derive measures in Datalog**; a library's function-typed
  callee is named as the target (2,843 of tsdl's 2,870 "indirect" calls were
  vitest).
- **Checking answers against the source found every call-graph trap.** Untested
  exports on tsdl went 5 → 0 across library-invoked callbacks, callbacks in object
  literals, and structural implementations CHA cannot see.
  `hidden_coupling`'s top tsdl pair was real.

**Removed**
- The writer's header-only-CSV workaround for empty tables, once `010` was fixed.
- `skill_body`'s directory-wide listing, including the wrapper it always listed
  and never copied.

**Decided — *added 2026-09-11***
- **`imports.runtime` comes from the emitter** (§17 amendment; P8). The user
  asked whether import facts separate type from runtime imports: they did only
  syntactically, and TypeScript elides any import whose bindings only annotate.
  `runtime_dep` was wrong on every project not under `verbatimModuleSyntax`.

**Next up**
- **Point it at a real, larger codebase** than tsdl — a class-heavy one, since
  tsdl has no classes and the cohesion library has only fixture evidence.
- **One unexplained `npm test` failure**, seen once in ~25 full runs; capture the
  next with the default reporter.
- A TypeScript 7 backend when its API stabilizes; accessors, spread and instance
  method values in the dataflow layer.
- Unchanged from before: size the provenance run with `--repeats`; the
  `SKILL.md` exit-code gap.
