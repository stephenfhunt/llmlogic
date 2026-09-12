# Defects — `code-analysis/`

One file per defect: `NNN-short-slug.md`. **The convention is
[`../../datalog/bugs/README.md`](../../datalog/bugs/README.md)'s** — location is
the status (`bugs/[0-9]*.md` is exactly the open set, resolving one is
`git mv … bugs/resolved/`), the same frontmatter, and a file does not move until
it says how it was resolved. That file is the normative statement; this one only
says what is different here.

Two differences, both from what this project is:

- **`area`** is `extractor` | `lib` | `playbook` | `reference` | `python`,
  rather than the engine's lexer/parser/lower/… — and a defect in the *engine*
  belongs in `../../datalog/bugs/`, not here.
- **`spec:`** points at [`../decisions.md`](../decisions.md) by date, since this
  project has no spec; `notes/code-facts.md` sections are fair game too.

Most of what is filed here was found by running the playbook on a codebase
nobody here wrote, not by a test — see `../notes/code-facts.md` § Dogfooding.
That is the pattern to expect: the fixtures are a few dozen files and cannot
reach a trap that needs a monorepo, a million facts, or a two-year history.
