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
- **The library at a million facts** — every library under 30 s on VS Code's
  `vs/base` (1.33M facts); `checks.dl` 199.6 s → 9.3 s. _shipped_ —
  `decisions.md` 2026-09-11 (later), `notes/code-facts.md` § At a million facts.
- **`bench/`** — times each library over a fact directory and digests its
  answers. _shipped_ — `npm run bench -- --facts <dir>`.
- **`callreach.dl` whole-project does not fit** a 14k-function call graph;
  `callreach_seeded.dl` is the seeded closure. _shipped_ — same decision.
- **Asset imports** — `import './x.css'` resolves through a wildcard
  `declare module`, so `imports.target_ambient` names the pattern and
  `checks.dl` counts it as a target. _shipped_ — `decisions.md` 2026-09-11
  (later ii); `checks.dl` clean on `vs/base`.
- **A `require`d JSON module is a dangling `target_file`** — `resolveJsonModule`
  resolves `require('../product.json')` on disk, but a CommonJS `require` never
  puts the file in the TypeScript program, so no `file` row is ever emitted and
  `checks.dl` reports the contradiction (2 rows on VS Code's `src/`). Needs a
  schema call: a `json` value for `file.lang` and `file` rows for resolved
  non-program targets, against leaving `target_file` for files the fact base
  covers. _queued_ — `decisions.md` 2026-09-12; found dogfooding, and `vs/base`
  alone does not contain the shape.
- **Python dataflow layer** (points-to and taint for Python). _parked_ until the
  first Python version has been used.
- **A TypeScript 7 backend**, when its compiler API stabilizes. _parked._
