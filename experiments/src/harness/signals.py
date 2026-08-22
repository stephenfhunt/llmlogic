"""Process signals — what the answer alone does not carry.

The most valuable thing the old checklist ever produced was not an answer: it was
*the three questions the model answered with `grep` because the engine could not
express them*. **A skill that cannot say something loses the question silently,
and the model does not announce the switch.** A prompt-only harness cannot see
that, because its subject has no `grep`. Ours does, so this module watches for it.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass

from harness.engine_use import program_from_call, uses_engine
from harness.transcript import ToolCall, Transcript

_SEARCH_COMMANDS = ("grep", "rg", "ripgrep", "awk", "sed -n")


def _is_engine_call(call: ToolCall) -> bool:
    return uses_engine(call.name, call.input)


def _is_search_call(call: ToolCall) -> bool:
    if call.name in ("Grep", "Glob"):
        return True
    if call.name != "Bash":
        return False
    command = str(call.input.get("command", ""))
    return any(tool in command for tool in _SEARCH_COMMANDS)


def _is_program_write(call: ToolCall) -> bool:
    """Did the subject write Datalog source — in a file, or straight into a pipe?

    The first real cell wrote no ``.dl`` file and still wrote several programs.
    """
    return program_from_call(call.name, call.input) is not None


@dataclass(frozen=True)
class Signals:
    """Counts, not conclusions.

    What *"reached for it"* should mean is an open question in ``decisions.md`` —
    a boolean will not carry the case where the subject asks the engine one
    question and then answers from `grep`. So this records the raw counts and
    leaves the classification to be settled against real transcripts.
    """

    wrote_program: bool
    ran_engine: bool
    engine_calls: int
    #: Searches issued **after** the engine was first used. This is the silent
    #: switch: the subject had the engine, tried it, and went back to text.
    searches_after_engine: int
    searches_total: int
    #: Engine invocations before the answer was written — a rough round count.
    rounds: int

    def to_dict(self) -> dict:
        return asdict(self)


def measure(transcript: Transcript) -> Signals:
    calls = sorted(transcript.tool_calls, key=lambda c: c.turn)
    engine_turns = [c.turn for c in calls if _is_engine_call(c)]
    first_engine_turn = engine_turns[0] if engine_turns else None

    searches = [c for c in calls if _is_search_call(c)]
    after = (
        [c for c in searches if c.turn > first_engine_turn] if first_engine_turn is not None else []
    )

    return Signals(
        wrote_program=any(_is_program_write(c) for c in calls),
        ran_engine=bool(engine_turns),
        engine_calls=len(engine_turns),
        searches_after_engine=len(after),
        searches_total=len(searches),
        rounds=len(engine_turns),
    )
