# Restructuring `spec.md` §17 — design note

*Recorded 2026-07-26, deferring the work to its own session. Status: **queued**.
This note is the long form of the `ROADMAP.md` item under "Spec hygiene & §6"; the
ROADMAP keeps the one-liner and points here. Nothing below is decided except where
marked — it exists so the next session starts from measurements rather than
re-deriving them.*

## The problem, measured

`spec.md` is 2,448 lines; §17 is **1,180 of them (48.2%)**, and its share is still
climbing:

| | 2026-07-03 | 2026-07-26 | factor |
|---|---|---|---|
| §1–§16 | 265 | 1,268 | 4.8× |
| §17 | 33 | 1,180 | **36×** |
| §17 share | 11% | 48.2% | |

**But volume of decisions is not the cause.** Across 59 entries:

- median entry **8 lines**, mean 16.3
- top 5 entries: **364 lines — 38% of all decision text**
- 9 entries over 30 lines: **521 lines — 54%**
- largest: **127 lines** (the milestone 8–9 correctness review)

So ~50 entries hold ~440 lines, which is a healthy log. Nine entries hold more
than all the others combined. The largest is not a decision at all — it is a
session review containing four distinct decisions under one heading. **§17's
growth is design-session write-ups being composed inside a bulleted list**, not a
project that makes too many decisions.

`ROADMAP.md` has the identical shape: 9 of 39 items hold 299 of 435 lines. Those
are *not* duplication — they are live design documents for unstarted work
(termination, the `as` cast, parenthesized expressions), which is why compressing
them to one line each would lose real work rather than remove redundancy.

## What is already decided

- **Not a move — a rewrite** (2026-07-25). Sized as "cut §17, paste it elsewhere"
  it will be *done* as that, and the residue is the whole problem: §1–§16 are
  written against a reader who has the decision log in the same file, so they
  narrate changes and restate rules the log explains.
- **Acceptance criterion is about §1–§16, not §17**: they must read as the best
  current understanding of the language — no dates, no "formerly", no residue of
  how we got here, every rule stated once with a pointer for the why.
- **Rewriting the log is permitted** (user, 2026-07-26), against §17's standing
  append-only rule, provided nothing is lost and future sessions are not confused.
- **Sizing**: the 2026-07-26 `AGENTS.md` pass is this item at small scale and the
  ratio held — extraction was ~30 lines, deleting the residue was ~97. Measure
  §1–§16, not §17, when estimating.

## The proposal: topic-keyed, not chronological

Today a decision that has been amended three times presents as an original entry
plus markers scattered across hundreds of lines. An agent asking "why is negation
safety this way" must read all of it and correctly order the amendments.

Invert it: **one entry per topic**, stating the current decision, its rationale,
and — folded in — what was rejected or falsified along the way. Chronology is
preserved by git and by `docs/worklog.md`, both of which already carry it.

This is not a concession to the "don't lose anything" constraint; it is *better*
against the "don't confuse agents" constraint, because the current answer and its
failed alternatives arrive together instead of needing reassembly.

**What must survive the rewrite.** §17's preamble argues that a rationale which
turned out wrong is the most useful thing in the log, because it shows where the
reasoning misleads. Topic-keyed entries must therefore keep rejected and falsified
reasoning *as content*, not drop it as obsolete. An entry that records only the
current answer has failed this criterion.

## Sequencing

1. **Caps first** (done 2026-07-26, `docs/rules/editing-docs.md`): ~15 lines per
   decision, ~3 per ROADMAP item, overflow to this directory. Without them any
   cleanup is re-accreted within a month — the intake rule is the only lever that
   bends the curve, and it is now in place, so the restructure is no longer racing
   its own regrowth.
2. **Then the restructure**, in a session with nothing else in it. 59 entries of
   judgement about what is still true is exactly the work where a mistake silently
   misleads later sessions.
3. **`bugs/003`** is the same cleanup for assertions that are outright false, and
   pairs naturally.

## Open questions

- Does §17 stay in `spec.md` or become `decisions.md`? The split is orthogonal to
  the topic-keying and could be either order. Splitting first makes the rewrite
  reviewable in isolation; topic-keying first means moving less text.
- What happens to the 18 open questions currently living in §17 — do they belong
  with the decisions, or in `ROADMAP.md`, which already indexes open work?
- Is per-topic granularity the spec section (§7, §9) or finer? Section-level risks
  recreating today's problem inside each entry.
