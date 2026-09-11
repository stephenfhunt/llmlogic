# code-analysis — roadmap

The item index for `code-analysis/`: one line per item, a status, a pointer.
Status: **queued** · **building** · **parked** · **shipped**. Rationale lives in
`decisions.md`; the session log is `../docs/worklog.md`.

- **The project and the move** — `code-facts` (was `ts-facts`) and its library
  out of the datalog skill. _shipped_ — `decisions.md` 2026-09-11.
- **The playbook** (`skill/SKILL.md`) and `skill/reference/`. _shipped_; Python
  still to be added to both.
- **`package.sh`** — the standalone bundle. _shipped._
- **Python frontend** — structure and refs _shipped_ (P2-py, P4-py; sqlparse
  matches `static_analysis`'s `truth.py`); flow and quality _building_.
- **H-CA1** — does the domain skill beat the general one? Pre-registration, then
  an experiments pack. _queued_ — `../experiments/hypotheses.md`.
- **Python dataflow layer** (points-to and taint for Python). _parked_ until the
  first Python version has been used.
- **A TypeScript 7 backend**, when its compiler API stabilizes. _parked._
