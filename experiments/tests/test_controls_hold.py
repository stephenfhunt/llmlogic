"""The four controls, enforced rather than documented.

Each of these failing means a full grid of plausible verdicts about a question
nobody asked. They are cheap; the failure they prevent is silent.
"""

import pytest

from harness import ablate, arms, catalogue, domains
from harness.catalogue import MANDATE, SchemaDrift, assemble, verify
from harness.cell import ENGINE_ARMS, FIXTURE_TOKEN_BUDGET, HAIKU_4_5, OPUS_5, Cell
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
    """On the two arms control 3 governs. `engine-forced` is the deliberate
    exception and is checked separately, below."""
    for task in domains.load_all():
        for arm in ("prose", "engine"):
            prompt = assemble(task, arm).lower()
            for word in ("datalog", "logic engine", "prolog", "skill", "solver"):
                assert word not in prompt, (
                    f"{task.key} names {word!r} on the {arm} arm. That arm has to "
                    "reach for the engine unprompted, or 'did it reach for it?' is "
                    "not a measurement."
                )


def test_the_two_unprompted_arms_get_a_byte_identical_prompt(tmp_path):
    if not (arms.DATALOG_BIN_DIR / "datalog").exists():
        pytest.skip("datalog binary not built")
    task = controls_tasks.tasks()[0]
    engine = arms.build(Cell(task, "engine", OPUS_5), tmp_path)
    prose = arms.build(Cell(task, "prose", OPUS_5), tmp_path)
    assert engine.prompt == prose.prompt


def test_engine_forced_differs_from_the_base_prompt_by_the_mandate_alone():
    """The arm is only interpretable if the mandate is the *whole* difference.
    Any other drift between the texts confounds the comparison it exists for."""
    for task in domains.load_all():
        base = assemble(task, "prose")
        forced = assemble(task, "engine-forced")
        assert forced == base + MANDATE
        assert forced.startswith(base)


def test_engine_forced_actually_names_the_engine():
    """The mirror of control 3, and worth pinning: an arm that mandates the
    engine without naming it is just the `engine` arm with extra words."""
    prompt = assemble(controls_tasks.tasks()[0], "engine-forced").lower()
    assert "datalog" in prompt


def test_engine_briefed_differs_from_engine_forced_by_the_briefing_alone(tmp_path):
    """The pair is the measurement: `briefed − forced` is what discovery costs,
    and it only means that while the briefing is the whole difference."""
    for task in domains.load_all():
        forced = assemble(task, "engine-forced")
        briefed = assemble(task, "engine-briefed", arms.briefing())
        assert briefed.startswith(forced)
        assert briefed == forced + catalogue.BRIEFING_HEADER + arms.briefing()


def test_provenance_arm_differs_from_briefed_by_the_mandate_alone():
    """The primary endpoint of the provenance run is `provenance − briefed`, and
    it only means what it says while this block is the whole difference.

    Note the *order*: the mandate goes after the briefing, not before it, so
    `engine-briefed` stays a strict prefix. Inserted anywhere else the briefed
    arm's own prompt would change and the pair would stop being comparable —
    which would silently invalidate the run's control arm, not just this one."""
    for task in domains.load_all():
        briefed = assemble(task, "engine-briefed", arms.briefing())
        provenance = assemble(task, "engine-briefed-provenance", arms.briefing())
        assert provenance.startswith(briefed)
        assert provenance == briefed + catalogue.PROVENANCE_MANDATE


def test_the_provenance_mandate_names_both_sigils_and_how_to_run_them():
    """The mirror of the block's own reason for existing. `SKILL.md` already
    *documents* provenance and produced zero invocations in 1,812 transcripts,
    so a block that only described it again would re-run a settled question.
    It has to name the call."""
    prompt = assemble(controls_tasks.tasks()[0], "engine-briefed-provenance", arms.briefing())
    mandate = catalogue.PROVENANCE_MANDATE
    assert "?whynot" in mandate
    assert "?why " in mandate
    assert "datalog" in mandate  # the invocation, not just the sigil
    assert mandate in prompt


def test_the_briefing_is_the_skill_the_workspace_carries(tmp_path):
    """Not a paraphrase of it, and not the checkout's copy with its markers.

    A briefing that drifted from the skill would make `briefed − forced` a
    comparison between two documents rather than between reading one and having
    to find it."""
    if not (arms.DATALOG_BIN_DIR / "datalog").exists():
        pytest.skip("datalog binary not built")
    workspace = arms.build(Cell(controls_tasks.tasks()[0], "engine-briefed", OPUS_5), tmp_path)
    copied = (workspace.path / ".claude" / "skills" / "datalog" / "SKILL.md").read_text()
    assert arms.briefing() == copied
    assert copied in workspace.prompt
    assert "<!-- block" not in workspace.prompt


def test_an_ablated_briefing_is_cut_like_the_skill_copy(tmp_path):
    """Otherwise the briefed arm reads in its prompt the paragraph its own skill
    copy had cut, and the ablation measures nothing."""
    if not (arms.DATALOG_BIN_DIR / "datalog").exists():
        pytest.skip("datalog binary not built")
    block = next(iter(ablate.catalogue(arms.DATALOG_SKILL_DIR)))
    plain = arms.briefing()
    cut = arms.briefing(block)
    assert len(cut) < len(plain)

    cell = Cell(controls_tasks.tasks()[0], "engine-briefed", OPUS_5, ablate=block)
    workspace = arms.build(cell, tmp_path)
    copied = (workspace.path / ".claude" / "skills" / "datalog" / "SKILL.md").read_text()
    assert cut == copied
    assert cut in workspace.prompt


def test_a_block_that_is_not_in_the_skill_refuses_the_briefing():
    with pytest.raises(ablate.UnknownBlock, match="no block named"):
        arms.briefing("not-a-block")


def test_a_briefed_arm_without_a_briefing_is_refused():
    """It would be an `engine-forced` cell recorded under the briefed arm's
    name — the arm's whole content, silently absent."""
    task = controls_tasks.tasks()[0]
    with pytest.raises(ValueError, match="none was supplied"):
        assemble(task, "engine-briefed")
    with pytest.raises(ValueError, match="only engine-briefed"):
        assemble(task, "engine-forced", "some documentation")


def test_the_briefing_costs_a_readable_share_of_the_window():
    """It is bounded, and the bound is the reason the arm is affordable at all:
    a compliant `engine-forced` cell pays about this much for the `Skill` call
    it should have made, so briefing mostly moves *when* the tokens are spent."""
    approximate_tokens = len(arms.briefing()) // 4
    assert approximate_tokens < 5_000, approximate_tokens


def test_every_engine_arm_gets_the_engine(tmp_path):
    if not (arms.DATALOG_BIN_DIR / "datalog").exists():
        pytest.skip("datalog binary not built")
    task = controls_tasks.tasks()[0]
    for arm in ENGINE_ARMS:
        assert arms.build(Cell(task, arm, OPUS_5), tmp_path).has_engine, arm
    assert not arms.build(Cell(task, "prose", OPUS_5), tmp_path).has_engine


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


def test_no_in_context_fixture_defeats_the_prose_arm_on_size_alone():
    # The engine arm keeps the fact base on disk; the prose arm has to hold it in
    # context. A fixture over the budget therefore measures the context window,
    # and the delta it produces is not a reasoning delta. Binary copies are not
    # counted: nothing reads a Parquet file into a prompt.
    #
    # The budget is the **definition of the `in-context` track**, not a global
    # rule (`decisions.md` 2026-08-25). An `at-scale` task exceeds it on purpose,
    # answers a different question, and is reported in its own table.
    for task in domains.load_all():
        if task.track != "in-context":
            continue
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
