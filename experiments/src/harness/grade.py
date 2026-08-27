"""Turning what the subject left behind into a verdict.

Grading is identical for both arms and never consults the datalog engine — the
task's ``truth`` came from the domain's plain-Python ``truth.py`` (control 1).
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path

from harness.task import Answer, Task

ANSWER_FILE = "answer.txt"

#: Why an answer would not parse. Three, because they have three different
#: causes and only one of them is the subject's: `WRONG_ARITY` on a
#: single-column question is the pipe-joining habit the format bullet used to
#: invite (`catalogue._fields_line`), `PROSE` is a sentence where rows belong,
#: and `EMPTY_FIELD` is a line with a missing value.
WRONG_ARITY = "wrong-arity"
PROSE = "prose-in-answer"
EMPTY_FIELD = "empty-field"


class Verdict(StrEnum):
    CORRECT = "correct"
    WRONG = "wrong"
    #: An answer file exists but nothing parseable is in it. Distinct from WRONG
    #: on purpose: a subject that reasoned correctly and formatted badly did not
    #: get the question wrong, and merging the two lets a formatting quirk pass
    #: for a reasoning result.
    UNPARSEABLE = "unparseable"
    NO_ANSWER = "no-answer"
    #: The cell itself failed — an API error, a timeout, a crashed subject.
    ERROR = "error"


@dataclass(frozen=True)
class Grade:
    verdict: Verdict
    answer: Answer | None
    raw: str | None
    #: Populated on WRONG. The asymmetry matters: a subject that returned a
    #: *subset* of the truth silently dropped rows (the failure mode that survives
    #: rounds of feedback because nothing in the output looks wrong), while one
    #: that returned a superset over-derived.
    missing: int = 0
    extra: int = 0
    #: Populated on UNPARSEABLE: *which* of the three ways it failed to parse.
    #: One bucket could not tell a prose sentence from a single-column answer
    #: joined with `|`, and the second is a prompt defect while the first is the
    #: subject ignoring the format — `catalogue._fields_line` was found this way.
    reason: str | None = None

    @property
    def correct(self) -> bool:
        return self.verdict is Verdict.CORRECT

    @property
    def silently_wrong(self) -> bool:
        """Wrong, well-formed, and plausible — nothing in it announces the error.

        This is the shape S1 exists to catch: `dead(F)` returning six plainly-used
        functions read exactly like `dead(F)` returning nothing.
        """
        return self.verdict is Verdict.WRONG and self.answer is not None


def grade(task: Task, workspace: Path) -> Grade:
    path = workspace / ANSWER_FILE
    if not path.exists():
        return Grade(Verdict.NO_ANSWER, None, None)

    raw = path.read_text(encoding="utf-8", errors="replace")
    answer = Answer.parse(raw)
    if answer is None:
        return Grade(Verdict.UNPARSEABLE, None, raw, reason=EMPTY_FIELD)

    # Format noise is not a reasoning failure, and separating the two is not
    # cosmetic: the prose arm writes sentences more often than the engine arm, so
    # counting a sentence as a wrong answer biases the headline toward the engine
    # — the one direction of bias this harness cannot afford.
    expected_arity = len(task.answer_shape)
    if answer.rows and answer.arities() != {expected_arity}:
        return Grade(Verdict.UNPARSEABLE, None, raw, reason=WRONG_ARITY)

    # Arity alone cannot catch a one-column answer: "o2" and "The orders over 100
    # are o2, o4 and o5." are both single fields. Fall back to the shape of the
    # truth — if no true value contains whitespace, a submitted one that does is
    # prose, not an answer. The guard disables itself on any domain whose answers
    # legitimately contain spaces, which is the behaviour we want.
    truth_has_spaces = any(any(" " in field for field in row) for row in task.truth.rows)
    if not truth_has_spaces and task.truth.rows:
        if any(any(" " in field for field in row) for row in answer.rows):
            return Grade(Verdict.UNPARSEABLE, None, raw, reason=PROSE)

    if answer.rows == task.truth.rows:
        return Grade(Verdict.CORRECT, answer, raw)

    return Grade(
        Verdict.WRONG,
        answer,
        raw,
        missing=len(task.truth.rows - answer.rows),
        extra=len(answer.rows - task.truth.rows),
    )
