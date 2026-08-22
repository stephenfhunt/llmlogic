"""A real codebase, and the questions you can only answer by traversing it.

Every other pack hands the subject a fact base. This one hands it 4,300 lines of
someone else's Python and asks questions whose facts do not exist yet: the
subject has to extract them first, and *how* it extracts them is the finding.
The recipe this domain carries (`../../../../datalog/skill/recipes/source-analysis.md`)
says the shape is `real parser → fact tables → import → ask`, and that every
false result in the original run came from extracting with a regex instead.

The corpus is `sqlparse`, **fetched and pinned** rather than vendored
(`harness.corpus`), so nothing here is a copy of someone else's licence.

**The questions define their own abstractions, syntactically.** "Called" means
the name appears as the callee of a call expression — not "reachable at runtime".
That is deliberate: real name resolution is ambiguous, the recipe says so at
length, and an oracle that guessed at semantics would void the domain. What is
left is what the domain is for — traversing every file without dropping a case.
"""

from __future__ import annotations

from harness.corpus import SQLPARSE, CorpusMissing


def unavailable() -> str | None:
    try:
        SQLPARSE.require()
    except CorpusMissing as missing:
        return str(missing)
    return None
