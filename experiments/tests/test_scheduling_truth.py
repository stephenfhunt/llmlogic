"""The scheduling oracle: one predicate, checked hard, and three uses of it.

Everything in this domain reduces to "do these two intervals overlap", so that
is where the property tests are. The rest is checked against the roster it was
computed from, not against a second reading of the same code.
"""

from datetime import datetime, timedelta

import pytest
from hypothesis import given
from hypothesis import strategies as st

from harness.domains.scheduling import fixture, truth

_MOMENT = st.integers(min_value=0, max_value=48).map(
    lambda hours: datetime(2026, 3, 2) + timedelta(hours=hours)
)


def by_extremes(start, end, other_start, other_end):
    """Overlap as "the latest start is before the earliest end".

    A different arrangement of the same comparison, and the one people reach for
    second — if the two disagree anywhere, the fixture's answers rest on which
    was typed first.
    """
    return max(start, other_start) < min(end, other_end)


def _interval(a, b):
    """A shift of at least an hour. Zero-length intervals are excluded on
    purpose: the two formulations disagree there — `overlaps` calls an empty
    interval inside a longer one an overlap and `by_extremes` does not — and the
    disagreement is about a shift that occupies no time, which
    `test_no_shift_is_zero_length` shows the roster cannot contain."""
    start, end = min(a, b), max(a, b)
    return start, max(end, start + timedelta(hours=1))


@given(a=_MOMENT, b=_MOMENT, c=_MOMENT, d=_MOMENT)
def test_overlap_agrees_with_an_independent_formulation(a, b, c, d):
    start, end = _interval(a, b)
    other_start, other_end = _interval(c, d)
    assert truth.overlaps(start, end, other_start, other_end) == by_extremes(
        start, end, other_start, other_end
    )


def test_no_shift_is_zero_length():
    for name, _, start, end in fixture.SHIFT_ROWS:
        assert end > start, name


@given(a=_MOMENT, b=_MOMENT, c=_MOMENT, d=_MOMENT)
def test_overlap_is_symmetric(a, b, c, d):
    start, end = _interval(a, b)
    other_start, other_end = _interval(c, d)
    assert truth.overlaps(start, end, other_start, other_end) == truth.overlaps(
        other_start, other_end, start, end
    )


def test_touching_shifts_do_not_overlap():
    # The boundary the question states, and the one an inclusive comparison gets
    # wrong: a 07:00–15:00 and a 15:00–23:00 shift are back to back, not clashing.
    morning = (datetime(2026, 3, 2, 7), datetime(2026, 3, 2, 15))
    evening = (datetime(2026, 3, 2, 15), datetime(2026, 3, 2, 23))
    assert not truth.overlaps(*morning, *evening)
    assert truth.overlaps(*morning, datetime(2026, 3, 2, 11), datetime(2026, 3, 2, 19))


def test_the_roster_contains_overlapping_shifts_at_all():
    # If the day tiles cleanly, three of the four questions are about nothing.
    pairs = [
        (left[0], right[0])
        for left in fixture.SHIFT_ROWS
        for right in fixture.SHIFT_ROWS
        if left[0] < right[0] and truth.overlaps(left[2], left[3], right[2], right[3])
    ]
    assert pairs


def test_every_double_booking_really_clashes_with_another_of_their_shifts():
    for person, shift_id in truth.double_booked().rows:
        _, _, start, end = truth.shift(shift_id)
        clashes = [
            other
            for other in truth.shifts_of(person)
            if other != shift_id and truth.overlaps(start, end, *truth.shift(other)[2:])
        ]
        assert clashes, (person, shift_id)


def test_a_double_booking_reports_both_halves_of_the_pair():
    # Reporting one row per clashing *pair* would need a canonical order; one row
    # per assignment does not, and both shifts are things a manager has to see.
    rows = truth.double_booked().rows
    for person, shift_id in rows:
        partners = [
            other
            for other in truth.shifts_of(person)
            if other != shift_id
            and truth.overlaps(*truth.shift(shift_id)[2:], *truth.shift(other)[2:])
        ]
        for partner in partners:
            assert (person, partner) in rows


def test_an_unstaffable_shift_has_nobody_qualified_and_free():
    unstaffable = {shift_id for (shift_id,) in truth.unstaffable_shifts().rows}
    assert unstaffable
    for shift_id in unstaffable:
        _, role, _, _ = truth.shift(shift_id)
        for person in fixture.PEOPLE:
            assert not (truth.is_qualified(person, role) and truth.is_available(person, shift_id))


def test_a_staffable_shift_is_not_reported():
    unstaffable = {shift_id for (shift_id,) in truth.unstaffable_shifts().rows}
    covered = [name for name, *_ in fixture.SHIFT_ROWS if name not in unstaffable]
    assert covered
    for shift_id in covered:
        assert truth.candidates(shift_id)


def test_a_forced_assignment_has_exactly_one_candidate():
    forced = truth.forced_assignments().rows
    assert forced
    for shift_id, person in forced:
        assert truth.candidates(shift_id) == [person]


def test_a_shift_with_two_candidates_is_not_forced():
    forced = {shift_id for shift_id, _ in truth.forced_assignments().rows}
    plural = [name for name, *_ in fixture.SHIFT_ROWS if len(truth.candidates(name)) > 1]
    assert plural
    assert not (set(plural) & forced)


def test_rest_violations_are_never_overlaps():
    # The two faults are reported separately, and a shift that overlaps another is
    # not also "too soon after" it.
    clashes = truth.double_booked().rows
    for person, shift_id in truth.rest_violations().rows:
        _, _, start, end = truth.shift(shift_id)
        for other in truth.shifts_of(person):
            if other != shift_id and truth.overlaps(start, end, *truth.shift(other)[2:]):
                assert (person, shift_id) in clashes


def test_a_rest_violation_crosses_the_day_boundary_somewhere():
    # The case a clock-time comparison gets wrong: off at 07:00 having started at
    # 23:00 the day before.
    crossing = [
        (person, shift_id)
        for person, shift_id in truth.rest_violations().rows
        for other in truth.shifts_of(person)
        if other != shift_id and truth.shift(other)[2].date() != truth.shift(other)[3].date()
    ]
    assert crossing


@given(hours=st.integers(min_value=0, max_value=24))
def test_a_longer_rest_requirement_never_removes_a_violation(hours):
    fewer = truth.rest_violations(timedelta(hours=hours)).rows
    more = truth.rest_violations(timedelta(hours=hours + 1)).rows
    assert fewer <= more


# ---- The roster obeys its own rules -------------------------------------------


def test_every_assignment_is_one_the_person_could_actually_work():
    """The property whose absence made `double-booked` unanswerable.

    The questions state that a person can work a shift only if they are qualified
    and available. A roster that breaks that rule contradicts its own preamble,
    and the contradiction is not benign: applying the rule to `assignment.csv`
    before looking for clashes — a reasonable reading — deleted every clash, so
    the honest answer became the empty set. 21 of the 29 assignments were
    ineligible when the 2026-08-24 grid ran, including the planted clash itself.
    """
    for person, shift_id in fixture.ASSIGNMENT_ROWS:
        _, role, _, _ = truth.shift(shift_id)
        assert truth.is_qualified(person, role), (person, shift_id, role)
        assert truth.is_available(person, shift_id), (person, shift_id)


def test_nobody_is_assigned_to_a_shift_nobody_can_work():
    # Follows from the above, and is the absurdity that made it visible: the
    # roster used to put people on the very shifts `unstaffable-shifts` reports.
    unstaffable = {shift_id for (shift_id,) in truth.unstaffable_shifts().rows}
    assigned = {shift_id for _, shift_id in fixture.ASSIGNMENT_ROWS}
    assert not (unstaffable & assigned)


@pytest.mark.parametrize(
    "answer",
    [
        truth.double_booked(),
        truth.unstaffable_shifts(),
        truth.forced_assignments(),
        truth.rest_violations(),
    ],
    ids=["double-booked", "unstaffable-shifts", "forced-assignments", "rest-violations"],
)
def test_no_question_can_be_answered_by_finding_one_thing(answer):
    """An arm that finds the single planted row and stops must not score the same
    as one that checked the whole roster. Making the roster coherent works
    against this — every qualification is another candidate, and a clash now
    needs one person holding both roles of an overlapping pair — which is why
    `FORCED` plants two and `ROLES_EACH` is 2."""
    assert len(answer.rows) > 1


def test_a_clash_arises_from_the_draw_and_not_only_from_the_planting():
    # `DOUBLE_BOOKED` is planted so the question always has an answer. If it were
    # the *only* answer, the question would measure whether the subject found the
    # thing we hid rather than whether it checked every pair.
    planted = set(fixture.DOUBLE_BOOKED)
    assert truth.double_booked().rows - planted
