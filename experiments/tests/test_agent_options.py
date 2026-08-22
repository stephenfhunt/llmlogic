"""The subject's configuration, checked without spending anything.

These are contamination controls. Each failure here is a run that measures the
operator's machine instead of the engine, and reports a number anyway.
"""

import os
from pathlib import Path

import pytest

from harness import arms
from harness.agent import AgentSubject
from harness.cell import OPUS_5, Cell
from harness.domains.controls import tasks as controls_tasks

pytestmark = pytest.mark.skipif(
    not (arms.DATALOG_BIN_DIR / "datalog").exists(),
    reason="datalog binary not built; the engine arm cannot be materialized",
)

TASK = controls_tasks.tasks()[0]


def _options(arm, tmp_path):
    workspace = arms.build(Cell(TASK, arm, OPUS_5), tmp_path)
    return AgentSubject().options(Cell(TASK, arm, OPUS_5), workspace, hooks={})


def test_neither_arm_reads_the_operators_own_settings(tmp_path):
    # "user" would load whatever skills the operator has installed globally —
    # including, plausibly, this very engine — into *both* arms.
    for arm in ("engine", "prose"):
        options = _options(arm, tmp_path)
        assert "user" not in (options.setting_sources or [])


def test_only_the_engine_arm_is_given_the_skill(tmp_path):
    engine = _options("engine", tmp_path)
    assert engine.setting_sources == ["project"]
    assert engine.skills == ["datalog"]

    prose = _options("prose", tmp_path)
    assert prose.setting_sources == []
    assert prose.skills == []


def test_the_prose_arm_cannot_find_an_installed_engine_on_path(tmp_path):
    prose = _options("prose", tmp_path)
    for entry in prose.env["PATH"].split(os.pathsep):
        assert not (Path(entry) / "datalog").exists()


def test_the_engine_arm_finds_the_engine_inside_its_own_workspace(tmp_path):
    # The binary is materialized *in* the workspace rather than referenced where
    # it was built, so a sandbox confined to the workspace can still run it — and
    # nothing legitimate needs to reach into the checkout.
    workspace = arms.build(Cell(TASK, "engine", OPUS_5), tmp_path)
    first = Path(workspace.env["PATH"].split(os.pathsep)[0])
    assert first == workspace.path / "bin"
    assert (first / "datalog").exists()


def test_the_engine_binary_is_linked_when_the_filesystem_allows_it(tmp_path):
    # 50 MB per engine-arm cell would be gigabytes across a grid, so this is a
    # hardlink on the real workspace root. Under pytest's tmp_path — often a
    # different filesystem — it falls back to a copy, which is the point of the
    # fallback; assert whichever applies rather than assuming one.
    workspace = arms.build(Cell(TASK, "engine", OPUS_5), tmp_path)
    linked = (workspace.path / "bin" / "datalog").stat()
    source = (arms.DATALOG_BIN_DIR / "datalog").stat()
    if linked.st_dev == source.st_dev:
        assert linked.st_ino == source.st_ino, "same filesystem but not hardlinked"
    else:
        assert linked.st_size == source.st_size


def test_every_cell_runs_under_the_os_sandbox(tmp_path):
    for arm in ("engine", "prose"):
        options = _options(arm, tmp_path)
        assert options.sandbox is not None
        assert options.sandbox["enabled"] is True
        # Closes the `dangerouslyDisableSandbox` escape.
        assert options.sandbox["allowUnsandboxedCommands"] is False


def test_the_sandbox_denies_the_network(tmp_path):
    # A subject that can reach the network can look up an answer. This is a
    # validity control before it is a security one.
    options = _options("engine", tmp_path)
    assert options.sandbox["network"]["allowedDomains"] == []
    assert options.sandbox["network"]["allowLocalBinding"] is False


def test_both_arms_get_the_same_tools(tmp_path):
    # The independent variable is the engine, not the tooling. The prose arm keeps
    # bash and search, and is free to write a script.
    engine = _options("engine", tmp_path)
    prose = _options("prose", tmp_path)
    assert engine.allowed_tools == prose.allowed_tools
    assert "Bash" in prose.allowed_tools
    assert "Grep" in prose.allowed_tools


def test_neither_arm_can_reach_the_web(tmp_path):
    # A subject that can look up the answer is not answering the question.
    for arm in ("engine", "prose"):
        options = _options(arm, tmp_path)
        assert "WebSearch" in options.disallowed_tools
        assert "WebFetch" in options.disallowed_tools


def test_every_cell_carries_a_hard_cost_ceiling(tmp_path):
    options = _options("engine", tmp_path)
    assert options.max_budget_usd is not None
    assert options.max_turns is not None


def test_the_model_is_the_cells_strength(tmp_path):
    options = _options("engine", tmp_path)
    assert options.model == OPUS_5.model == "claude-opus-5"


def test_the_workspace_path_does_not_reveal_the_arm(tmp_path):
    # A subject that runs `pwd` must not learn which arm of an experiment it is
    # in. Named after the cell, the directory would say so outright.
    engine = arms.build(Cell(TASK, "engine", OPUS_5), tmp_path)
    prose = arms.build(Cell(TASK, "prose", OPUS_5), tmp_path)
    for workspace in (engine, prose):
        name = workspace.path.name
        for leak in ("engine", "prose", "opus", "haiku", TASK.id, TASK.domain):
            assert leak not in name, f"{name} leaks {leak!r} to anything that runs pwd"
    assert engine.path != prose.path


def test_the_subject_is_told_where_it_is(tmp_path):
    # Otherwise it guesses: five wasted turns and five denials on the first real
    # cell before it thought to ask.
    workspace = arms.build(Cell(TASK, "engine", OPUS_5), tmp_path)
    options = AgentSubject().options(Cell(TASK, "engine", OPUS_5), workspace, hooks={})
    assert str(workspace.path) in options.system_prompt
