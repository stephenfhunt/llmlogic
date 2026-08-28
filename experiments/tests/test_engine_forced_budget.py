"""What `engine-forced` was actually measuring, and the three fixes for it.

Measured 2026-08-27 over every run on disk: `engine-forced` ended at its turn cap
in **48%** of cells against 16% for `prose`, 75% of those wrote no answer, and its
5%-correct was substantially a reading of that cap rather than of the engine. Two
further causes sat underneath — a mandate with no legal move once the engine
refuses a program, and an answer contract the engine's own output format violates.

Each test here pins one of the three, and between them they pin the thing that
made the defect invisible for three sessions: a cell stopped by the turn cap
recorded *nothing* in its transcript.
"""

import json

import pytest

from harness import arms, catalogue, domains, local
from harness.cell import ARM_BUDGET, Cell, Strength, budget_for
from harness.grade import Grade, Verdict
from harness.local import ABANDON_ACTION, FINAL_ACTION, LocalSubject, action_schema
from harness.transcript import Transcript

pytestmark = pytest.mark.skipif(
    not (arms.DATALOG_BIN_DIR / "datalog").exists(),
    reason="datalog binary not built; the engine arm cannot be materialized",
)

TASK = next(t for t in domains.load("controls") if t.id == "department-of")


def _strength():
    return Strength(
        "local", "test-model", 0.0, 0.0, 32768, endpoint="http://x/v1", tool_protocol="structured"
    )


def _cell(arm):
    return Cell(task=TASK, arm=arm, strength=_strength())


def _action(name, arguments, thought="because"):
    body = json.dumps({"thought": thought, "action": name, "arguments": arguments})
    return {"choices": [{"message": {"role": "assistant", "content": body}}], "usage": {}}


class Scripted(LocalSubject):
    def __init__(self, replies, **kwargs):
        super().__init__(**kwargs)
        self.replies = list(replies)
        self.calls = 0

    def complete(self, messages, tools=None, schema=None, **served):
        self.calls += 1
        return self.replies.pop(0) if self.replies else _action("Read", {"file_path": "x"})


class TestTheArmsDoNotShareOneBudget:
    """An equal cap over unequal work is a handicap, not a held constant."""

    def test_engine_forced_is_given_more_turns_than_prose(self, tmp_path):
        """`prose` answers in read-then-write; `engine-forced` must write a
        program, run it, read the diagnostic, repair and re-run before it can
        write anything at all."""
        assert budget_for("engine-forced") > budget_for("prose")
        assert budget_for("prose") == budget_for("engine") == 1.0

    def test_the_extra_turns_are_actually_spent(self, tmp_path):
        """Not just declared in a constant — the loop has to run that many."""
        turns = {}
        for arm in ("prose", "engine-forced"):
            subject = Scripted([], max_turns=4, max_cell_seconds=600)
            workspace = arms.build(_cell(arm), tmp_path / arm)
            subject.run(_cell(arm), workspace)
            turns[arm] = subject.calls
        assert turns["engine-forced"] == turns["prose"] * ARM_BUDGET["engine-forced"]

    def test_the_wall_clock_scales_with_the_turns(self, tmp_path):
        """Scaling only the turns would move which stopping rule fires, not
        remove it: `engine-forced` averaged 617s against a 900s cap already."""
        seen = {}

        class Deadlines(Scripted):
            def complete(self, messages, tools=None, schema=None, **served):
                seen.setdefault(self._arm, served["deadline"])
                return super().complete(messages, tools, schema, **served)

        for arm in ("prose", "engine-forced"):
            subject = Deadlines([_action(FINAL_ACTION, {})], max_turns=4, max_cell_seconds=100)
            subject._arm = arm
            workspace = arms.build(_cell(arm), tmp_path / f"d-{arm}")
            subject.run(_cell(arm), workspace)
        assert seen["engine-forced"] - seen["prose"] == pytest.approx(100, abs=5)


class TestACellStoppedByTheTurnCapSaysSo:
    """The silence that hid the defect. Before this, the loop simply ended."""

    def test_running_out_of_turns_is_recorded(self, tmp_path):
        subject = Scripted([], max_turns=2, max_cell_seconds=600)
        workspace = arms.build(_cell("prose"), tmp_path)
        transcript = subject.run(_cell("prose"), workspace)
        assert transcript.error == local._OUT_OF_TURNS

    def test_it_is_a_stopping_rule_and_not_an_instrument_failure(self):
        """So the cell is still graded on what it left behind, rather than
        discarded — the distinction `runner` already draws for the wall clock."""
        from harness.runner import STOPPING_RULE

        assert STOPPING_RULE.search(local._OUT_OF_TURNS)

    def test_finishing_cleanly_records_no_stopping_rule(self, tmp_path):
        """The answer file has to exist, or the completion reminder sends the
        subject round again and it reaches the cap after all — which is the
        reminder working, and is why this test writes one."""
        subject = Scripted([_action(FINAL_ACTION, {})], max_turns=4, max_cell_seconds=600)
        workspace = arms.build(_cell("prose"), tmp_path)
        (workspace.path / "answer.txt").write_text("engineering\n")
        transcript = subject.run(_cell("prose"), workspace)
        assert transcript.error is None


class TestTheEngineArmsCanSayTheEngineIsUnusable:
    """The missing legal move. `engine-forced` forbids answering from anything
    but the engine, so a subject whose program will not run had nothing to do
    but loop until a stopping rule ended it."""

    def test_the_action_is_offered_only_where_the_engine_is(self):
        with_engine = json.dumps(action_schema(True))
        without = json.dumps(action_schema(False))
        assert ABANDON_ACTION in with_engine
        assert ABANDON_ACTION not in without

    def test_it_requires_a_reason_because_the_reason_is_the_measurement(self):
        variant = next(
            v
            for v in action_schema(True)["oneOf"]
            if v["properties"]["action"].get("const") == ABANDON_ACTION
        )
        assert variant["properties"]["arguments"]["required"] == ["reason"]

    def test_both_protocols_offer_the_same_move(self):
        """Otherwise the protocol becomes a second independent variable."""
        assert ABANDON_ACTION in json.dumps(local.abandon_schema())
        assert ABANDON_ACTION in local.protocol_brief(has_engine=True)
        assert ABANDON_ACTION not in local.protocol_brief(has_engine=False)

    def test_declaring_it_ends_the_cell_and_keeps_the_reason(self, tmp_path):
        subject = Scripted(
            [_action(ABANDON_ACTION, {"reason": "import is rejected every way I write it"})],
            max_turns=8,
            max_cell_seconds=600,
        )
        workspace = arms.build(_cell("engine-forced"), tmp_path)
        transcript = subject.run(_cell("engine-forced"), workspace)
        assert transcript.abandoned == "import is rejected every way I write it"
        assert subject.calls == 1

    def test_it_is_not_nudged_back_into_the_loop(self, tmp_path):
        """The completion reminder exists for a subject that has an answer and
        forgot the file. This one is saying it has none, and reminding it would
        spend the turns the action exists to save."""
        subject = Scripted(
            [_action(ABANDON_ACTION, {"reason": "no"}), _action(FINAL_ACTION, {})],
            max_turns=8,
            max_cell_seconds=600,
        )
        workspace = arms.build(_cell("engine-forced"), tmp_path)
        subject.run(_cell("engine-forced"), workspace)
        assert subject.calls == 1


class TestTheDeclarationBecomesItsOwnVerdict:
    def test_an_abandoned_cell_with_no_answer_is_engine_unusable(self):
        from harness.runner import _verdict_for

        graded = Grade(Verdict.NO_ANSWER, None, None)
        transcript = Transcript(cell_id="c", abandoned="the engine rejects every import")
        assert _verdict_for(transcript, graded).verdict is Verdict.ENGINE_UNUSABLE

    def test_a_declaration_over_a_real_answer_is_graded_on_the_answer(self):
        """What it produced is the better evidence, and the two would otherwise
        disagree in the record."""
        from harness.runner import _verdict_for

        graded = Grade(Verdict.CORRECT, None, "engineering")
        transcript = Transcript(cell_id="c", abandoned="gave up")
        assert _verdict_for(transcript, graded).verdict is Verdict.CORRECT

    def test_it_is_not_correct(self):
        """Every accuracy figure treats it as a miss; only the *reason* survives."""
        assert Verdict.ENGINE_UNUSABLE != Verdict.CORRECT
        assert Verdict.ENGINE_UNUSABLE.value == "engine-unusable"


class TestTheMandateNamesTheAnswerFormat:
    """The engine prints `answer("x").`; the contract wants `x`. Measured across
    every run on disk: fact-shaped answers were 3.7% of `engine-forced` answers
    and 0% of both other arms, all graded `wrong`."""

    def test_the_mandate_distinguishes_engine_output_from_the_answer_file(self):
        assert 'answer("x").' in catalogue.MANDATE

    def test_it_tells_the_subject_about_its_honest_exit(self):
        assert ABANDON_ACTION in catalogue.MANDATE

    def test_the_mandate_is_still_the_only_difference_from_prose(self):
        """Control 3 unchanged: `engine-forced` is the base prompt plus this
        block, appended, and nothing else."""
        base = catalogue.assemble(TASK, "prose")
        forced = catalogue.assemble(TASK, "engine-forced")
        assert forced == base + catalogue.MANDATE
        assert catalogue.assemble(TASK, "engine") == base
