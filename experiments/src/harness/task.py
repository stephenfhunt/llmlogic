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

#: Which question a task is evidence for (``decisions.md`` 2026-08-25).
#:
#: ``in-context`` holds ``cell.FIXTURE_TOKEN_BUDGET``: every arm can see the whole
#: fact base, so a delta is a claim about **reasoning**. ``at-scale`` exceeds the
#: prose arm's window on purpose, so a delta there is a claim about **scale**.
#: They are reported in separate tables and never averaged — saying which claim
#: is being made is what keeps the second from reading as rigged.
Track = Literal["in-context", "at-scale"]

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

#: Formats a relation can arrive in — the §13 import formats the engine reads.
#: A file with one of these suffixes is *data*, so it must have a declared
#: schema; anything else in a fixture is an asset (a source tree, say).
DATA_SUFFIXES = (".csv", ".jsonl", ".parquet")


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

    A fixture carries two kinds of file. **Relations** are the declared schemas,
    each backed by one or more data files — ``<relation>.csv`` by default, and
    whatever ``sources`` names otherwise. **Assets** are everything else: a source
    tree the subject extracts its own facts from has no schema, because inventing
    one is the task.
    """

    #: filename -> contents, written verbatim into both arms' workspaces. ``str``
    #: is written as text, ``bytes`` verbatim (Parquet). Keys may be nested paths.
    files: dict[str, str | bytes]
    #: relation name -> ordered field names, derived from the data, not prose.
    schemas: dict[str, tuple[str, ...]]
    #: relation name -> every filename that carries it, for relations not stored
    #: as a single ``<relation>.csv``. More than one spelling is allowed and is
    #: how a table ships as text *and* as Parquet: the extra copy is redundant on
    #: purpose, so no cell is decided by which formats an arm can open.
    sources: dict[str, tuple[str, ...]] = field(default_factory=dict)

    def spellings(self, relation: str) -> tuple[str, ...]:
        return self.sources.get(relation, (f"{relation}.csv",))

    def source(self, relation: str) -> str:
        """The canonical file for a relation — the one row counts come from."""
        return self.spellings(relation)[0]

    def declared_sources(self) -> set[str]:
        return {name for relation in self.schemas for name in self.spellings(relation)}

    def assets(self) -> list[str]:
        """Files that are not a relation — the source tree, if there is one."""
        declared = self.declared_sources()
        return sorted(
            name for name in self.files if name not in declared and not name.endswith(DATA_SUFFIXES)
        )

    def text(self, filename: str) -> str:
        """The contents of a text file. Raises on a binary one, which is the
        point: nothing should be quietly decoding Parquet as UTF-8."""
        contents = self.files[filename]
        if isinstance(contents, bytes):
            raise TypeError(f"{filename!r} is binary")
        return contents

    def fact_count(self) -> int:
        """Rows across the declared relations, headers excluded.

        Assets are not facts — a source tree is what the subject extracts facts
        *from*, and counting its lines here would make an extraction domain look
        like the largest fact base in the slate.
        """
        total = 0
        for relation in self.schemas:
            filename = self.source(relation)
            if filename.endswith(".parquet"):
                continue
            rows = len([ln for ln in self.text(filename).splitlines() if ln.strip()])
            total += rows - 1 if filename.endswith(".csv") else rows
        return total


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
    #: Which of the two tracks this task belongs to. Defaults to the one the
    #: existing slate is in, so the 28 pinned tasks keep their meaning untouched.
    track: Track = "in-context"
    #: The generator setting that produced it, or 0 for a hand-authored task.
    #: Recorded so a calibrated slate can say *what* it selected, not just which.
    difficulty: int = 0
    notes: str = field(default="")

    @property
    def key(self) -> str:
        return f"{self.domain}/{self.id}"
