# Worklog

A running handoff log for chaining agentic coding sessions. Each session ends by
adding an entry so the next session can get oriented in seconds — without re-reading
raw transcripts (Claude Code auto-saves those under
`~/.claude/projects/<repo-slug>/*.jsonl`; resume with `claude --resume`).

**Conventions**
- Newest entry on top (reverse-chronological).
- Keep each entry short and high-signal. Four fields:
  - **Done** — what changed this session (link commits/PRs where useful).
  - **Decided** — key decisions made (design decisions also go in `datalog/spec.md`
    §17; note them here too so the timeline is complete).
  - **Removed** — what you deleted, merged, or replaced.
  - **Next up** — the concrete next threads, so the following session starts oriented.
- This is a curated summary, not a transcript. Don't paste raw output here.
- **Entries stay under ~50 lines**, and **this file keeps the most recent three.**
  Older entries rotate verbatim into [`worklog-archive/`](worklog-archive/) by
  month, so session-start orientation stays a fixed cost instead of a growing one.
  A session needing more room than that is describing work that wants its own
  document — put the long form in `datalog/notes/` and link it from the entry.
  (50 rather than 40 because the entry that set this rule landed at 48, and a cap
  nobody meets gets ignored — cf. the `Stable` rung, deleted for the same reason.)

---

## 2026-09-14 (evening) — the datalog skill read as a stranger's agent would

Asked for a fresh read of the datalog skill as published, by a user and agent who
know nothing of this repo, on a general agent harness. Planned; the user chose to
drop INSTALL.md's source pointer, reorder `SKILL.md`, and accept `--help`.

**Done**
- `253fd2d` **`SKILL.md`**:
  - run by path (`<skill>/datalog`); `./datalog` needed the skill's directory as
    the working directory;
  - named arguments and `declare` (its own `-q` example failed without a schema);
    comments, numeric types, `;` in queries, and import paths for scratch programs;
  - the worked example before the sections that read its file; the gating
    sentence says what it means.
- `39cd64d` **The recipe** defines every table and relation it uses. Each rule
  was run verbatim over a JSONL fixture, which caught two errors in the rewrite:
  without `line`, `calls` merges call sites (the count trap read 2 = 2), and
  dead code listed tests.
- `9fed6c5`: `--help` / `-h` print the usage, which now names `?why` and the exit
  codes, and exit 0.
- `92c4638` **INSTALL.md**: install for any harness, no pointer to an unreachable source.
  The bundle was built and run from outside the repo.
- `bf74af4` **experiments**: an engine-arm skill copy carries the binary; the
  ablation test follows the recipe block's text.
- Most of the fixes already existed in code-analysis's fork of these texts.

**Decided**
- §17 2026-09-14 (evening); experiments `decisions.md` 2026-09-14 (evening).
- ***Consequences*** on code-analysis 2026-09-13 (night ii): dual maintenance's
  cost is silent; the fork's fixes never flowed back.

**Removed** — the recipe's disjunction trap (now in `SKILL.md`), its anchored-join
cost note, format claim and run counts; INSTALL.md's source pointer; `./datalog`;
the oldest worklog entry.

**Next up**
- **The other direction:** code-analysis's `reference/datalog.md` lacks what this
  session added (comments, `declare`, scratch import paths, the gating sentence,
  `--help`).
- An ablation compared across this session ran on other surrounding text; rerun
  its control.
- Push `trunk` when the user says so.
- **Measure the code-analysis playbook** with a fresh agent and only the bundle;
  that bundle still ships `tools/code-facts/src`, whose comments name subjects.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-14 (late afternoon) — the fact store merged; the datalog skill's texts written for a stranger

Asked to merge `fact-store` to trunk, then to clean up the datalog skill's leaks,
the code-analysis skill's sweep applied to this one.

**Done**
- **Merged.** The random-seed deep run passed 525/525; `trunk` fast-forwarded to
  `11231b2`, the branch deleted. Not pushed.
- **The sweep** (§17 2026-09-14 (afternoon)):
  - `SKILL.md`'s *More* linked three documents the bundle lacks; it now points at
    `examples/`.
  - The source-analysis recipe narrated a dated run on this engine's own source:
    module names, timings, counts, "the crate above", and a numeric-id tip left
    from strings having no order. Rewritten as cases; its four traps re-checked
    against the binary.
  - A section citation in `examples/aggregation.dl`, and in nine CLI messages.
  - `experiments`' `static_analysis` docstring, which paraphrased the recipe.
- **Guard:** `tests/published_text.rs` scans the shipped skill files and every
  source line the binary can print from. Each check has a caught sample and
  lookalikes; a `§4` put back in a parser message, and a dated line in the
  recipe, each turn it red.

**Decided**
- The user's: the datalog skill's texts are published for a stranger, as
  code-analysis's are. The binary's messages follow the same rule.
- `verify-before-you-believe` keeps its guidance with its narrative rewritten, so
  ablations of it before today ran on other words.

**Removed** — the recipe's run narrative and numbers; `SKILL.md`'s out-of-bundle
links; ten section citations; the oldest worklog entry.

**Next up**
- Push `trunk` when the user says so (42+ commits ahead of `github/trunk`).
- **Measure the code-analysis playbook** with a fresh agent and only the bundle.
- The code-analysis bundle still ships `tools/code-facts/src`, whose comments
  name subjects and bugs.
- **Open**: `datalog/bugs/015`, `016`.

## 2026-09-14 (afternoon) — fact store step 4: values interned; faster than the baseline everywhere

Asked, in the interning review, for both kinds interned and interning alone,
measured before anything more is decided.

**Done** — branch `fact-store`; per-commit gate green; harness diff empty
(1,109 cases); deep seeds 2 and 3 pass 525/525 at 351 MB.
- `60a5495`: `Value::Symbol` and `String` hold a `Sym`, interned once for the
  process; a `Value` is 24 bytes; `Value::symbol`/`string` constructors.
  - **A17**, interned equality is content equality, mutation-verified. Ordering by
    pointer escapes every B-series differential; only A17 and hand tests catch it.
- **Measured against `efcda71`**, one sitting, median of 3:
  - `pointsto.dl` 10.6 → 7.4 s; its `?why` 9.1 → 6.0 s, 815 → 410 MB;
  - `callreach.dl` `?why` 1.46 → 0.60 s; `sparse_800` 1.22 → 0.41 s;
  - `q_coh.dl` 6.4 → 4.1 s, `lib/cohesion.dl` 7.8 → 4.8 s, `lib/flow.dl`
    23.2 → 12.6 s.
- **`bugs/015`:** seed 2 unskipped on `7ecfc33` failed an allocation at 14.2 GB,
  with only B13 at `Deep` still running. Which test holds the memory is now
  established.

**Decided**
- The user's, in review: accept the interner's process-lifetime leak; intern
  symbols and strings both; interning alone, measured first.
- §17 2026-07-19's "interning deferred" is marked ***Superseded***.
- The user's: merge to trunk now. The cross-case copy and any further seek work
  are trunk work.

**Removed** — `Value`'s owned `String`s; `coerce_borrowed`, folded into `coerce`;
the oldest worklog entry (rotated).

**Next up**
- **Merged.** The random-seed deep run passed 525/525 (1,327 s, 1.1 GB); `trunk`
  fast-forwarded to `fact-store`, and the branch deleted.
- **The datalog skill's leaks** (the user's next): `SKILL.md`'s links out of the
  bundle, the source-analysis recipe's dated narrative about this engine's own
  code, and `§` citations in an example and nine CLI messages.
- **Open**: `datalog/bugs/015`, `016`.
