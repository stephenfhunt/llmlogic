"""Process signals — chiefly the silent switch back to text.

A prompt-only harness cannot see this: its subject has no `grep`. Ours does, and
the most valuable finding the old checklist ever produced was exactly this shape —
questions answered with `grep` because the engine could not express them, with
nothing in the transcript announcing the switch.
"""

from harness.signals import measure
from harness.transcript import ToolCall, Transcript


def test_a_subject_that_never_touched_the_engine():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Read", {"file_path": "employee.csv"}),
            ToolCall(2, "Grep", {"pattern": "engineering"}),
            ToolCall(3, "Write", {"file_path": "answer.txt"}),
        ],
    )
    signals = measure(transcript)
    assert signals.ran_engine is False
    assert signals.wrote_program is False
    assert signals.searches_total == 1
    # No engine call, so there is no "after" to speak of. Counting these as
    # switches would make every prose-arm cell look like an abandoned engine.
    assert signals.searches_after_engine == 0


def test_the_silent_switch_counts_only_searches_after_the_engine():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Grep", {"pattern": "scoping"}),  # before: orientation
            ToolCall(2, "Write", {"file_path": "q.dl"}),
            ToolCall(3, "Bash", {"command": "datalog q.dl -q 'reaches(X, Y)'"}),
            ToolCall(4, "Grep", {"pattern": "prefix"}),  # after: the switch
            ToolCall(5, "Bash", {"command": "rg 'ordering' src/"}),  # after
        ],
    )
    signals = measure(transcript)
    assert signals.ran_engine is True
    assert signals.wrote_program is True
    assert signals.searches_total == 3
    assert signals.searches_after_engine == 2


def test_rounds_counts_engine_invocations():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Bash", {"command": "datalog q.dl"}),
            ToolCall(2, "Edit", {"file_path": "q.dl"}),
            ToolCall(3, "Bash", {"command": "datalog q.dl"}),
            ToolCall(4, "Bash", {"command": "datalog q.dl -q 'answer(X)'"}),
        ],
    )
    signals = measure(transcript)
    assert signals.rounds == 3
    assert signals.engine_calls == 3


def test_a_non_datalog_bash_call_is_not_an_engine_call():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[ToolCall(1, "Bash", {"command": "python3 solve.py"})],
    )
    signals = measure(transcript)
    assert signals.ran_engine is False
    # The prose arm writing a script is the honest counterfactual, not a failure.
    assert signals.searches_after_engine == 0


def test_writing_a_non_program_file_is_not_writing_a_program():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[ToolCall(1, "Write", {"file_path": "answer.txt"})],
    )
    assert measure(transcript).wrote_program is False
