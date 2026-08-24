"""Finishing a run that stopped.

A full grid does not fit in one five-hour window alongside the session driving
it, so resume is the ordinary shape of a run and not only a recovery path.
"""

import json

import pytest

from harness import arms, report, resume
from harness.cell import grid
from harness.domains.controls import tasks as controls_tasks
from harness.record import RecordStore
from harness.runner import run_cell, run_grid
from harness.subject import StubSubject
from harness.transcript import Transcript

pytestmark = pytest.mark.skipif(
    not (arms.DATALOG_BIN_DIR / "datalog").exists(),
    reason="datalog binary not built; the engine arm cannot be materialized",
)


class Halting:
    """Answers ``good`` cells through the stub; errors on everything else."""

    def __init__(self, good: set[str]) -> None:
        self.good = good
        self.seen: list[str] = []

    def run(self, cell, workspace) -> Transcript:
        self.seen.append(cell.id)
        if cell.id in self.good:
            return StubSubject().run(cell, workspace)
        return Transcript(cell_id=cell.id, error="ResultError: session limit")


def _stopped_run(tmp_path, done: int):
    """A run directory holding `done` settled cells and one errored one."""
    cells = grid(controls_tasks.tasks())
    store = RecordStore(tmp_path / "results", "run-test")
    store.write_metadata(
        dry_run=False,
        domains=["controls"],
        tasks=len(controls_tasks.tasks()),
        cells=len(cells),
        strengths=["opus-5", "haiku-4.5"],
        arms=["engine", "prose"],
        ablate=None,
        max_turns=30,
        max_budget_usd=2.0,
    )
    subject = Halting({cell.id for cell in cells[:done]})
    result = run_grid(cells, subject, store, tmp_path / "ws")
    return cells, store, result


def test_a_stopped_run_owes_the_cells_it_never_reached(tmp_path):
    cells, store, result = _stopped_run(tmp_path, done=5)

    assert result.halted
    assert len(result.records) == 6  # five settled, then the one that stopped it

    owed = resume.outstanding(cells, store.dir)
    # The errored cell is owed again, and so is everything after it.
    assert [c.id for c in owed] == [c.id for c in cells[5:]]


def test_settled_excludes_the_errored_cell(tmp_path):
    cells, store, _ = _stopped_run(tmp_path, done=3)
    settled = resume.settled(store.dir)

    assert settled == {cell.id for cell in cells[:3]}


def test_resuming_finishes_the_grid_and_counts_each_cell_once(tmp_path):
    cells, store, _ = _stopped_run(tmp_path, done=5)
    owed = resume.outstanding(cells, store.dir)

    # The same directory, appended to — one run id stays one grid.
    again = RecordStore(tmp_path / "results", "run-test")
    run_grid(owed, StubSubject(), again, tmp_path / "ws")

    attempts = RecordStore.load(store.dir)
    assert len(attempts) > len(cells)  # the failed attempt is still on file
    assert len(report.latest(attempts)) == len(cells)
    assert resume.outstanding(cells, store.dir) == []

    rendered = report.render(store.dir)
    assert f"{len(cells)} cells" in rendered
    assert "0 errored" in rendered
    assert "**Resumed**" in rendered


def test_the_later_attempt_is_the_one_that_counts(tmp_path):
    cells, store, _ = _stopped_run(tmp_path, done=0)
    first = RecordStore.load(store.dir)[0]
    assert first["verdict"] == "error"

    again = RecordStore(tmp_path / "results", "run-test")
    run_cell("run-test", cells[0], StubSubject(), again, tmp_path / "ws")

    counted = {r["cell_id"]: r for r in report.latest(RecordStore.load(store.dir))}
    assert counted[cells[0].id]["verdict"] != "error"


def test_a_dry_run_is_not_resumable(tmp_path):
    store = RecordStore(tmp_path / "results", "dry-test")
    store.write_metadata(dry_run=True, domains=["controls"], cells=16)

    with pytest.raises(resume.NotResumable):
        resume.metadata(store.dir)


def test_a_run_without_metadata_is_not_resumable(tmp_path):
    (tmp_path / "run-empty").mkdir()

    with pytest.raises(resume.NotResumable):
        resume.metadata(tmp_path / "run-empty")


def test_metadata_carries_what_the_grid_needs_rebuilding(tmp_path):
    _, store, _ = _stopped_run(tmp_path, done=2)
    meta = resume.metadata(store.dir)

    # Resuming rebuilds from these, not from the flags given later: a resume with
    # a different slate would join two different experiments.
    for key in ("domains", "strengths", "arms", "cells", "max_turns", "max_budget_usd"):
        assert key in meta
    assert json.loads((store.dir / "run.json").read_text())["dry_run"] is False


def test_a_record_written_before_the_error_verdict_existed_is_still_read_as_failed(tmp_path):
    """The 46 cells of 2026-08-24, as they sit on disk.

    A session limit killed them and the runner of the day filed each as
    ``no-answer`` with the reason in the ``error`` field. Reading the field means
    they are excluded from the denominators and owed again by a resume, without
    a single verdict in ``results/`` being rewritten.
    """
    cells = grid(controls_tasks.tasks())
    store = RecordStore(tmp_path / "results", "run-old")
    store.write_metadata(
        dry_run=False,
        domains=["controls"],
        cells=len(cells),
        strengths=["opus-5", "haiku-4.5"],
        arms=["engine", "prose"],
        max_turns=30,
        max_budget_usd=2.0,
    )
    run_grid(cells[:2], StubSubject(), store, tmp_path / "ws")

    # Written the way the old runner wrote them: graded, not errored.
    stale = json.loads(store.records_path.read_text().splitlines()[0])
    stale.update(
        cell_id=cells[2].id,
        verdict="no-answer",
        error="ResultError: You've hit your session limit",
        cost_usd=0.0,
    )
    with store.records_path.open("a") as handle:
        handle.write(json.dumps(stale) + "\n")

    assert resume.failed(stale)
    assert cells[2].id not in resume.settled(store.dir)
    assert cells[2].id in {c.id for c in resume.outstanding(cells, store.dir)}

    rendered = report.render(store.dir)
    assert "1 errored" in rendered


def test_a_renamed_task_is_caught_even_though_the_count_still_matches(tmp_path):
    """The cell count is not identity. A renamed task keeps the arithmetic and
    changes the question, so the resume compares ids and refuses."""
    cells = grid(controls_tasks.tasks())
    store = RecordStore(tmp_path / "results", "run-test")
    store.write_metadata(dry_run=False, domains=["controls"], cells=len(cells))
    run_grid(cells[:2], StubSubject(), store, tmp_path / "ws")

    assert resume.strangers(cells, store.dir) == set()

    stale = json.loads(store.records_path.read_text().splitlines()[0])
    stale["cell_id"] = "controls.a-task-that-no-longer-exists.engine.opus-5"
    with store.records_path.open("a") as handle:
        handle.write(json.dumps(stale) + "\n")

    assert resume.strangers(cells, store.dir) == {stale["cell_id"]}
