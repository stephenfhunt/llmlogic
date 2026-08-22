"""The seam the game arm will slot into.

A single-shot task is an **episode of length one**. Stating that as a protocol
now — rather than as a comment — is what keeps the core from foreclosing the
game arm, where a cell is a whole episode with a score instead of one question
with an answer (``ROADMAP.md``, *The game arm*).

Nothing here is speculative machinery for its own sake: the runner drives
``Environment``, so when Minesweeper arrives it implements this and the runner
does not change.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

from harness.task import Answer, Task


@dataclass(frozen=True)
class Score:
    """How well an episode went. For a single-shot task this is 0.0 or 1.0."""

    value: float
    of: float = 1.0

    @property
    def fraction(self) -> float:
        return self.value / self.of if self.of else 0.0


class Environment(Protocol):
    """What the subject acts on. One question, or one game."""

    def prompt(self) -> str:
        """What to put in front of the subject for the current step."""

    def step(self, action: Answer | None) -> None:
        """Apply the subject's action."""

    def done(self) -> bool:
        """Has the episode ended?"""

    def score(self) -> Score:
        """Score the episode. Only meaningful once ``done()``."""


class SingleShot:
    """An ``Environment`` of exactly one step: ask, answer, score.

    Ground truth comes from the task, which got it from the domain's ``truth.py``
    — never from the datalog engine (control 1).
    """

    def __init__(self, task: Task) -> None:
        self.task = task
        self.answer: Answer | None = None
        self._stepped = False

    def prompt(self) -> str:
        return self.task.question

    def step(self, action: Answer | None) -> None:
        self.answer = action
        self._stepped = True

    def done(self) -> bool:
        return self._stepped

    def score(self) -> Score:
        if self.answer is None:
            return Score(0.0)
        return Score(1.0 if self.answer.rows == self.task.truth.rows else 0.0)
