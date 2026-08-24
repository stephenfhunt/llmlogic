# This run is void — kept as the evidence for why

The first paid `access_control` pilot. Two of its sixteen cells were graded on an
`answer.txt` left in the workspace by an earlier `--dry-run`: a cell directory is
a hash of the cell id, `arms.build` created it with `exist_ok=True`, and the stub
subject's answer was still sitting there when the real subject arrived.

- `delete-without-read.engine.haiku-4.5` — scored **wrong** on the stub's
  deliberately truncated truth (`missing=1`). Its transcript shows the subject
  never ran a program: it invoked the `Skill` tool, then read `answer.txt`.
- `who-can-read-r03.engine.haiku-4.5` — scored **correct** without doing the work.

Both are engine-arm cells, so the contamination moved the S1 delta in both
directions at once. **Do not read the numbers in `report.md`.**

Re-run clean after the fix as `run-20260824T013829Z`: 16/16, no wrong verdict.
Fix and rationale in `../../decisions.md` 2026-08-23; `results/` keeps this run
because a run that was made is a run that was made, and because it is the only
record of how the failure looked from the inside.
