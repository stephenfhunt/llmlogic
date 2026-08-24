"""Picking a stopped run back up.

A full grid does not fit in one of the account's five-hour windows alongside the
session driving it, so *session-by-session* is the ordinary shape of a run rather
than a recovery path. Resume re-runs the cells a run is **missing** and the ones
it recorded as ``ERROR``, and appends them to the same run directory: one run id
stays one grid.

Nothing here edits a past record. ``records.jsonl`` keeps every attempt in the
order it happened — including the errored one — and ``report`` reads the last
record for each cell id. That is what keeps ``results/`` append-only while still
letting a grid be finished.
"""

from __future__ import annotations

import json
from pathlib import Path

from harness.cell import Cell
from harness.record import RecordStore


class NotResumable(Exception):
    """The run directory is missing what a resume needs to rebuild its grid."""


def metadata(run_dir: Path) -> dict:
    path = run_dir / "run.json"
    if not path.exists():
        raise NotResumable(f"no run.json in {run_dir}")
    meta = json.loads(path.read_text())
    if meta.get("dry_run"):
        raise NotResumable("that run was a --dry-run; there is nothing to resume")
    return meta


def failed(record: dict) -> bool:
    """Did this record come from a cell whose subject failed?

    Both halves are load-bearing. The verdict is the rule the runner applies now;
    the ``error`` field catches records written *before* it did — the 46 cells of
    2026-08-24 that a session limit killed and the old runner filed as
    ``no-answer``. Reading the field means those records are read correctly
    without anything in ``results/`` being rewritten.
    """
    return record["verdict"] == "error" or bool(record.get("error"))


def settled(run_dir: Path) -> set[str]:
    """Cell ids that produced a verdict worth keeping.

    A failed cell is deliberately *not* settled: it is the cell that did not run.
    Its record stays in the file as the evidence of what stopped the run.
    """
    return {r["cell_id"] for r in RecordStore.load(run_dir) if not failed(r)}


def outstanding(cells: list[Cell], run_dir: Path) -> list[Cell]:
    """The cells of this grid still owed a verdict, in grid order."""
    done = settled(run_dir)
    return [cell for cell in cells if cell.id not in done]


def strangers(cells: list[Cell], run_dir: Path) -> set[str]:
    """Recorded cell ids the rebuilt grid does not contain.

    The count matching is not enough: a renamed task keeps the arithmetic and
    changes the question. Any recorded id the grid cannot account for means the
    slate moved under the run, and the two halves would not be one experiment.
    """
    known = {cell.id for cell in cells}
    return {r["cell_id"] for r in RecordStore.load(run_dir)} - known
