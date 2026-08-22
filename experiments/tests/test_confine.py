"""The containment gate.

Every case here was reachable before the gate existed. The first two are the ones
that matter most, and both were verified by hand against the real layout: from a
workspace inside the checkout, the answer key and the engine binary were two and
three directories up respectively.
"""

from pathlib import Path

from harness.confine import violation


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
