---
name: code-analysis
description: >-
  Analyze a codebase's design with a logic engine instead of reading it file by
  file. Extracts facts from TypeScript, Python or Go source and git history, then
  answers what depends on what, what a change affects, where the cycles and
  layering violations are, how modules are coupled and how cohesive they are,
  what is dead or untested, what changes together, and whether package manifests
  match the code. Use for architecture reviews, refactoring and impact questions,
  and code-health audits over a whole repository.
---

# code-analysis — a codebase's design, asked of a logic engine

Questions about a whole codebase are graph and set questions: what reaches what,
what nothing reaches, what changes together without sharing code. Reading files
one at a time answers them slowly and drops cases without saying so. Here an
extractor turns the code and its history into fact tables — every file, symbol,
import, call, reference, control-flow edge and commit — and a Datalog engine
answers over all of them at once. **You choose the questions, follow what they
show, and check the answers in the source**; the engine makes each question
cheap and exhaustive.

Two executables sit in this skill's directory, written `<skill>/` below:
`code-facts` (the extractor) and `datalog` (the engine). Run them by path, from
wherever you are working.

## The method

1. **Find the question.** An audit with no question returns every measure's
   outliers; a question returns an answer. If you can ask whoever set the task,
   ask what they want to know or change. Then read what the project already says
   it intends: import restrictions in its lint config (`no-restricted-paths`
   zones, custom import rules — and a rule written but switched off),
   `CODEOWNERS`, architecture docs, workspace boundaries. Each is a rule to check
   (*Architecture rules, checked*), and one the project wrote but does not
   enforce is usually the question it most needs answered. With no question and
   no stated intent, the audit is the way in — then take its strongest
   convergence as the question.
2. **Extract** into a directory outside the source tree — `/tmp/facts` below, or
   wherever this project keeps scratch output; the facts are large and are not
   the project's files. For TypeScript, give every tsconfig the project uses —
   tests often have their own; for Python or Go, the project's directory (or its
   `pyproject.toml`, `go.mod` or `go.work`); for a mixed repository, each:
   ```sh
   <skill>/code-facts tsconfig.json tsconfig.test.json -o /tmp/facts
   <skill>/code-facts path/to/python-project -o /tmp/facts
   <skill>/code-facts path/to/go-module -o /tmp/facts
   ```
   Git history comes along when the project is a repository.

   Then read `reference/typescript.md`, `reference/python.md` or
   `reference/go.md` — what the facts can and cannot say differs by language,
   and most sharply for Python, which has no type checker behind it. For another language, see
   `reference/bring-your-own.md`.
3. **Check the facts.** `<skill>/datalog /tmp/facts/lib/checks.dl` exits **1**
   when the facts are consistent. Exit 0 prints violations — stop and read them
   before analysing anything.
4. **Orient.** `<skill>/datalog /tmp/facts/lib/orient.dl` prints the size and
   shape, the most imported file, the most complex function, runtime import
   cycles, and the **blind spots**: calls and names the extractor could not
   resolve. Those bound every "nothing uses X" you will say later.
5. **Investigate** by concern (below). Write each question as a small file next
   to the facts — `import "lib/<library>.dl".` then rules — ask it with `-q`, and
   follow what it shows down to what to do about it (*How to investigate*).
6. **Verify.** Open the source for every finding you intend to report, and probe
   every negative. The engine is exact about the facts; whether the facts say
   what you think is the open question. Each trap in the language references is
   a case where a fact looked right and the source said otherwise.
7. **Synthesise, then report** (below).

## What to explore

Libraries are in `/tmp/facts/lib/`; each imports what it needs, so import the one
that answers the question. `reference/typescript.md` §3 lists every relation each
derives, and `/tmp/facts/SCHEMA.md` every fact the extraction holds — including
the ones no library reads.

| concern | ask | where |
|---|---|---|
| **architecture** | import cycles that exist at run time; dependencies pointing up a layer; which rules your architecture should obey (below) | `modgraph.dl`: `runtime_dep`, `unit_dep`, `cycle_edge`, `in_cycle`, `in_namespace_cycle`; `coupling.dl`: `sdp_violation` |
| **impact of a change** | files that import a changed file through any chain, and the tests among them; everything that calls a changed function | a closure grown from the change (below), over `modgraph.dl`'s `dep` and `callgraph.dl`'s `called_by` |
| **what a change did** | any question below, asked at two commits, answers diffed | two extractions (below) |
| **how much coupling** | afferent / efferent / instability / distance per file (`G = -1`), directory depth (`G = N`), package (`G = -2`), namespace — a Go package, a Python module (`G = -3`); `uncounted_dependent` for the importers afferent leaves out (a re-export is no reference, and a shallower file has no component) | `coupling.dl` |
| **how two modules are coupled** | the strongest kind per file pair — content, common, external, control, stamp, data — and the member, variable, literal or parameter that makes it so | `coupling_kinds.dl`: `worst_coupling`, then the per-kind relations |
| **hidden coupling** | files that change together with no import or reference between them *today* — a link removed inside the history window looks the same, so check both files' `first_change` or narrow `--git-since` | `cochange.dl`: `hidden_coupling` |
| **cohesion** | files that are several modules sharing a name; classes that want to split | `cohesion.dl`: `module_lcom4` first (much code has no classes), then `lcom4`, `tcc` |
| **complexity and risk** | the most complex functions; hot spots where complexity meets churn | `fn` (`cyclomatic`, `cognitive`); `cochange.dl`: `revisions` |
| **hazards in behaviour** | promises nothing awaits; errors nothing checks; what can throw past which callers; suppressed checks where the code churns; data from a source reaching a sink | facts: `floating_promise`, `ignored_error`, `throw_site` / `catch_site` over `call_edge_lexical`, `ts_directive` / `lint_directive` against `revisions`, `comment_marker`; `taint.dl` (supply `source/1`, `sink/1`) |
| **dead and unreached** | dead code, dead stores, exports nothing uses, exported functions no test reaches | `flow.dl`: `unreachable`, `dead_store`; `exports.dl`: `dead_export` (supply `entry/1`); `pointsto.dl`: `call_edge_pt_lexical` from test files |
| **dependencies** | undeclared, unused, dev-only-in-production, types-only packages | `packages.dl` |
| **change and people** | churn, ownership, files every change drags along, coupled code owned by different people | `cochange.dl`: `churn`, `main_author`, `cochange` |
| **API and types** | internal types leaking into exported signatures; where `any` enters; functions taking a record and reading one field | `type_ref`, `any_site`, `coupling_kinds.dl`: `stamp_param` |

`reference/typescript.md` §4 spells out the queries that need a choice the table
cannot show — dead exports (name your entry points to `exports.dl`) and untested exports (over
`call_edge_pt_lexical`, or three kinds of false positive).

## How to investigate

A library's answer is where an investigation starts, not what it reports. Take a
signal, say what would explain it, ask the query that would show that explanation
wrong, and go one level down — a file to the declarations in it, a declaration to
the files that use it, a file to its commits and authors — until the answer says
what to do.

- **Rank, then read the top few.** The engine has no `ORDER BY` or `LIMIT`:
  compute the maximum (`M = max { C | fn(cyclomatic: C) }`), filter on a
  threshold, or count per bucket, then open the files that come out on top.
  Filter `file(is_generated: false)` first: generated code tops every size,
  complexity and cohesion ranking.
- **Cross signals; a convergence is one finding.** One metric is a hint; two
  agreeing is a finding. High efferent coupling with a high `module_lcom4`; high
  cyclomatic with many `revisions`; `hidden_coupling` with different
  `main_author`s. When five measures land on one directory, the directory is the
  finding and the five are its evidence.
- **Go down, not across.** A measure says *that* something is wrong; the
  questions under it say what to do. A file several modules share:
  ```datalog
  % drill.dl — which modules, used by whom, changing with whom?
  import "lib/cohesion.dl".
  import "lib/cochange.dl".
  target("src/components/Table/utils.ts").
  group(R, E) :- target(T), module_component(T, R, E).
  uses_group(R, U) :- group(R, E), ref(from: S, to: E), symbol(id: S, file: U), target(T), U != T.
  users(R, N) :- group(R, _), N = count { U | uses_group(R, U) }.
  changes_with(R, U, N) :- uses_group(R, U), target(T), cochange(T, U, N).
  changes_with(R, U, N) :- uses_group(R, U), target(T), cochange(U, T, N).
  ```
  A common answer is one large group used widely and a tail of singletons used by
  one to three files each. Each singleton is a candidate to move next to its
  users, `changes_with` says which of those users already change with the file,
  and `main_author` says who to ask.
- **Ask the query that would prove you wrong.** Before a claim goes in the
  report, say what would make it false and ask that. A cycle — does it survive
  `runtime_dep`? A module nearly free of a dependency — are the imports left
  type-only (`imports.runtime`)? A codebase that writes `import { type X }`
  rather than `import type` shows `imports.kind: static` on imports that never
  reach run time.
- **Probe every negative.** An empty answer rests on what the extraction saw, so
  it is the claim most often wrong. For each one you report:
  - ask `?whynot` of the fact you expected — it names the literal that blocked
    and the next `?whynot` to ask, so following it down ends at the missing
    fact (an import with no `target_file`, say), not at "no";
  - count what the question cannot see: imports that resolved to no file
    (`target_ambient`, `resolved: false`), tsconfigs not given (`.storybook/`,
    `e2e/` and script configs are easy to miss), layers not extracted, and
    `orient.dl`'s unresolved calls.

  A typical miss: a stylesheet imported across a boundary resolves to an ambient
  pattern rather than a file, from a directory whose tsconfig was not given — so
  "nothing under `packages/` imports `app/`" answers 0 and is false.
- **Sample, and report precision.** A long answer is a class of claims. Open
  rows spread across it, not only the top, and say how many held ("4 of 5
  confirmed"). A false positive with a pattern is a query to fix and re-run.
- **Ask `?why` before you explain a derived fact.** It prints the chain down to
  the facts, each with its file and line — and the route is often the finding:
  ```sh
  <skill>/datalog q.dl -q '?why cycle_edge("src/a.ts", "src/b.ts")'
  ```
- **Project before you aggregate.** An aggregate beside a wide atom counts the
  wrong thing (`reference/bring-your-own.md` §5, the count trap):
  `kind(K) :- call_site(dispatch: K).` first, then count per `kind(K)`.
- **Metrics are relative.** An instability of 0.8 means nothing alone; the same
  file being the least stable thing everything depends on means a lot. Compare
  within the codebase, not against a textbook threshold.

## Impact, and what a change did

What a change affects is a closure grown backwards from the change — seeded, so it
costs what the change reaches, not the whole graph:

```datalog
% impact.dl
import "lib/modgraph.dl".
import "lib/callgraph.dl".
changed("src/components/Table/utils.ts").
impacted(F) :- changed(C), dep(F, C).
impacted(F) :- impacted(G), dep(F, G).
impacted_test(F) :- impacted(F), file(path: F, is_test: true).
changed_fn(F) :- changed(P), symbol(id: F, file: P, kind: function, exported: true).
caller(A) :- changed_fn(F), called_by(F, A).
caller(A) :- caller(B), called_by(B, A).
```

`dep` follows type-only imports too — right for "what might stop compiling";
`runtime_dep` answers "what behaves differently". A barrel that re-exports the
file puts everything importing the barrel in `impacted`, so `?why impacted(…)`
before claiming a file is affected. `caller` misses callbacks handed to a library
and indirect calls (`reference/typescript.md` §5, traps 2–4).

What a change *did* is any question asked at two commits. Answers print as sorted,
canonical facts, so the diff of two runs is the change — cycles made or broken,
layer rules newly violated, coupling that moved. Check out the base commit
somewhere outside the working tree — a worktree, as here, or a separate clone if
the project's conventions keep its git state untouched:

```sh
git worktree add /tmp/base <base-commit>
<skill>/code-facts /tmp/base/tsconfig.json -o /tmp/facts-base
<skill>/code-facts tsconfig.json -o /tmp/facts-head
cp q.dl /tmp/facts-base/ && cp q.dl /tmp/facts-head/
diff <(<skill>/datalog /tmp/facts-base/q.dl) <(<skill>/datalog /tmp/facts-head/q.dl)
git worktree remove /tmp/base
```

## Architecture rules, checked

Write the architecture you intend as facts, and let the exit code enforce it:

```datalog
% arch.dl, next to the facts
import "lib/modgraph.dl".
layer("src/domain", 1).
layer("src/app", 2).
layer("src/infra", 3).
in_layer(F, L) :- file_ancestor(file: F, dir: D), layer(D, L).
violation(A, B) :- runtime_dep(A, B), in_layer(A, LA), in_layer(B, LB), LA < LB.
```

```sh
<skill>/datalog arch.dl -q 'not violation(_, _)' && echo "architecture holds"
<skill>/datalog arch.dl -q 'violation(A, B)'                    # which edges break it
<skill>/datalog arch.dl -q '?why violation("src/domain/x.ts", "src/infra/db.ts")'
```

The last line ends at the import statement that breaks the rule. The same shape
checks "tests are the only importers of `test/`", "nothing outside `src/api`
imports `src/internal`", or "no package depends on a less stable one" — and the
first line is a CI check as it stands.

Encode the rules step 1 found the same way. For one the code does not meet yet,
the useful answer is the distance to it: the violating imports per directory,
split by `imports.runtime`, and their targets ranked by how many directories each
one blocks. A small file that blocks many, or a directory nothing else imports,
is the cheapest first move.

## Synthesise, then report

The findings are evidence; the report is what they add up to. Where the project
has conventions for reports — a format, a location, a tracker — follow them.

1. **Group** findings that share a cause — one directory, one module, one missing
   boundary — into one, with each signal as its evidence.
2. **For each**: the claim; the evidence — files and lines, which the facts
   carry; the query, so it can be re-run; and how it was checked — opened in the
   source, *k* of *n* sampled, the negative probed.
3. **Rank** by what acting on it buys against what it costs: churn and co-change
   say how often the problem is paid for, the impact closure how far a fix
   reaches.
4. **Recommend** the next move — the smallest change that clears the most — and
   **say what stays open**: the questions the analysis raised and did not answer,
   and what the extraction could not see where it bears on a claim.

A confident negative over a blind spot is the one kind of wrong answer this
toolkit makes easy.

## Reference

- `reference/typescript.md` — the facts: layers and relations, ids, the library, the
  questions worth asking, and twelve traps.
- `reference/python.md` — what differs for Python: what resolves without a type
  checker, what the library can and cannot compute, and seven traps.
- `reference/go.md` — what differs for Go: how its constructs map onto the
  shared relations, the facts only Go has, and ten traps.
- `reference/datalog.md` — the Datalog language: syntax, negation, aggregation,
  imports, `?why` / `?whynot`, exit codes.
- `reference/bring-your-own.md` — extracting facts for another language yourself.
- `/tmp/facts/SCHEMA.md` — every relation and column of the extraction you made.
