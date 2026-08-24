# This run was measured across three session windows, not one sitting

The first full 112-cell grid, and complete: 112 cells, 0 errored. It stopped
twice on the account's five-hour session window — at **cell 67**, then again 36
cells into the resume — and was finished by `--resume` each time, into the same
run id, because they are one grid.

**Three things to know before reading `report.md`.**

**1. Its first rendering was fiction, and it is worth knowing what that looked
like.** `AgentSubject` catches an SDK failure into `transcript.error` and returns
normally, so the runner of the day graded the 46 empty workspaces the session
limit left behind and filed each as `no-answer` — a verdict that counts against
the arm. That rendering said:

- `112 cells · 9.22 USD · **0 errored**`, when 46 had errored;
- `controls 0/8` and `static_analysis 0/8` in **both** arms — two whole domains,
  one of them the negative controls, reading as total failure;
- **Delta: +2 points**, computed over denominators padded with cells that never
  ran.

Nothing in it announced the problem. The tell was that the negative controls —
single-hop lookups — read 0%, which is not a result any model produces.

**2. Nothing here was rewritten.** `records.jsonl` holds every attempt in the
order it happened, the 46 dead ones included. What changed is how a record is
*read*: a record carrying an `error` is failed whatever its verdict says
(`resume.failed`), so it leaves every denominator and is owed again by a resume.
The 46 are still on file as the evidence for why the run stopped, and the report
counts each cell once, at its later attempt.

**3. `scheduling`'s 3/8 in both arms is not evidence about the models.** Two of
its four questions were ambiguous, and the subjects are what found them. The
roster assigned people to shifts they were not qualified or available for — 21 of
29 assignments — while every question's preamble stated that rule, so a reader
who applied it to `assignment.csv` before looking for clashes got the empty set.
Three of the four `double-booked` cells answered with an empty file, correctly.
`forced-assignments` never said whether someone already on an overlapping shift
still counts; all four cells read it the other way from the oracle.

The domain was repaired the same day, but **not in this run**: changing a task
changes the slate, so those cells are void and stay void here. Read the S1 tables
with `scheduling` struck out — **engine 38/40, prose 38/40** — and re-measure the
domain under a new run id.

Fixes and rationale in `../../decisions.md` 2026-08-24 — the error verdict,
halt-on-fatal, and `--resume`. The instrument defect is the third this project
has found by running the instrument, and the same species as the first two: **the
run went on producing plausible numbers after it stopped measuring anything.**
