# llmlogic

**Experiments in giving language models a formal reasoning engine to lean on.**

Language models are unreliable at exactly the problems a solver is exact at:
transitive closure, multi-hop deduction, negation ("everything with no …"),
constraint satisfaction. Chain-of-thought on those is a guess that often looks
like an answer. A logic engine returns the complete, correct set of consequences
or tells you why it can't.

Engines like that have existed for decades. What's missing is the *interface* —
a syntax a model generates reliably on the first try, errors it can act on
instead of thrashing against, and output it can read and feed onward. This repo
is a set of tools and skills built to close that gap: not new solving theory, but
the connective tissue that makes an old, reliable idea something an agent
actually reaches for.

## Design pillars

Three commitments that recur in every project here:

1. **Provenance / explainability** — the engine can explain *why* a fact was
   derived, not just assert it. An unexplained answer is a different kind of
   black box, not a fix for one.
2. **LLM-friendly syntax and structured errors** — a familiar, unambiguous
   surface a model gets right unprompted, and errors that name the problem and
   the fix rather than a parser state.
3. **An agent-native interface** — the tool's output is valid input to the tool.
   Results come back as facts, so runs compose over pipes and an agent never has
   to parse a report format.

## Projects

Each project is self-contained: its own build system, tests, and docs.

| project | what it is | status |
|---|---|---|
| [`datalog/`](datalog/) | a Datalog engine in Rust for LLM/agent use, with fact-table imports from CSV / JSONL / Parquet / URLs | v1 core working end-to-end; ships as a [Claude Code](https://claude.com/claude-code) skill |

## See it work

```sh
cd datalog && cargo build --release   # the binary lands in target/release/datalog
```

Ancestry — the canonical case where prose reasoning slips a generation and the
engine doesn't:

```datalog
% family.dl
parent("alice", "bob").
parent("bob", "carol").
parent("carol", "dave").

ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).
```

```console
$ datalog family.dl -q 'ancestor("alice", Who)'
ancestor("alice", "bob").
ancestor("alice", "carol").
ancestor("alice", "dave").
```

Answers come back as ground facts in the same syntax you fed in, so a run's
output is another run's input:

```console
$ datalog family.dl -q 'descendant(Y) :- ancestor("alice", Y)' \
    | datalog - -q 'descendant(Y), Y != "bob"'
answer("carol").
answer("dave").
```

And when a program is wrong, the error says what to do about it:

```console
$ datalog -q 'p(X) :- q(Y)'
semantic error: unsafe rule for `p`: head variable `X` is not bound by the body — it must occur in a positive body atom, or be bound by an `=`-assignment or an aggregate result
```

Full engine documentation — imports, aggregation, negation, the agent-facing CLI
guide, and how to install the skill — is in [`datalog/README.md`](datalog/README.md).

## Where this is going

Inside `datalog`, the next substantial piece is the provenance *query* surface:
the engine already records derivations, but `?why` / `?whynot` and proof trees an
agent can read aren't exposed yet.

Beyond the engine, two other ways to put it in front of a model — a Claude API
agent-loop harness and an MCP server — are deliberately parked. The Claude Code
skill is the first experiment, and how well a model actually drives it is what
should decide whether a second form earns its keep. Future projects (other Rust
crates, Python packages) get their own top-level directory; there is no root
Cargo workspace.

The open backlog, with status on every item, is [`datalog/ROADMAP.md`](datalog/ROADMAP.md).

## How it's built

This repo is also an experiment in *how* to build software with an AI agent over
many sessions, and the process artifacts are checked in on purpose:

- **Spec-first.** [`datalog/spec.md`](datalog/spec.md) is a living specification
  and the design workspace — features are designed there, against canonical
  example programs, before they're implemented. Its §17 is an **append-only
  decisions log**: every non-obvious choice with its date, rationale, and the
  alternative it rejected.
- **Session handoff.** [`docs/worklog.md`](docs/worklog.md) is how one agent
  session hands off to the next in seconds instead of by re-reading transcripts.
- **Defects are files, and location is status.**
  [`datalog/bugs/`](datalog/bugs/README.md) — `bugs/*.md` is exactly the open
  set; resolving one moves it to `bugs/resolved/` with a resolution note.
- **Equivalence claims ship as properties, not unit tests.**
  [`datalog/testing.md`](datalog/testing.md). The record is exact: every "these
  two spellings mean the same thing" claim backed by a property has held; both
  that shipped with only a unit test later became defects
  (`bugs/resolved/001`, `bugs/resolved/002`). ~390 tests, unit through system.
- **Documents are edited under a discipline.**
  [`docs/rules/editing-docs.md`](docs/rules/editing-docs.md) — current-state docs
  get rewritten, append-only records never do, each rule has one normative home,
  and every record caps its item length so orientation cost stays flat instead of
  growing every week. Three of one week's four defects were a doc claim that had
  quietly stopped being true; the rule is the response to that.

[`AGENTS.md`](AGENTS.md) is the entry point for an agent working here, and each
project has its own.

## Status & license

Early and moving fast — the Datalog core works end-to-end, but the language is
still being designed in the open and things change.

**No license yet.** All rights reserved by default; the choice is deliberately
deferred rather than overlooked. If you want to use any of this, ask.
