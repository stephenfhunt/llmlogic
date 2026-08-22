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
    arm: str
    strength: str
    model: str

    verdict: str
    missing: int
    extra: int
    answer_raw: str | None

    first_program: str | None
    first_program_turn: int | None

    signals: dict
    input_tokens: int
    output_tokens: int
    cache_read_tokens: int
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
            arm=cell.arm,
            strength=cell.strength.name,
            model=cell.strength.model,
            verdict=str(grade.verdict),
            missing=grade.missing,
            extra=grade.extra,
            answer_raw=grade.raw,
            first_program=transcript.first_program,
            first_program_turn=transcript.first_program_turn,
            signals=signals.to_dict(),
            input_tokens=transcript.usage.input_tokens,
            output_tokens=transcript.usage.output_tokens,
            cache_read_tokens=transcript.usage.cache_read_tokens,
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
