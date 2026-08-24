"""The unit of measurement: one ``(task, arm, strength)`` triple.

The **arm** is the independent variable and the only difference between two runs
of the same task: both arms are the same agent, with the same tools, in the same
workspace, over the same files. The engine arm additionally has the ``datalog``
binary and its skill. See ``decisions.md``, 2026-08-21.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from harness.task import Task

Arm = Literal["engine", "prose"]
ARMS: tuple[Arm, ...] = ("engine", "prose")


@dataclass(frozen=True)
class Strength:
    """A model strength, with the prices needed to account for a run.

    Two, per ``decisions.md``: the weaker arm is the informative one, because the
    strongest model routes around gaps instead of falling into them.
    """

    name: str
    model: str
    input_per_mtok: float
    output_per_mtok: float
    #: Context window. Load-bearing: on a large fact base the prose arm must hold
    #: the facts in context while the engine arm does not, so an oversized fixture
    #: measures context rather than reasoning. Fixtures are capped against this.
    context_tokens: int

    def cost(self, input_tokens: int, output_tokens: int) -> float:
        return (
            input_tokens * self.input_per_mtok + output_tokens * self.output_per_mtok
        ) / 1_000_000


OPUS_5 = Strength(
    name="opus-5",
    model="claude-opus-5",
    input_per_mtok=5.00,
    output_per_mtok=25.00,
    context_tokens=1_000_000,
)

HAIKU_4_5 = Strength(
    name="haiku-4.5",
    model="claude-haiku-4-5",
    input_per_mtok=1.00,
    output_per_mtok=5.00,
    context_tokens=200_000,
)

STRENGTHS: tuple[Strength, ...] = (OPUS_5, HAIKU_4_5)

#: The smallest context across strengths, minus room for the agent's own turns.
#: A fixture larger than this defeats the prose arm on context alone.
FIXTURE_TOKEN_BUDGET = 100_000


@dataclass(frozen=True)
class Cell:
    task: Task
    arm: Arm
    strength: Strength
    #: A named documentation block cut from this cell's skill copy, or ``None``
    #: for the ordinary cell. Part of the identity, not a flag beside it: the
    #: ablated cell and its control are two cells and have to record as two.
    #: Meaningless on the prose arm, which has no skill to cut from.
    ablate: str | None = None

    @property
    def id(self) -> str:
        base = f"{self.task.domain}.{self.task.id}.{self.arm}.{self.strength.name}"
        return f"{base}.minus-{self.ablate}" if self.ablate else base


def grid(
    tasks: list[Task],
    strengths: tuple[Strength, ...] = STRENGTHS,
    arms: tuple[Arm, ...] = ARMS,
    ablate: str | None = None,
) -> list[Cell]:
    """Every task, every arm, every strength — the full crossing.

    ``arms`` narrows for an **ablation**, which is engine-arm only: cutting a
    block of the engine's documentation cannot change what an arm that never had
    it does, so a prose cell in an ablation grid is money spent re-measuring the
    control.
    """
    return [
        Cell(task=task, arm=arm, strength=strength, ablate=ablate)
        for task in tasks
        for arm in arms
        for strength in strengths
    ]
