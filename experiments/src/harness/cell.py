"""The unit of measurement: one ``(task, arm, strength)`` triple.

The **arm** is the independent variable. Every arm is the same agent, with the
same tools, in the same workspace, over the same files.

There are three, because one arm cannot answer two questions that pull in
opposite directions (``decisions.md`` 2026-08-25). ``engine`` keeps control 3 —
the subject is never told the engine is there — which is what makes *"did it
reach for it?"* a measurement. ``engine-forced`` mandates a program, which is
what makes *"does using it help?"* answerable at all. The first grid had only the
former, reached in 9 of 56 cells, and could not be read.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from harness.task import Task

Arm = Literal["prose", "engine", "engine-forced"]
ARMS: tuple[Arm, ...] = ("prose", "engine", "engine-forced")

#: The arms that get the binary and the skill. Membership here, not a string
#: comparison at each site: the engine arms have to grow together or the
#: independent variable stops meaning one thing.
ENGINE_ARMS: frozenset[str] = frozenset({"engine", "engine-forced"})

#: How much of a cell's budget — turns *and* wall clock — each arm is given, as a
#: multiple of the run's own cap.
#:
#: **An equal cap is not a held constant when the arms need unequal turns.**
#: `prose` answers in read-then-write; `engine-forced` must additionally write a
#: program, run it, read the diagnostic, repair, and re-run before it can write
#: anything at all, and every repair is a round trip. Measured over every run on
#: disk (2026-08-27): the cells that *succeeded* took a median 4 turns on
#: `engine`, 13 on `engine-forced`, and `engine-forced` ended at the cap in
#: **48%** of cells against 16% for `prose` — 75% of those writing no answer. Its
#: 5% correct was substantially a measurement of this number.
#:
#: 2.0 rather than the 3.25 the observed ratio implies: the successful cells top
#: out at 21 turns against a 24 cap, so that distribution is itself truncated and
#: the honest reading is *at least* twice, not exactly 3.25.
#:
#: **The cost, stated plainly:** this is an arm-asymmetric instrument parameter
#: on the arm carrying the primary endpoint, so a gain on `engine-forced` now has
#: two candidate causes — the engine, or the budget. `report.py` prints the
#: cap-hit rate per arm for exactly this reason: a result is only free of the
#: confound while no arm is ending at its cap. See `decisions.md` 2026-08-27.
ARM_BUDGET: dict[str, float] = {"prose": 1.0, "engine": 1.0, "engine-forced": 2.0}


def budget_for(arm: str) -> float:
    """The arm's multiple of the run's turn and wall-clock caps."""
    return ARM_BUDGET.get(arm, 1.0)


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

    # --- the local seam ---------------------------------------------------
    # A locally-served subject varies along axes an Anthropic model does not,
    # and they belong on the strength because that is what a cell is crossed by:
    # `qwen3-8b/structured` and `qwen3-8b/native` are two subjects to compare,
    # so they must be two strengths, recorded separately and tabled separately.
    # `None` throughout means "an Anthropic model", and `STRENGTHS` is untouched.

    #: OpenAI-compatible base URL. Set means *this strength is served locally*,
    #: which is also how the runner knows which subject to hand the cell to.
    endpoint: str | None = None
    #: ``native`` (a ``tools`` array) or ``structured`` (a constrained decoder).
    #: Which one wins is a property of the model, not of the harness: measured
    #: 2026-08-26, `qwen2.5-coder` cannot emit a parseable call natively at all,
    #: `llama3.1` goes 0/4 → 2/4 under a grammar, and `qwen3` goes 2/4 → 0/4.
    tool_protocol: str = "native"
    #: Passed through when set. ``none`` turns a thinking model's thinking off —
    #: 12.4s and 699 output tokens against 2.0s and 21, on `qwen3:8b`.
    reasoning_effort: str | None = None

    @property
    def is_local(self) -> bool:
        return self.endpoint is not None

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
    #: Which repeat of this cell. Part of the identity for the same reason
    #: ``ablate`` is: two trials are two cells, recorded twice and resumed
    #: independently. **Trial 0 spells its id exactly as before**, so a
    #: single-trial run is byte-comparable with every run already in
    #: ``results/`` and a resume of one still matches.
    trial: int = 0

    @property
    def id(self) -> str:
        base = f"{self.task.domain}.{self.task.id}.{self.arm}.{self.strength.name}"
        if self.ablate:
            base = f"{base}.minus-{self.ablate}"
        return f"{base}#{self.trial}" if self.trial else base


def grid(
    tasks: list[Task],
    strengths: tuple[Strength, ...] = STRENGTHS,
    arms: tuple[Arm, ...] = ARMS,
    ablate: str | None = None,
    repeats: int = 1,
) -> list[Cell]:
    """Every task, every arm, every strength — the full crossing.

    ``arms`` narrows for an **ablation**, which is engine-arm only: cutting a
    block of the engine's documentation cannot change what an arm that never had
    it does, so a prose cell in an ablation grid is money spent re-measuring the
    control.

    ``repeats`` runs each cell more than once. The subject is stochastic and
    every cell in every run so far has been run exactly once, so there is no
    variance estimate anywhere in the record — a single trial cannot tell a
    reliable answer from a lucky one. Trials are ordered last so a truncated
    sitting holds a complete first pass over the grid rather than a complete
    first task.
    """
    if repeats < 1:
        raise ValueError("repeats must be at least 1")
    return [
        Cell(task=task, arm=arm, strength=strength, ablate=ablate, trial=trial)
        for trial in range(repeats)
        for task in tasks
        for arm in arms
        for strength in strengths
    ]
