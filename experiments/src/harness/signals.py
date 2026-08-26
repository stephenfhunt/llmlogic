"""Process signals — what the answer alone does not carry.

The most valuable thing the old checklist ever produced was not an answer: it was
*the three questions the model answered with `grep` because the engine could not
express them*. **A skill that cannot say something loses the question silently,
and the model does not announce the switch.** A prompt-only harness cannot see
that, because its subject has no `grep`. Ours does, so this module watches for it.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
from enum import StrEnum

from harness.engine_use import invokes_engine, program_from_call, uses_engine
from harness.grade import ANSWER_FILE
from harness.transcript import ToolCall, Transcript

_SEARCH_COMMANDS = ("grep", "rg", "ripgrep", "awk", "sed -n")


class EngineUse(StrEnum):
    """How far the subject actually got with the engine.

    Three values, not a boolean, and settled against the transcript that forced
    the question (``decisions.md`` 2026-08-25): a haiku cell invoked the ``Skill``
    tool with an entire Datalog program in ``args`` and **nothing ran**. Counting
    that as engine use is what let a cell with no answer of its own read as a
    success.
    """

    #: Never reached for it.
    NONE = "none"
    #: Reached for it — a program written, or the skill invoked — but no engine
    #: process ever ran. The middle case, and the one a boolean loses.
    INVOKED = "invoked"
    #: The engine ran, and the answer was written afterwards.
    #:
    #: This is the strongest claim the transcript supports. It is *not* proof the
    #: numbers came from the engine's output — nothing short of reading the answer
    #: against the engine's stdout would be — but it is the only value that
    #: excludes both "asked and abandoned" and "ran it, then answered from `grep`
    #: anyway", which are the two failures worth telling apart.
    ANSWERED_FROM = "answered-from"


def _is_engine_call(call: ToolCall) -> bool:
    return uses_engine(call.name, call.input)


def _is_search_call(call: ToolCall) -> bool:
    if call.name in ("Grep", "Glob"):
        return True
    if call.name != "Bash":
        return False
    command = str(call.input.get("command", ""))
    return any(tool in command for tool in _SEARCH_COMMANDS)


def _is_answer_write(call: ToolCall) -> bool:
    """Did this call write the answer file?

    Both spellings: the ``Write``/``Edit`` tools, and a shell redirect, which is
    how a subject with `bash` writes a file when it is already in a pipeline.
    """
    if call.name in ("Write", "Edit"):
        return str(call.input.get("file_path", "")).endswith(ANSWER_FILE)
    if call.name == "Bash":
        return ANSWER_FILE in str(call.input.get("command", ""))
    return False


def _ran_engine(call: ToolCall) -> bool:
    """Did a `datalog` **process** run?

    Narrower than ``uses_engine``, which also counts a ``Skill`` invocation. The
    difference between the two is exactly the ``invoked`` case.
    """
    return call.name == "Bash" and invokes_engine(str(call.input.get("command", "")))


def _is_program_write(call: ToolCall) -> bool:
    """Did the subject write Datalog source — in a file, or straight into a pipe?

    The first real cell wrote no ``.dl`` file and still wrote several programs.
    """
    return program_from_call(call.name, call.input) is not None


@dataclass(frozen=True)
class Signals:
    """Counts, and the one classification that is now settled.

    ``engine_use`` answers what *"reached for it"* means (``decisions.md``
    2026-08-25); everything else here stays a raw count, because a count is what
    a later question can still be asked of.
    """

    #: The three-valued reach. ``str`` on the wire — ``EngineUse`` is a
    #: ``StrEnum``, so a record round-trips through JSON unchanged and an older
    #: run's records stay readable.
    engine_use: str
    wrote_program: bool
    #: A `datalog` **process** ran. Narrow on purpose, and it has to agree with
    #: `engine_use`: `invoked` is *defined* as having reached for the engine
    #: without one ever running, so a transcript cannot be `invoked` and have
    #: this true. It could, until 2026-08-26 — `measure` used the same wide
    #: predicate as `engine_calls`, which counts a `Skill` invocation, while
    #: `classify` used the narrow one. A cell whose only tool call was `Skill`
    #: reported `engine_use=invoked` and `ran_engine=True` in the same record.
    #: `decisions.md` 2026-08-25 records that a haiku cell did exactly that.
    ran_engine: bool
    #: Calls that **reached for** the engine, a `Skill` invocation included. The
    #: wide count, and deliberately not the same number as `rounds`.
    engine_calls: int
    #: Searches issued **after** the engine was first used. This is the silent
    #: switch: the subject had the engine, tried it, and went back to text.
    searches_after_engine: int
    searches_total: int
    #: Times the engine actually **ran** — a rough count of feedback rounds.
    #: Reaching for the skill is not a round: nothing came back to repair from.
    rounds: int

    def to_dict(self) -> dict:
        return asdict(self)


def classify(transcript: Transcript) -> EngineUse:
    """How far the subject got. See ``EngineUse`` for what each value excludes."""
    calls = sorted(transcript.tool_calls, key=lambda c: c.turn)
    ran = [c.turn for c in calls if _ran_engine(c)]
    if not ran:
        reached = any(_is_engine_call(c) or _is_program_write(c) for c in calls)
        return EngineUse.INVOKED if reached else EngineUse.NONE
    answered = [c.turn for c in calls if _is_answer_write(c)]
    if answered and max(answered) > min(ran):
        return EngineUse.ANSWERED_FROM
    # It ran, and either no answer was written or the answer predates the run.
    # Both are "asked and did not use it", which is what `invoked` means.
    return EngineUse.INVOKED


def measure(transcript: Transcript) -> Signals:
    calls = sorted(transcript.tool_calls, key=lambda c: c.turn)
    engine_turns = [c.turn for c in calls if _is_engine_call(c)]
    first_engine_turn = engine_turns[0] if engine_turns else None

    searches = [c for c in calls if _is_search_call(c)]
    after = (
        [c for c in searches if c.turn > first_engine_turn] if first_engine_turn is not None else []
    )

    # Two counts, two predicates. `engine_turns` is *reaching for* the engine and
    # is what a search after it is measured against; `ran` is the engine having
    # actually run. Collapsing them is what made `ran_engine` contradict
    # `engine_use` on any transcript that invoked the skill without running
    # anything.
    ran = [c.turn for c in calls if _ran_engine(c)]

    return Signals(
        engine_use=str(classify(transcript)),
        wrote_program=any(_is_program_write(c) for c in calls),
        ran_engine=bool(ran),
        engine_calls=len(engine_turns),
        searches_after_engine=len(after),
        searches_total=len(searches),
        rounds=len(ran),
    )
