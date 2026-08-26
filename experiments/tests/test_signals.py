"""Process signals — chiefly the silent switch back to text.

A prompt-only harness cannot see this: its subject has no `grep`. Ours does, and
the most valuable finding the old checklist ever produced was exactly this shape —
questions answered with `grep` because the engine could not express them, with
nothing in the transcript announcing the switch.
"""

from harness.signals import EngineUse, classify, measure
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


class TestEngineUse:
    """The three-valued reach (`decisions.md` 2026-08-25).

    A boolean could not carry the transcript that forced this: a `Skill` call
    holding an entire Datalog program, with nothing executed. That cell produced
    no answer of its own and was still counted as having used the engine.
    """

    def test_no_engine_and_no_program_is_none(self):
        transcript = Transcript(
            cell_id="t",
            tool_calls=[
                ToolCall(1, "Read", {"file_path": "member.csv"}),
                ToolCall(2, "Grep", {"pattern": "u01"}),
                ToolCall(3, "Write", {"file_path": "answer.txt"}),
            ],
        )
        assert classify(transcript) is EngineUse.NONE

    def test_the_skill_call_that_never_ran_is_invoked(self):
        """The case this enum exists for."""
        transcript = Transcript(
            cell_id="t",
            tool_calls=[
                ToolCall(
                    1, "Skill", {"command": "datalog", "args": "ancestor(X,Y) :- parent(X,Y)."}
                ),
                ToolCall(2, "Write", {"file_path": "answer.txt"}),
            ],
        )
        assert classify(transcript) is EngineUse.INVOKED

    def test_a_program_written_but_never_run_is_invoked(self):
        transcript = Transcript(
            cell_id="t",
            tool_calls=[
                ToolCall(1, "Write", {"file_path": "q.dl", "content": "p(X) :- q(X)."}),
                ToolCall(2, "Write", {"file_path": "answer.txt"}),
            ],
        )
        assert classify(transcript) is EngineUse.INVOKED

    def test_running_it_and_then_answering_is_answered_from(self):
        transcript = Transcript(
            cell_id="t",
            tool_calls=[
                ToolCall(1, "Write", {"file_path": "q.dl", "content": "p(X) :- q(X)."}),
                ToolCall(2, "Bash", {"command": "datalog q.dl -q 'p(X)'"}),
                ToolCall(3, "Write", {"file_path": "answer.txt"}),
            ],
        )
        assert classify(transcript) is EngineUse.ANSWERED_FROM

    def test_a_shell_redirect_counts_as_writing_the_answer(self):
        """A subject already in a pipeline writes the file with `>`, not `Write`."""
        transcript = Transcript(
            cell_id="t",
            tool_calls=[
                ToolCall(1, "Bash", {"command": "datalog q.dl -q 'p(X)' > out.txt"}),
                ToolCall(2, "Bash", {"command": "cut -d'\"' -f2 out.txt > answer.txt"}),
            ],
        )
        assert classify(transcript) is EngineUse.ANSWERED_FROM

    def test_an_answer_written_before_the_engine_ran_is_not_answered_from(self):
        """It ran the engine *after* committing an answer, so the answer cannot
        have come from it. Ordering is the whole distinction."""
        transcript = Transcript(
            cell_id="t",
            tool_calls=[
                ToolCall(1, "Grep", {"pattern": "u01"}),
                ToolCall(2, "Write", {"file_path": "answer.txt"}),
                ToolCall(3, "Bash", {"command": "datalog q.dl -q 'p(X)'"}),
            ],
        )
        assert classify(transcript) is EngineUse.INVOKED

    def test_running_it_and_never_answering_is_invoked(self):
        transcript = Transcript(
            cell_id="t",
            tool_calls=[ToolCall(1, "Bash", {"command": "datalog q.dl -q 'p(X)'"})],
        )
        assert classify(transcript) is EngineUse.INVOKED

    def test_listing_the_skill_directory_is_not_reaching_for_it(self):
        """The false positive `engine_use.py` was written to kill: `ls` over a
        path containing the word is not use."""
        transcript = Transcript(
            cell_id="t",
            tool_calls=[
                ToolCall(1, "Bash", {"command": "ls .claude/skills/datalog/"}),
                ToolCall(2, "Write", {"file_path": "answer.txt"}),
            ],
        )
        assert classify(transcript) is EngineUse.NONE

    def test_it_rides_on_the_record_as_a_string(self):
        """`Signals.to_dict` goes to JSONL; the value has to survive the trip."""
        transcript = Transcript(
            cell_id="t",
            tool_calls=[
                ToolCall(1, "Bash", {"command": "datalog q.dl -q 'p(X)'"}),
                ToolCall(2, "Write", {"file_path": "answer.txt"}),
            ],
        )
        assert measure(transcript).to_dict()["engine_use"] == "answered-from"


class TestReachAndRunningAgree:
    """`engine_use` and `ran_engine` are two readings of one transcript, and they
    have to be consistent. They were not: `measure` counted a `Skill` invocation
    as the engine having run, while `classify` did not, so a cell whose only tool
    call was `Skill` recorded `engine_use=invoked` *and* `ran_engine=True` —
    where `invoked` means, among other things, that no engine process ever ran.
    `decisions.md` 2026-08-25 records a haiku cell that did exactly that.
    """

    def test_invoking_the_skill_is_not_the_engine_running(self):
        transcript = Transcript(cell_id="skill-only")
        transcript.tool_calls.append(ToolCall(1, "Skill", {"name": "datalog"}))

        measured = measure(transcript)

        assert measured.engine_use == "invoked"
        assert measured.ran_engine is False
        assert measured.rounds == 0
        # Still counted as having *reached* for it — that is the wide number, and
        # it is deliberately not the same one.
        assert measured.engine_calls == 1

    def test_running_it_is(self):
        transcript = Transcript(cell_id="ran")
        transcript.tool_calls.append(ToolCall(1, "Bash", {"command": "datalog q.dl"}))

        measured = measure(transcript)

        assert measured.ran_engine is True
        assert measured.rounds == 1

    def test_the_two_readings_agree_on_every_shape(self):
        shapes = [
            [],
            [ToolCall(1, "Grep", {"pattern": "x"})],
            [ToolCall(1, "Skill", {"name": "datalog"})],
            [ToolCall(1, "Write", {"file_path": "q.dl", "content": "a(X) :- b(X)."})],
            [ToolCall(1, "Bash", {"command": "datalog q.dl"})],
            [
                ToolCall(1, "Skill", {"name": "datalog"}),
                ToolCall(2, "Bash", {"command": "datalog q.dl"}),
                ToolCall(3, "Write", {"file_path": "answer.txt", "content": "x"}),
            ],
        ]
        for calls in shapes:
            transcript = Transcript(cell_id="t")
            transcript.tool_calls.extend(calls)
            measured = measure(transcript)

            if measured.engine_use == "none":
                assert measured.ran_engine is False
                assert measured.engine_calls == 0
            if measured.engine_use == "answered-from":
                assert measured.ran_engine is True
            # Running it is one way of reaching for it, so the narrow count can
            # never exceed the wide one.
            assert measured.rounds <= measured.engine_calls
            assert measured.ran_engine == (measured.rounds > 0)
