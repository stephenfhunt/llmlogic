# datalog

A Datalog engine written in Rust, targeted at **LLM/agent use**. It aims to make
formal, explainable reasoning convenient for language-model agents, and to make it
easy to load fact tables from external sources (files and databases).

Part of the [`llmlogic`](../) repository — experiments in LLM agents that use formal
logic engines for better reasoning. This is the first project; it is entirely
self-contained within the `datalog/` directory.

## Design pillars

1. **Provenance / explainability** — the engine can explain *why* a fact was derived.
2. **LLM-friendly syntax + structured errors** — a familiar, unambiguous surface
   syntax that models generate reliably, with errors that are structured and actionable.
3. **Agent-native interface** — an agent drives the executable directly
   (skill-based, CLI-first). Datalog is the interchange format in both directions:
   query results are emitted as facts, so output is valid input and runs compose
   over pipes (the jq pattern, made Datalog-native). JSON serves the
   machine-readable edges (structured errors, provenance trees).

The language is being designed spec-first. See [`spec.md`](spec.md) for the living
specification, the design process, and the decisions log, and
[`references.md`](references.md) for the annotated bibliography (classic Datalog
papers and notable implementations, grouped by topic) that guides the design.

## Status

Working end-to-end for v1 core: lexer, recursive-descent parser, lowering, static
type inference, stratified semi-naive evaluation with provenance, and a canonical
printer are implemented, wired as `parse → lower → typecheck → eval`. The CLI runs
programs and answers one-shot `-q` queries. Still pending: external imports (§13),
aggregation (§9), and the provenance query surface (§11).

## CLI usage

```sh
datalog [<file> | -] [-q <query>]…
```

Feed the engine facts and rules; it derives new facts and answers your queries as
**ground facts in the same Datalog syntax** — so output is valid input and runs
compose over pipes.

```sh
# query a program file
datalog family.dl -q 'ancestor("alice", X)'

# define-and-select in one flag
datalog family.dl -q 'grandparent(X, Z) :- parent(X, Y), parent(Y, Z)'

# compose over pipes ("-" reads stdin)
datalog people.dl -q 'adult(N) :- person(name: N, age: A), A >= 18.' \
  | datalog - -q 'adult(N), N != "bob"'
```

The full agent-facing guide is [`docs/agent-skill.md`](docs/agent-skill.md).

## Build, test, run

```sh
cargo build          # compile
cargo test           # unit + integration + system tests
cargo run -- <file>  # run a program (or `-` for stdin)
cargo clippy         # lints
cargo fmt --check    # formatting check
```
