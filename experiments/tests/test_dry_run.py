"""The offline gate: the whole pipeline, no API calls.

If this stops passing, the harness can only be tested by spending money — which
is how a harness stops being tested.
"""

import pytest

from harness import arms, report
from harness.cell import ARMS, STRENGTHS, grid
from harness.domains.controls import tasks as controls_tasks
from harness.record import RecordStore
from harness.runner import run_grid
from harness.subject import StubSubject

pytestmark = pytest.mark.skipif(
    not (arms.DATALOG_BIN_DIR / "datalog").exists(),
    reason="datalog binary not built; the engine arm cannot be materialized",
)


def test_the_grid_crosses_every_task_with_every_arm_and_strength():
    tasks = controls_tasks.tasks()
    cells = grid(tasks)
    assert len(cells) == len(tasks) * len(ARMS) * len(STRENGTHS)
    assert len({cell.id for cell in cells}) == len(cells)


def test_a_full_offline_run_produces_records_and_a_report(tmp_path):
    tasks = controls_tasks.tasks()
    cells = grid(tasks)
    store = RecordStore(tmp_path / "results", "dry-test")
    store.write_metadata(dry_run=True, cells=len(cells))

    result = run_grid(cells, StubSubject(), store, tmp_path / "workspaces")

    assert len(result.records) == len(cells)
    assert store.records_path.exists()
    assert len(RecordStore.load(store.dir)) == len(cells)

    # Nothing errored: a workspace that cannot be built is a broken harness, and
    # it would otherwise be quietly absorbed into the ERROR verdict.
    assert [r for r in result.records if r.verdict == "error"] == []

    rendered = report.render(store.dir)
    assert "Negative controls" in rendered
    assert "Process signals" in rendered


def test_a_dry_run_is_reproducible(tmp_path):
    cells = grid(controls_tasks.tasks())

    first = run_grid(
        cells,
        StubSubject(),
        RecordStore(tmp_path / "a", "dry-test"),
        tmp_path / "ws-a",
    )
    second = run_grid(
        cells,
        StubSubject(),
        RecordStore(tmp_path / "b", "dry-test"),
        tmp_path / "ws-b",
    )

    # Seeded from the cell id, so a diff between two dry runs means a real change
    # rather than noise.
    assert [r.verdict for r in first.records] == [r.verdict for r in second.records]


def test_each_arm_answers_every_task(tmp_path):
    cells = grid(controls_tasks.tasks())
    store = RecordStore(tmp_path / "results", "dry-test")
    result = run_grid(cells, StubSubject(), store, tmp_path / "workspaces")

    for arm in ARMS:
        answered = {r.task_id for r in result.records if r.arm == arm}
        assert answered == {task.id for task in controls_tasks.tasks()}
