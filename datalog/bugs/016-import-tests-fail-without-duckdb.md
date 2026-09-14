---
id: 016
title: three import tests fail under `--no-default-features` — they need DuckDB and are not gated on it
severity: crash
area: import
spec: ["§13", "§15"]
found: 2026-09-14
resolution:
---

`crash` is the nearest severity. Nothing here shows the engine wrong, but the
`--no-default-features` test lane panics, and ROADMAP records it green since
2026-08-24. Found while gating branch `fact-store`, where `efcda71` fails the same
way.

## Repro

`cargo test --no-default-features --lib` at trunk `e2de06c`: 510 passed, 3
failed. With default features all three pass.

- `api::tests::b13_import_generator_skips_and_reads_a_relation_that_matters` —
  `run_pruning(…).expect("full run")` (`src/api.rs:2172`).
- `api::tests::b13_pruned_loading_changes_no_answer` — `full.expect("full run")`
  (`src/api.rs:2204`).
- `sources::tests::a_skipped_import_is_checked_but_not_read` —
  `load_imports_where(…, &|_| false).expect("not read")`
  (`src/sources/mod.rs:251`): `UnsupportedFormat … this build has no import
  support — imports need the duckdb feature`.

## Root cause

All three arrived in `694a6ff` (2026-09-12, reading only the imports a goal
reaches), after the lane was made green. None carries `#[cfg(feature =
"duckdb")]`, which every other import-dependent test does: `tests/imports.rs`
whole, and the `system.rs` and `pipeline.rs` tests one by one.

- **The two B13 tests** read an import on their full run, which a build without
  DuckDB refuses. They need the gate and nothing else.
- **`a_skipped_import_is_checked_but_not_read` gates only its read half**
  (`src/sources/mod.rs:261`). Its first assertion expects a skipped import to be
  accepted, but a build without DuckDB refuses it with the feature error before
  the skip is consulted. By the test's own sentence ("refused for everything a
  read would refuse before its first row") that refusal is consistent, since a
  read refuses the same way. So it is not settled whether the assertion or the
  behaviour is wrong.

## Acceptance criteria

`cargo test --no-default-features` is green in every test binary, and the
skipped-import choice is recorded here. Candidates:

1. **Gate all three tests,** the skipped half of the third included. No behaviour
   changes.
2. **Accept a skipped import in a build without DuckDB,** since it is never read,
   and pin that with a `#[cfg(not(feature = "duckdb"))]` test. This is a §13
   behaviour decision, not a test fix.

No property can catch a missing feature gate; the guard is running the lane.
`testing.md` Phase F names it, and it was not run after `694a6ff`.

## Fallout

- ROADMAP's "`--no-default-features` builds and tests green" item, ✅ 2026-08-24,
  has not held since `694a6ff`.
