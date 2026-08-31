"""The unit of measurement: one ``(task, arm, strength)`` triple.

The **arm** is the independent variable. Every arm is the same agent, with the
same tools, in the same workspace, over the same files.

There are five, because one arm cannot answer questions that pull in opposite
directions (``decisions.md`` 2026-08-25), and each new one isolates a step the
previous arm left confounded. ``engine`` keeps control 3 — the subject is never
told the engine is there — which is what makes *"did it reach for it?"* a
measurement. ``engine-forced`` mandates a program, which is what makes *"does
using it help?"* answerable at all. The first grid had only the former, reached
in 9 of 56 cells, and could not be read. ``engine-briefed`` hands over the
manual, separating *discovery* from *use*. ``engine-briefed-provenance``
instructs the subject to interrogate its own empty results, separating
*documented* from *instructed*.

**The prompts nest as strict suffixes, in that order**, so every pairwise delta
has exactly one cause. ``tests/test_controls_hold.py`` pins the nesting.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from harness.task import Task

Arm = Literal[
    "prose",
    "engine",
    "engine-forced",
    "engine-briefed",
    "engine-briefed-provenance",
]
ARMS: tuple[Arm, ...] = (
    "prose",
    "engine",
    "engine-forced",
    "engine-briefed",
    "engine-briefed-provenance",
)

#: The arms that get the binary and the skill. Membership here, not a string
#: comparison at each site: the engine arms have to grow together or the
#: independent variable stops meaning one thing.
ENGINE_ARMS: frozenset[str] = frozenset(
    {"engine", "engine-forced", "engine-briefed", "engine-briefed-provenance"}
)

#: The arms told to use the engine. `engine` is not one of them — its prompt is
#: byte-identical to `prose`, which is control 3.
MANDATED_ARMS: frozenset[str] = frozenset(
    {"engine-forced", "engine-briefed", "engine-briefed-provenance"}
)

#: The arm that is handed the engine's reference documentation in its prompt
#: instead of having to find it. **Discovery is the confound it separates
#: out**: measured 2026-08-28, all four `engine-forced` cells of a `controls`
#: gate invented a way to load a CSV — `read_csv/4`, `csv_load/3`,
#: `csv_read_line/2`, `csv_read/4` — against an engine that spells it
#: `import "f.csv" as r.`, and **not one called `Skill`**, though the reference
#: was in the workspace and advertised as a tool. An arm that reaches for the
#: engine and then writes a language it invented is not measuring the engine.
#: `engine-briefed − engine-forced` is what discovery costs; the pair is the
#: measurement, which is why this is a fourth arm and not an edit to the third.
BRIEFED_ARM = "engine-briefed"

#: The arm that is additionally *instructed* to interrogate its own failures —
#: `engine-briefed` plus `catalogue.PROVENANCE_MANDATE`, and nothing else.
#:
#: **It exists because documenting the feature demonstrably did not surface it.**
#: `?why` / `?whynot` are in `SKILL.md` with worked examples and the
#: `repair: ask ?whynot …` chaining idiom, and `engine-briefed` puts that whole
#: document in the prompt. Measured across every run on disk: **zero `?why` or
#: `?whynot` invocations in 1,812 transcripts.** So the briefed arm already
#: answers *"does telling it about provenance surface provenance?"* — no — and
#: a fifth arm is needed to ask the next question, which is whether *instructing*
#: it does. That is the same step `MANDATE` is for engine use, taken again one
#: level in.
#:
#: The failure it targets is not hypothetical. Of the 12 failing `engine-briefed`
#: cells in `run-20260828T203413Z`, **nine derived nothing at all**
#: (`missing == truth_size`, `extra == 0`) and rewrote the program rather than
#: asking why it was empty — `who-can-read-r03` for 35 turns, `delete-without-read`
#: for 28. See `hypotheses.md`, 2026-08-31 addendum.
PROVENANCE_ARM = "engine-briefed-provenance"

#: The arms handed the engine's reference documentation. Membership, not
#: equality: the provenance arm is a strict suffix *of the briefed prompt*, so
#: it needs the same briefing, and the two have to stay in step for the delta
#: between them to have one cause.
BRIEFED_ARMS: frozenset[str] = frozenset({BRIEFED_ARM, PROVENANCE_ARM})

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
#:
#: `engine-briefed` gets the same 2.0 as `engine-forced` and for the same
#: reason — it does the same work — so the pair differs by the briefing alone.
#: A budget that moved with the briefing would put two causes under one delta,
#: which is the mistake this dict's own comment is about.
ARM_BUDGET: dict[str, float] = {
    "prose": 1.0,
    "engine": 1.0,
    "engine-forced": 2.0,
    "engine-briefed": 2.0,
    # The same 2.0 again, and the temptation to raise it is the reason to write
    # this down. `engine-briefed-provenance` is instructed to take *strictly
    # more* steps than `engine-briefed` — ask, read, then repair — so a cap that
    # fitted the briefed arm may not fit this one. Raising it anyway would put
    # two causes under the primary endpoint: the instruction, or the extra
    # turns. The delta has to be attributable to the block alone, so the budget
    # is held and `report._budget_line`'s per-arm cap-hit rate is what makes the
    # cost visible instead of silent. If this arm ends at its cap where the
    # briefed one does not, that is a finding about the instruction's cost and
    # it is reported as one.
    "engine-briefed-provenance": 2.0,
}


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
