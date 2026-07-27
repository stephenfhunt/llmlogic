---
paths:
  - "datalog/spec.md"
  - "datalog/README.md"
  - "datalog/ROADMAP.md"
  - "datalog/testing.md"
  - "datalog/skill/SKILL.md"
  - "datalog/bugs/**/*.md"
  - "docs/worklog.md"
  - "datalog/src/**/*.rs"
---

# Changing what already exists

Most of the damage so far has come from editing, not building — three of the four
2026-07-25 defects were caused by a doc claim that had quietly stopped being true.

**Know which kind of document you are in.** They have opposite disciplines:

| | current-state | append-only record |
|---|---|---|
| what | `spec.md` §1–§16, `README.md`, `SKILL.md`, code and doc comments | `spec.md` §17, `docs/worklog.md`, `bugs/resolved/` |
| discipline | **rewrite** it to state present truth | **append**; never rewrite |
| history | *point* to the decision; never narrate the change | history is the payload |

The test for any sentence in a current-state document: *would this still be here
if the feature had always worked this way?* If not, it is narration — cut it and
leave the pointer. "See §17 2026-07-25" is fine; "relaxed from positively bound"
is not.

- **One normative home per rule.** State a rule in exactly one section; everywhere
  else cross-references it. `bugs/003` names this as *the drift mechanism* — the
  same safety rule lived in four sections, and updating three looked like done.
- **Changing a rule means sweeping §17** for entries resting on it. Fixing
  `bugs/001` turned up three needing amendment; that was diligence, not process.
- **Annotate a decision when its consequences land, not only when it is
  overturned.** The most valuable note has no change attached — that the
  2026-07-19 wildcard entry's instinct was right and the entry overturning it was
  wrong is something no diff can recover. Record what it actually cost, whether
  the stated rationale held, and what the *rejected* alternative would have done;
  the last is the part a later session cannot reconstruct.
