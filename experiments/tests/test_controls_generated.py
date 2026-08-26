"""The generated controls, and the one way they can fail: by getting hard.

§1 says a negative S1 result is a finding rather than a failure to ship. That is
only true if the instrument can produce one, and a slate of transitive-closure
questions cannot. These four calibrate the null — and they only calibrate it
while they stay questions a model answers in its head.

So every assertion here is a ceiling, not a floor. That is the opposite of every
other pack's generated tests, and it is why this file exists separately.
"""

from __future__ import annotations

import pytest

from harness.domains.controls import fixture, truth
from harness.domains.controls.tasks import MAX_ROWS, check, generated
from harness.generate import Degenerate, validate

SEEDS = [s * 7919 for s in range(1, 13)]
DIFFICULTIES = sorted(fixture.DIFFICULTY)


class TestTheyStayControls:
    @pytest.mark.parametrize("seed", SEEDS)
    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_every_generated_control_is_still_a_control(self, seed, difficulty):
        for task in generated(seed, difficulty):
            validate(task)
            check(task)
            assert task.engine_expected_to_help is False
            assert task.question_class in ("single-hop", "one-step")

    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_the_fact_base_never_grows_past_a_glance(self, difficulty):
        """The rows grow a little so a calibrated slate has some range. They do
        not grow into a retrieval problem."""
        for task in generated(20260825, difficulty):
            rows = sum(
                len(contents.splitlines()) - 1
                for contents in task.fixture.files.values()
                if isinstance(contents, str)
            )
            assert rows <= MAX_ROWS

    @pytest.mark.parametrize("difficulty", DIFFICULTIES)
    def test_no_answer_needs_more_than_one_table(self, difficulty):
        """Each answer is a lookup, a comparison or a count **within one
        relation**. Nothing here joins the two tables, and nothing closes over
        anything — that is what `single-hop` and `one-step` mean, and a control
        that quietly acquired a join would stop calibrating the null."""
        for task in generated(20260825, difficulty):
            assert len(task.answer_shape) == 1
            answered = {row[0] for row in task.truth.rows}
            sources = [
                name
                for name, contents in task.fixture.files.items()
                if isinstance(contents, str)
                and answered
                <= {
                    field.strip() for line in contents.splitlines()[1:] for field in line.split(",")
                }
            ]
            # A count is in neither table, which is what makes it the one-step
            # question rather than a lookup.
            assert len(sources) <= 1, f"{task.id} draws its answer from {sources}"

    def test_there_is_no_at_scale_track(self):
        """A single-hop lookup in a fact base nobody can hold is a question
        about retrieval, and a positive delta there has an innocent explanation
        the null is supposed to rule out."""
        with pytest.raises(ValueError, match="stops being a control"):
            fixture.generate(20260825, 1, "at-scale")

    def test_a_single_row_answer_is_allowed_here_and_nowhere_else(self):
        """`generate.validate` refuses a one-row truth as guessable. On a
        control it is the entire point — *which department is carol in?* has one
        row by construction."""
        task = next(t for t in generated(20260825, 3) if t.id.endswith("department-of"))
        assert len(task.truth.rows) == 1
        validate(task)

        from dataclasses import replace

        with pytest.raises(Degenerate, match="guessable"):
            validate(replace(task, engine_expected_to_help=True))


class TestOracle:
    @pytest.mark.parametrize("seed", SEEDS)
    def test_every_department_has_somebody_in_it(self, seed):
        """Round-robin rather than drawn: *how many are in this department?*
        must never answer zero, which is an answer a subject reaches by not
        looking."""
        books = fixture.generate(seed, 3)
        for department in books.departments:
            (count,) = next(iter(truth.headcount(department, books).rows))
            assert int(count) > 0

    @pytest.mark.parametrize("seed", SEEDS)
    def test_the_threshold_splits_the_orders(self, seed):
        """Fixed at 100 against a draw that sits above it, *which orders exceed
        this?* answers with every row and copying a column scores correct."""
        for difficulty in DIFFICULTIES:
            books = fixture.generate(seed, difficulty)
            above = len(truth.orders_above(books=books).rows)
            assert 0 < above < len(books.orders)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_each_lookup_has_exactly_one_answer(self, seed):
        books = fixture.generate(seed, 3)
        assert len(truth.department_of(books.subject_employee, books).rows) == 1
        assert len(truth.customer_of(books.subject_order, books).rows) == 1

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_the_same_seed_gives_the_same_books(self, seed):
        assert fixture.generate(seed, 3) == fixture.generate(seed, 3)

    @pytest.mark.parametrize("seed", SEEDS[:6])
    def test_two_seeds_give_different_identifiers(self, seed):
        """A memorised control calibrates nothing."""
        one = {name for name, _, _ in fixture.generate(seed, 3).employees}
        other = {name for name, _, _ in fixture.generate(seed + 1, 3).employees}
        assert not (one & other)

    def test_an_unknown_difficulty_is_refused(self):
        with pytest.raises(ValueError):
            fixture.generate(20260825, 99)


class TestCheck:
    def test_a_control_that_expects_the_engine_to_help_is_refused(self):
        from dataclasses import replace

        task = generated(20260825, 3)[0]
        with pytest.raises(Degenerate, match="is not a control"):
            check(replace(task, engine_expected_to_help=True))

    def test_a_control_that_grew_a_question_class_is_refused(self):
        from dataclasses import replace

        task = generated(20260825, 3)[0]
        with pytest.raises(Degenerate, match="question class"):
            check(replace(task, question_class="recursion"))

    def test_a_control_with_a_large_fact_base_is_refused(self):
        from dataclasses import replace

        from harness.task import Fixture

        task = generated(20260825, 3)[0]
        swollen = Fixture(
            files={
                "employee.csv": "name,department,salary\n"
                + "".join(f"e{i},d0,{i}\n" for i in range(MAX_ROWS + 5))
            },
            schemas={"employee": ("name", "department", "salary")},
        )
        with pytest.raises(Degenerate, match="in its head"):
            check(replace(task, fixture=swollen))
