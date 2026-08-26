"""Ground truth, in plain Python. Control 1: never the datalog engine.

Every answer here rests on one predicate — do two intervals overlap — so it is
written once, in the form that is hard to get wrong (`start < other end` in both
directions), and property-tested against a second formulation.

Every function takes the ``Roster`` it is asked about, defaulting to the pinned
one. That is what lets the oracle be checked against a second formulation on a
*generated* roster: an oracle that can only run against the single roster it was
written for can only ever be checked against that roster's own answers, which is
how a fixture comes to be tuned to its truth.

**Shape, not cleverness, is what makes it fast enough.** `(person, role)`
membership in a tuple is a scan, which is free on ten people and quadratic at
`at-scale` sizes; the indexes below are built once per roster.
"""

from __future__ import annotations

from collections import defaultdict
from datetime import datetime, timedelta
from functools import lru_cache

from harness.domains.scheduling.fixture import PINNED, Roster
from harness.task import Answer

#: The pinned roster's relations and rest rule, under the names the four pinned
#: questions and the reference corpus are written against.
PEOPLE = PINNED.people
SHIFT_ROWS = PINNED.shifts
QUALIFIED_ROWS = PINNED.qualified
UNAVAILABLE_ROWS = PINNED.unavailable
ASSIGNMENT_ROWS = PINNED.assignments
MIN_REST = PINNED.min_rest


def overlaps(start: datetime, end: datetime, other_start: datetime, other_end: datetime) -> bool:
    """Half-open intervals: shifts that merely touch do not overlap."""
    return start < other_end and other_start < end


@lru_cache(maxsize=8)
def _index(roster: Roster):
    """``(shift by id, shifts per person, roles per person, blocked shifts)``.

    ``Roster`` is frozen with tuple fields, so it hashes; a different roster is
    a different cache key.
    """
    by_id = {row[0]: row for row in roster.shifts}
    assigned: dict[str, list[str]] = defaultdict(list)
    for person, shift_id in roster.assignments:
        assigned[person].append(shift_id)
    holds: dict[str, set[str]] = defaultdict(set)
    for person, role in roster.qualified:
        holds[person].add(role)
    blocked: dict[str, set[str]] = defaultdict(set)
    for person, shift_id in roster.unavailable:
        blocked[person].add(shift_id)
    return by_id, assigned, holds, blocked


def shift(shift_id: str, roster: Roster = PINNED):
    return _index(roster)[0][shift_id]


def shifts_of(person: str, roster: Roster = PINNED) -> list[str]:
    return sorted(_index(roster)[1][person])


def is_qualified(person: str, role: str, roster: Roster = PINNED) -> bool:
    return role in _index(roster)[2][person]


def is_available(person: str, shift_id: str, roster: Roster = PINNED) -> bool:
    return shift_id not in _index(roster)[3][person]


def candidates(shift_id: str, roster: Roster = PINNED) -> list[str]:
    """Everyone both qualified for the shift's role and available for it."""
    by_id, _, holds, blocked = _index(roster)
    role = by_id[shift_id][1]
    return [
        person
        for person in roster.people
        if role in holds[person] and shift_id not in blocked[person]
    ]


@lru_cache(maxsize=8)
def _candidate_counts(roster: Roster) -> dict[str, list[str]]:
    """Every shift's candidates, in one pass over the people rather than one
    pass per shift. The obvious spelling is `shifts × people` and is what an
    `at-scale` roster makes expensive."""
    by_id, _, holds, blocked = _index(roster)
    by_role: dict[str, list[str]] = defaultdict(list)
    for person in roster.people:
        for role in holds[person]:
            by_role[role].append(person)
    return {
        shift_id: [person for person in by_role[row[1]] if shift_id not in blocked[person]]
        for shift_id, row in by_id.items()
    }


def double_booked(roster: Roster = PINNED) -> Answer:
    """``(person, shift)`` for every shift that overlaps another of theirs."""
    by_id, assigned, _, _ = _index(roster)
    rows = []
    for person in roster.people:
        held = assigned[person]
        for shift_id in held:
            _, _, start, end = by_id[shift_id]
            if any(
                other != shift_id and overlaps(start, end, by_id[other][2], by_id[other][3])
                for other in held
            ):
                rows.append((person, shift_id))
    return Answer.of(*rows)


def unstaffable_shifts(roster: Roster = PINNED) -> Answer:
    counts = _candidate_counts(roster)
    return Answer.of(*[name for name, *_ in roster.shifts if not counts[name]])


def forced_assignments(roster: Roster = PINNED) -> Answer:
    """Shifts with exactly one possible person — so that person must take it."""
    counts = _candidate_counts(roster)
    return Answer.of(
        *[(name, counts[name][0]) for name, *_ in roster.shifts if len(counts[name]) == 1]
    )


def rest_violations(minimum: timedelta | None = None, roster: Roster = PINNED) -> Answer:
    """``(person, shift)`` for a shift starting too soon after another ends.

    Overlapping shifts are excluded: those are a double booking, which is a
    different fault and already has its own question. The row names the shift
    that starts too early, which is the one a manager would move.
    """
    window = roster.min_rest if minimum is None else minimum
    by_id, assigned, _, _ = _index(roster)
    rows = []
    for person in roster.people:
        held = assigned[person]
        for later in held:
            _, _, later_start, later_end = by_id[later]
            for earlier in held:
                if earlier == later:
                    continue
                _, _, earlier_start, earlier_end = by_id[earlier]
                if overlaps(later_start, later_end, earlier_start, earlier_end):
                    continue
                if earlier_end <= later_start < earlier_end + window:
                    rows.append((person, later))
    return Answer.of(*rows)


# ---- The generated slate's questions -----------------------------------------


def uncovered_but_staffable(roster: Roster = PINNED) -> Answer:
    """Shifts nobody is assigned to, though somebody could work them.

    The question the pinned slate cannot ask. Every wrong answer to the four
    pinned questions is a **subset** of the right one, so their shape cannot say
    which mistake was made. This one is two negations over two different
    relations: an arm that gets the candidates wrong drops rows, and one that
    misses an assignment adds them, so the answer moves in both directions.
    """
    counts = _candidate_counts(roster)
    covered = {shift_id for _, shift_id in roster.assignments}
    return Answer.of(*[name for name, *_ in roster.shifts if name not in covered and counts[name]])
