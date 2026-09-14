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

## 2026-09-13 — progress; still open, and why

- **The first choice was built and withdrawn the same session.** Proofs only up
  to `Large`, with B13 unrecorded at `Deep`: this dropped proof coverage at
  `Deep`, and its justification, E9, never runs at `Deep`.
- **B13 now compares proofs one step per live fact** (`ProofTree::step`). By
  induction on first round this is exactly as strong as comparing trees, it is
  cheaper, and only one recorded store is alive at a time. *Mutation*: the pruned
  run restarts round stamps each stratum → red at every tier.
- **It is not enough, and the root cause above is confirmed and sharpened.**
  Recorded at `Deep` alone, three runs gave 34 s / 664 MB, then 245 s / 8.2 GB,
  then a kill at 19.5 GB. The store keeps every rule instance. On one fixed
  sample, idx 129 stores 8.2 M derivations for 2,257 derived facts: 93–99% are
  later-round rediscoveries no proof reads, at ~800 bytes each
  (`notes/recorder-at-scale.md`).
- **"Which test held the memory" is answered.** Unrecorded at `Deep`, B5 alone is
  26 s / 22 MB and B13 alone 39 s / 26 MB; recorded, B13 is the one.
- **Not taken, by the user's ruling:** a derivation budget or any size cap on the
  test. The recorder is the defect. This file closes with the recorder design
  session (ROADMAP § Provenance surface).
- **Correction to this session's measurements:** `systemd-run --user -p
  MemoryMax` is not enforced on this machine, whose user cgroups delegate only
  `pids`. `prlimit --as` is.

## 2026-09-13 (later) — the recorder cut by constants; still open

- **Two cuts that keep every derivation** (`91b730a`, `ef82398`;
  `notes/recorder-at-scale.md` § After the first two cuts):
  - idx 129 now records at 5.43 GB peak live, down from 6.96 GB;
  - idx 119 at 2.78 GB, down from 3.58 GB.
- **The deep run on `ef82398` still does not finish.**
  - Under `prlimit --as=20000000000` it failed an allocation at 372 s, with
    18.1 GB peak RSS.
  - B1, B5 and B13 at their deep tiers and `d6_printing_is_the_eager_rendering_at_large`
    were each past 60 s. Which test held the memory is not established.
  - A smaller constant does not close this file.
- **Seen once at the per-commit tiers.** A plain `cargo test` of the tree that
  became `ef82398` held one `engine::tests` property at 16.4 GB RSS for
  10 minutes, until it was stopped.
  - It had not failed, so no seed was saved.
  - Two reruns passed: 52 s / 78 MB, and 23 s / 153 MB.
  - Which test is not known. If it recurs, the per-commit suite has the same
    exposure as the deep run.
- **Ruled again after the cuts** (the user's): no cap on the `Deep` generator
  either. This file waits on the recorder design, where fact references come
  first.

## 2026-09-13 (later iii) — shared tuples reverted; still open

- **Fact references built as shared tuples** (`5035215`) cut a `?why`'s memory by
  43–50%, made runs up to 50% slower, and were reverted (`efcda71`). The deep run
  was not re-measured on them.
- **This file waits on the fact store with one owner** (`notes/fact-store.md`).
  Its premises are row references, which removes the cloned premise tuples a
  recorded `Deep` run spends most of its store on.

## 2026-09-14 — seen again at the per-commit tier

- **Where:** branch `fact-store`, after imports became flat blocks. One uncapped
  `cargo test --lib` held 19.8 GB RSS at 10 minutes and was killed. The test was
  not named.
- **Ten reruns** under a 4 GB `prlimit --as` cap all passed, in 13–30 s. One took
  108 s, with `b13_pruning_changes_no_live_relation_at_large` running past 60 s:
  the recorded B13 this file is about, at its per-commit tier.
- **So a per-commit `cargo test` can draw a case that exhausts memory,** not only
  the deep run. This agrees with the 16 GB per-commit draw recorded earlier. That
  the runaway was B13 is likely, not established.
- **Not implicated:** the branch's import change, since generated programs contain
  no imports. The flat store beneath it is not ruled out.
- **Under an address-space cap,** a runaway is a named allocation failure with a
  saved seed, not a killed process.

## 2026-09-14 (later) — the runaway is B13, on the fact store

- **Where:** branch `fact-store` at `7ecfc33`, deep tier, seed 2, nothing skipped,
  under a 16 GB `prlimit --as` cap.
- **It failed an allocation at 385 s, at 14.2 GB peak RSS.** 522 of 523 tests had
  passed, B5 at `Large` among them. The one still running was
  `b13_pruning_changes_no_live_relation_at_large`. So the test holding the memory is
  B13, established where the entries above could say only likely.
- **The fact store did not close this file.** With B13 skipped, the gate's seeds 2
  and 3 peak at 442 MB (from about 980 MB), but this draw still exhausts 16 GB.
- **Left open, as ruled:** chasing it is not the refactor's point.
