"""The subject: what actually answers the question.

``Subject`` is the seam between the harness and the model. ``StubSubject`` is the
offline implementation the ``--dry-run`` grid uses — the real Agent SDK driver
implements the same protocol, so the whole pipeline (workspaces, grading,
signals, records, report) is exercised without spending a cent. A change that can
only be tested by paying for it is a change that stops being tested.
"""

from __future__ import annotations

import hashlib
import random
from typing import Protocol

from harness.arms import Workspace
from harness.cell import Cell
from harness.grade import ANSWER_FILE
from harness.task import FIELD_SEPARATOR
from harness.transcript import ToolCall, Transcript, Usage


class Subject(Protocol):
    def run(self, cell: Cell, workspace: Workspace) -> Transcript: ...


class StubSubject:
    """A deterministic fake that produces every outcome the pipeline must handle.

    Seeded from the cell id, so a dry run is reproducible and a diff between two
    dry runs means a real change. It reads the task's truth to decide what to
    write — it is a fixture for the harness, not a model, and nothing it produces
    is evidence about anything.
    """

    def run(self, cell: Cell, workspace: Workspace) -> Transcript:
        seed = int(hashlib.sha256(cell.id.encode()).hexdigest()[:8], 16)
        rng = random.Random(seed)
        transcript = Transcript(cell_id=cell.id)
        turn = 0

        # The declared relations only. A fixture that carries a source tree has
        # dozens of files, and a stub that "reads" each of them would inflate
        # every process signal in a dry run with turns no subject took.
        for filename in sorted(cell.task.fixture.declared_sources()):
            turn += 1
            transcript.tool_calls.append(
                ToolCall(turn, "Read", {"file_path": str(workspace.path / filename)})
            )

        if workspace.has_engine and rng.random() < 0.8:
            turn += 1
            program = f"% {cell.task.id}\nanswer(X) :- fact(X).\n"
            transcript.tool_calls.append(
                ToolCall(turn, "Write", {"file_path": str(workspace.path / "q.dl")})
            )
            transcript.first_program = program
            transcript.first_program_turn = turn

            turn += 1
            transcript.tool_calls.append(
                ToolCall(turn, "Bash", {"command": f"datalog {workspace.path}/q.dl"})
            )

            if rng.random() < 0.3:  # the silent switch back to text
                turn += 1
                transcript.tool_calls.append(ToolCall(turn, "Grep", {"pattern": "alice"}))
        else:
            turn += 1
            transcript.tool_calls.append(ToolCall(turn, "Grep", {"pattern": "alice"}))

        outcome = rng.random()
        rows = sorted(cell.task.truth.rows)
        if outcome < 0.05:
            pass  # no answer file at all
        elif outcome < 0.10:
            (workspace.path / ANSWER_FILE).write_text("I could not work this out.\n||\n")
        else:
            if outcome < 0.45 and rows:
                rows = rows[:-1]  # the silent subset — rows dropped, nothing looks wrong
            text = "\n".join(FIELD_SEPARATOR.join(row) for row in rows)
            (workspace.path / ANSWER_FILE).write_text(text + "\n" if text else "")
            turn += 1
            transcript.tool_calls.append(
                ToolCall(turn, "Write", {"file_path": str(workspace.path / ANSWER_FILE)})
            )

        transcript.final_text = "Done."
        transcript.usage = Usage(
            input_tokens=rng.randint(20_000, 60_000),
            output_tokens=rng.randint(500, 4_000),
        )
        transcript.wall_seconds = round(rng.uniform(5.0, 90.0), 2)
        return transcript
