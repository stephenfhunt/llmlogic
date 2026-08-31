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


# ---- Provenance: did it ask the engine *why*, and did anything follow? -------
#
# The baseline these pin against is a measurement, not an assumption: **zero
# `?why` / `?whynot` invocations across 1,812 archived transcripts**, every run
# on disk, `engine-briefed` cells included — with `SKILL.md`'s worked examples
# in the prompt. `engine-briefed-provenance` exists to move that number, and
# these tests are what stop a bug in the counter from moving it instead.


def test_a_subject_that_never_asked_why():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "q.dl", "content": "p(X) :- q(X)."}),
            ToolCall(2, "Bash", {"command": "datalog q.dl -q 'p(X)'"}),
            ToolCall(3, "Write", {"file_path": "answer.txt", "content": "a"}),
        ],
    )
    signals = measure(transcript)
    assert signals.provenance_use == "none"
    assert signals.provenance_runs == 0


def test_asking_whynot_and_then_repairing_is_acted_on():
    """The shape the arm was built to produce: empty result, ask, repair."""
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "q.dl", "content": "p(X) :- q(X)."}),
            ToolCall(2, "Bash", {"command": "datalog q.dl -q 'p(X)'"}),
            ToolCall(3, "Bash", {"command": "datalog q.dl -q '?whynot p(\"a\")'"}),
            ToolCall(4, "Write", {"file_path": "q.dl", "content": "p(X) :- r(X)."}),
            ToolCall(5, "Write", {"file_path": "answer.txt", "content": "a"}),
        ],
    )
    signals = measure(transcript)
    assert signals.provenance_use == "acted-on"
    assert signals.provenance_runs == 1


def test_asking_on_the_way_out_the_door_is_not_acted_on():
    """It ran, and nothing followed. That is not a repair, and counting it as
    one would let a cell that learned nothing read as the arm working."""
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "q.dl", "content": "p(X) :- q(X)."}),
            ToolCall(2, "Write", {"file_path": "answer.txt", "content": ""}),
            ToolCall(3, "Bash", {"command": "datalog q.dl -q '?whynot p(\"a\")'"}),
        ],
    )
    signals = measure(transcript)
    assert signals.provenance_use == "asked"
    # The count is what keeps this distinguishable from the case below.
    assert signals.provenance_runs == 1


def test_a_goal_written_down_but_never_run_explains_nothing():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "q.dl", "content": '?whynot p("a").'}),
            ToolCall(2, "Write", {"file_path": "answer.txt", "content": ""}),
        ],
    )
    signals = measure(transcript)
    assert signals.provenance_use == "asked"
    assert signals.provenance_runs == 0


def test_reading_a_file_that_mentions_whynot_is_not_asking():
    """The near-miss this module exists for. `engine_use.py`'s docstring records
    the original: `ls .../skills/datalog/` recorded as a first program. Reading
    the manual, which documents `?whynot`, must not record as using it."""
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Read", {"file_path": ".claude/skills/datalog/SKILL.md"}),
            ToolCall(2, "Grep", {"pattern": "?whynot"}),
            ToolCall(3, "Write", {"file_path": "answer.txt", "content": ""}),
        ],
    )
    signals = measure(transcript)
    assert signals.provenance_use == "none"
    assert signals.provenance_runs == 0


def test_why_does_not_swallow_whynot():
    """Two sigils, two questions. A counter that merged them would report a
    `?whynot` run as a `?why` one and lose which the subject actually asked."""
    from harness.engine_use import _PROVENANCE_GOAL

    assert _PROVENANCE_GOAL.findall("datalog q.dl -q '?whynot p(\"a\")'") == ["?whynot"]
    assert _PROVENANCE_GOAL.findall("datalog q.dl -q '?why p(\"a\")'") == ["?why"]


# ---- The counterfactual: ad-hoc code, which nothing recorded for four grids --
#
# The founding decision names ad-hoc code as the honest alternative to a logic
# engine. Measured retroactively over the 2026-08-31 Haiku ladder, `prose` wrote
# and ran a script in **36 of 44** cells and `engine-briefed` in **0 of 44** — so
# that run's `+0 at every rung` was a script tying with the engine, not prose
# tying with it. These pin the counter that says so.


def test_writing_and_running_a_script_then_answering():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "solve.py", "content": "print(1)"}),
            ToolCall(2, "Bash", {"command": "python3 solve.py > out.txt"}),
            ToolCall(3, "Write", {"file_path": "answer.txt", "content": "a"}),
        ],
    )
    signals = measure(transcript)
    assert signals.script_use == "answered-from"
    assert signals.script_runs == 1


def test_a_heredoc_into_python_is_authoring_and_running_at_once():
    """The shape a `.py`-suffix test misses entirely, and the one a subject
    already inside a pipeline actually uses."""
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Bash", {"command": "python3 <<'EOF'\nprint(1)\nEOF"}),
            ToolCall(2, "Write", {"file_path": "answer.txt", "content": "a"}),
        ],
    )
    signals = measure(transcript)
    assert signals.script_use == "answered-from"
    assert signals.script_runs == 1


def test_a_script_written_but_never_run():
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "solve.py", "content": "print(1)"}),
            ToolCall(2, "Write", {"file_path": "answer.txt", "content": "a"}),
        ],
    )
    assert measure(transcript).script_use == "wrote"


def test_reading_a_python_file_is_not_running_one():
    """`static_analysis` hands the subject a source tree to read. If reading it
    counted, every cell of that domain would record as having written code."""
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Read", {"file_path": "sqlparse/engine/filter_stack.py"}),
            ToolCall(2, "Bash", {"command": "cat sqlparse/sql.py"}),
            ToolCall(3, "Grep", {"pattern": "def ", "path": "sqlparse"}),
            ToolCall(4, "Write", {"file_path": "answer.txt", "content": "a"}),
        ],
    )
    signals = measure(transcript)
    assert signals.script_use == "none"
    assert signals.script_runs == 0


def test_the_engine_is_not_a_script():
    """The two counters partition the same transcript; a cell that ran only the
    engine must not also read as having brought its own code."""
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "q.dl", "content": "p(X) :- q(X)."}),
            ToolCall(2, "Bash", {"command": "datalog q.dl -q 'p(X)' > answer.txt"}),
        ],
    )
    signals = measure(transcript)
    assert signals.script_use == "none"
    assert signals.engine_use == "answered-from"


def test_redirecting_an_interpreter_into_the_answer_is_answering_from_it():
    """One call that runs the script and writes the answer out of its stdout.
    The dominant idiom, not an edge case: 11 of the 14 cells a strict `>` put in
    `wrote` on the 2026-08-31 ladder were exactly this shape."""
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "solve.py", "content": "print(1)"}),
            ToolCall(2, "Bash", {"command": "python3 solve.py > answer.txt"}),
        ],
    )
    assert measure(transcript).script_use == "answered-from"


def test_the_same_holds_for_the_engine():
    """`datalog q.dl -q 'goal(X)' > answer.txt` — the strongest form of
    answering from the engine, and read as `invoked` (non-compliance, on a
    mandated arm) until 2026-08-31."""
    transcript = Transcript(
        cell_id="t",
        tool_calls=[
            ToolCall(1, "Write", {"file_path": "q.dl", "content": "p(X) :- q(X)."}),
            ToolCall(2, "Bash", {"command": "datalog q.dl -q 'p(X)' > answer.txt"}),
        ],
    )
    signals = measure(transcript)
    assert signals.engine_use == "answered-from"
    assert signals.ran_engine is True
