"""A codebase too large to read, and design questions about its shape.

`static_analysis` hands the subject 21 files of Python and asks what calls what.
This asks the same *kind* of question two orders of magnitude up — VS Code's
`src/vs/base`, 507 files and 161k lines, which no arm can hold in its context.
That is the `at-scale` track's whole claim, and where the engine's advantage
should be if it has one (`../../../hypotheses.md`, 2026-09-11: **H-CA1**).

Three arms, not two: `prose`, `engine` (the datalog skill, with `code-facts` on
PATH and unmentioned), and `code-analysis` (the domain skill, which documents
both). The primary endpoint is `code-analysis` − `engine`, which is what the
playbook and the documented extractor are worth with the engine held fixed.

**The corpus is assembled, not copied** — see `fixture.py`. A TypeScript project
is what its `tsconfig` includes plus everything that resolves from there, so
shipping a directory would not ship the project.

Questions and oracles are `tasks.py` and `truth.py`; until they exist this pack
is installed but **not in `SLATE`**, so no grid draws it.
"""

from __future__ import annotations

from harness.domains.code_design.fixture import unavailable

__all__ = ["unavailable"]
