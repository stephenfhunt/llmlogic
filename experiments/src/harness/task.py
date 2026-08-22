"""What a task is, and what counts as an answer.

The answer contract is deliberately **arm-neutral**. The engine arm's natural
output is Datalog facts and the prose arm's is prose, so asking for either would
grade format instead of reasoning and hand one arm the win. Both arms are asked
for the same thing: lines in ``answer.txt``, one item per line, fields separated
by ``|``.

Parsing failure is tracked separately from wrongness (see ``Verdict``) — an agent
that reasoned correctly and formatted badly did not get the question wrong, and
collapsing the two would let a formatting quirk masquerade as a reasoning result.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

#: The question classes S1 names, plus the two the negative controls need.
QuestionClass = Literal[
    "recursion",
    "negation",
    "constraint",
    "aggregation",
    "temporal",
    "single-hop",  # negative control: the engine should not help
    "one-step",  # negative control: arithmetic a model does in its head
]

FIELD_SEPARATOR = "|"


@dataclass(frozen=True)
class Answer:
    """A normalized answer: an unordered set of tuples.

    Order is never significant — the engine returns a relation, and requiring a
    particular row order would penalize the arm that is being faithful to its
    semantics.
    """

    rows: frozenset[tuple[str, ...]]

    @staticmethod
    def of(*rows: tuple[str, ...] | str) -> Answer:
        """Build an answer from literal rows, for a domain's ``truth.py``."""
        normalized = set()
        for row in rows:
            normalized.add((row,) if isinstance(row, str) else tuple(row))
        return Answer(frozenset(normalized))

    @staticmethod
    def parse(text: str) -> Answer | None:
        """Parse ``answer.txt``. Returns ``None`` only if content is there and is
        not parseable.

        **An empty file is an empty answer, not a parse failure.** The prompt
        tells both arms to write an empty file when the answer is empty, and
        "there are none" is the correct answer to a whole question class —
        negation over a closed set. Grading that as malformed would penalize
        exactly the questions S1 is about.

        Tolerates blank lines and ``#`` comments, and strips whitespace around
        fields. Nothing cleverer: every leniency added here is a failure mode the
        run can no longer see.
        """
        rows = set()
        for line in text.splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            fields = tuple(f.strip() for f in line.split(FIELD_SEPARATOR))
            if any(f == "" for f in fields):
                return None
            rows.add(fields)
        return Answer(frozenset(rows))

    def arities(self) -> set[int]:
        return {len(row) for row in self.rows}

    def __len__(self) -> int:
        return len(self.rows)


@dataclass(frozen=True)
class Fixture:
    """The fact base a task is asked about, plus the schemas describing it.

    ``schemas`` is the source the prompt's relation catalogue is derived from
    (control 2): the catalogue is never written by hand, so a renamed field
    cannot silently change what the experiment measures.
    """

    #: filename -> file contents, written verbatim into both arms' workspaces.
    files: dict[str, str]
    #: relation name -> ordered field names, derived from the data, not prose.
    schemas: dict[str, tuple[str, ...]]

    def fact_count(self) -> int:
        return sum(1 for text in self.files.values() for line in text.splitlines() if line.strip())


@dataclass(frozen=True)
class Task:
    """One question over one fixture, with ground truth computed independently."""

    domain: str
    id: str
    question: str
    truth: Answer
    fixture: Fixture
    question_class: QuestionClass
    #: What each answer row means, e.g. ("user", "resource"). Stated to both arms
    #: so neither has to guess the shape it is being graded on.
    answer_shape: tuple[str, ...]
    #: Set on the negative controls. Documents the expectation that the engine
    #: does *not* help here, so a null result reads as designed rather than broken.
    engine_expected_to_help: bool = True
    notes: str = field(default="")

    @property
    def key(self) -> str:
        return f"{self.domain}/{self.id}"
