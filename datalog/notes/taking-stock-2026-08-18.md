# Taking stock at feature-complete — seven holes, and what the backlog could not show

*Recorded 2026-08-18, after §6 shipped and the open-defect set went empty. The
question asked was "any major design holes?", and the method was deliberately not
to re-read `ROADMAP.md`: **a backlog only lists holes someone already noticed.**
This note reads §§1–15, `src/main.rs` and the exported API instead, and records
what that turned up. Three findings were untracked; four were tracked and, on this
evidence, mis-ranked.*

**Status: findings, not decisions.** Each has a `ROADMAP.md` item or an annotation;
none is ratified, and the recommended sequence at the bottom is a recommendation.

## The untracked three

### 1. No integrity constraints, and no exit code that carries an answer

`grep -i constraint spec.md` finds nothing about the language: no `constraint`
declaration, no denial rule (`:- body.`), no way to say **this must never happen**.
The skill's own description advertises "constraint/consistency checking
(scheduling, assignment, logic-grid puzzles)". Today that is written as
`conflict(X, Y) :- …`, queried, and **the caller must parse stdout to learn the
answer**.

The exit code cannot help. `src/main.rs` returns `ExitCode::SUCCESS` for every run
that completes, and an empty answer set prints nothing (§14: silence means no). So
`0` means *ran*, not *consistent*, and `datalog check.dl && deploy` is a shell
pipeline that cannot express what it looks like it expresses. For **pillar 3**
(agent-native, CLI-first) this is the gap that matters: an agent can ask a question
and cannot branch on the answer without a parser.

**The finding that makes it cheap: this design already exists as an open question,
reached from the other side.** The truncation contract (§17 2026-08-16, decided and
unbuilt) left exactly one thing open — the CLI shape — and stated it as *"'withhold'
has to mean an exit code and a stdout discipline, not a return field."* Withholding
an unsound answer and signalling a violated constraint are the same mechanism: a
run that completed, produced no rows, and needs the caller to know **why not**.
Designing them apart would produce two vocabularies for one exit code.

### 2. Nothing is reusable across runs, and there is no REPL

Every invocation re-parses, re-imports and re-runs the fixpoint from zero; §13
materializes every import eagerly. The agent workflow is iterative by nature —
write, run, read, fix, run — so a 170k-row import is paid once per iteration, and
an agent's *first* program is rarely its last.

Nothing on the backlog covers reuse across runs. "Filter pushdown" and
"parallelism" both make **one** run faster, which is a different axis. The reason
it stayed invisible is measurement shape: `notes/cross-engine-benchmark.md` times
whole processes, best-of-3, which is precisely the design that cannot see a
per-iteration reload.

Related and separable: **there is no REPL.** `datalog/AGENTS.md` claimed one twice
("a thin binary (CLI/REPL)", "`cargo run` # launch the CLI/REPL"); `cargo run` with
no arguments prints usage and exits 2. Corrected 2026-08-18. It is the drift
mechanism `docs/rules/editing-docs.md` names, in the file every session loads.

### 3. Import provenance is predicate-level, not row-level

`src/engine/mod.rs` (`validate`): *"Imported facts are ordinary base facts by the
time the engine runs (§13): the source layer materialized them into `program.facts`
before lowering, and `ImportSpec` survives only as provenance/definedness
metadata."*

So the anchor is the **relation**, not the row. When `?why` lands, a fact out of a
CSV will explain as "because `employees.csv`" and never "because row 4,182" — and
for an engine whose §13 pitch is reading real files, the row is the answer people
will want. §11 says base facts are "anchored by the program text (or, later, the
import)", which reads as a granularity that does not exist yet.

Recording the row costs memory on exactly the workloads already at 1.2 GB
(`notes/cross-engine-benchmark.md`), so this is a real trade and not an oversight
to fix by default. It belongs in the same decision as finding 4.

## The tracked four, re-ranked

### 4. Pillar 1 costs full price on every run and returns nothing to its consumer

The derivation recorder runs unconditionally inside the fixpoint, with no flag.
`notes/cross-engine-benchmark.md` ranks it the **top** profiling target and a
sibling engine measures **13×** for its own on a cyclic graph. On the other side,
`?why`/`?whynot` are designed (§17 2026-08-16) and unbuilt — absent from the CLI,
and absent from the exported API, which is `run*` plus `RunResult` (`src/lib.rs`).
A Rust consumer can reach derivations through `pub mod engine` and `pub mod
provenance`; the agent-native surface, pillar 3, has nothing.

The backlog carries this as **two items in two sections** — the query surface under
Provenance, "does the derivation store earn its cost" under Provenance-after-
profiling — and nowhere states them as one fact. **They have to be decided
together**: a profile is under pressure to make the recorder optional, and pillar 1
is the only argument that it should not be. Whichever is decided first silently
constrains the other, and finding 3 is a third input to the same decision.

### 5. §1 has never been written, and filing it under "spec hygiene" is the error

§1 owes goals, non-goals, target users and success criteria; §2 has four of five
principles still candidates. `ROADMAP.md` files both under **Spec hygiene &
(formerly) §6**, beside chronology normalisation and the decisions-log restructure.

That is a category error with a practical cost. §1 is not tidiness, it is the
**definition of done** — and the question that prompted this note was "we're feature
complete, right?", which is not answerable against an unwritten §1. The same absence
is why "are temporal types in scope", "is a hosted surface in scope" and "is v1
finished" are each decided ad hoc, at the moment they are asked, by whoever asks.

It is also among the cheapest items left, and it makes every other ranking on this
list decidable rather than arguable.

### 6. The project's own hypothesis has never been measured

The repo exists to test whether LLM agents reason better with a formal logic engine
than with their own chain of thought. `EXPERIMENTS.md` is, in the backlog's own
words, "an honest checklist eyeballed in a session, run twice" — while §1 states
the three pillars "have driven every decision in §17 and are cited as settled
authority."

Settled authority resting on an unmeasured premise. The item exists but sits under
**Agent skill**, near the bottom, framed as skill polish rather than as the
project's validity question. It also *gates* the other-agent-surfaces decision,
which is the one place several parked calls (hosted surface, no-budget termination)
would have to be revisited.

### 7. Temporal types outrank the profile

Tracked under **The value model (§4)**, queued, with no ranking against
Performance. On this evidence it should be above it: date columns are ubiquitous in
the files §13 exists to read, and a fast engine that cannot read a date column loses
to a slow one that can. The item already carries the sibling engine's finding
(`duration / duration → number`, so the divisor names the unit) — the design input
is in hand, which is not true of the profile's ranked leads.

## The recommended sequence

Not the profile next.

1. **§1 (and §2's remaining four).** Short, and it makes the rest decidable.
2. **The caller's contract** — constraints, exit codes and the stdout discipline,
   merged with the truncation contract's open half. One session, one vocabulary.
3. **Temporal types**, per finding 7.
4. **The profile**, with the pillar-1 question (finding 4) and the row-provenance
   trade (finding 3) attached to it rather than trailing it.

The measurement item (finding 6) is the one that could reorder all four, which is
an argument for doing it early rather than an argument for doing it first.
