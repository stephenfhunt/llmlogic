---
# Broad on purpose. This rule is repo-wide and must fire for any project's
# documents, so it cannot enumerate one project's filenames — which project has
# a `spec.md` is not something a repo-level file should claim to know.
paths:
  - "**/*.md"
  - "**/*.rs"
---

# Changing what already exists

Most of the damage so far has come from editing, not building — three of the four
2026-07-25 defects were caused by a doc claim that had quietly stopped being true.

**Know which kind of document you are in.** They have opposite disciplines:

| | current-state | append-only record |
|---|---|---|
| discipline | **rewrite** it to state present truth | **append**; never rewrite |
| history | *point* to the decision; never narrate the change | history is the payload |
| examples | a spec's body, `README`, `SKILL.md`, code and doc comments | a decisions log, `docs/worklog.md`, resolved-defect notes |

**Which of a project's documents are which is stated in that project's
`AGENTS.md`**, under its document map — this file governs the *discipline*, not
the inventory.

The test for any sentence in a current-state document: *would this still be here
if the feature had always worked this way?* If not, it is narration — cut it and
leave the pointer. "See §17 2026-07-25" is fine; "relaxed from positively bound"
is not.

- **One normative home per rule.** State a rule in exactly one section; everywhere
  else cross-references it. `datalog/bugs/003` names this as *the drift mechanism*
  — the same safety rule lived in four sections, and updating three looked done.
- **Changing a rule means sweeping the decisions log** for entries resting on it.
  Fixing `datalog/bugs/001` turned up three needing amendment; that was diligence,
  not process.
- **Annotate a decision when its consequences land, not only when it is
  overturned.** The most valuable note has no change attached — that the
  2026-07-19 wildcard entry's instinct was right and the entry overturning it was
  wrong is something no diff can recover. Record what it actually cost, whether
  the stated rationale held, and what the *rejected* alternative would have done;
  the last is the part a later session cannot reconstruct.

## Length caps, and where the overflow goes

The append-only records grow without bound by construction, and an agent is told
to read them to orient. Measured 2026-07-26: session-start reading was **2,238
lines**, of which the always-loaded `AGENTS.md` — just trimmed in that same
session — was 6%. In every one of these documents a handful of oversized items
held most of the text: 9 of 59 `datalog` decision entries held 54% of them, 9 of
39 `ROADMAP.md` items held 69%, and the worklog's newest entry was its longest
ever.

So each record caps the *item*, and long-form goes to a file of its own:

| record | unit | cap | overflow target |
|---|---|---|---|
| `docs/worklog.md` | entry | ~50 lines, 3 entries live | `docs/worklog-archive/YYYY-MM.md` |
| a project's `ROADMAP.md` | item | ~3 lines: what, status, pointer | that project's `notes/` |
| a project's decisions log | decision | ~15 lines | that project's `notes/` |

**A project's `notes/` is its designated overflow.** The pattern is already proven
in `datalog/`: the 2026-07-20 semiring decision is nine lines in §17 pointing at
`notes/semiring-provenance.md` (~185 lines). Had the five largest §17 entries done
the same, §17 would be ~350 lines lighter with nothing lost.

Repo-wide long-form — anything not owned by one project, like this rule's own
rationale — goes in `docs/`.

A cap is not a request to write less. It is the trigger to ask **which document
this belongs in** — an entry straining its cap is usually a design write-up that
wants its own file, not a decision that needs more room. Splitting it costs one
file and a link; not splitting it costs every future session that reads past it.

Note the asymmetry with the rest of this file: the caps are the *only* rule here
that bends the growth curve. Cleanup without them is re-accreted within a month.
