"""Verdicts. WRONG and UNPARSEABLE are kept apart on purpose."""

from harness.domains.controls import tasks as controls_tasks
from harness.grade import ANSWER_FILE, Verdict, grade

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
