"""The real subject: a Claude Code agent, driven by the Agent SDK.

Deliberately the *same* agent on both arms — filesystem, bash, search — because
the independent variable is the engine, not the tooling (``decisions.md``,
2026-08-21). Two consequences run through this module:

- **The operator's machine is not part of the experiment.** ``setting_sources``
  never includes ``"user"``, so a globally-installed skill cannot leak into either
  arm, and ``arms.scrubbed_path`` keeps a globally-installed binary out of the
  prose arm.
- **Nothing about the engine is said out loud.** The prompt is the same on both
  arms; only the workspace differs. That is what leaves *"did it reach for the
  engine?"* free to be measured rather than instructed.
"""

from __future__ import annotations

import asyncio
import time

from claude_agent_sdk import (
    AssistantMessage,
    ClaudeAgentOptions,
    HookMatcher,
    ResultMessage,
    SandboxSettings,
    TextBlock,
    query,
)

from harness.arms import Workspace
from harness.cell import Cell
from harness.confine import violation
from harness.engine_use import program_from_call
from harness.runner import FATAL as runner_fatal
from harness.transcript import ToolCall, Transcript, Usage

#: The subject gets what an agent normally has, minus the web. A subject that can
#: look up the answer is not answering the question.
ALLOWED_TOOLS = ["Read", "Write", "Edit", "Bash", "Grep", "Glob"]
DISALLOWED_TOOLS = ["WebSearch", "WebFetch"]

#: Stopping rule. Recorded in the run metadata, because a cell that ran out of
#: turns and one that finished are not the same evidence.
DEFAULT_MAX_TURNS = 30

#: Hard per-cell ceiling. A runaway loop should cost one cell, not a run.
DEFAULT_MAX_BUDGET_USD = 2.00

#: OS-level confinement for `bash` (macOS/Linux). The subject's workspace is
#: self-contained — fixtures, and on the engine arm the binary and skill are
#: materialized inside it — so nothing legitimate needs to leave.
#:
#: `allowUnsandboxedCommands: False` closes the `dangerouslyDisableSandbox`
#: escape, and the network is denied outright: a subject that can reach the
#: network is a subject that can look up an answer, which is a validity problem
#: before it is a security one.
SANDBOX: SandboxSettings = {
    "enabled": True,
    "autoAllowBashIfSandboxed": True,
    "allowUnsandboxedCommands": False,
    "excludedCommands": [],
    "network": {
        "allowedDomains": [],
        "allowLocalBinding": False,
        "allowAllUnixSockets": False,
    },
}


class AgentSubject:
    """Runs one cell and returns a ``Transcript``.

    Implements the same protocol as ``StubSubject``, so the offline grid and a
    paid run exercise the same pipeline.
    """

    def __init__(
        self,
        max_turns: int = DEFAULT_MAX_TURNS,
        max_budget_usd: float = DEFAULT_MAX_BUDGET_USD,
        effort: str | None = None,
    ) -> None:
        self.max_turns = max_turns
        self.max_budget_usd = max_budget_usd
        self.effort = effort

    def fatal(self, error: str) -> bool:
        """The account's window closing, and its neighbours. See `runner._fatal`."""
        return bool(runner_fatal.search(error))

    def run(self, cell: Cell, workspace: Workspace) -> Transcript:
        return asyncio.run(self._run(cell, workspace))

    def options(self, cell: Cell, workspace: Workspace, hooks) -> ClaudeAgentOptions:
        return ClaudeAgentOptions(
            cwd=str(workspace.path),
            # Without this the subject does not know its working directory and
            # guesses — five wasted turns and five denials on the first real
            # cell, before it thought to ask. The directory name is a hash, so
            # naming it here tells the subject nothing about its arm.
            system_prompt=(
                f"You are working in {workspace.path}. Every file you need is in "
                "that directory, and you should not need to go outside it."
            ),
            model=cell.strength.model,
            allowed_tools=ALLOWED_TOOLS,
            disallowed_tools=DISALLOWED_TOOLS,
            permission_mode="bypassPermissions",
            # "project" reads the workspace's own .claude/, which is where the
            # engine arm's skill was copied. Never "user": see the module note.
            setting_sources=["project"] if workspace.has_engine else [],
            skills=["datalog"] if workspace.has_engine else [],
            env=workspace.env,
            hooks=hooks,
            max_turns=self.max_turns,
            max_budget_usd=self.max_budget_usd,
            effort=self.effort,
            sandbox=SANDBOX,
        )

    async def _run(self, cell: Cell, workspace: Workspace) -> Transcript:
        transcript = Transcript(cell_id=cell.id)
        turn = 0
        started = time.monotonic()

        async def capture(hook_input, tool_use_id, context):  # noqa: ARG001
            nonlocal turn
            turn += 1
            tool_name = hook_input.get("tool_name", "")
            tool_input = hook_input.get("tool_input", {}) or {}
            transcript.tool_calls.append(ToolCall(turn, tool_name, tool_input))

            # Control 4: captured in PreToolUse, so it is recorded *before* the
            # tool runs and before any result comes back. That program read the
            # skill; every later one read the diagnostics.
            if transcript.first_program is None:
                program = program_from_call(tool_name, tool_input)
                if program:
                    transcript.first_program = program
                    transcript.first_program_turn = turn

            # The gate. Under `bypassPermissions` the SDK auto-approves every
            # call before `can_use_tool` is consulted and directs callers here
            # instead. A cell that reaches outside its workspace can read the
            # answer key or, on the prose arm, run the engine — so the denial is
            # recorded and the run continues.
            reason = violation(tool_name, tool_input, workspace.path)
            if reason:
                transcript.denials.append(f"turn {turn}: {tool_name} — {reason}")
                return {
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": (f"{reason}. Work only inside this directory."),
                    }
                }
            return {}

        hooks = {"PreToolUse": [HookMatcher(hooks=[capture])]}
        texts: list[str] = []

        try:
            async for message in query(
                prompt=workspace.prompt, options=self.options(cell, workspace, hooks)
            ):
                if isinstance(message, AssistantMessage):
                    for block in message.content:
                        if isinstance(block, TextBlock):
                            texts.append(block.text)
                elif isinstance(message, ResultMessage):
                    transcript.usage = _usage(message)
                    transcript.cost_usd = message.total_cost_usd
                    if message.is_error:
                        transcript.error = message.stop_reason or "result error"
        except Exception as exc:  # noqa: BLE001 — a failed cell is data, not a crash
            transcript.error = f"{type(exc).__name__}: {exc}"

        transcript.final_text = "\n".join(texts)
        transcript.wall_seconds = round(time.monotonic() - started, 2)
        return transcript


def _usage(message: ResultMessage) -> Usage:
    usage = message.usage or {}
    return Usage(
        input_tokens=int(usage.get("input_tokens", 0) or 0),
        output_tokens=int(usage.get("output_tokens", 0) or 0),
        cache_read_tokens=int(usage.get("cache_read_input_tokens", 0) or 0),
        cache_creation_tokens=int(usage.get("cache_creation_input_tokens", 0) or 0),
    )
