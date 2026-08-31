"""The record of one cell, and the JSONL store a run writes it to.

``results/`` is an append-only record (``AGENTS.md``): a run that was made is a
run that was made. Re-rendering a report from records is fine; rewriting a
verdict is not.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from datetime import UTC, datetime
from pathlib import Path

from harness.cell import Cell
from harness.grade import Grade
from harness.signals import Signals
from harness.transcript import Transcript


@dataclass
class Record:
    run_id: str
    cell_id: str
    domain: str
    task_id: str
    question_class: str
    engine_expected_to_help: bool
    #: Which track this cell's task belongs to. Recorded rather than looked up,
    #: because the slate moves and `results/` does not: a report reading a past
    #: run must not have to reconstruct what the task was that day.
    track: str
    #: The generator setting that produced this cell's task, or 0 for a
    #: hand-authored one. Recorded for the reason `track` is: a difficulty ladder
    #: is grouped by its own axis, and reconstructing the rung by parsing
    #: ``g{tag}-d{n}-…`` out of a task id would make the report depend on a
    #: naming convention rather than on what the run measured. Absent on every
    #: record written before 2026-08-30, and read as `n/a` rather than as 0.
    difficulty: int
    arm: str
    strength: str
    model: str
    #: The documentation block cut from this cell, or ``None``. Recorded rather
    #: than inferred from the cell id, so a report can group ablated cells
    #: against their controls without parsing names.
    ablated: str | None

    verdict: str
    missing: int
    extra: int
    #: Rows in the truth. With `missing` and `extra` this is everything per-item
    #: F1 needs, and F1 is what separates an answer that dropped one row of forty
    #: from one that returned nothing — both of which grade `wrong`.
    truth_size: int
    answer_raw: str | None
    #: Set on an `unparseable` verdict: which of `grade`'s three ways it failed.
    #: Absent on every record written before it existed, and the report says so
    #: rather than filling it in.
    unparseable_reason: str | None

    first_program: str | None
    first_program_turn: int | None

    #: The three ways a completion came back without a usable action, kept apart
    #: because they have three different causes and one of them is the harness's
    #: own cap. `Transcript` has carried `malformed_calls` since it was written
    #: and the record dropped it, so the counter with the docstring explaining
    #: why tool-syntax failure must not be folded into a reasoning number was
    #: itself invisible. Absent on every record written before 2026-08-28.
    malformed_calls: int
    truncated_completions: int
    empty_replies: int
    finished_without_answer: bool

    signals: dict
    input_tokens: int
    output_tokens: int
    cache_read_tokens: int
    #: `Usage` has always carried this and the record dropped it, so a run could
    #: not explain its own cost — the pilot's $0.07/cell took three fields and
    #: half an hour to re-derive as a warmed number (`decisions.md` 2026-08-24).
    cache_creation_tokens: int
    cost_usd: float
    wall_seconds: float
    turns: int
    denials: list[str] = field(default_factory=list)
    error: str | None = None
    recorded_at: str = field(default_factory=lambda: datetime.now(UTC).isoformat())

    @staticmethod
    def build(
        run_id: str,
        cell: Cell,
        transcript: Transcript,
        grade: Grade,
        signals: Signals,
    ) -> Record:
        return Record(
            run_id=run_id,
            cell_id=cell.id,
            domain=cell.task.domain,
            task_id=cell.task.id,
            question_class=cell.task.question_class,
            engine_expected_to_help=cell.task.engine_expected_to_help,
            track=cell.task.track,
            difficulty=cell.task.difficulty,
            arm=cell.arm,
            strength=cell.strength.name,
            model=cell.strength.model,
            ablated=cell.ablate,
            verdict=str(grade.verdict),
            missing=grade.missing,
            extra=grade.extra,
            truth_size=len(cell.task.truth.rows),
            answer_raw=grade.raw,
            unparseable_reason=grade.reason,
            first_program=transcript.first_program,
            first_program_turn=transcript.first_program_turn,
            malformed_calls=transcript.malformed_calls,
            truncated_completions=transcript.truncated_completions,
            empty_replies=transcript.empty_replies,
            finished_without_answer=transcript.finished_without_answer,
            signals=signals.to_dict(),
            input_tokens=transcript.usage.input_tokens,
            output_tokens=transcript.usage.output_tokens,
            cache_read_tokens=transcript.usage.cache_read_tokens,
            cache_creation_tokens=transcript.usage.cache_creation_tokens,
            cost_usd=(
                transcript.cost_usd
                if transcript.cost_usd is not None
                else cell.strength.cost(
                    transcript.usage.input_tokens, transcript.usage.output_tokens
                )
            ),
            wall_seconds=transcript.wall_seconds,
            turns=transcript.turns,
            denials=list(transcript.denials),
            error=transcript.error,
        )


class RecordStore:
    """One directory per run: ``run.json`` metadata, ``records.jsonl`` verdicts."""

    def __init__(self, root: Path, run_id: str) -> None:
        self.run_id = run_id
        self.dir = root / run_id
        self.dir.mkdir(parents=True, exist_ok=True)
        self.records_path = self.dir / "records.jsonl"
        self.transcripts_dir = self.dir / "transcripts"

    def write_metadata(self, **metadata) -> None:
        payload = {"run_id": self.run_id, "started_at": datetime.now(UTC).isoformat()}
        payload.update(metadata)
        (self.dir / "run.json").write_text(json.dumps(payload, indent=2) + "\n")

    def append(self, record: Record) -> None:
        with self.records_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(asdict(record)) + "\n")

    def save_transcript(self, transcript: Transcript) -> None:
        self.transcripts_dir.mkdir(parents=True, exist_ok=True)
        path = self.transcripts_dir / f"{transcript.cell_id}.json"
        path.write_text(
            json.dumps(
                {
                    "cell_id": transcript.cell_id,
                    "final_text": transcript.final_text,
                    "first_program": transcript.first_program,
                    "reasoning": transcript.reasoning,
                    "tool_calls": [
                        {"turn": c.turn, "name": c.name, "input": c.input}
                        for c in transcript.tool_calls
                    ],
                },
                indent=2,
            )
            + "\n"
        )

    @staticmethod
    def load(run_dir: Path) -> list[dict]:
        path = run_dir / "records.jsonl"
        if not path.exists():
            return []
        return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
