"""A week of shifts, who can work them, and who was put on them.

The shifts are laid out rather than generated — three a day for four days, with a
night shift that crosses midnight, because "the interval that wraps" is a case
worth having on purpose. Qualifications, availability and the assignments are
seeded, with a handful of rows planted where a random draw does not reliably
produce the case a question is about.
"""

from __future__ import annotations

import random
from datetime import datetime, timedelta

from harness.task import Fixture

SEED = 20260822

PEOPLE = tuple(f"p{i:02d}" for i in range(1, 11))
ROLES = ("nurse", "tech", "porter", "clerk")

WEEK_START = datetime(2026, 3, 2, 7, 0)
DAYS = 4

#: `(offset from 07:00, length)`. Four a day: morning, a twilight shift that
#: **overlaps both of its neighbours**, evening, and a night shift running into
#: the next day. Without the overlapping one the day tiles cleanly and "who is
#: double-booked?" has no answer at all; without the night one no rest gap ever
#: crosses midnight.
SLOTS = (
    (timedelta(0), timedelta(hours=8)),
    (timedelta(hours=4), timedelta(hours=8)),
    (timedelta(hours=8), timedelta(hours=8)),
    (timedelta(hours=16), timedelta(hours=8)),
)

#: Nobody qualified is available for these, so "which shifts can nobody cover?"
#: has an answer that does not depend on the draw. Two of them, not one: with a
#: single answer, an arm that finds it and stops scores the same as one that
#: checked every shift.
UNSTAFFABLE_SHIFTS = ("s07", "s14")
#: Exactly one qualified person is left available for this one.
FORCED_SHIFT = "s05"
FORCED_PERSON = "p03"
#: Assigned to two shifts that overlap.
DOUBLE_BOOKED = (("p01", "s01"), ("p01", "s02"))
#: Assigned to a shift that starts before they have had the minimum rest.
TIGHT_TURNAROUND = (("p05", "s03"), ("p05", "s04"))


def _shifts():
    shifts = []
    for day in range(DAYS):
        for index, (offset, length) in enumerate(SLOTS):
            start = WEEK_START + timedelta(days=day) + offset
            shifts.append(
                (
                    f"s{len(shifts) + 1:02d}",
                    ROLES[(day + index) % len(ROLES)],
                    start,
                    start + length,
                )
            )
    return tuple(shifts)


SHIFT_ROWS = _shifts()


def _build():
    rng = random.Random(SEED)
    qualified = set()
    for person in PEOPLE:
        for role in rng.sample(ROLES, rng.randint(1, 2)):
            qualified.add((person, role))
    # The planted person has to be qualified for the shift they are forced onto.
    forced_role = next(role for name, role, _, _ in SHIFT_ROWS if name == FORCED_SHIFT)
    qualified.add((FORCED_PERSON, forced_role))

    role_of = {name: role for name, role, _, _ in SHIFT_ROWS}
    unavailable = {(rng.choice(PEOPLE), rng.choice(SHIFT_ROWS)[0]) for _ in range(12)}
    # Nobody qualified is free for the unstaffable shift, and only one person is
    # for the forced one.
    for person in PEOPLE:
        for shift_id in UNSTAFFABLE_SHIFTS:
            if (person, role_of[shift_id]) in qualified:
                unavailable.add((person, shift_id))
        if (person, role_of[FORCED_SHIFT]) in qualified and person != FORCED_PERSON:
            unavailable.add((person, FORCED_SHIFT))
    unavailable.discard((FORCED_PERSON, FORCED_SHIFT))

    assignments = set(DOUBLE_BOOKED) | set(TIGHT_TURNAROUND)
    # Two or three shifts each: with one apiece the roster is so sparse that the
    # only double booking is the planted one, and a question with one crafted
    # answer measures whether the subject found the thing we hid.
    for person in PEOPLE:
        for _ in range(rng.randint(2, 3)):
            assignments.add((person, rng.choice(SHIFT_ROWS)[0]))

    return (
        tuple(sorted(qualified)),
        tuple(sorted(unavailable)),
        tuple(sorted(assignments)),
    )


QUALIFIED_ROWS, UNAVAILABLE_ROWS, ASSIGNMENT_ROWS = _build()


def build() -> Fixture:
    shift_csv = "id,role,start,end\n" + "".join(
        f"{name},{role},{start.isoformat()},{end.isoformat()}\n"
        for name, role, start, end in SHIFT_ROWS
    )
    qualified_csv = "person,role\n" + "".join(
        f"{person},{role}\n" for person, role in QUALIFIED_ROWS
    )
    unavailable_csv = "person,shift\n" + "".join(
        f"{person},{shift}\n" for person, shift in UNAVAILABLE_ROWS
    )
    assignment_csv = "person,shift\n" + "".join(
        f"{person},{shift}\n" for person, shift in ASSIGNMENT_ROWS
    )
    return Fixture(
        files={
            "shift.csv": shift_csv,
            "qualified.csv": qualified_csv,
            "unavailable.csv": unavailable_csv,
            "assignment.csv": assignment_csv,
        },
        schemas={
            "shift": ("id", "role", "start", "end"),
            "qualified": ("person", "role"),
            "unavailable": ("person", "shift"),
            "assignment": ("person", "shift"),
        },
    )
