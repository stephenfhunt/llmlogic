"""What the harness does when the subject fails, rather than answers.

Both regressions here were live on 2026-08-24: a full grid ran out of session
window at cell 67, and the harness recorded 46 phantom cells and reported them as
wrong answers. `controls` and `static_analysis` read 0/8, the header said
"0 errored", and the S1 delta was computed over denominators padded with cells
that never ran.
"""

from pathlib import Path

import pytest

from harness import arms, report
from harness.cell import Cell
from harness.domains.controls import tasks as controls_tasks
from harness.grade import ANSWER_FILE
from harness.record import RecordStore
from harness.runner import FATAL, run_cell, run_grid
from harness.subject import StubSubject
from harness.task import FIELD_SEPARATOR
from harness.transcript import Transcript

pytestmark = pytest.mark.skipif(
    not (arms.DATALOG_BIN_DIR / "datalog").exists(),
    reason="datalog binary not built; the engine arm cannot be materialized",
)

SESSION_LIMIT = (
    "ResultError: Claude Code returned an error result: "
    "You've hit your session limit · resets 11:20am (America/New_York) (exit code: 1)"
)


class FailingSubject:
    """Reports an error the way ``AgentSubject`` does — caught, not raised."""

    def __init__(self, error: str = SESSION_LIMIT, answer: bool = False) -> None:
        self.error = error
        self.answer = answer
        self.cells: list[str] = []

    def run(self, cell: Cell, workspace) -> Transcript:
        self.cells.append(cell.id)
        if self.answer:
            rows = sorted(cell.task.truth.rows)
            text = "\n".join(FIELD_SEPARATOR.join(row) for row in rows)
            (workspace.path / ANSWER_FILE).write_text(text + "\n")
        return Transcript(cell_id=cell.id, error=self.error)


def _grid():
    from harness.cell import grid

    return grid(controls_tasks.tasks())


def test_a_reported_error_is_ERROR_not_a_wrong_answer(tmp_path):
    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")
    record = run_cell("test", cells[0], FailingSubject(), store, tmp_path / "ws")

    # Not NO_ANSWER: that verdict counts against the arm in every denominator,
    # which is how 46 dead cells became two 0/8 rows.
    assert record.verdict == "error"
    assert record.error == SESSION_LIMIT


def test_an_error_beats_an_answer_that_happens_to_parse(tmp_path):
    """The cell died at turn 3 with a complete answer.txt on disk.

    Grading it would make the verdict depend on where in the turn sequence the
    failure landed. It is excluded instead — the direction of bias this project
    can afford.
    """
    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")
    subject = FailingSubject(error="ResultError: something broke", answer=True)
    record = run_cell("test", cells[0], subject, store, tmp_path / "ws")

    assert record.verdict == "error"
    # The answer is kept on the record, so a discarded cell can still be read.
    assert record.answer_raw


def test_a_session_limit_halts_the_grid(tmp_path):
    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")
    subject = FailingSubject()

    result = run_grid(cells, subject, store, tmp_path / "ws")

    # The cell that hit the limit is recorded — it is the evidence for why the
    # run stopped. Every cell after it is neither run nor recorded, so a resume
    # can still owe them.
    assert result.halted == SESSION_LIMIT
    assert len(result.records) == 1
    assert subject.cells == [cells[0].id]
    assert len(RecordStore.load(store.dir)) == 1


def test_an_ordinary_error_does_not_halt_the_grid(tmp_path):
    """Guessing that an unknown error is fatal would stop a run that could finish."""
    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")
    subject = FailingSubject(error="ResultError: tool timed out")

    result = run_grid(cells, subject, store, tmp_path / "ws")

    assert result.halted is None
    assert len(result.records) == len(cells)


@pytest.mark.parametrize(
    "text",
    [
        "You've hit your session limit · resets 11:20am",
        "rate limit exceeded",
        "HTTP 429 Too Many Requests",
        "Usage limit reached",
    ],
)
def test_fatal_recognises_the_limits(text):
    assert FATAL.search(text)


@pytest.mark.parametrize("text", ["tool timed out", "connection reset", "exit code: 1"])
def test_fatal_leaves_ordinary_failures_alone(text):
    assert not FATAL.search(text)


def test_errored_cells_stay_out_of_the_report_denominators(tmp_path):
    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")
    run_grid(cells[:2], StubSubject(), store, tmp_path / "ws")
    run_cell("test", cells[2], FailingSubject(error="ResultError: nope"), store, tmp_path / "ws")

    records = RecordStore.load(store.dir)
    assert sum(1 for r in records if r["verdict"] == "error") == 1
    rendered = report.render(Path(store.dir))
    assert "1 errored" in rendered


TURN_CAP = (
    "ResultError: Claude Code returned an error result: "
    "Reached maximum number of turns (30) (exit code: 1)"
)


def test_the_turn_cap_is_graded_not_discarded(tmp_path):
    """Running out of turns is the harness's own stopping rule, not a failure of
    the instrument. Dropping those cells would reward a model that flails by
    removing its failures from the denominator."""
    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")
    subject = FailingSubject(error=TURN_CAP, answer=True)

    record = run_cell("test", cells[0], subject, store, tmp_path / "ws")

    assert record.verdict == "correct"
    assert record.error == TURN_CAP


def test_a_turn_cap_cell_that_answered_nothing_is_still_not_an_error(tmp_path):
    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")
    subject = FailingSubject(error=TURN_CAP, answer=False)

    record = run_cell("test", cells[0], subject, store, tmp_path / "ws")

    assert record.verdict == "no-answer"


def test_the_turn_cap_does_not_halt_the_grid(tmp_path):
    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")

    result = run_grid(cells, FailingSubject(error=TURN_CAP), store, tmp_path / "ws")

    assert result.halted is None
    assert len(result.records) == len(cells)


def test_a_turn_cap_cell_counts_and_is_not_owed_again(tmp_path):
    """It is settled evidence: a resume must not re-run it."""
    from harness import resume

    cells = _grid()
    store = RecordStore(tmp_path / "results", "test")
    run_cell("test", cells[0], FailingSubject(error=TURN_CAP, answer=True), store, tmp_path / "ws")

    recorded = RecordStore.load(store.dir)[0]
    assert not resume.failed(recorded)
    assert cells[0].id in resume.settled(store.dir)
