"""Ground truth, in plain Python. Control 1: never the datalog engine.

Every answer here rests on one predicate — do two intervals overlap — so it is
written once, in the form that is hard to get wrong (`start < other end` in both
directions), and property-tested against a second formulation.
"""

from __future__ import annotations

from datetime import datetime, timedelta

from harness.domains.scheduling.fixture import (
    ASSIGNMENT_ROWS,
    PEOPLE,
    QUALIFIED_ROWS,
    SHIFT_ROWS,
    UNAVAILABLE_ROWS,
)
from harness.task import Answer

#: A person must have this long between the end of one shift and the start of the
#: next. Short enough that the night-into-morning pairs are the violations and an
#: ordinary evening-then-next-afternoon pair is not.
MIN_REST = timedelta(hours=10)


def overlaps(start: datetime, end: datetime, other_start: datetime, other_end: datetime) -> bool:
    """Half-open intervals: shifts that merely touch do not overlap."""
    return start < other_end and other_start < end


def shift(shift_id: str):
    return next(row for row in SHIFT_ROWS if row[0] == shift_id)


def shifts_of(person: str) -> list[str]:
    return sorted(shift_id for name, shift_id in ASSIGNMENT_ROWS if name == person)


def is_qualified(person: str, role: str) -> bool:
    return (person, role) in QUALIFIED_ROWS


def is_available(person: str, shift_id: str) -> bool:
    return (person, shift_id) not in UNAVAILABLE_ROWS


def candidates(shift_id: str) -> list[str]:
    """Everyone both qualified for the shift's role and available for it."""
    _, role, _, _ = shift(shift_id)
    return [
        person for person in PEOPLE if is_qualified(person, role) and is_available(person, shift_id)
    ]


def double_booked() -> Answer:
    """``(person, shift)`` for every shift that overlaps another of theirs."""
    rows = []
    for person in PEOPLE:
        assigned = shifts_of(person)
        for shift_id in assigned:
            _, _, start, end = shift(shift_id)
            if any(
                other != shift_id and overlaps(start, end, *shift(other)[2:]) for other in assigned
            ):
                rows.append((person, shift_id))
    return Answer.of(*rows)


def unstaffable_shifts() -> Answer:
    return Answer.of(*[name for name, *_ in SHIFT_ROWS if not candidates(name)])


def forced_assignments() -> Answer:
    """Shifts with exactly one possible person — so that person must take it."""
    rows = []
    for name, *_ in SHIFT_ROWS:
        possible = candidates(name)
        if len(possible) == 1:
            rows.append((name, possible[0]))
    return Answer.of(*rows)


def rest_violations(minimum: timedelta = MIN_REST) -> Answer:
    """``(person, shift)`` for a shift starting too soon after another ends.

    Overlapping shifts are excluded: those are a double booking, which is a
    different fault and already has its own question. The row names the shift
    that starts too early, which is the one a manager would move.
    """
    rows = []
    for person in PEOPLE:
        assigned = shifts_of(person)
        for later in assigned:
            _, _, later_start, later_end = shift(later)
            for earlier in assigned:
                if earlier == later:
                    continue
                _, _, earlier_start, earlier_end = shift(earlier)
                if overlaps(later_start, later_end, earlier_start, earlier_end):
                    continue
                if earlier_end <= later_start < earlier_end + minimum:
                    rows.append((person, later))
    return Answer.of(*rows)
