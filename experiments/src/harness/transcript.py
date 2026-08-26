"""What a subject did, in a form both the real driver and the stub produce.

Kept deliberately thin: the runner and ``signals.py`` depend on this shape, not
on the Agent SDK's, so the offline stub and the real subject are interchangeable
and the ``--dry-run`` grid exercises the same code path a paid run does.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class ToolCall:
    """One tool invocation. ``turn`` orders them; ``name`` is the SDK's tool name."""

    turn: int
    name: str
    input: dict


@dataclass(frozen=True)
class Usage:
    input_tokens: int = 0
    output_tokens: int = 0
    cache_read_tokens: int = 0
    cache_creation_tokens: int = 0

    def __add__(self, other: Usage) -> Usage:
        return Usage(
            self.input_tokens + other.input_tokens,
            self.output_tokens + other.output_tokens,
            self.cache_read_tokens + other.cache_read_tokens,
            self.cache_creation_tokens + other.cache_creation_tokens,
        )


@dataclass
class Transcript:
    cell_id: str
    tool_calls: list[ToolCall] = field(default_factory=list)
    final_text: str = ""
    usage: Usage = field(default_factory=Usage)
    wall_seconds: float = 0.0
    error: str | None = None
    #: Tool calls refused for leaving the workspace. Recorded rather than
    #: silently dropped: a cell that kept trying to read the answer key is a
    #: fact about the run, and a spike here means the containment is doing
    #: work the fixtures should have made unnecessary.
    denials: list[str] = field(default_factory=list)
    #: What the SDK says the cell cost. Preferred over the harness's own
    #: price arithmetic when present — it knows about cache reads and any
    #: mid-run model fallback, and the price table here can go stale.
    cost_usd: float | None = None

    #: What the model thought, per completion, when it thinks out loud. Kept
    #: because it is the *thing under test*: S1's claim is that a model's own
    #: chain of thought is error-prone where an engine is reliable, and a harness
    #: that discards the chain of thought cannot show the failure it is named
    #: for. Empty for a model that does not reason separately, and for every SDK
    #: cell — the Agent SDK does not hand it back.
    reasoning: list[str] = field(default_factory=list)

    #: Tool calls the model emitted with arguments that would not parse. A weak
    #: model fails at tool *syntax* as well as at reasoning, and folding the two
    #: together puts a tool-calling failure into a reasoning number — the engine
    #: arm losing because the subject could not spell a `Bash` call is an
    #: instrument result, not a finding about logic. Always 0 for the SDK
    #: subject, which never hands over a malformed call.
    malformed_calls: int = 0

    #: The first Datalog program the subject wrote, captured **before any tool
    #: result came back** — that one read the skill; every later one read the
    #: diagnostics. Control 4, and the half of S1 that a final-answer-only
    #: harness cannot see.
    first_program: str | None = None
    first_program_turn: int | None = None

    @property
    def turns(self) -> int:
        return max((c.turn for c in self.tool_calls), default=0)
