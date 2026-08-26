"""A week of shifts, who can work them, and who was put on them.

Two entry points, answering different questions.

``build()`` returns the **pinned** roster — shifts laid out rather than
generated, with a twilight shift that overlaps both its neighbours and a night
shift running past midnight, because "the interval that wraps" is a case worth
having on purpose. Qualifications, availability and the assignments are seeded,
with a handful of rows planted where a random draw does not reliably produce the
case a question is about.

``generate(seed, difficulty, track)`` returns a fresh one. Difficulty is **how
much the day overlaps itself** and how tight the rest window is, not how many
rows there are: with slots that tile cleanly nobody can be double-booked at all,
and with a generous rest window every gap is legal.

The rule this pack exists to respect, learnt the expensive way: **the roster
obeys the eligibility rule its own questions state.** On 2026-08-24, 21 of 29
assignments put someone on a shift they could not work, so a subject that
applied the stated rule before looking for clashes correctly answered with an
empty file, and the domain's numbers were void (`decisions.md` 2026-08-24).
Assignments are drawn only from eligible pairs, and `tasks.check` asserts it on
every generated item rather than trusting this comment.
"""

from __future__ import annotations

import random
from dataclasses import dataclass
from datetime import datetime, timedelta
from itertools import combinations, zip_longest

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

#: A person must have this long between the end of one shift and the start of
#: the next. Short enough that the night-into-morning pairs are the violations
#: and an ordinary evening-then-next-afternoon pair is not.
MIN_REST = timedelta(hours=10)

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

#: ``(people, days, slots per day, roles, roles each, overlap hours, rest hours,
#: unstaffable, forced)``.
#:
#: The knob that matters is the overlap: at one hour the day nearly tiles and a
#: clash needs two shifts that barely touch, and at five it is dense enough that
#: every pair of a person's shifts has to be compared. The rest window shortens
#: with it, so a violation stops being "the night shift" and starts being a pair
#: an arm has to subtract two timestamps to find.
DIFFICULTY: dict[int, tuple[int, int, int, int, int, int, int, int, int]] = {
    1: (10, 4, 4, 4, 2, 2, 12, 2, 2),  # the pinned shape's neighbourhood
    2: (16, 5, 4, 4, 2, 3, 11, 2, 3),
    3: (26, 6, 5, 5, 2, 4, 10, 3, 3),
    4: (40, 7, 5, 5, 3, 4, 9, 3, 4),
    5: (60, 8, 6, 6, 3, 5, 8, 4, 4),
}

#: The ``at-scale`` multiplier on the **people**, not the shifts. Every question
#: here is about a shift or a pair of a person's shifts, so multiplying the
#: shifts multiplies the answers; multiplying the people multiplies the fact base
#: and leaves the answers the size of the roster. Sized so the fixture exceeds
#: `cell.FIXTURE_TOKEN_BUDGET`; a test pins it.
AT_SCALE = 900

#: The highest difficulty the ``at-scale`` track accepts.
AT_SCALE_MAX_DIFFICULTY = 2

#: Ceilings on the crafted cases, so an `at-scale` answer stays writable.
MAX_DOUBLE_BOOKINGS = 6
MAX_TIGHT_TURNAROUNDS = 6

#: Shifts per person at `at-scale`. **One**, for everyone but the crafted
#: handful. Two or three apiece over 9,000 people put 3,534 rows in the
#: double-booking answer and 1,841 in the rest-violation one, and an answer
#: nobody can transcribe measures transcription rather than reasoning. A large
#: casual pool where a few people work twice is also the roster this shape
#: actually describes. It is why the `at-scale` questions are the same four
#: questions rather than narrowed ones: the answers are already bounded by what
#: was planted.
AT_SCALE_SHIFTS_EACH = (1, 1)


@dataclass(frozen=True)
class Roster:
    """One roster, and the rest rule it is judged against.

    A value rather than module globals, so the oracle can be handed a
    *generated* roster and checked against a second formulation on it. With
    globals the oracle can only ever be checked against the one roster it was
    written for, which is how a fixture comes to be tuned to its truth.
    """

    people: tuple[str, ...]
    roles: tuple[str, ...]
    shifts: tuple[tuple[str, str, datetime, datetime], ...]
    qualified: tuple[tuple[str, str], ...]
    unavailable: tuple[tuple[str, str], ...]
    assignments: tuple[tuple[str, str], ...]
    min_rest: timedelta = MIN_REST
    #: A role the `at-scale` questions are scoped to, so the fact base stays huge
    #: and the answer stays small. Empty on the pinned roster.
    scoped_role: str = ""

    def __hash__(self) -> int:
        """The generated hash, computed once.

        The oracle's per-lookup API — `shift`, `is_qualified`, `is_available` —
        takes the roster as an argument and reaches an `lru_cache` on it. A
        frozen dataclass hashes its fields on **every** call, which is O(rows),
        so an `at-scale` roster rehashed 60,000 rows per lookup and one test
        took eleven seconds. Value semantics are unchanged: the same fields go
        in, `__eq__` still decides equality, and only the arithmetic is reused.
        """
        cached = self.__dict__.get("_hash")
        if cached is None:
            cached = hash(
                (
                    self.people,
                    self.shifts,
                    self.qualified,
                    self.unavailable,
                    self.assignments,
                    self.min_rest,
                    self.scoped_role,
                )
            )
            object.__setattr__(self, "_hash", cached)
        return cached


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

PINNED = Roster(
    people=PEOPLE,
    roles=ROLES,
    shifts=SHIFT_ROWS,
    qualified=QUALIFIED_ROWS,
    unavailable=UNAVAILABLE_ROWS,
    assignments=ASSIGNMENT_ROWS,
)


def _overlaps(one, other) -> bool:
    """Half-open intervals: shifts that merely touch do not overlap. The oracle
    has its own copy — this one only decides where to plant."""
    return one[2] < other[3] and other[2] < one[3]


def generate(seed: int, difficulty: int = 3, track: str = "in-context") -> Roster:
    """A fresh roster.

    Identifiers are regenerated per seed, so nothing here can be answered from
    memory and two runs at the same difficulty are two samples rather than the
    same items twice.

    The order is the pinned build's, and it is load-bearing: qualifications
    first, then the reservations that close a shift off, then assignments drawn
    **only from eligible pairs**. Reserving before qualifying lets a later
    qualification re-open a shift that another question needs closed.
    """
    if difficulty not in DIFFICULTY:
        raise ValueError(f"difficulty must be one of {sorted(DIFFICULTY)}")
    if track == "at-scale" and difficulty > AT_SCALE_MAX_DIFFICULTY:
        raise ValueError(
            f"at-scale takes difficulty 1-{AT_SCALE_MAX_DIFFICULTY}: scale is the "
            "variable on that track, and crossing it with structure produces an "
            "item that is about neither"
        )
    (
        n_people,
        days,
        per_day,
        n_roles,
        roles_each,
        overlap,
        rest,
        n_unstaffable,
        n_forced,
    ) = DIFFICULTY[difficulty]
    if track == "at-scale":
        n_people *= AT_SCALE

    rng = random.Random(seed)
    tag = f"{seed:x}"[-4:]
    people = tuple(f"p{tag}{index:05d}" for index in range(n_people))
    roles = tuple(f"r{tag}{index}" for index in range(n_roles))
    min_rest = timedelta(hours=rest)

    # Slots that overlap their neighbour by `overlap` hours, the last one running
    # past midnight. A day that tiles cleanly has no double bookings in it at
    # all, so the overlap is the difficulty rather than the shift count.
    step = 24 // per_day
    length = step + overlap
    shifts = []
    for day in range(days):
        for index in range(per_day):
            start = WEEK_START + timedelta(days=day, hours=index * step)
            shifts.append(
                (
                    f"s{tag}{len(shifts):04d}",
                    roles[(day + index) % n_roles],
                    start,
                    start + timedelta(hours=length),
                )
            )
    shifts = tuple(shifts)
    by_id = {shift[0]: shift for shift in shifts}
    role_of = {shift[0]: shift[1] for shift in shifts}

    # Pairs worth planting on, found once. An overlapping pair needs two
    # *different* roles, or one person could never hold both and the clash cannot
    # be sited without breaking the eligibility rule.
    clashing = [
        (one[0], other[0])
        for one, other in combinations(shifts, 2)
        if one[1] != other[1] and _overlaps(one, other)
    ]
    tight = [
        (one[0], other[0])
        for one, other in combinations(shifts, 2)
        if not _overlaps(one, other) and one[3] <= other[2] < one[3] + min_rest
    ]
    rng.shuffle(clashing)
    rng.shuffle(tight)

    unstaffable = tuple(shift[0] for shift in shifts[:n_unstaffable])
    forced_shifts = [shift[0] for shift in shifts[n_unstaffable : n_unstaffable + n_forced]]
    reserved = set(unstaffable) | set(forced_shifts)

    doubles = [pair for pair in clashing if not set(pair) & reserved][:MAX_DOUBLE_BOOKINGS]
    turnarounds = [pair for pair in tight if not set(pair) & reserved][:MAX_TIGHT_TURNAROUNDS]
    # Interleaved, not concatenated: the room below is tight at the small end,
    # and taking them in order planted five double bookings and no short
    # turnaround at all, which leaves one of the four questions with nothing to
    # find.
    interleaved = [pair for both in zip_longest(doubles, turnarounds) for pair in both if pair]
    # Room for the forced people and for a population that is not entirely
    # crafted: one person per pair, never the same person twice, because two
    # crafted pairs on one person make one question's answer depend on another's.
    room = max(2, len(people) - n_forced - max(3, len(people) // 3))
    crafted = [(people[index], pair) for index, pair in enumerate(interleaved[:room])]
    forced = [
        (shift_id, people[len(crafted) + index]) for index, shift_id in enumerate(forced_shifts)
    ]

    qualified = set()
    for person in people:
        for role in rng.sample(roles, min(roles_each, n_roles)):
            qualified.add((person, role))
    qualified.update((person, role_of[shift_id]) for shift_id, person in forced)
    for person, pair in crafted:
        qualified.update((person, role_of[shift_id]) for shift_id in pair)

    unavailable = {
        (rng.choice(people), rng.choice(shifts)[0]) for _ in range(len(shifts) * len(people) // 8)
    }
    # Nobody qualified is free for an unstaffable shift, and exactly one person
    # is for a forced one. After the qualifications are settled, so a
    # qualification added above cannot open one of them back up.
    for person in people:
        for shift_id in unstaffable:
            if (person, role_of[shift_id]) in qualified:
                unavailable.add((person, shift_id))
        for shift_id, only in forced:
            if (person, role_of[shift_id]) in qualified and person != only:
                unavailable.add((person, shift_id))
    unavailable.difference_update((person, shift_id) for shift_id, person in forced)
    unavailable.difference_update(
        (person, shift_id) for person, pair in crafted for shift_id in pair
    )

    def eligible(person: str, shift_id: str) -> bool:
        return (person, role_of[shift_id]) in qualified and (person, shift_id) not in unavailable

    assignments = {(person, shift_id) for person, pair in crafted for shift_id in pair}
    # Drawn only from shifts the person could actually work. This is the
    # 2026-08-24 defect, and the reason it is a loop over *eligible* shifts
    # rather than over all of them.
    low, high = AT_SCALE_SHIFTS_EACH if track == "at-scale" else SHIFTS_EACH
    for person in people:
        if any(person == who for who, _ in crafted):
            continue
        open_to_them = [name for name, *_ in shifts if eligible(person, name)]
        wanted = min(rng.randint(low, high), len(open_to_them))
        assignments.update((person, shift_id) for shift_id in rng.sample(open_to_them, wanted))

    # Some shifts left with nobody on them, though somebody could work them —
    # the fifth question's answer, and a roster with no gaps in it at all is not
    # one anybody recognises.
    uncovered = {shift[0] for shift in shifts[-max(2, len(shifts) // 6) :]} - reserved
    assignments = {pair for pair in assignments if pair[1] not in uncovered}

    return Roster(
        people=people,
        roles=roles,
        shifts=tuple(sorted(by_id[shift[0]] for shift in shifts)),
        qualified=tuple(sorted(qualified)),
        unavailable=tuple(sorted(unavailable)),
        assignments=tuple(sorted(assignments)),
        min_rest=min_rest,
        scoped_role=roles[0],
    )


def to_fixture(roster: Roster) -> Fixture:
    """The four CSVs a workspace gets. One place, so a generated roster and the
    pinned one are laid out identically and no cell is decided by layout."""
    return Fixture(
        files={
            "shift.csv": "id,role,start,end\n"
            + "".join(
                f"{name},{role},{start.isoformat()},{end.isoformat()}\n"
                for name, role, start, end in roster.shifts
            ),
            "qualified.csv": "person,role\n"
            + "".join(f"{person},{role}\n" for person, role in roster.qualified),
            "unavailable.csv": "person,shift\n"
            + "".join(f"{person},{shift}\n" for person, shift in roster.unavailable),
            "assignment.csv": "person,shift\n"
            + "".join(f"{person},{shift}\n" for person, shift in roster.assignments),
        },
        schemas={
            "shift": ("id", "role", "start", "end"),
            "qualified": ("person", "role"),
            "unavailable": ("person", "shift"),
            "assignment": ("person", "shift"),
        },
    )


def build() -> Fixture:
    return to_fixture(PINNED)
