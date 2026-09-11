# code-analysis — roadmap

The item index for `code-analysis/`: one line per item, a status, a pointer.
Status: **queued** · **building** · **parked** · **shipped**. Rationale lives in
`decisions.md`; the session log is `../docs/worklog.md`.

- **The project and the move** — `code-facts` (was `ts-facts`) and its library
  out of the datalog skill. _building_ — `decisions.md` 2026-09-11.
- **The playbook** (`skill/SKILL.md`) and `skill/reference/`. _queued._
- **`package.sh`** — the standalone bundle. _queued._
- **Python frontend** — structure and refs, then flow and quality; validated
  against the experiments' `static_analysis` answer key. _queued._
- **H-CA1** — does the domain skill beat the general one? Pre-registration, then
  an experiments pack. _queued_ — `../experiments/hypotheses.md`.
- **Python dataflow layer** (points-to and taint for Python). _parked_ until the
  first Python version has been used.
- **A TypeScript 7 backend**, when its compiler API stabilizes. _parked._
