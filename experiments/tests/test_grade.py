"""Verdicts. WRONG and UNPARSEABLE are kept apart on purpose."""

from harness import catalogue, domains
from harness.domains.controls import tasks as controls_tasks
from harness.grade import (
    ANSWER_FILE,
    EMPTY_FIELD,
    PROSE,
    WRONG_ARITY,
    Verdict,
    grade,
)

TASK = {task.id: task for task in controls_tasks.tasks()}


def test_correct_answer(tmp_path):
    task = TASK["department-of"]
    (tmp_path / ANSWER_FILE).write_text("engineering\n")
    result = grade(task, tmp_path)
    assert result.verdict is Verdict.CORRECT
    assert result.correct
    assert not result.silently_wrong


def test_missing_file_is_no_answer(tmp_path):
    result = grade(TASK["department-of"], tmp_path)
    assert result.verdict is Verdict.NO_ANSWER
    assert result.answer is None


def test_dropped_rows_are_wrong_and_silently_so(tmp_path):
    # The failure mode that survives rounds of feedback: a strict subset of the
    # truth, where nothing in the output announces the missing rows.
    task = TASK["orders-above-100"]
    (tmp_path / ANSWER_FILE).write_text("o2\no4\n")  # o5 dropped
    result = grade(task, tmp_path)
    assert result.verdict is Verdict.WRONG
    assert result.missing == 1
    assert result.extra == 0
    assert result.silently_wrong


def test_over_derivation_is_wrong_with_extras(tmp_path):
    task = TASK["orders-above-100"]
    (tmp_path / ANSWER_FILE).write_text("o2\no4\no5\no3\n")
    result = grade(task, tmp_path)
    assert result.verdict is Verdict.WRONG
    assert result.missing == 0
    assert result.extra == 1


def test_prose_in_the_answer_file_is_unparseable_not_wrong(tmp_path):
    # A subject that reasoned correctly and formatted badly did not get the
    # question wrong. The arity check is what separates the two.
    task = TASK["orders-above-100"]
    (tmp_path / ANSWER_FILE).write_text("The orders over 100 are o2, o4 and o5.\n")
    result = grade(task, tmp_path)
    assert result.verdict is Verdict.UNPARSEABLE


def test_empty_answer_file_grades_against_truth_rather_than_failing(tmp_path):
    task = TASK["orders-above-100"]
    (tmp_path / ANSWER_FILE).write_text("")
    result = grade(task, tmp_path)
    assert result.verdict is Verdict.WRONG  # the truth is non-empty here
    assert result.missing == 3


class TestWhyItWouldNotParse:
    """Three causes, one verdict — and only one of them is the subject's.

    Measured across `results/` before the format bullet was split by arity:
    15.4% of single-column cells graded `unparseable` against 1.9% of
    two-column ones, and 144 of the 145 single-column ones had more `|`-fields
    on a line than the question has columns. Grading stays strict; the reason
    is what makes the difference visible.
    """

    def test_a_single_column_answer_joined_with_pipes_is_wrong_arity(self, tmp_path):
        task = next(t for t in domains.load("controls") if t.id == "orders-above-100")
        (tmp_path / ANSWER_FILE).write_text("o2|o4|o5\n")

        result = grade(task, tmp_path)

        assert result.verdict is Verdict.UNPARSEABLE
        assert result.reason == WRONG_ARITY
        # Not silently rescued into a right answer: flattening it would have
        # been the grader choosing a reading the answer did not commit to.
        assert result.answer is None

    def test_a_sentence_is_prose_not_arity(self, tmp_path):
        task = next(t for t in domains.load("controls") if t.id == "orders-above-100")
        (tmp_path / ANSWER_FILE).write_text("The orders over 100 are o2, o4 and o5.\n")

        result = grade(task, tmp_path)

        assert result.verdict is Verdict.UNPARSEABLE
        assert result.reason == PROSE

    def test_a_missing_field_is_its_own_reason(self, tmp_path):
        task = next(t for t in domains.load("scheduling") if len(t.answer_shape) == 2)
        (tmp_path / ANSWER_FILE).write_text("alice|\n")

        result = grade(task, tmp_path)

        assert result.verdict is Verdict.UNPARSEABLE
        assert result.reason == EMPTY_FIELD

    def test_a_graded_answer_carries_no_reason(self, tmp_path):
        task = next(t for t in domains.load("controls") if t.id == "orders-above-100")
        (tmp_path / ANSWER_FILE).write_text("\n".join(row[0] for row in task.truth.rows) + "\n")

        result = grade(task, tmp_path)

        assert result.verdict is Verdict.CORRECT
        assert result.reason is None


class TestTheFormatBulletMatchesTheArity:
    def test_a_single_column_question_is_told_not_to_join(self):
        task = next(t for t in domains.load("controls") if t.id == "orders-above-100")
        line = catalogue._fields_line(task)
        assert "do not join values" in line
        assert "separated by" not in line

    def test_a_multi_column_question_still_names_the_separator(self):
        task = next(t for t in domains.load("scheduling") if len(t.answer_shape) == 2)
        line = catalogue._fields_line(task)
        assert "separated by `|`" in line
