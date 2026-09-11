---
name: code-analysis
description: >-
  Analyze a codebase's design with a logic engine instead of reading it file by
  file. Extracts facts from TypeScript source and git history, then answers what
  depends on what, where the cycles and layering violations are, how modules are
  coupled and how cohesive they are, what is dead or untested, what changes
  together, and whether package manifests match the code. Use for architecture
  reviews, refactoring and impact questions, and code-health audits over a whole
  repository.
---

# code-analysis — a codebase's design, asked of a logic engine

Questions about a whole codebase are graph and set questions: what reaches what,
what nothing reaches, what changes together without sharing code. Reading files
one at a time answers them slowly and drops cases without saying so. Here an
extractor turns the code and its history into fact tables — every file, symbol,
import, call, reference, control-flow edge and commit — and a Datalog engine
answers over all of them at once. **You choose the questions and check the
answers in the source**; the engine makes each question cheap and exhaustive.

Two executables sit in this skill's directory: `code-facts` (the extractor) and
`datalog` (the engine). Run them by path, from wherever you are working.

## The method

1. **Extract.** Give every tsconfig the project uses — tests often have their own:
   ```sh
   <skill>/code-facts tsconfig.json tsconfig.test.json -o /tmp/facts
   ```
   Git history comes along when the project is a repository. A 24k-line project
   takes about 6 s. For a language code-facts does not read, see
   `reference/bring-your-own.md`.
2. **Check the facts.** `<skill>/datalog /tmp/facts/lib/checks.dl` exits **1**
   when the facts are consistent. Exit 0 prints violations — stop and read them
   before analysing anything.
3. **Orient.** `<skill>/datalog /tmp/facts/lib/orient.dl` prints the size and
   shape, the most imported file, the most complex function, runtime import
   cycles, and the **blind spots**: calls and names the extractor could not
   resolve. Those bound every "nothing uses X" you will say later.
4. **Explore** by concern (below). Write each question as a small file next to
   the facts — `import "lib/<library>.dl".` then rules — and ask it with `-q`.
5. **Verify.** Open the source for every finding you intend to report. The
   engine is exact about the facts; whether the facts say what you think is the
   open question, and every trap in `reference/typescript.md` §5 was found by
   opening a file.
6. **Report** each finding with its evidence (below).

## What to explore

Libraries are in `/tmp/facts/lib/`; each imports what it needs, and each costs
what it imports, so import one. `reference/typescript.md` §3 lists every
relation each derives, with measured times.

| concern | ask | where |
|---|---|---|
| **architecture** | import cycles that exist at run time; dependencies pointing up a layer; which rules your architecture should obey (below) | `modgraph.dl`: `runtime_dep`, `cycle_edge`, `unit_dep`; `coupling.dl`: `sdp_violation` |
| **how much coupling** | afferent / efferent / instability / distance per file (`G = -1`), directory depth (`G = N`), package (`G = -2`) | `coupling.dl` |
| **how two modules are coupled** | the strongest kind per file pair — content, common, external, control, stamp, data — and the member, variable, literal or parameter that makes it so | `coupling_kinds.dl`: `worst_coupling`, then the per-kind relations |
| **hidden coupling** | files that change together with no import or reference between them | `cochange.dl`: `hidden_coupling` |
| **cohesion** | files that are several modules sharing a name; classes that want to split | `cohesion.dl`: `module_lcom4` first (much code has no classes), then `lcom4`, `tcc` |
| **complexity and risk** | the most complex functions; hot spots where complexity meets churn | `fn` (`cyclomatic`, `cognitive`); `cochange.dl`: `revisions` |
| **dead and unreached** | dead code, dead stores, exports nothing uses, exported functions no test reaches | `flow.dl`: `unreachable`, `dead_store`; `pointsto.dl`: `call_edge_pt_lexical` from test files |
| **dependencies** | undeclared, unused, dev-only-in-production, types-only packages | `packages.dl` |
| **change and people** | churn, ownership, files every change drags along, coupled code owned by different people | `cochange.dl`: `churn`, `main_author`, `cochange` |
| **API and types** | internal types leaking into exported signatures; where `any` enters; functions taking a record and reading one field | `type_ref`, `any_site`, `coupling_kinds.dl`: `stamp_param` |

`reference/typescript.md` §4 spells out the queries that need a choice the table
cannot show — dead exports (name your entry points) and untested exports (over
`call_edge_pt_lexical`, or three kinds of false positive).

## How to explore well

- **Rank, then read the top few.** The engine has no `ORDER BY` or `LIMIT`:
  compute the maximum (`M = max { C | fn(cyclomatic: C) }`), filter on a
  threshold, or count per bucket, then open the files that come out on top.
- **Cross two signals.** One metric is a hint; two agreeing is a finding. High
  efferent coupling with a high `module_lcom4`; high cyclomatic with many
  `revisions`; `hidden_coupling` with different `main_author`s.
- **A negative is a result** — "no runtime import cycle", "no dead private
  function" — but only as good as the blind spots `orient.dl` counted. Say so.
- **Ask `?why` before you explain a derived fact.** It prints the chain down to
  the facts, each with its file and line:
  ```sh
  <skill>/datalog q.dl -q '?why cycle_edge("src/a.ts", "src/b.ts")'
  ```
- **Narrow before you close over the whole graph.** Reachability over every call
  edge is the expensive query; seed it from the files you care about.
- **Project before you aggregate.** An aggregate beside a wide atom both counts
  the wrong thing (`reference/bring-your-own.md` §5, the count trap) and runs once
  per row of that atom: `kind(K) :- call_site(dispatch: K).` first, then count
  per `kind(K)`.
- **Metrics are relative.** An instability of 0.8 means nothing alone; the same
  file being the least stable thing everything depends on means a lot. Compare
  within the codebase, not against a textbook threshold.

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
imports `src/internal`", or "no package depends on a less stable one".

## Reporting a finding

For each: **the claim**; **the evidence** — files and lines, which the facts carry;
**the query** that found it, so it can be re-run; and **whether the source
confirmed it**. Say what the analysis could not see — the unresolved counts, a
tsconfig not given, a layer not extracted — when it bears on the claim. A
confident negative over a blind spot is the one kind of wrong answer this
toolkit makes easy.

## Reference

- `reference/typescript.md` — the facts: layers and relations, ids, the library
  with measured costs, the questions worth asking, and ten traps.
- `reference/datalog.md` — the Datalog language: syntax, negation, aggregation,
  imports, `?why` / `?whynot`, exit codes.
- `reference/bring-your-own.md` — extracting facts for another language yourself.
- `/tmp/facts/SCHEMA.md` — every relation and column of the extraction you made.
