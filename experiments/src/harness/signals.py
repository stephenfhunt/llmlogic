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

from harness.engine_use import (
    asks_provenance,
    invokes_engine,
    program_from_call,
    provenance_in_command,
    runs_script,
    uses_engine,
    writes_script,
)
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


class ProvenanceUse(StrEnum):
    """How far the subject got with `?why` / `?whynot`.

    Three values, shaped like ``EngineUse`` but **not** sharing its third name,
    because the claim is not the same one. ``EngineUse.ANSWERED_FROM`` says the
    answer came after the engine ran. The interesting question for an
    *explanation* is whether the subject then did something with it — provenance
    is a repair step, so asking on the last turn and stopping is not use.

    The baseline this measures against: **zero invocations across 1,812
    archived transcripts**, on every run on disk, including 12 failing cells
    that had the manual in the prompt.
    """

    #: Never asked.
    NONE = "none"
    #: Asked, and nothing came of it. Merges two cases on purpose — a goal
    #: written down but never run, and one that ran on the way out the door —
    #: because both are "got no repair from it". ``provenance_runs`` keeps them
    #: separable: this value with a non-zero count is the second case.
    ASKED = "asked"
    #: A `datalog` process ran an explanation goal, and the subject afterwards
    #: wrote a program or the answer. The strongest claim the transcript
    #: supports for *acting on* an explanation.
    ACTED_ON = "acted-on"


class ScriptUse(StrEnum):
    """How far the subject got writing code instead of a Datalog program.

    **The counterpart to ``EngineUse``, and the reason it exists.** The founding
    decision names ad-hoc code as the honest counterfactual — *"an agent's real
    alternative to a logic engine is not careful prose, it is ad-hoc code"* — and
    for four grids nothing recorded it. On the 2026-08-31 Haiku ladder that was
    the whole mechanism: measured retroactively from the archived transcripts,
    ``prose`` wrote and ran a script in **36 of 44** cells and ``engine-briefed``
    in **0 of 44**, so the `+0 at every rung` was never prose against the engine.
    It was a script against the engine, tying.

    Retroactive is the operative word: ``ROADMAP.md`` filed this as *must land
    before the next run* on the grounds that transcripts are not persisted. They
    are — 1,812 of them under ``results/*/transcripts/``, each carrying
    ``tool_calls`` — so this backfills over every run already on disk.
    """

    #: No script written, none run.
    NONE = "none"
    #: Authored but never executed — the middle case, for the same reason
    #: ``EngineUse`` keeps one.
    WROTE = "wrote"
    #: An interpreter ran and the answer was written afterwards.
    ANSWERED_FROM = "answered-from"


def _is_engine_call(call: ToolCall) -> bool:
    return uses_engine(call.name, call.input)


def _ran_script(call: ToolCall) -> bool:
    return call.name == "Bash" and runs_script(str(call.input.get("command", "")))


def _ran_provenance(call: ToolCall) -> bool:
    """Did a `datalog` **process** carry an explanation goal?"""
    return call.name == "Bash" and provenance_in_command(str(call.input.get("command", "")))


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
    #: A `datalog` **process** ran. Narrow on purpose — `engine_calls` uses the
    #: wide predicate, which counts a `Skill` invocation, and `measure` used the
    #: wide one here until 2026-08-26, so a cell whose only tool call was `Skill`
    #: reported `engine_use=invoked` and `ran_engine=True` together
    #: (`decisions.md` 2026-08-25).
    #:
    #: **This field does not imply `engine_use != "invoked"`, and the comment
    #: here claimed it did until 2026-08-31.** `classify` returns `invoked` for
    #: *ran it and answered from something else* as much as for *never ran it* —
    #: deliberately, and its own comment says so. Measured over `results/`: 201
    #: records pair `invoked` with this true, 188 of them because the engine ran
    #: and no answer was ever written. That is the classifier working. The
    #: over-strong claim was the comment's, and reading it as an invariant is
    #: what makes a legitimate 188 look like a defect.
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
    #: The three-valued provenance reach. ``str`` on the wire for the same
    #: reason ``engine_use`` is: records from before this field existed stay
    #: readable, and they read as absent rather than as ``none`` — which is the
    #: honest reading, since nothing measured them.
    provenance_use: str = str(ProvenanceUse.NONE)
    #: Times a `datalog` process ran an explanation goal. The raw count that
    #: keeps ``ASKED``'s two merged cases apart, per this module's rule that a
    #: count is what a later question can still be asked of.
    provenance_runs: int = 0
    #: The three-valued script reach — the counterfactual arm's own `engine_use`.
    #: Defaulted for the same reason the provenance fields are: a record written
    #: before this existed reads as absent, not as `none`.
    script_use: str = str(ScriptUse.NONE)
    #: Times an interpreter ran.
    script_runs: int = 0

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
    # `>=`, not `>`. A subject already in a pipeline writes
    # `datalog q.dl -q 'goal(X)' > answer.txt` — one call that runs the engine
    # and writes the answer out of its stdout, which is the *strongest* form of
    # answering from the engine and read as `invoked` until 2026-08-31. On a
    # mandated arm `invoked` is non-compliance, and compliance decides whether a
    # comparison is void, so the cost of this was a correct cell voiding the
    # arm it belonged to. Two cells on disk (both `llama3.1-8b-native`).
    #
    # Same-turn is no weaker a claim than the strictly-after case the third
    # value already accepts: neither reads the answer against the engine's
    # stdout, as ``ANSWERED_FROM`` says in as many words.
    if answered and max(answered) >= min(ran):
        return EngineUse.ANSWERED_FROM
    # It ran, and either no answer was written or the answer predates the run.
    # Both are "asked and did not use it", which is what `invoked` means.
    return EngineUse.INVOKED


def classify_provenance(transcript: Transcript) -> ProvenanceUse:
    """How far the subject got with an explanation. See ``ProvenanceUse``."""
    calls = sorted(transcript.tool_calls, key=lambda c: c.turn)
    ran = [c.turn for c in calls if _ran_provenance(c)]
    if not ran:
        reached = any(asks_provenance(c.name, c.input) for c in calls)
        return ProvenanceUse.ASKED if reached else ProvenanceUse.NONE
    # It ran. Did anything follow from it? A program or an answer written after
    # the first explanation is the only evidence the transcript carries that the
    # subject read the thing it asked for.
    worked_after = any(
        c.turn > min(ran) and (_is_program_write(c) or _is_answer_write(c)) for c in calls
    )
    return ProvenanceUse.ACTED_ON if worked_after else ProvenanceUse.ASKED


def classify_script(transcript: Transcript) -> ScriptUse:
    """How far the subject got with ad-hoc code. See ``ScriptUse``."""
    calls = sorted(transcript.tool_calls, key=lambda c: c.turn)
    ran = [c.turn for c in calls if _ran_script(c)]
    if not ran:
        wrote = any(writes_script(c.name, c.input) for c in calls)
        return ScriptUse.WROTE if wrote else ScriptUse.NONE
    answered = [c.turn for c in calls if _is_answer_write(c)]
    # `>=` for the reason `classify` uses it, and more urgently: redirecting an
    # interpreter straight into the answer file is the *dominant* idiom, not an
    # edge case. Measured on the 2026-08-31 ladder's prose arm, 11 of the 14
    # cells that `>` put in `WROTE` were one call doing
    # `python3 solve.py > answer.txt`. Under the strict test the counterfactual
    # arm read 22/44; it is 33/44.
    if answered and max(answered) >= min(ran):
        return ScriptUse.ANSWERED_FROM
    return ScriptUse.WROTE


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
        provenance_use=str(classify_provenance(transcript)),
        provenance_runs=sum(1 for c in calls if _ran_provenance(c)),
        script_use=str(classify_script(transcript)),
        script_runs=sum(1 for c in calls if _ran_script(c)),
    )
