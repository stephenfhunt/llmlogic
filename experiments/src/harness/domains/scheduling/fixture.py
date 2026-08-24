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
#: Exactly one qualified person is left available for each of these. Two, for the
#: same reason as above — and planted rather than drawn, because a coherent
#: roster works against them: every qualification the draw hands out is another
#: candidate, so the number of shifts that happen to have exactly one falls as
#: the fixture gets more realistic. Relying on the draw left this question with a
#: single answer row.
FORCED = (("s05", "p03"), ("s11", "p07"))
#: Assigned to two shifts that overlap.
DOUBLE_BOOKED = (("p01", "s01"), ("p01", "s02"))
#: Assigned to a shift that starts before they have had the minimum rest.
TIGHT_TURNAROUND = (("p05", "s03"), ("p05", "s04"))
#: Every planted assignment, which the roster has to be able to accommodate:
#: each person is made qualified and available for the shift they are put on.
#: None of these shifts may be one of the crafted ones above, or the planting
#: would undo the guarantee those rest on — `_build` asserts it rather than
#: leaving it to be re-checked by eye.
PLANTED = DOUBLE_BOOKED + TIGHT_TURNAROUND

#: Two roles each. One apiece and nobody can be double-booked at all: no two
#: overlapping shifts share a role (`SLOTS` walks `ROLES` as it walks the day),
#: so a clash needs a person holding *both* roles of an overlapping pair. That is
#: the price of a roster that obeys its own eligibility rule, and this is what
#: pays it.
ROLES_EACH = 2
#: Shifts per person. Enough that clashes and short turnarounds arise from the
#: draw rather than only from `PLANTED`.
SHIFTS_EACH = (2, 3)


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
    role_of = {name: role for name, role, _, _ in SHIFT_ROWS}
    reserved = set(UNSTAFFABLE_SHIFTS) | {shift_id for shift_id, _ in FORCED}
    assert not {shift_id for _, shift_id in PLANTED} & reserved

    qualified = set()
    for person in PEOPLE:
        for role in rng.sample(ROLES, ROLES_EACH):
            qualified.add((person, role))
    # The forced people have to be qualified for the shift they are the only
    # candidate for, and everyone planted onto a shift has to be able to work it.
    qualified.update((person, role_of[shift_id]) for shift_id, person in FORCED)
    qualified.update((person, role_of[shift_id]) for person, shift_id in PLANTED)

    unavailable = {(rng.choice(PEOPLE), rng.choice(SHIFT_ROWS)[0]) for _ in range(12)}
    # Nobody qualified is free for the unstaffable shift, and only one person is
    # for the forced one. This runs after the qualifications are settled, so a
    # qualification added just above cannot open one of them back up.
    for person in PEOPLE:
        for shift_id in UNSTAFFABLE_SHIFTS:
            if (person, role_of[shift_id]) in qualified:
                unavailable.add((person, shift_id))
        for shift_id, only in FORCED:
            if (person, role_of[shift_id]) in qualified and person != only:
                unavailable.add((person, shift_id))
    unavailable.difference_update((person, shift_id) for shift_id, person in FORCED)
    unavailable.difference_update(PLANTED)

    def eligible(person: str, shift_id: str) -> bool:
        return (person, role_of[shift_id]) in qualified and (person, shift_id) not in unavailable

    assignments = set(PLANTED)
    # Two or three shifts each: with one apiece the roster is so sparse that the
    # only double booking is the planted one, and a question with one crafted
    # answer measures whether the subject found the thing we hid.
    #
    # Drawn only from shifts the person could actually work. A roster that
    # contradicts its own eligibility rule made "which assignments clash?"
    # unanswerable — the rule, applied first, deletes the clashes — and three of
    # the four subjects on 2026-08-24 answered with an empty file, correctly.
    # See ``decisions.md`` 2026-08-24.
    for person in PEOPLE:
        open_to_them = [name for name, *_ in SHIFT_ROWS if eligible(person, name)]
        wanted = min(rng.randint(*SHIFTS_EACH), len(open_to_them))
        assignments.update((person, shift_id) for shift_id in rng.sample(open_to_them, wanted))

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
