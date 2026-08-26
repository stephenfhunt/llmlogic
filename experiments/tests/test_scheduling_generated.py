"""The generated rosters, and the one defect this pack has already shipped.

On 2026-08-24, 21 of `scheduling`'s 29 assignments put someone on a shift they
could not work. Three of the four subjects answered *which assignments clash?*
with an empty file, correctly — the eligibility rule the question states, applied
first, deletes the clashes — and the domain's numbers for that grid are void
(`decisions.md` 2026-08-24). Hand-verification had passed it: it proved the
oracle matched the author's reading, not that the fixture obeyed the question.

So the first thing asserted here is not the oracle. It is that **the roster obeys
the rule its own questions state**, on every seed and every difficulty.
"""

from __future__ import annotations

from datetime import timedelta

import pytest

from harness.cell import FIXTURE_TOKEN_BUDGET
from harness.domains.scheduling import fixture, truth
from harness.domains.scheduling.tasks import check, generated
from harness.generate import Degenerate, validate

SEEDS = [s * 7919 for s in range(1, 13)]
DIFFICULTIES = sorted(fixture.DIFFICULTY)


def _by_sweep(person: str, roster: fixture.Roster) -> set[str]:
    """One person's overlapping shifts, found by walking their shifts in time
    order instead of comparing every pair.

    A second, independent formulation of the one predicate every answer in this
    pack rests on. Sorted by start, a shift overlaps something iff it starts
    before the latest end seen so far — which is a different argument from
    `start < other end and other start < end`, and agrees with it or one of them
    is wrong.
    """
    held = sorted(
        (truth.shift(shift_id, roster) for shift_id in truth.shifts_of(person, roster)),
        key=lambda row: (row[2], row[3]),
    )
    clashing: set[str] = set()
    for index, (name, _, start, _) in enumerate(held):
        for other_name, _, _other_start, other_end in held[:index]:
            if start < other_end:
                clashing.add(name)
                clashing.add(other_name)
    return clashing


class TestTheRosterObeysItsOwnRule:
    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_every_assignment_is_one_the_person_could_work(self, seed, difficulty):
        roster = fixture.generate(seed, difficulty)
        for person, shift_id in roster.assignments:
            role = truth.shift(shift_id, roster)[1]
            assert truth.is_qualified(person, role, roster), (
                f"{person} not qualified for {shift_id}"
            )
            assert truth.is_available(person, shift_id, roster), (
                f"{person} unavailable for {shift_id}"
            )

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_same_holds_at_scale(self, seed):
        roster = fixture.generate(seed, 1, "at-scale")
        for person, shift_id in roster.assignments:
            role = truth.shift(shift_id, roster)[1]
            assert truth.is_qualified(person, role, roster)
            assert truth.is_available(person, shift_id, roster)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_nobody_is_assigned_to_a_shift_nobody_can_work(self, seed):
        """The contradiction that would make two answers disagree: an
        unstaffable shift with somebody on it."""
        roster = fixture.generate(seed, 3)
        unstaffable = {row[0] for row in truth.unstaffable_shifts(roster).rows}
        assert not {shift_id for _, shift_id in roster.assignments} & unstaffable


class TestOracle:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_overlap_agrees_with_a_sweep(self, seed):
        roster = fixture.generate(seed, 3)
        clashing = {(person, shift_id) for person, shift_id in truth.double_booked(roster).rows}
        for person in roster.people:
            assert {(person, name) for name in _by_sweep(person, roster)} == {
                pair for pair in clashing if pair[0] == person
            }

    @pytest.mark.parametrize("seed", SEEDS[:4])
    def test_candidates_agree_with_the_unindexed_definition(self, seed):
        """The fast shape has to compute what the slow, obviously-correct one
        does. The slow spelling is `shifts × people`, which is why it is not
        what ships — and exactly why it is worth keeping as the check."""
        roster = fixture.generate(seed, 2)
        for name, role, _, _ in roster.shifts:
            expected = [
                person
                for person in roster.people
                if (person, role) in set(roster.qualified)
                and (person, name) not in set(roster.unavailable)
            ]
            assert truth.candidates(name, roster) == expected

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_a_rest_violation_is_never_an_overlap(self, seed):
        """Two faults, kept apart: the question says so, and a pair counted as
        both would be reported twice under two different names."""
        roster = fixture.generate(seed, 3)
        clashes = set(truth.double_booked(roster).rows)
        for person, shift_id in truth.rest_violations(roster=roster).rows:
            _, _, start, end = truth.shift(shift_id, roster)
            for other in truth.shifts_of(person, roster):
                if other == shift_id:
                    continue
                _, _, other_start, other_end = truth.shift(other, roster)
                if truth.overlaps(start, end, other_start, other_end):
                    assert (person, shift_id) in clashes

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_a_rest_gap_is_measured_across_the_date_boundary(self, seed):
        """The night shift ends on the following date, so a comparison on clock
        time alone gets a plausible wrong answer. The oracle subtracts
        timestamps; this asserts the fixture actually contains such a pair."""
        roster = fixture.generate(seed, 3)
        assert any(start.date() != end.date() for _, _, start, end in roster.shifts)


class TestGeneratedItems:
    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_every_generated_item_is_worth_asking(self, seed, difficulty):
        rejected = 0
        for task in generated(seed, difficulty):
            try:
                validate(task)
                check(task)
            except Degenerate:
                rejected += 1
        assert rejected == 0

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_each_question_has_more_than_one_thing_to_find(self, seed):
        """An arm that finds one and stops must not score the same as one that
        checked everything — the reason the pinned fixture plants *two* of each."""
        roster = fixture.generate(seed, 3)
        for answer in (
            truth.double_booked(roster),
            truth.unstaffable_shifts(roster),
            truth.forced_assignments(roster),
            truth.rest_violations(roster=roster),
            truth.uncovered_but_staffable(roster),
        ):
            assert len(answer.rows) >= 2

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_difficulty_makes_the_day_overlap_itself_more(self, seed):
        """The knob this pack's difficulty actually turns. A day that tiles
        cleanly has no double booking in it at all, whatever the roster."""
        shallow = fixture.generate(seed, 1)
        deep = fixture.generate(seed, 5)
        overlapping = lambda roster: sum(  # noqa: E731
            1
            for one in roster.shifts
            for other in roster.shifts
            if one[0] < other[0] and truth.overlaps(one[2], one[3], other[2], other[3])
        )
        assert overlapping(deep) > overlapping(shallow)
        assert deep.min_rest < shallow.min_rest

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_same_seed_gives_the_same_roster(self, seed):
        assert fixture.generate(seed, 3) == fixture.generate(seed, 3)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_two_seeds_give_different_identifiers(self, seed):
        one = set(fixture.generate(seed, 3).people)
        other = set(fixture.generate(seed + 1, 3).people)
        assert not (one & other)

    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_an_in_context_fixture_stays_inside_the_budget(self, difficulty):
        task = generated(20260825, difficulty)[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 < FIXTURE_TOKEN_BUDGET
        assert task.track == "in-context"

    def test_an_at_scale_fixture_actually_exceeds_it(self):
        task = generated(20260825, 1, "at-scale")[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 > FIXTURE_TOKEN_BUDGET
        assert task.track == "at-scale"

    def test_at_scale_answers_stay_bounded_by_what_was_planted(self):
        """Why this pack asks the *same* questions on both tracks. Two or three
        shifts apiece over 9,000 people put 3,534 rows in the double-booking
        answer; one apiece leaves it the size of the crafted handful."""
        for task in generated(20260825, 2, "at-scale"):
            assert len(task.truth.rows) <= 20, f"{task.id} answers {len(task.truth.rows)} rows"

    def test_at_scale_refuses_a_difficulty_it_cannot_carry(self):
        with pytest.raises(ValueError, match="at-scale takes difficulty"):
            fixture.generate(20260825, 5, "at-scale")

    def test_an_unknown_difficulty_is_refused(self):
        with pytest.raises(ValueError):
            fixture.generate(20260825, 99)

    def test_the_generated_slate_asks_something_the_pinned_one_cannot(self):
        ids = {task.id for task in generated(20260825, 3)}
        assert any(task_id.endswith("uncovered-but-staffable") for task_id in ids)


class TestCheck:
    def test_an_ineligible_assignment_is_refused(self):
        """The 2026-08-24 defect, manufactured. This is the check that would
        have caught it before 112 cells were paid for."""
        from dataclasses import replace

        roster = fixture.generate(20260825, 3)
        broken = replace(roster, assignments=roster.assignments + ((roster.people[0], "nowhere"),))
        # A shift the person is definitely not qualified for: give it a role
        # nobody holds, so the assignment is illegal by the stated rule.
        broken = replace(
            broken,
            shifts=broken.shifts + (("nowhere", "role-nobody-holds", *broken.shifts[0][2:]),),
        )
        task = replace(generated(20260825, 3)[0], fixture=fixture.to_fixture(broken))
        with pytest.raises(Degenerate, match="break the eligibility rule"):
            check(task)

    def test_a_single_unstaffable_shift_is_refused(self):
        from dataclasses import replace

        from harness.task import Answer

        task = next(t for t in generated(20260825, 3) if t.id.endswith("unstaffable-shifts"))
        with pytest.raises(Degenerate, match="finds it and stops"):
            check(replace(task, truth=Answer.of(sorted(task.truth.rows)[0])))


def test_the_pinned_rest_window_is_what_the_pinned_questions_quote():
    """`MIN_REST` moved from `truth.py` to the roster it describes. The pinned
    questions quote it, so a move that changed its value would rewrite the
    comparable slate silently."""
    assert truth.MIN_REST == timedelta(hours=10)
