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

import hashlib
import json
from pathlib import Path

from harness.cell import Cell
from harness.record import RecordStore
from harness.runner import FATAL
from harness.task import Task


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

    Both halves are load-bearing.

    The **verdict** is the rule the runner applies now, and for a record it wrote
    it is the whole answer: an infrastructure failure is ``ERROR``, while a cell
    stopped by the harness's own turn cap keeps the verdict it earned and carries
    its error text alongside.

    The **error text** catches records written *before* that rule existed — the
    46 cells of 2026-08-24 that a session limit killed and the old runner filed
    as ``no-answer``. Matching `FATAL` rather than any error is what keeps a
    turn-cap cell out of this: it did measure something.

    Reading it this way means both are read correctly with nothing in
    ``results/`` rewritten.
    """
    return record["verdict"] == "error" or bool(
        record.get("error") and FATAL.search(record["error"])
    )


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


def fingerprint(task: Task) -> str:
    """A hash of the bytes a task's fixture actually puts in a workspace.

    The cell-id and count checks catch a task that was **renamed or added**. They
    are blind to the thing that actually moved on 2026-08-24: `scheduling`'s
    roster changed under the same four task ids, and 21 of its 29 assignments had
    been illegal all along. Nothing owed by the open run was a `scheduling` cell,
    so it did not bite — a resume that had owed one would have joined two
    different experiments with no signal at all.

    Generators make that the ordinary case rather than the unlucky one: a fixture
    now moves whenever a seed or a difficulty setting does.

    The question text and the truth are hashed too. A fixture can stay
    byte-identical while the question asked of it changes, and that is the same
    defect wearing different clothes.
    """
    digest = hashlib.sha256()
    digest.update(task.key.encode())
    digest.update(b"\0")
    digest.update(task.question.encode())
    digest.update(b"\0")
    for filename in sorted(task.fixture.files):
        contents = task.fixture.files[filename]
        digest.update(filename.encode())
        digest.update(b"\0")
        digest.update(contents if isinstance(contents, bytes) else contents.encode())
        digest.update(b"\0")
    for row in sorted(task.truth.rows):
        digest.update("\x1f".join(row).encode())
        digest.update(b"\0")
    return digest.hexdigest()[:16]


def fingerprints(tasks: list[Task]) -> dict[str, str]:
    return {task.key: fingerprint(task) for task in tasks}


def moved(tasks: list[Task], meta: dict) -> dict[str, tuple[str, str]]:
    """Tasks whose fixture, question or truth differs from what the run recorded.

    Returns ``key -> (recorded, now)``. A run made before fingerprints existed
    records none, and gets none back: the guard cannot speak about evidence it
    does not have, and inventing an answer there would be the defect it exists to
    prevent.
    """
    recorded = meta.get("fingerprints") or {}
    if not recorded:
        return {}
    current = fingerprints(tasks)
    return {
        key: (recorded[key], current[key])
        for key in sorted(set(recorded) & set(current))
        if recorded[key] != current[key]
    }
