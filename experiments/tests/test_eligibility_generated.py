"""The generated caseloads: is the oracle right, is the item worth asking?

Hand-checking 28 tasks proved the oracle matched the *author's* reading, and it
could not prove there was only one (`decisions.md` 2026-08-24). Generation
multiplies that by the number of items, so the checks are mechanical here and run
over many seeds rather than one.

The second formulation is deliberately **the rendered file**, not a second pass
over the same in-memory rows. This pack's whole trap is that a blank cell is not
a zero, and the only artefact a subject ever sees is `applicant.csv`. An oracle
that agreed with the generator's tuples while the CSV said something else would
be right about the wrong thing.
"""

from __future__ import annotations

import pytest

from harness.cell import FIXTURE_TOKEN_BUDGET
from harness.domains.eligibility import fixture, truth
from harness.domains.eligibility.tasks import check, generated
from harness.generate import Degenerate, validate

SEEDS = [s * 7919 for s in range(1, 13)]
DIFFICULTIES = sorted(fixture.DIFFICULTY)


def _from_the_csv(caseload: fixture.Caseload) -> tuple[set[str], set[tuple[str, str]], set[str]]:
    """`(eligible, blocked-by-one, undetermined)`, read off the emitted files.

    A second, independent formulation: it parses the CSVs the workspace gets,
    treats an empty field as unknown, and applies the four criteria in the order
    the question states them. Nothing here consults `truth.py`.
    """
    fx = fixture.to_fixture(caseload)
    sanctioned = {
        line.split(",")[0] for line in fx.text("sanction.csv").splitlines()[1:] if line.strip()
    }
    eligible, blocked, undetermined = set(), set(), set()
    for line in fx.text("applicant.csv").splitlines()[1:]:
        if not line.strip():
            continue
        name, age, income, residency, _ = line.split(",")
        failed, unknown = [], []
        for value, criterion, holds in (
            (age, "age", lambda v: int(v) >= caseload.min_age),
            (income, "income", lambda v: int(v) < caseload.max_income),
            (residency, "residency", lambda v: int(v) >= caseload.min_residency_years),
        ):
            if value == "":
                unknown.append(criterion)
            elif not holds(value):
                failed.append(criterion)
        if name in sanctioned:
            failed.append("sanction")
        if not failed and not unknown:
            eligible.add(name)
        if not unknown and len(failed) == 1:
            blocked.add((name, failed[0]))
        if unknown and not failed:
            undetermined.add(name)
    return eligible, blocked, undetermined


class TestOracle:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_the_oracle_agrees_with_the_rendered_file(self, seed):
        caseload = fixture.generate(seed, 3)
        eligible, blocked, undetermined = _from_the_csv(caseload)
        assert {row[0] for row in truth.eligible(caseload).rows} == eligible
        assert set(truth.blocked_by_exactly_one(caseload).rows) == blocked
        assert {row[0] for row in truth.undetermined(caseload).rows} == undetermined

    @pytest.mark.parametrize("seed", SEEDS[:6])
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_a_blank_is_never_read_as_a_zero(self, seed, difficulty):
        """The failure mode the pack exists for: an applicant with no income is
        not an applicant on nothing. Read as a zero they would all qualify."""
        caseload = fixture.generate(seed, difficulty)
        qualified = {row[0] for row in truth.eligible(caseload).rows}
        for name, age, income, residency, _ in caseload.applicants:
            if None in (age, income, residency):
                assert name not in qualified, f"{name} qualified with an absent value"

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_three_verdicts_partition_nothing_twice(self, seed):
        """Eligible, undetermined and blocked-by-one are disjoint by definition,
        and an oracle that let an applicant into two of them would be reporting
        the same person as a pass and a pending."""
        caseload = fixture.generate(seed, 4)
        qualified = {row[0] for row in truth.eligible(caseload).rows}
        pending = {row[0] for row in truth.undetermined(caseload).rows}
        blocked = {row[0] for row, _ in truth.blocked_by_exactly_one(caseload).rows}
        assert not qualified & pending
        assert not qualified & blocked
        assert not pending & blocked


class TestGeneratedItems:
    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_every_generated_item_is_worth_asking(self, seed, difficulty):
        """`validate` refuses; it does not silently filter. A generator that
        drops a third of its items is one whose difficulty setting no longer
        means what it says, so the rejection rate is watched, not absorbed."""
        rejected = 0
        for task in generated(seed, difficulty):
            try:
                validate(task)
                check(task)
            except Degenerate:
                rejected += 1
        assert rejected == 0

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_undetermined_is_never_just_the_blank_column(self, seed):
        """An arm that answers by listing the blanks must score wrong. This is
        the pinned slate's `a13` — no income *and* failing residency — made a
        property instead of a planted row."""
        caseload = fixture.generate(seed, 3)
        pending = {row[0] for row in truth.undetermined(caseload).rows}
        for column in fixture.UNKNOWABLE:
            assert pending != truth.blank_column(column, caseload)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_every_criterion_is_somebody_s_only_failure(self, seed):
        """Otherwise an arm that never checks that criterion scores full marks
        on `blocked-by-exactly-one`, and nothing in the answer says so."""
        caseload = fixture.generate(seed, 3)
        named = {criterion for _, criterion in truth.blocked_by_exactly_one(caseload).rows}
        assert named == set(truth.CRITERIA)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_difficulty_admits_more_ways_for_a_value_to_be_absent(self, seed):
        """The knob this pack's difficulty actually turns: at the bottom only
        income can be blank, at the top three columns can."""
        shallow = fixture.generate(seed, 1)
        deep = fixture.generate(seed, 5)
        blank_columns = lambda load: {  # noqa: E731
            column for column in fixture.UNKNOWABLE if truth.blank_column(column, load)
        }
        assert blank_columns(shallow) == {"income"}
        assert len(blank_columns(deep)) > 1
        assert len(deep.applicants) > len(shallow.applicants)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_same_seed_gives_the_same_caseload(self, seed):
        assert fixture.generate(seed, 3) == fixture.generate(seed, 3)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_two_seeds_give_different_identifiers(self, seed):
        """Anti-memorisation: nothing here should be answerable from having seen
        it before."""
        one = {row[0] for row in fixture.generate(seed, 3).applicants}
        other = {row[0] for row in fixture.generate(seed + 1, 3).applicants}
        assert not (one & other)

    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_an_in_context_fixture_stays_inside_the_budget(self, difficulty):
        task = generated(20260825, difficulty)[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 < FIXTURE_TOKEN_BUDGET
        assert task.track == "in-context"

    def test_an_at_scale_fixture_actually_exceeds_it(self):
        """Otherwise it is an `in-context` item wearing the wrong label."""
        task = generated(20260825, 1, "at-scale")[0]
        chars = sum(len(c) for c in task.fixture.files.values() if isinstance(c, str))
        assert chars // 4 > FIXTURE_TOKEN_BUDGET
        assert task.track == "at-scale"

    def test_at_scale_answers_stay_small(self):
        """The combination the track is about: a fact base nobody can hold, and
        an answer somebody can write. Unnarrowed, `undetermined` came back with
        1,493 rows, which measures transcription."""
        for task in generated(20260825, 2, "at-scale"):
            assert len(task.truth.rows) <= 60, f"{task.id} answers {len(task.truth.rows)} rows"

    def test_at_scale_narrows_rather_than_repeats(self):
        """The `at-scale` `undetermined` item adds a second stratum — *and
        nobody in their household qualifies*. If that selected every
        undetermined applicant it would be the `in-context` item with longer
        prose."""
        caseload = fixture.generate(20260825, 2, "at-scale")
        assert len(truth.undetermined_where_nobody_qualifies(caseload).rows) < len(
            truth.undetermined(caseload).rows
        )

    def test_at_scale_refuses_a_difficulty_it_cannot_carry(self):
        with pytest.raises(ValueError, match="at-scale takes difficulty"):
            fixture.generate(20260825, 5, "at-scale")

    def test_an_unknown_difficulty_is_refused(self):
        with pytest.raises(ValueError):
            fixture.generate(20260825, 99)

    def test_the_generated_slate_asks_something_the_pinned_one_cannot(self):
        """Every wrong answer to the four pinned questions is a subset of the
        right one, so their shape cannot say which mistake was made. Comparing
        two counts is false in both directions, so it can."""
        ids = {task.id for task in generated(20260825, 3)}
        assert any(task_id.endswith("more-undetermined-than-eligible") for task_id in ids)


class TestCheck:
    def test_an_undetermined_answer_that_is_just_the_blanks_is_refused(self):
        from dataclasses import replace

        from harness.task import Answer

        task = next(t for t in generated(20260825, 3) if t.id.endswith("undetermined"))
        blanks = truth.blank_column("income", fixture.generate(20260825, 3))
        with pytest.raises(Degenerate, match="listing the blanks"):
            check(replace(task, truth=Answer.of(*sorted(blanks))))

    def test_a_criterion_nobody_fails_alone_is_refused(self):
        from dataclasses import replace

        from harness.task import Answer

        task = next(t for t in generated(20260825, 3) if t.id.endswith("blocked-by-exactly-one"))
        thinned = Answer.of(*[row for row in task.truth.rows if row[1] != "sanction"])
        with pytest.raises(Degenerate, match="fails only"):
            check(replace(task, truth=thinned))
