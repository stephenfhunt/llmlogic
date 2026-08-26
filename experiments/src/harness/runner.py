"""Driving the grid: build a workspace, run the subject, grade, record.

The same path runs for the stub and the real subject. A cell that blows up is
recorded as an ``ERROR`` verdict rather than aborting the run — a run that dies
on cell 40 of 112 has spent the money and kept none of the evidence.

Two failures are *not* per-cell data, and both were learned the hard way on
2026-08-24:

- **A subject that reported an error did not answer the question.** Its workspace
  has no answer file, and grading it produces ``NO_ANSWER`` — a verdict that
  counts against the arm in every denominator. 46 cells were scored that way.
- **A session or rate limit ends the run**, not the cell. Continuing walks the
  remaining grid in seconds, recording a phantom cell for each, and the report
  then averages them in.
"""

from __future__ import annotations

import re
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

#: Errors that end the run rather than the cell. The account's five-hour window
#: closing is the one that has actually happened; a 429 is the same shape.
#: Anything unmatched stays per-cell, because guessing that an unknown error is
#: fatal would stop a run that could have finished.
FATAL = re.compile(r"session limit|rate limit|usage limit|\b429\b", re.IGNORECASE)

#: The harness's *own* stopping rule biting, which the SDK reports as an error
#: result like any other. It is not an instrument failure: a cell that ran out of
#: turns answered badly or not at all, which is evidence. Dropping it would
#: reward a model that flails by removing its failures from the denominator —
#: bias in the opposite direction from the one ``ERROR`` exists to prevent, and
#: the reason this is matched apart. See ``decisions.md`` 2026-08-24.
#:
#: The local subject's per-cell **wall clock** is the same kind of rule and is
#: matched here too. Without that it reads as ``ERROR``, and an ERROR cell is one
#: `resume` owes forever — it would time out again on every sitting and the run
#: could never finish.
STOPPING_RULE = re.compile(r"maximum number of turns|max_turns|wall clock", re.IGNORECASE)


class RunHalted(Exception):
    """The run stopped before the grid was finished. Carries the reason."""

    def __init__(self, reason: str, completed: int, total: int) -> None:
        super().__init__(reason)
        self.reason = reason
        self.completed = completed
        self.total = total


def new_run_id(prefix: str = "run") -> str:
    return f"{prefix}-{datetime.now(UTC).strftime('%Y%m%dT%H%M%SZ')}"


@dataclass
class RunResult:
    run_id: str
    records: list[Record]
    #: Set when a fatal error stopped the grid early. The cells after it were
    #: never run and, deliberately, never recorded — so a resume can pick them up.
    halted: str | None = None

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
        # A cell whose subject failed is not clean evidence, even if an answer
        # file happens to parse: the verdict would then depend on where in the
        # turn sequence the failure landed. Excluding it is the direction of bias
        # this project can afford — see ``decisions.md`` 2026-08-24. The turn cap
        # is the exception, because it is the harness's own rule and not a
        # failure of the instrument: that cell is graded on what it left behind.
        if transcript.error and not STOPPING_RULE.search(transcript.error):
            # The raw text survives so a discarded cell can still be read; the
            # parsed answer does not, because this cell contributed none.
            result = Grade(Verdict.ERROR, None, result.raw)
    except Exception as exc:  # noqa: BLE001 — a failed cell is data, not a crash
        transcript = Transcript(cell_id=cell.id, error=f"{type(exc).__name__}: {exc}")
        result = Grade(Verdict.ERROR, None, None)

    episode.step(result.answer)
    measured = signals.measure(transcript)
    record = Record.build(run_id, cell, transcript, result, measured)
    store.append(record)
    store.save_transcript(transcript)
    return record


def _fatal(subject: Subject, error: str) -> bool:
    """Does this error end the run, or only this cell?

    A subject that knows its own fatal errors says so; anything else falls back
    to `FATAL`, which is Anthropic-shaped by history. The local subject's fatal
    set is disjoint — a server that is not there, a model never pulled — and
    misfiling one of those as an ordinary wrong answer is the 2026-08-24 defect
    that filed 46 phantom cells, in different clothes.
    """
    classify = getattr(subject, "fatal", None)
    return bool(classify(error)) if classify else bool(FATAL.search(error))


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
        if record.error and _fatal(subject, record.error):
            return RunResult(store.run_id, records, halted=record.error)
    return RunResult(run_id=store.run_id, records=records)
