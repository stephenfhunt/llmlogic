"""The local subject: the loop, the tools, and the controls that ride on them.

None of this needs a model server. The loop is exercised against scripted
replies, which is the only way a *multi-turn* defect shows up in CI — the bug
this file was started for ran correctly for one turn and died on the second,
because the per-cell configuration dict was named `call` and the inner loop
rebinds `call` to each tool call the model sends.
"""

import json

import pytest

from harness import arms, domains, local
from harness.cell import Cell, Strength
from harness.local import FINAL_ACTION, LocalSubject, Tools, action_schema, tool_schemas

pytestmark = pytest.mark.skipif(
    not (arms.DATALOG_BIN_DIR / "datalog").exists(),
    reason="datalog binary not built; the engine arm cannot be materialized",
)

TASK = next(t for t in domains.load("controls") if t.id == "department-of")


def workspace_for(arm, tmp_path):
    strength = Strength("local", "test-model", 0.0, 0.0, 32768, endpoint="http://x/v1")
    return arms.build(Cell(task=TASK, arm=arm, strength=strength), tmp_path)


def cell_for(protocol="native", model="test-model", effort=None):
    strength = Strength(
        "local",
        model,
        0.0,
        0.0,
        32768,
        endpoint="http://x/v1",
        tool_protocol=protocol,
        reasoning_effort=effort,
    )
    return Cell(task=TASK, arm="prose", strength=strength)


class Scripted(LocalSubject):
    """A subject whose endpoint is a list of replies, in order."""

    def __init__(self, replies, **kwargs):
        super().__init__(**kwargs)
        self.replies = list(replies)
        self.seen = []

    def complete(self, messages, tools=None, schema=None, **served):
        self.seen.append({"messages": list(messages), "tools": tools, "schema": schema, **served})
        return self.replies.pop(0) if self.replies else _text("")


def _text(content):
    return {"choices": [{"message": {"role": "assistant", "content": content}}], "usage": {}}


def _tool_call(name, arguments, call_id="c1", content=""):
    return {
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": content,
                    "tool_calls": [
                        {
                            "id": call_id,
                            "type": "function",
                            "function": {"name": name, "arguments": json.dumps(arguments)},
                        }
                    ],
                }
            }
        ],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5},
    }


def _action(name, arguments, thought="because"):
    return _text(json.dumps({"thought": thought, "action": name, "arguments": arguments}))


class TestTheNativeLoop:
    def test_it_survives_more_than_one_turn(self, tmp_path):
        """The regression this file exists for. Two tool-calling turns, because
        one turn passed while the second raised `unexpected keyword argument
        'id'` — the tool call itself was being splatted into the request."""
        subject = Scripted(
            [
                _tool_call("Read", {"file_path": "employee.csv"}),
                _tool_call("Grep", {"pattern": "carol"}, call_id="c2"),
                _tool_call(
                    "Write",
                    {"file_path": "answer.txt", "content": "engineering"},
                    call_id="c3",
                ),
                _text("done"),
            ]
        )
        transcript = subject.run(cell_for("native"), workspace_for("prose", tmp_path))
        assert transcript.error is None
        assert [c.name for c in transcript.tool_calls] == ["Read", "Grep", "Write"]
        assert transcript.final_text == "done"

    def test_a_malformed_call_is_counted_and_not_recorded_as_a_tool_call(self, tmp_path):
        broken = {
            "choices": [
                {
                    "message": {
                        "tool_calls": [
                            {
                                "id": "c1",
                                "type": "function",
                                "function": {"name": "Read", "arguments": "{not json"},
                            }
                        ]
                    }
                }
            ],
            "usage": {},
        }
        subject = Scripted([broken, _text("giving up")])
        transcript = subject.run(cell_for("native"), workspace_for("prose", tmp_path))
        assert transcript.malformed_calls == 1
        assert transcript.tool_calls == []

    def test_usage_accumulates_across_turns(self, tmp_path):
        subject = Scripted(
            [
                _tool_call("Read", {"file_path": "employee.csv"}),
                _tool_call("Read", {"file_path": "order.csv"}, call_id="c2"),
                _text("done"),
            ]
        )
        transcript = subject.run(cell_for("native"), workspace_for("prose", tmp_path))
        assert transcript.usage.input_tokens == 20
        assert transcript.usage.output_tokens == 10


class TestTheStructuredLoop:
    def test_one_action_per_turn_until_final(self, tmp_path):
        subject = Scripted(
            [
                _action("Read", {"file_path": "employee.csv"}),
                _action("Write", {"file_path": "answer.txt", "content": "engineering"}),
                _action(FINAL_ACTION, {}),
            ]
        )
        workspace = workspace_for("prose", tmp_path)
        transcript = subject.run(cell_for("structured"), workspace)
        assert [c.name for c in transcript.tool_calls] == ["Read", "Write"]
        assert (workspace.path / "answer.txt").read_text() == "engineering"

    def test_the_request_carries_the_schema_and_no_tools_array(self, tmp_path):
        subject = Scripted([_action(FINAL_ACTION, {})])
        subject.run(cell_for("structured"), workspace_for("prose", tmp_path))
        assert subject.seen[0]["tools"] is None
        assert subject.seen[0]["schema"] == action_schema(has_engine=False)

    def test_final_has_nowhere_to_put_an_answer(self):
        """Giving `final` a text field is an invitation to answer *there*, and
        qwen3 took it: four cells of `no-answer` it had actually got right."""
        variants = {v["properties"]["action"]["const"]: v for v in action_schema(True)["oneOf"]}
        assert variants[FINAL_ACTION]["properties"]["arguments"]["properties"] == {}

    def test_every_action_carries_somewhere_to_think(self):
        """Parity, not courtesy: `native` lets a model reason in the assistant
        message alongside its call, and the first structured schema gave it
        nowhere at all. 58 cells of the 2026-08-26 sweep looped to the turn cap
        without writing an answer — what a subject that cannot plan looks like."""
        for variant in action_schema(has_engine=True)["oneOf"]:
            assert "thought" in variant["properties"]
            assert "thought" in variant["required"]

    def test_the_thought_is_kept_as_reasoning(self, tmp_path):
        """Beside a thinking model's own chain of thought — the two are the same
        evidence about the same claim."""
        subject = Scripted(
            [
                _action("Read", {"file_path": "employee.csv"}, thought="check the file first"),
                _action(FINAL_ACTION, {}, thought="done"),
            ]
        )
        transcript = subject.run(cell_for("structured"), workspace_for("prose", tmp_path))
        assert "check the file first" in transcript.reasoning

    def test_every_action_pins_its_own_argument_names(self):
        """Constrained to *some* JSON, three of three models still invented
        `file` for `file_path`. The union is what makes that unrepresentable."""
        for variant in action_schema(has_engine=True)["oneOf"]:
            arguments = variant["properties"]["arguments"]
            assert arguments["additionalProperties"] is False


class TestPerCellConfiguration:
    def test_the_strength_chooses_the_model_and_protocol(self, tmp_path):
        """One subject instance covers a whole sweep, because the crossing is the
        grid's job — `AgentSubject` reads `cell.strength.model` the same way."""
        subject = Scripted([_action(FINAL_ACTION, {})])
        subject.run(
            cell_for("structured", model="qwen3:8b", effort="none"),
            workspace_for("prose", tmp_path),
        )
        assert subject.seen[0]["model"] == "qwen3:8b"
        assert subject.seen[0]["reasoning_effort"] == "none"
        assert subject.seen[0]["schema"] is not None

    def test_an_unknown_protocol_is_refused(self):
        with pytest.raises(ValueError, match="no such tool protocol"):
            LocalSubject(tool_protocol="interpretive-dance")


class TestTheCompletionReminder:
    """A real agent harness checks the exit condition and says something; ours
    let the subject walk away. Measured on `qwen3:8b`, the thoughts are correct
    — "Carol is listed under the 'engineering' department" — and then it finishes
    without a `Write`, because in conversation saying the answer *is* delivering
    it. This is a deliberate asymmetry with the SDK subject, recorded in
    `decisions.md`, applied identically to every arm."""

    def test_finishing_without_an_answer_is_questioned_once(self, tmp_path):
        subject = Scripted(
            [
                _action(FINAL_ACTION, {}),
                _action("Write", {"file_path": "answer.txt", "content": "engineering"}),
                _action(FINAL_ACTION, {}),
            ]
        )
        workspace = workspace_for("prose", tmp_path)
        transcript = subject.run(cell_for("structured"), workspace)
        assert (workspace.path / "answer.txt").read_text() == "engineering"
        assert transcript.error is None

    def test_the_reminder_says_nothing_about_the_answer(self):
        """Only that the file already asked for is not there. Anything more
        would be coaching the subject on the task."""
        assert "answer.txt" in local.REMINDER
        for leak in ("engineering", "correct", "hint", "try"):
            assert leak not in local.REMINDER.lower()

    def test_a_subject_that_ignores_it_is_let_go(self, tmp_path):
        """Bounded: one that ignores two reminders will not write the file on the
        third, and the turn cap should not be spent finding out."""
        subject = Scripted([_action(FINAL_ACTION, {})] * 6)
        transcript = subject.run(cell_for("structured"), workspace_for("prose", tmp_path))
        assert transcript.final_text == "(finished)"
        assert len(subject.seen) == local.COMPLETION_REMINDERS + 1

    def test_a_subject_that_already_answered_is_not_nagged(self, tmp_path):
        workspace = workspace_for("prose", tmp_path)
        (workspace.path / "answer.txt").write_text("engineering")
        subject = Scripted([_action(FINAL_ACTION, {})])
        subject.run(cell_for("structured"), workspace)
        assert len(subject.seen) == 1


class TestControlsThatRideOnTheLoop:
    def test_the_first_program_is_captured_before_the_tool_runs(self, tmp_path):
        """Control 4. The program written off the skill, not off a diagnostic."""
        subject = Scripted(
            [
                _tool_call("Write", {"file_path": "q.dl", "content": "answer(X) :- fact(X)."}),
                _text("done"),
            ]
        )
        transcript = subject.run(cell_for("native"), workspace_for("engine", tmp_path))
        assert transcript.first_program == "answer(X) :- fact(X)."
        assert transcript.first_program_turn == 1

    def test_the_tools_are_spelled_the_way_the_sdk_spells_them(self):
        """`engine_use` keys on these names; a different spelling makes control 4
        go blind while still reporting a number."""
        names = {t["function"]["name"] for t in tool_schemas(has_engine=True)}
        assert {"Read", "Write", "Edit", "Bash", "Grep", "Glob", "Skill"} == names
        assert "Skill" not in {t["function"]["name"] for t in tool_schemas(has_engine=False)}

    def test_the_skill_is_advertised_verbatim_from_its_frontmatter(self):
        """Writing a fresh description would tell the local subject something the
        SDK subject was never told — control 3, lost where nobody would look."""
        name, description = local.skill_advertisement()
        source = (arms.DATALOG_SKILL_DIR / "SKILL.md").read_text()
        assert name == "datalog"
        for fragment in description.split():
            if len(fragment) > 8:
                assert fragment.strip(",.") in source

    def test_a_call_that_leaves_the_workspace_is_denied_and_recorded(self, tmp_path):
        workspace = workspace_for("prose", tmp_path)
        result = Tools(workspace).run("Read", {"file_path": "/etc/passwd"})
        assert result.denied
        assert "outside the cell's workspace" in result.text

    def test_a_denial_reaches_the_transcript(self, tmp_path):
        subject = Scripted(
            [
                _tool_call("Read", {"file_path": "/etc/passwd"}),
                _text("fine"),
            ]
        )
        transcript = subject.run(cell_for("native"), workspace_for("prose", tmp_path))
        assert len(transcript.denials) == 1


class TestOneTurnCannotOutliveItsCell:
    """Measured on a real sweep: `llama3.1:8b` ran one completion to 11,963
    tokens at 56 t/s and was still going when it was killed. `max_turns` and the
    cell's wall clock both bound the *loop*; nothing bounded the turn."""

    def _fake_urlopen(self, monkeypatch, raises=None):
        seen = {}

        class Response:
            def __enter__(self):
                return self

            def __exit__(self, *args):
                return False

            def read(self):
                return json.dumps(_text("ok")).encode()

        def urlopen(request, timeout=None):
            seen["timeout"] = timeout
            seen["payload"] = json.loads(request.data.decode())
            if raises:
                raise raises
            return Response()

        monkeypatch.setattr(local.urllib.request, "urlopen", urlopen)
        return seen

    def test_a_completion_is_capped_in_tokens(self, monkeypatch):
        seen = self._fake_urlopen(monkeypatch)
        LocalSubject().complete([{"role": "user", "content": "hi"}])
        assert seen["payload"]["max_tokens"] == local.DEFAULT_MAX_OUTPUT_TOKENS

    def test_the_call_is_clamped_by_what_is_left_of_the_cell(self, monkeypatch):
        import time as clock

        seen = self._fake_urlopen(monkeypatch)
        LocalSubject(request_timeout=600).complete(
            [{"role": "user", "content": "hi"}], deadline=clock.monotonic() + 3
        )
        assert seen["timeout"] < 600

    def test_a_timed_out_request_is_a_stopping_rule_not_an_error(self, monkeypatch, tmp_path):
        """As an ERROR it is a cell `resume` owes forever — it would time out
        again on every sitting and the run could never finish."""
        from harness.runner import STOPPING_RULE

        self._fake_urlopen(monkeypatch, raises=TimeoutError("timed out"))
        transcript = LocalSubject().run(cell_for("native"), workspace_for("prose", tmp_path))
        assert transcript.error == local._OUT_OF_TIME
        assert STOPPING_RULE.search(transcript.error)


def _fake_models(ids):
    """Stand in for the endpoint's `/models` listing."""

    class Response:
        def __enter__(self):
            return self

        def __exit__(self, *args):
            return False

        def read(self):
            return json.dumps({"data": [{"id": name} for name in ids]}).encode()

    def urlopen(url, timeout=None):
        return Response()

    return urlopen


class TestAConversationCannotOutgrowItsWindow:
    """Measured on the first correct-context sweep: the largest fixture in it was
    ~275 tokens and the worst cell reached 762,441 input tokens over 30 turns.
    The fact base is not what fills the window — the conversation is."""

    def test_a_cell_that_fills_its_window_stops(self, tmp_path):
        """A turn that keeps working — tool calls, not a final answer — while the
        transcript grows past the window it is being sent into."""
        wordy = _tool_call("Read", {"file_path": "employee.csv"}, content="x" * 120_000)
        subject = Scripted([wordy, wordy, wordy, _text("done")])
        transcript = subject.run(cell_for("native"), workspace_for("prose", tmp_path))
        assert transcript.error == local._OUT_OF_CONTEXT
        assert len(transcript.tool_calls) < 3  # it stopped rather than carrying on

    def test_stopping_beats_being_silently_truncated(self):
        """The server shifts the far end of the window, which is where the
        question is. A subject answering a question it can no longer see still
        produces a verdict, and the verdict reads like reasoning."""
        from harness.runner import STOPPING_RULE

        assert STOPPING_RULE.search(local._OUT_OF_CONTEXT)

    def test_a_short_conversation_is_left_alone(self, tmp_path):
        subject = Scripted([_tool_call("Read", {"file_path": "employee.csv"}), _text("done")])
        transcript = subject.run(cell_for("native"), workspace_for("prose", tmp_path))
        assert transcript.error is None


class TestSweepPlumbing:
    def test_a_strength_name_is_an_identifier_not_a_path(self):
        """It lands in `Cell.id`, which names a transcript file."""
        for strength in local.strengths(["qwen2.5-coder:7b"], ["native", "structured"]):
            assert "/" not in strength.name and ":" not in strength.name
            assert strength.is_local

    def test_every_model_crosses_every_protocol(self):
        built = local.strengths(["a", "b"], ["native", "structured"])
        assert len(built) == 4
        assert len({s.name for s in built}) == 4

    def test_preflight_refuses_a_window_smaller_than_the_run_assumes(self, monkeypatch):
        """The defect this check exists for cost a whole sweep and announced
        nothing: models declaring 32,768 tokens were served at ollama's default
        4,096, so every cell ran in a quarter of the window its strength claimed
        and the run produced plausible, worthless numbers."""
        monkeypatch.setattr(local, "served_context", lambda endpoint, model: 4096)
        monkeypatch.setattr(local.urllib.request, "urlopen", _fake_models({"qwen3:8b"}))
        problems = local.preflight("http://x/v1", ["qwen3:8b"], min_context=32768)
        assert problems and "4096-token context" in problems[0]

    def test_preflight_does_not_invent_a_refusal_it_cannot_justify(self, monkeypatch):
        """A server with no `/api/ps` — vLLM — cannot answer, and an unanswerable
        check is not a failure."""
        monkeypatch.setattr(local, "served_context", lambda endpoint, model: None)
        monkeypatch.setattr(local.urllib.request, "urlopen", _fake_models({"qwen3:8b"}))
        assert local.preflight("http://x/v1", ["qwen3:8b"], min_context=32768) == []

    def test_preflight_reports_a_server_that_is_not_there(self):
        problems = local.preflight("http://127.0.0.1:9/v1", ["whatever"])
        assert problems and "no model server" in problems[0]

    def test_fatal_errors_end_the_run_not_the_cell(self):
        subject = LocalSubject()
        assert subject.fatal("URLError: connection refused")
        assert subject.fatal("model requires more system memory")
        assert not subject.fatal("the model wrote a bad answer")

    def test_the_wall_clock_reads_as_a_stopping_rule_not_an_error(self):
        """Otherwise the cell records ERROR, and an ERROR cell is one `resume`
        owes forever — it would time out again on every sitting."""
        from harness.runner import STOPPING_RULE

        assert STOPPING_RULE.search(local._OUT_OF_TIME)
