# The reference corpus

Datalog programs whose output is **pinned byte-exact**, in two halves.

- **`correct/`** — one program per domain, answering that domain's four
  questions. These are the programs actually verified row-for-row against each
  pack's `truth.py` when the pack was built, kept rather than thrown away.
- **`malformed/`** — programs that are wrong in a named way, pinned to the
  diagnostic the engine prints. A corpus of correct programs cannot say what the
  tool does when a run goes wrong, and that is most of what a subject reads while
  it is still getting its program right.

## What it is for

The harness measures an agent against an engine that moves under it. A grid run
in August and one in October are the same measurement only if the engine answered
the same way in between, and nothing else in `experiments/` checks that.

So this is a tripwire, not a test suite for the engine — `datalog/tests/` is that.
**A pin that moves is not automatically a regression.** It is a change in the
instrument: look at it, decide what it means for the runs on either side of it,
and re-pin deliberately.

```sh
harness reference           # run every entry against its pin; non-zero if any moved
harness reference --repin   # adopt what it printed — after reading the diff
```

`tests/test_reference_corpus.py` runs the same entries and additionally checks,
task by task, that each relation still agrees with the domain's plain-Python
oracle. That second check is what makes this a *reference* rather than a
snapshot: a program can be pinned and wrong, and a pin over a wrong program
defends the error.

## Layout

```
correct/<domain>.dl              the program, with a `?-` query per task
correct/<domain>.out             pinned stdout
correct/<domain>.err             pinned stderr (usually empty)
correct/<domain>.extract.py      only where the fixture ships no fact tables
malformed/<defect>.dl            the program
malformed/<defect>.err           pinned diagnostic
```

A correct entry is named for its domain and runs against that pack's fixture,
materialized the same way `arms.build` materializes it for a real cell — so the
program runs against the bytes a subject would have been given. A malformed entry
carries its own facts and needs no fixture.

`tests/test_reference_corpus.py` holds the `ANSWERS` map from task id to the
relation that answers it. It is written out rather than derived, so renaming a
relation cannot silently uncheck a task.

## Two things worth knowing

**`static_analysis` is the only entry with an extractor.** Its fixture is a source
tree with no schemas, because inventing the fact table is the task — so the
program cannot be pinned without pinning the extraction it assumes.
`static_analysis.extract.py` emits `call` with two columns and no line number,
and that is load-bearing: a relation is a set, so with a line column every call
site becomes a distinct fact and `count { M | call(name: F, module: M) }` counts
call sites instead of modules. Same rule text, **13 answers instead of 7**, exit
0, no warning. See `datalog/ROADMAP.md`, *Count-distinct, and the invisible
wildcard*.

**`malformed/type-clash` pins a diagnostic that is partly false.** Its second line
claims `item.weight` holds int values; it holds `1.5`. That is
`datalog/bugs/008`, and the pin is deliberate — when 008 is fixed this file's
`.err` changes and the test is what says so.
