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
- **`callreach_seeded.dl`** — the call closure grown from a seed you supply.
  _shipped_ — same decision.
- **Asset imports** — `import './x.css'` resolves through a wildcard
  `declare module`, so `imports.target_ambient` names the pattern and
  `checks.dl` counts it as a target. _shipped_ — `decisions.md` 2026-09-11
  (later ii); `checks.dl` clean on `vs/base`.
- **JSON modules are data, and a resolved target is always a `file` row** —
  `json` is a `file.lang`, and a target the program does not include is
  synthesised. _shipped_ — `decisions.md` 2026-09-12; `checks.dl` clean on VS
  Code's whole `src/`.
- **`orient.dl` gates every count on its layer** — no `functions(0)` over a layer
  that was not extracted, and `code_lines` excludes data. _shipped_ — same entry.
- **The import-graph closure lives in `modgraph.dl` and costs only when asked** —
  split into `reach.dl` while the engine evaluated every rule, folded back once it
  stopped; answers byte-identical. _shipped_ — `decisions.md` 2026-09-12 (later
  still).
- **`packages.dl` sees workspace siblings** — a monorepo import resolves to a
  file, not a package name, so `imported_workspace` reads the dependency off
  `file.package`. _shipped_ — same entry; an unnamed `package.json` is no longer
  a package.
- **`lib/keys.dl`'s re-keyings stay until the engine seeks a non-leading column** —
  the one library workaround left for a missing engine feature (`reach.dl`, the
  other, is unwound above). The skill no longer teaches re-keying, and a library
  workaround is engine debt (`decisions.md` 2026-09-12 evening). Re-measure with
  the bench before removing any. _queued_ — `../datalog/ROADMAP.md` § Performance,
  *a bound column that is not leading still scans*.
- **Profile the TypeScript extractor** — 389 s and 16.8 GB for Grafana's frontend
  with `refs,quality,git`; VS Code's `src/` exhausts a 12 GB heap in `dataflow`.
  Attribute by phase before changing anything, as the engine's profile did.
  _queued_ — `notes/code-facts.md` § Dogfooding.
- **`pointsto.dl` does not fit `vs/base`** — profiled as an engine vehicle
  2026-09-12. A 70% cut now finishes evaluating (6.8 GB); the full base does not,
  and `@grafana/ui` runs in 9 s. _open_ — `../datalog/ROADMAP.md` § Performance,
  whose profile note ranks what is left.
- **The decorator layer has never met a real subject** — `decorator` is zero rows
  in every fact base on disk, and `implements` is 2 rows on `@grafana/ui`. A
  decorator-saturated, nominally-typed codebase (NestJS, TypeORM, Angular) is the
  next dogfood subject on shape grounds, not size. _queued_ —
  `notes/code-facts.md` § Subjects, which also lists what each candidate adds.
- **A public-API-surface relation.** "Is this symbol reachable from a published
  entry point" was rebuilt by hand three times in one session and was still not
  certainly complete (`export * from` chains, `export { x as default }`,
  conditional `exports` maps). For a published library that is the primary object
  of study, and `packages.dl` already reads the `package.json` the `exports` map
  lives in. _queued._
- **Python dataflow layer** (points-to and taint for Python). _parked_ until the
  first Python version has been used.
- **A TypeScript 7 backend**, when its compiler API stabilizes. _parked._
