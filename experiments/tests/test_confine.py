"""The containment gate.

Every case here was reachable before the gate existed. The first two are the ones
that matter most, and both were verified by hand against the real layout: from a
workspace inside the checkout, the answer key and the engine binary were two and
three directories up respectively.
"""

from pathlib import Path

import pytest

from harness import arms
from harness.cell import OPUS_5, Cell
from harness.confine import violation
from harness.domains.controls import tasks as controls_tasks


def _workspace(tmp_path: Path) -> Path:
    workspace = tmp_path / "cell"
    (workspace / "bin").mkdir(parents=True)
    (workspace / "member.csv").write_text("user,group\nu01,eng\n")
    return workspace


def test_reading_the_answer_key_is_denied(tmp_path):
    # `truth.py` holds the expected answers for the domain being measured. A
    # subject that reads it scores perfectly and the run means nothing.
    workspace = _workspace(tmp_path)
    answer_key = tmp_path / "src" / "harness" / "domains" / "access_control"
    answer_key.mkdir(parents=True)
    (answer_key / "truth.py").write_text("# the answers\n")

    reason = violation("Read", {"file_path": str(answer_key / "truth.py")}, workspace)
    assert reason is not None
    assert "outside" in reason


def test_climbing_out_with_dot_dot_is_denied(tmp_path):
    workspace = _workspace(tmp_path)
    assert violation("Read", {"file_path": "../../src/harness/domains"}, workspace)
    assert violation("Bash", {"command": "cat ../../src/harness/domains/x/truth.py"}, workspace)


def test_running_the_engine_by_absolute_path_is_denied(tmp_path):
    # This is what makes PATH-scrubbing insufficient on its own: scrubbing does
    # nothing against /abs/path/to/datalog, and a prose arm that runs the engine
    # erases the only difference between the arms.
    workspace = _workspace(tmp_path)
    engine = tmp_path / "datalog" / "target" / "release"
    engine.mkdir(parents=True)
    (engine / "datalog").write_text("#!/bin/sh\n")

    reason = violation("Bash", {"command": f"{engine / 'datalog'} facts.dl"}, workspace)
    assert reason is not None


def test_ordinary_work_inside_the_workspace_is_allowed(tmp_path):
    workspace = _workspace(tmp_path)
    assert violation("Read", {"file_path": str(workspace / "member.csv")}, workspace) is None
    assert violation("Write", {"file_path": str(workspace / "answer.txt")}, workspace) is None
    assert violation("Bash", {"command": "cat member.csv | sort"}, workspace) is None
    assert violation("Bash", {"command": "datalog q.dl -q 'p(X)'"}, workspace) is None


def test_the_engine_arm_may_run_its_own_linked_binary(tmp_path):
    # The binary lives inside the workspace precisely so containment and the
    # engine arm are not in conflict.
    workspace = _workspace(tmp_path)
    (workspace / "bin" / "datalog").write_text("#!/bin/sh\n")
    command = f"{workspace / 'bin' / 'datalog'} facts.dl"
    assert violation("Bash", {"command": command}, workspace) is None


def test_system_tools_are_not_mistaken_for_escapes(tmp_path):
    # The subject needs a shell. Denying /usr/bin/python3 would make the prose
    # arm's honest alternative — writing a script — impossible, which would bias
    # the comparison toward the engine.
    workspace = _workspace(tmp_path)
    assert violation("Bash", {"command": "/usr/bin/python3 solve.py"}, workspace) is None
    assert violation("Bash", {"command": "/bin/ls -la"}, workspace) is None


def test_a_relative_path_is_resolved_against_the_workspace(tmp_path):
    workspace = _workspace(tmp_path)
    assert violation("Read", {"file_path": "member.csv"}, workspace) is None


def test_a_path_that_does_not_exist_is_not_flagged(tmp_path):
    # The subject writing a new file it has not created yet is ordinary.
    workspace = _workspace(tmp_path)
    assert violation("Write", {"file_path": str(workspace / "q.dl")}, workspace) is None


# ---- A cell starts from an empty directory -----------------------------------
#
# Found on the first pilot (2026-08-23), and it is a grading failure rather than
# an untidiness: `--dry-run` and a paid run derive the same directory from the
# same cell id, so the stub's `answer.txt` was still there when the real subject
# arrived. Two engine cells were graded on it.


def test_a_rebuilt_workspace_has_no_trace_of_the_last_occupant(tmp_path):
    if not (arms.DATALOG_BIN_DIR / "datalog").exists():
        pytest.skip("datalog binary not built")
    task = controls_tasks.tasks()[0]
    cell = Cell(task, "engine", OPUS_5)

    first = arms.build(cell, tmp_path)
    (first.path / "answer.txt").write_text("wrong|answer\n")
    (first.path / "q.dl").write_text("stale(1).\n")

    second = arms.build(cell, tmp_path)
    assert second.path == first.path, "the same cell should reuse the same name"
    assert not (second.path / "answer.txt").exists(), (
        "a stale answer file is what the next subject gets graded on"
    )
    assert not (second.path / "q.dl").exists()
    # And the workspace is still whole.
    assert (second.path / "employee.csv").exists()
    assert (second.path / "bin" / "datalog").exists()
    assert (second.path / ".claude" / "skills" / "datalog" / "SKILL.md").exists()


def test_the_dry_run_and_a_paid_run_cannot_hand_each_other_an_answer(tmp_path):
    # The exact shape of the pilot's contamination: the same cell id, built
    # twice, with a stub answer left in between.
    task = controls_tasks.tasks()[0]
    cell = Cell(task, "prose", OPUS_5)
    stub = arms.build(cell, tmp_path)
    (stub.path / "answer.txt").write_text("engineering\n")  # what the stub writes
    live = arms.build(cell, tmp_path)
    assert not (live.path / "answer.txt").exists()


def test_clearing_refuses_a_path_that_is_not_a_cell_workspace(tmp_path):
    # The guard on an rmtree whose only safety is that the name came from a hash.
    (tmp_path / "not-a-hash").mkdir()
    with pytest.raises(ValueError, match="not a cell workspace name"):
        arms._clear(tmp_path / "not-a-hash", tmp_path)
    with pytest.raises(ValueError, match="not inside"):
        arms._clear(tmp_path, tmp_path)
