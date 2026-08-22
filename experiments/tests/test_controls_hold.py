"""The four controls, enforced rather than documented.

Each of these failing means a full grid of plausible verdicts about a question
nobody asked. They are cheap; the failure they prevent is silent.
"""

import pytest

from harness import arms, domains
from harness.catalogue import SchemaDrift, assemble, verify
from harness.cell import FIXTURE_TOKEN_BUDGET, HAIKU_4_5, OPUS_5, Cell
from harness.domains.controls import fixture as controls_fixture
from harness.domains.controls import tasks as controls_tasks
from harness.task import Fixture

# ---- Control 2: the catalogue is derived, and drift fails loudly ------------


def test_schema_drift_is_caught():
    good = controls_fixture.build()
    verify(good)  # baseline: the real fixture is consistent

    drifted = Fixture(
        files={
            **good.files,
            "employee.csv": good.files["employee.csv"].replace(
                "name,department,salary", "person,department,salary"
            ),
        },
        schemas=good.schemas,
    )
    with pytest.raises(SchemaDrift):
        verify(drifted)


def test_a_relation_with_no_schema_is_caught():
    good = controls_fixture.build()
    extra = Fixture(
        files={**good.files, "manager.csv": "name,reports_to\nalice,carol\n"},
        schemas=good.schemas,
    )
    with pytest.raises(SchemaDrift):
        verify(extra)


def test_catalogue_lists_every_relation_with_its_real_columns():
    task = controls_tasks.tasks()[0]
    prompt = assemble(task)
    assert "`employee.csv`" in prompt
    assert "name, department, salary" in prompt
    assert "`order.csv`" in prompt
    assert "id, customer, amount" in prompt
    assert "(6 rows)" in prompt  # the count comes from the data, not from prose


# ---- Control 3: the prompt never names the engine, and is arm-identical -----


def test_the_prompt_never_mentions_the_engine():
    for task in domains.load_all():
        prompt = assemble(task).lower()
        for word in ("datalog", "logic engine", "prolog", "skill", "solver"):
            assert word not in prompt, (
                f"{task.key} names {word!r}. The engine arm has to reach for the "
                "engine unprompted, or 'did it reach for it?' is not a measurement."
            )


def test_both_arms_get_a_byte_identical_prompt(tmp_path):
    if not (arms.DATALOG_BIN_DIR / "datalog").exists():
        pytest.skip("datalog binary not built")
    task = controls_tasks.tasks()[0]
    engine = arms.build(Cell(task, "engine", OPUS_5), tmp_path)
    prose = arms.build(Cell(task, "prose", OPUS_5), tmp_path)
    assert engine.prompt == prose.prompt


def test_only_the_engine_arm_gets_the_engine(tmp_path):
    if not (arms.DATALOG_BIN_DIR / "datalog").exists():
        pytest.skip("datalog binary not built")
    task = controls_tasks.tasks()[0]
    engine = arms.build(Cell(task, "engine", OPUS_5), tmp_path)
    prose = arms.build(Cell(task, "prose", HAIKU_4_5), tmp_path)

    assert engine.has_engine
    assert (engine.path / ".claude" / "skills" / "datalog" / "SKILL.md").exists()

    assert not prose.has_engine
    assert not (prose.path / ".claude").exists()

    # Same fixture files in both, so the fact base is not a variable.
    engine_files = sorted(p.name for p in engine.path.glob("*.csv"))
    prose_files = sorted(p.name for p in prose.path.glob("*.csv"))
    assert engine_files == prose_files == ["employee.csv", "order.csv"]


# ---- The slate ---------------------------------------------------------------


def test_negative_controls_are_marked_as_such():
    # If these ever get folded into the measured slate, a null result stops being
    # readable: four questions the engine was never expected to help with would
    # move the number that decides v1.
    for task in controls_tasks.tasks():
        assert task.engine_expected_to_help is False
        assert task.question_class in ("single-hop", "one-step")


def test_every_task_has_truth_matching_its_declared_shape():
    for task in domains.load_all():
        arity = len(task.answer_shape)
        for row in task.truth.rows:
            assert len(row) == arity, (
                f"{task.key} declares shape {task.answer_shape} but truth has {row}"
            )


def test_no_task_ships_with_an_empty_truth():
    # Hit twice while building `access_control`: a task whose answer is "none" is
    # one a subject passes by writing an empty file without looking. An empty
    # answer is a legitimate *result* — it is not a legitimate *fixture*.
    for task in domains.load_all():
        assert task.truth.rows, (
            f"{task.key} has an empty truth, so doing nothing scores correct. "
            "Change the fixture, not the grader."
        )


def test_no_fixture_defeats_the_prose_arm_on_size_alone():
    # The engine arm keeps the fact base on disk; the prose arm has to hold it in
    # context. A fixture over the budget therefore measures the context window,
    # and the delta it produces is not a reasoning delta. Binary copies are not
    # counted: nothing reads a Parquet file into a prompt.
    for task in domains.load_all():
        chars = sum(
            len(contents) for contents in task.fixture.files.values() if isinstance(contents, str)
        )
        estimated = chars // 4
        assert estimated < FIXTURE_TOKEN_BUDGET, (
            f"{task.key}'s fixture is ~{estimated} tokens, over the "
            f"{FIXTURE_TOKEN_BUDGET} budget in cell.py"
        )


def test_no_task_is_answered_by_naming_everything():
    # The mirror failure: if the answer is the whole universe, copying the input
    # column scores correct.
    for task in domains.load_all():
        if len(task.answer_shape) != 1:
            continue
        universe = set()
        for contents in task.fixture.files.values():
            # Text only: a Parquet copy carries the same values as the text one it
            # mirrors, and a source tree is not a set of values at all.
            if not isinstance(contents, str):
                continue
            lines = [ln for ln in contents.splitlines() if ln.strip()]
            for line in lines[1:]:
                universe.update(field.strip() for field in line.split(","))
        answered = {row[0] for row in task.truth.rows}
        assert answered != universe, (
            f"{task.key}'s answer is every value in the fixture — copying a column "
            "would score correct."
        )
