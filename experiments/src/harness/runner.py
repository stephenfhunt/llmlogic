"""Driving the grid: build a workspace, run the subject, grade, record.

The same path runs for the stub and the real subject. A cell that blows up is
recorded as an ``ERROR`` verdict rather than aborting the run — a run that dies
on cell 40 of 112 has spent the money and kept none of the evidence.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

from harness import arms, signals
from harness.cell import Cell
from harness.env import SingleShot
from harness.grade import Grade, Verdict, grade
from harness.record import Record, RecordStore
from harness.subject import Subject
from harness.transcript import Transcript


def new_run_id(prefix: str = "run") -> str:
    return f"{prefix}-{datetime.now(UTC).strftime('%Y%m%dT%H%M%SZ')}"


@dataclass
class RunResult:
    run_id: str
    records: list[Record]

    @property
    def cost_usd(self) -> float:
        return sum(record.cost_usd for record in self.records)


def run_cell(
    run_id: str,
    cell: Cell,
    subject: Subject,
    store: RecordStore,
    workspace_root: Path,
) -> Record:
    episode = SingleShot(cell.task)
    try:
        workspace = arms.build(cell, workspace_root)
        transcript = subject.run(cell, workspace)
        result = grade(cell.task, workspace.path)
    except Exception as exc:  # noqa: BLE001 — a failed cell is data, not a crash
        transcript = Transcript(cell_id=cell.id, error=f"{type(exc).__name__}: {exc}")
        result = Grade(Verdict.ERROR, None, None)

    episode.step(result.answer)
    measured = signals.measure(transcript)
    record = Record.build(run_id, cell, transcript, result, measured)
    store.append(record)
    store.save_transcript(transcript)
    return record


def run_grid(
    cells: list[Cell],
    subject: Subject,
    store: RecordStore,
    workspace_root: Path,
    on_cell=None,
) -> RunResult:
    records = []
    for index, cell in enumerate(cells, start=1):
        record = run_cell(store.run_id, cell, subject, store, workspace_root)
        records.append(record)
        if on_cell:
            on_cell(index, len(cells), record)
    return RunResult(run_id=store.run_id, records=records)
