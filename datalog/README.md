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
3. **Programmatic / agent API** — a JSON-in/JSON-out interface so an agent can load
   facts, add rules, query, and inspect derivations without parsing text.

The language is being designed spec-first. See [`spec.md`](spec.md) for the living
specification, the design process, and the decisions log, and
[`references.md`](references.md) for the annotated bibliography (classic Datalog
papers and notable implementations, grouped by topic) that guides the design.

## Status

Early scaffold. The module layout mirrors the intended architecture, but the lexer,
parser, evaluator, provenance, and import layers are stubs pending the spec.

## Build, test, run

```sh
cargo build          # compile
cargo test           # unit + integration tests
cargo run            # launch the (stub) CLI/REPL
cargo clippy         # lints
cargo fmt --check    # formatting check
```
