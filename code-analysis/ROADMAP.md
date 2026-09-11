# code-analysis — roadmap

The item index for `code-analysis/`: one line per item, a status, a pointer.
Status: **queued** · **building** · **parked** · **shipped**. Rationale lives in
`decisions.md`; the session log is `../docs/worklog.md`.

- **The project and the move** — `code-facts` (was `ts-facts`) and its library
  out of the datalog skill. _shipped_ — `decisions.md` 2026-09-11.
- **The playbook** (`skill/SKILL.md`) and `skill/reference/`, TypeScript and
  Python. _shipped._
- **`package.sh`** — the standalone bundle. _shipped._
- **Python frontend** — all layers but dataflow. _shipped_ — P1-py–P4-py,
  `notes/code-facts.md` § The Python frontend.
- **H-CA1** — does the domain skill beat the general one? Pre-registered;
  the pack is next. _designing_ — `../experiments/hypotheses.md` 2026-09-11.
- **Python dataflow layer** (points-to and taint for Python). _parked_ until the
  first Python version has been used.
- **A TypeScript 7 backend**, when its compiler API stabilizes. _parked._
