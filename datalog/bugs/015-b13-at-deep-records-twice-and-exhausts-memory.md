---
id: 015
title: the deep run does not finish on a 30 GB machine — B13 at `Deep` ran past 14.9 GB, and a run without it was still killed for memory
severity: crash
area: engine
spec: ["§11", "§15"]
found: 2026-09-13
resolution:
---

`crash` is the nearest severity: nothing in the engine is wrong, but the test
suite the § Performance gate relies on (`DATALOG_PBT=deep cargo test --lib`)
did not finish.

## Repro

On trunk `c3fdbad`, `DATALOG_PBT=deep cargo test --lib`. Every other test
passed by the 4-minute mark; `b13_pruning_changes_no_live_relation_at_large`
(which runs at `Tier::Deep` under the deep run) was still running, at **14.9 GB
RSS and growing**, and was killed. `testing.md` records a green deep run of the
same commit's properties at 134 s / 1.5 GB, so the draw decides: proptest seeds
each run afresh.

**B13 is not the only one.** Rerun the same day with
`--skip b13_pruning_changes_no_live_relation_at_large` under `ulimit -v 12000000`,
the run was killed for system memory (the harness's low-memory kill, not the
cap's allocation failure) while `b5_body_order_is_irrelevant_at_large` — B5 at
`Deep`, recorded as peaking at 55 MB — was still running past 60 s. Which test
held the memory is not established; `/usr/bin/time` never reported.

## Root cause

`b13_holds` (`src/engine/mod.rs`) evaluates the program twice with
`Provenance::Recorded` — the full run and the pruned one — and then compares
`ProofTree::explain` for every live fact. At `Deep` a recorded run keeps every
instance of a five-atom self-join; `testing.md` § Generator sizes already
records this for B5, whose first deep run was killed the same way and which
now runs `Unrecorded`. B13 cannot simply follow: its proof clause reads the
store.

## Acceptance criteria

The deep run finishes under a memory cap across several seeds. Options, not yet
chosen: the proof clause of B13 only up to `Large` (the fact and answer clauses
unrecorded at `Deep`); or proofs compared for a bounded sample of live facts;
or `Deep` drawn with a smaller self-join ceiling. It is the user's
testing-first ruling this gates, so the choice is theirs.
