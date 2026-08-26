"""A local model as a subject: our own tool loop over an OpenAI-compatible endpoint.

Two of the three claims in scope need a model weaker than haiku, and there is
none (`notes/discriminating-instrument.md` §*A local subject*): *the engine helps
at the weak end*, and *a small model with the engine matches a frontier model
without it* — the strongest single number this repo could produce.

**This is a second subject, not a second experiment.** `Subject` is the seam, and
everything downstream reads a `Transcript`, so a local cell and an SDK cell are
the same evidence in the same shape. `confine.violation` and
`engine_use.program_from_call` are *imported* rather than reimplemented, which is
what makes controls 3 and 4 hold here by construction rather than by a second
careful reading.

Three things Claude Code was supplying that are built here:

- **the tool loop**, with the tools spelled exactly as the SDK spells them, so
  `signals.py` and `engine_use.py` transfer untouched;
- **the skill**, as a `Skill` tool whose description is SKILL.md's own frontmatter
  and whose call returns the body. That is what Claude Code does — advertise by
  name and description, load on invocation — and it keeps control 3: the prompt
  still never says the engine is there;
- **network isolation and a per-call timeout.** The SDK sandbox is gone, so
  `bash` runs under `unshare -rn`, and the wall-clock cap is the harness's. The
  engine has no fuel, cap or timeout **by decision** (`../datalog/spec.md`
  non-goals, rejected twice) and a weak model will write value-creating
  recursion; wrapping the binary in a `timeout` shim was rejected because it
  changes the engine arm's environment, which is the one thing held fixed.

Stdlib only. `stats.py` set the precedent: a dependency is a decision
(`AGENTS.md`), and an OpenAI-compatible POST is `urllib.request`.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path

from harness import arms
from harness.arms import Workspace
from harness.cell import Cell
from harness.confine import violation
from harness.engine_use import program_from_call
from harness.transcript import ToolCall, Transcript, Usage

#: Where the model is served. An OpenAI-compatible base URL and nothing else is
#: assumed, which is what keeps vLLM a substitution rather than a rewrite.
DEFAULT_BASE_URL = os.environ.get("HARNESS_LOCAL_BASE_URL", "http://127.0.0.1:11434/v1")
DEFAULT_MODEL = os.environ.get("HARNESS_LOCAL_MODEL", "qwen2.5-coder:7b")

#: Stopping rule, per cell. Lower than the SDK subject's 30: a small model that
#: is going to get there gets there early, and one that is not is looping.
DEFAULT_MAX_TURNS = 24

#: Wall clock for one tool call. See the module note — this is the harness's cap,
#: on the call, never a shim around the engine.
DEFAULT_TOOL_TIMEOUT = 120

#: Wall clock for one **cell**, across every turn it takes. Measured: one
#: `qwen3:8b` control cell with thinking on ran 25 turns and 60,000 characters of
#: reasoning before the turn cap stopped it. A grid left running overnight needs
#: to be bounded in the currency it is spending.
DEFAULT_MAX_CELL_SECONDS = 900

#: What a cell that ran out of wall clock records. Not fatal — that cell spent
#: its budget and the next one deserves its own.
_OUT_OF_TIME = "cell exceeded its wall clock"

#: Wall clock for one completion. A 7B on a 3060 answers in seconds; minutes
#: means the server is thrashing, and a cell that hangs is a sitting that ends.
DEFAULT_REQUEST_TIMEOUT = 600

#: How much of a tool's output goes back to the model. A weak model with a 32k
#: window that reads a whole fixture into context has no room left to think, and
#: an unbounded `Read` is how that happens without anything saying so.
MAX_TOOL_OUTPUT_CHARS = 24_000

#: Failures that are the *instrument*, not the subject. A local run has no
#: session limit; it has a server that is not there, a model that was never
#: pulled, and a context window that overflowed. Misfiling any of them as an
#: ordinary wrong answer is the 2026-08-24 defect — 46 phantom `no-answer` cells
#: — in local clothes. `runner.FATAL` is the Anthropic-shaped twin of this.
LOCAL_FATAL = re.compile(
    r"connection refused|failed to connect|not found, try pulling|"
    r"context (?:length|window) exceeded|model requires more system memory|"
    r"no such model",
    re.IGNORECASE,
)


class LocalError(Exception):
    """The endpoint could not be talked to, or answered with something unusable."""


def _fence(text: str) -> str:
    if len(text) <= MAX_TOOL_OUTPUT_CHARS:
        return text
    return text[:MAX_TOOL_OUTPUT_CHARS] + f"\n… [truncated at {MAX_TOOL_OUTPUT_CHARS} characters]"


def skill_advertisement(skill_dir: Path = arms.DATALOG_SKILL_DIR) -> tuple[str, str]:
    """The skill's ``name`` and ``description``, verbatim from its frontmatter.

    Verbatim matters. Claude Code advertises a skill by exactly these two fields,
    so using them here is what makes *"did it reach for the engine?"* the same
    question on both subjects. Writing a fresh description would be telling the
    local subject something the SDK subject was never told — control 3, lost in
    the one place nobody would look for it.
    """
    text = (skill_dir / "SKILL.md").read_text(encoding="utf-8")
    if not text.startswith("---"):
        raise LocalError(f"{skill_dir}/SKILL.md has no frontmatter to advertise")
    _, frontmatter, _ = text.split("---", 2)

    fields: dict[str, str] = {}
    key = None
    for line in frontmatter.splitlines():
        if not line.strip():
            continue
        match = re.match(r"^([a-z_]+):\s*(.*)$", line)
        if match:
            key, value = match.group(1), match.group(2).strip()
            # `>-` and `|` open a folded block; the value is the lines under it.
            fields[key] = "" if value in (">-", ">", "|", "|-") else value
        elif key:
            fields[key] = (fields[key] + " " + line.strip()).strip()
    if "name" not in fields or "description" not in fields:
        raise LocalError("SKILL.md frontmatter is missing `name` or `description`")
    return fields["name"], fields["description"]


def skill_body(skill_dir: Path = arms.DATALOG_SKILL_DIR) -> str:
    """Everything after the frontmatter, plus what else the skill ships.

    The extra files are *listed*, not inlined: Claude Code hands over SKILL.md and
    leaves the examples to be read, and inlining them here would give the local
    subject a larger skill than the SDK subject got.
    """
    text = (skill_dir / "SKILL.md").read_text(encoding="utf-8")
    body = text.split("---", 2)[2].lstrip("\n")
    extras = sorted(
        str(path.relative_to(skill_dir))
        for path in skill_dir.rglob("*")
        if path.is_file() and path.name != "SKILL.md"
    )
    if extras:
        listing = "\n".join(f"- .claude/skills/datalog/{name}" for name in extras)
        body += f"\n\nAlso available, in your working directory:\n{listing}\n"
    return body


def tool_schemas(has_engine: bool, skill: tuple[str, str] | None = None) -> list[dict]:
    """The tools the subject gets, named exactly as the SDK names them.

    The names are the load-bearing part. `engine_use.program_from_call` keys on
    ``Write``/``Edit``/``Bash`` and `engine_use.uses_engine` on ``Skill``; spell
    one of them differently here and control 4 goes quietly blind on the local
    subject while still reporting a number.
    """
    schemas = [
        _schema(
            "Read",
            "Read a file from the working directory.",
            {"file_path": ("string", "Path to the file, relative to the working directory.")},
            ["file_path"],
        ),
        _schema(
            "Write",
            "Write a file, replacing it if it exists.",
            {
                "file_path": ("string", "Path to the file."),
                "content": ("string", "The full contents to write."),
            },
            ["file_path", "content"],
        ),
        _schema(
            "Edit",
            "Replace one exact occurrence of a string in a file.",
            {
                "file_path": ("string", "Path to the file."),
                "old_string": ("string", "The exact text to replace; must occur once."),
                "new_string": ("string", "The text to replace it with."),
            },
            ["file_path", "old_string", "new_string"],
        ),
        _schema(
            "Bash",
            "Run a shell command in the working directory.",
            {"command": ("string", "The command to run.")},
            ["command"],
        ),
        _schema(
            "Grep",
            "Search files for a regular expression.",
            {
                "pattern": ("string", "A regular expression."),
                "path": ("string", "File or directory to search; defaults to everything."),
            },
            ["pattern"],
        ),
        _schema(
            "Glob",
            "List files matching a glob pattern.",
            {"pattern": ("string", "A glob, e.g. `*.csv`.")},
            ["pattern"],
        ),
    ]
    if has_engine:
        name, description = skill or skill_advertisement()
        schemas.append(
            _schema(
                "Skill",
                description,
                {"name": ("string", f"The skill to load: `{name}`.")},
                ["name"],
            )
        )
    return schemas


def _schema(name: str, description: str, properties: dict, required: list[str]) -> dict:
    return {
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": {
                    key: {"type": kind, "description": text}
                    for key, (kind, text) in properties.items()
                },
                "required": required,
            },
        },
    }


#: The action a `structured` turn must produce. Not a tool name — a subject that
#: is finished has to be able to *say* so inside the same grammar, or the only
#: way out of the loop is the turn cap.
FINAL_ACTION = "final"


def action_schema(has_engine: bool) -> dict:
    """One JSON schema covering every legal move, as a discriminated union.

    This is the whole point of the `structured` protocol. A free-form
    ``arguments`` object is not enough: constrained to *some* JSON, three of
    three models still invented ``file`` for ``file_path``, which is a malformed
    call wearing valid syntax. Pinning each action's arguments under its own
    ``const`` makes the wrong field name unrepresentable rather than unlikely.
    """
    variants = []
    for schema in tool_schemas(has_engine):
        function = schema["function"]
        parameters = dict(function["parameters"])
        parameters["additionalProperties"] = False
        variants.append(
            {
                "type": "object",
                "additionalProperties": False,
                "properties": {
                    "action": {"const": function["name"]},
                    "arguments": parameters,
                },
                "required": ["action", "arguments"],
            }
        )
    variants.append(
        {
            "type": "object",
            "additionalProperties": False,
            "properties": {
                "action": {"const": FINAL_ACTION},
                # **No text field.** Somewhere to put an answer is an invitation
                # to answer *there*, and qwen3 took it: it read the fixture,
                # derived `engineering`, returned it as `final.text` and never
                # wrote `answer.txt` — four cells of `no-answer` it had got
                # right. The prompt's contract is a file, and the protocol must
                # not offer a second way to satisfy it.
                "arguments": {
                    "type": "object",
                    "additionalProperties": False,
                    "properties": {},
                },
            },
            "required": ["action", "arguments"],
        }
    )
    return {"oneOf": variants}


def protocol_brief(has_engine: bool) -> str:
    """The tool descriptions, for a protocol that has no ``tools`` array.

    The `native` protocol carries these in the request; `structured` has to put
    them in the system message. Same text either way — a subject told *more* here
    than the tool schemas say would be a different instrument.
    """
    lines = ["You have these actions. Reply with exactly one JSON object per turn."]
    for schema in tool_schemas(has_engine):
        function = schema["function"]
        fields = ", ".join(function["parameters"]["properties"])
        lines.append(f"- {function['name']}({fields}) — {function['description']}")
    lines.append(f"- {FINAL_ACTION}(text) — you are done; the answer file is already written.")
    return "\n".join(lines)


@dataclass
class ToolResult:
    text: str
    denied: bool = False


class Tools:
    """Executing one tool call inside one workspace.

    Every path a tool touches is resolved against the workspace and checked by
    `confine.violation` **before** anything runs — the same gate the SDK subject
    gets from its PreToolUse hook, and the same function.
    """

    def __init__(self, workspace: Workspace, timeout: int = DEFAULT_TOOL_TIMEOUT) -> None:
        self.workspace = workspace
        self.root = workspace.path
        self.timeout = timeout

    def run(self, name: str, arguments: dict) -> ToolResult:
        reason = violation(name, arguments, self.root)
        if reason:
            return ToolResult(f"Denied: {reason}. Work only inside this directory.", denied=True)
        handler = getattr(self, f"_{name.lower()}", None)
        if handler is None:
            return ToolResult(f"No such tool: {name}")
        try:
            return ToolResult(_fence(handler(arguments)))
        except Exception as exc:  # noqa: BLE001 — a failed tool is a message, not a crash
            return ToolResult(f"{type(exc).__name__}: {exc}")

    def _resolve(self, raw: str) -> Path:
        path = Path(str(raw))
        return path if path.is_absolute() else self.root / path

    def _read(self, arguments: dict) -> str:
        path = self._resolve(arguments["file_path"])
        if not path.exists():
            return f"No such file: {arguments['file_path']}"
        return path.read_text(encoding="utf-8", errors="replace")

    def _write(self, arguments: dict) -> str:
        path = self._resolve(arguments["file_path"])
        path.parent.mkdir(parents=True, exist_ok=True)
        content = str(arguments.get("content", ""))
        path.write_text(content, encoding="utf-8")
        return f"Wrote {len(content)} characters to {arguments['file_path']}."

    def _edit(self, arguments: dict) -> str:
        path = self._resolve(arguments["file_path"])
        text = path.read_text(encoding="utf-8")
        old, new = str(arguments["old_string"]), str(arguments["new_string"])
        found = text.count(old)
        if found != 1:
            return f"`old_string` occurs {found} times; it must occur exactly once."
        path.write_text(text.replace(old, new), encoding="utf-8")
        return f"Edited {arguments['file_path']}."

    def _bash(self, arguments: dict) -> str:
        command = str(arguments.get("command", ""))
        if not shutil.which("unshare"):
            return (
                "Refused: this harness runs shell commands in a network namespace and "
                "`unshare` is not available, so the cell cannot be isolated."
            )
        environment = {
            "PATH": self.workspace.env.get("PATH", "/usr/bin:/bin"),
            "HOME": str(self.root),
            "PWD": str(self.root),
            "LANG": "C.UTF-8",
        }
        try:
            completed = subprocess.run(
                ["unshare", "-rn", "--", "/bin/sh", "-c", command],
                cwd=self.root,
                env=environment,
                capture_output=True,
                text=True,
                timeout=self.timeout,
            )
        except subprocess.TimeoutExpired:
            return f"Timed out after {self.timeout}s."
        output = (completed.stdout or "") + (completed.stderr or "")
        return output.strip() or f"(no output, exit {completed.returncode})"

    def _grep(self, arguments: dict) -> str:
        pattern = re.compile(str(arguments["pattern"]))
        target = self._resolve(arguments.get("path") or ".")
        files = (
            [target] if target.is_file() else sorted(p for p in target.rglob("*") if p.is_file())
        )
        hits = []
        for path in files:
            try:
                for number, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
                    if pattern.search(line):
                        hits.append(f"{path.relative_to(self.root)}:{number}:{line}")
            except OSError:
                continue
        return "\n".join(hits) if hits else "No matches."

    def _glob(self, arguments: dict) -> str:
        matches = sorted(
            str(path.relative_to(self.root)) for path in self.root.glob(str(arguments["pattern"]))
        )
        return "\n".join(matches) if matches else "No matches."

    def _skill(self, arguments: dict) -> str:  # noqa: ARG002 — one skill exists
        return skill_body()


def strengths(
    models: list[str],
    protocols: list[str],
    endpoint: str = DEFAULT_BASE_URL,
    reasoning_effort: str | None = None,
) -> tuple:
    """The local strengths a sweep crosses: every model × every protocol.

    A `Strength` per combination rather than a flag beside the run, because the
    grid crosses tasks with *strengths* — so two protocols are two columns in the
    report, two sets of records, and two things `resume` can owe. Which protocol
    a model should use is an empirical question (`Strength.tool_protocol`), and
    this is what lets the harness answer it the same way it answers every other
    one: by running both and tabling the result.
    """
    from harness.cell import Strength

    built = []
    for model in models:
        for protocol in protocols:
            # The name lands in `Cell.id`, which names a transcript file. A `:`
            # or `/` there is a path, not an identifier.
            safe = re.sub(r"[^A-Za-z0-9.]+", "-", f"{model}-{protocol}")
            built.append(
                Strength(
                    name=safe,
                    model=model,
                    input_per_mtok=0.0,
                    output_per_mtok=0.0,
                    context_tokens=32_768,
                    endpoint=endpoint,
                    tool_protocol=protocol,
                    reasoning_effort=reasoning_effort,
                )
            )
    return tuple(built)


def preflight(endpoint: str, models: list[str]) -> list[str]:
    """What would stop this run, checked before a single cell is built.

    An overnight run that dies on cell 3 because a model was never pulled has
    wasted the night, and the evidence that it did is a log nobody is awake to
    read. Everything here is knowable in one request.
    """
    url = f"{endpoint.rstrip('/')}/models"
    try:
        with urllib.request.urlopen(url, timeout=20) as response:
            served = {entry.get("id", "") for entry in json.load(response).get("data", [])}
    except Exception as exc:  # noqa: BLE001 — the reason is the message
        return [f"no model server at {endpoint} ({type(exc).__name__}: {exc})"]

    problems = []
    for model in models:
        if model not in served:
            near = sorted(name for name in served if name.split(":")[0] == model.split(":")[0])
            hint = f" — served: {', '.join(sorted(served)) or '(none)'}" if not near else ""
            problems.append(f"{model} is not served{hint}")
    return problems


class LocalSubject:
    """Runs one cell against a locally-served model, and returns a ``Transcript``.

    Implements the same protocol as ``StubSubject`` and ``AgentSubject``, so the
    grid, the grading, the signals and the report do not know the difference.
    """

    def __init__(
        self,
        base_url: str = DEFAULT_BASE_URL,
        model: str = DEFAULT_MODEL,
        max_turns: int = DEFAULT_MAX_TURNS,
        tool_timeout: int = DEFAULT_TOOL_TIMEOUT,
        request_timeout: int = DEFAULT_REQUEST_TIMEOUT,
        temperature: float = 0.0,
        reasoning_effort: str | None = None,
        tool_protocol: str = "native",
        max_cell_seconds: int = DEFAULT_MAX_CELL_SECONDS,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.model = model
        self.max_turns = max_turns
        self.tool_timeout = tool_timeout
        self.request_timeout = request_timeout
        self.temperature = temperature
        #: Passed through when set. A thinking model spends minutes and hundreds
        #: of tokens per turn on a single-hop lookup — measured on `qwen3:8b`,
        #: 12.4s and 699 output tokens against 2.0s and 21 with it off. That is
        #: the difference between a slate that fits a sitting and one that does
        #: not, so it is a knob rather than a default: whether thinking helps is
        #: itself a question this harness can ask.
        self.reasoning_effort = reasoning_effort
        #: ``native`` sends a ``tools`` array and reads ``tool_calls`` back;
        #: ``structured`` constrains the decoder to `action_schema` instead.
        #: Two protocols rather than one, because which to use is an empirical
        #: question about the subject and not a design commitment: `qwen2.5-coder`
        #: emits bare JSON that the native parser drops on the floor, and the
        #: same model under a grammar is exact. Measured, both ways, before this
        #: was written.
        if tool_protocol not in ("native", "structured"):
            raise ValueError(f"no such tool protocol: {tool_protocol}")
        self.tool_protocol = tool_protocol
        #: The cell's own wall clock. `max_turns` bounds *completions*, and a
        #: thinking model spending 60,000 characters a turn can hold a sitting
        #: open for an hour inside the cap. An overnight run needs a bound that
        #: is measured in time, because time is what it is being given.
        self.max_cell_seconds = max_cell_seconds

    def complete(
        self,
        messages: list[dict],
        tools: list[dict] | None = None,
        schema: dict | None = None,
        *,
        endpoint: str | None = None,
        model: str | None = None,
        reasoning_effort: str | None = None,
    ) -> dict:
        payload: dict = {
            "model": model or self.model,
            "messages": messages,
            "temperature": self.temperature,
            "stream": False,
        }
        if tools:
            payload["tools"] = tools
        if schema:
            # The portable spelling. vLLM reads the same field, which is what
            # keeps it a substitution rather than a rewrite.
            payload["response_format"] = {
                "type": "json_schema",
                "json_schema": {"name": "action", "strict": True, "schema": schema},
            }
        effort = reasoning_effort if reasoning_effort is not None else self.reasoning_effort
        if effort is not None:
            payload["reasoning_effort"] = effort
        request = urllib.request.Request(
            f"{(endpoint or self.base_url).rstrip('/')}/chat/completions",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(request, timeout=self.request_timeout) as response:
                return json.loads(response.read().decode())
        except urllib.error.HTTPError as exc:
            raise LocalError(f"HTTP {exc.code}: {exc.read().decode()[:400]}") from exc
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            raise LocalError(f"{type(exc).__name__}: {exc}") from exc

    def fatal(self, error: str) -> bool:
        """Errors that end the run rather than the cell.

        Consulted by `runner.run_grid`. A local run has no session limit; it has
        a server that stopped, a model that was never pulled, and a context
        window that overflowed. Left unclassified, the first of those walks the
        remaining grid in seconds recording an ERROR per cell — which is
        recoverable, since `resume` owes every ERROR cell, but it burns the
        sitting and hides when the trouble started.
        """
        return bool(LOCAL_FATAL.search(error))

    def _configure(self, cell: Cell) -> tuple[str, str, str, str | None]:
        """This cell's endpoint, model, protocol and effort.

        Read off ``cell.strength``, exactly as `AgentSubject` reads
        ``cell.strength.model``. That is what lets **one** subject instance run a
        sweep whose strengths differ in model *and* protocol — the crossing is
        the grid's job, not a second driver's. Constructor values are the
        fallback, so a subject built by hand still works.
        """
        strength = cell.strength
        return (
            (strength.endpoint or self.base_url).rstrip("/"),
            strength.model or self.model,
            strength.tool_protocol if strength.is_local else self.tool_protocol,
            strength.reasoning_effort if strength.is_local else self.reasoning_effort,
        )

    def _system(self, workspace: Workspace, protocol: str) -> str:
        base = (
            f"You are working in {workspace.path}. Every file you need is in that "
            "directory, and you should not need to go outside it."
        )
        if protocol == "structured":
            return base + "\n\n" + protocol_brief(workspace.has_engine)
        return base + " Use the tools provided; when you are finished, stop calling tools."

    def _record(self, transcript: Transcript, turn: int, name: str, arguments: dict) -> None:
        """Append the call, and take control 4's capture **before** it runs."""
        transcript.tool_calls.append(ToolCall(turn, name, arguments))
        if transcript.first_program is None:
            program = program_from_call(name, arguments)
            if program:
                transcript.first_program = program
                transcript.first_program_turn = turn

    def run(self, cell: Cell, workspace: Workspace) -> Transcript:
        transcript = Transcript(cell_id=cell.id)
        tools = Tools(workspace, self.tool_timeout)
        endpoint, model, protocol, effort = self._configure(cell)
        started = time.monotonic()
        deadline = started + self.max_cell_seconds
        served = dict(endpoint=endpoint, model=model, reasoning_effort=effort)
        messages = [
            {"role": "system", "content": self._system(workspace, protocol)},
            {"role": "user", "content": workspace.prompt},
        ]
        try:
            if protocol == "structured":
                self._structured(transcript, tools, workspace, messages, served, deadline)
            else:
                self._native(transcript, tools, workspace, messages, served, deadline)
        except LocalError as exc:
            transcript.error = str(exc)
        except Exception as exc:  # noqa: BLE001 — a failed cell is data, not a crash
            transcript.error = f"{type(exc).__name__}: {exc}"
        transcript.wall_seconds = round(time.monotonic() - started, 2)
        return transcript

    def _structured(self, transcript, tools, workspace, messages, served, deadline) -> None:
        """One grammar-constrained action per turn.

        A malformed call is not *recovered* here, it is unrepresentable: the
        decoder cannot emit a name outside the union or an argument outside that
        action's properties. What remains possible is a **wrong** call — the right
        shape pointed at the wrong file — which is the subject's mistake and is
        exactly what should stay measurable.
        """
        schema = action_schema(workspace.has_engine)
        turn = 0
        for _ in range(self.max_turns):
            if time.monotonic() > deadline:
                transcript.error = _OUT_OF_TIME
                return
            reply = self.complete(messages, schema=schema, **served)
            message = (reply.get("choices") or [{}])[0].get("message") or {}
            transcript.usage = transcript.usage + _usage(reply)
            _keep_reasoning(transcript, message)

            content = str(message.get("content") or "")
            messages.append({"role": "assistant", "content": content})
            try:
                action = json.loads(content)
                name = str(action["action"])
                arguments = dict(action.get("arguments") or {})
            except (json.JSONDecodeError, KeyError, TypeError, ValueError) as exc:
                transcript.malformed_calls += 1
                messages.append({"role": "user", "content": f"Malformed action: {exc}"})
                continue

            if name == FINAL_ACTION:
                transcript.final_text = "(finished)"
                return

            turn += 1
            self._record(transcript, turn, name, arguments)
            result = tools.run(name, arguments)
            if result.denied:
                transcript.denials.append(f"turn {turn}: {name} — {result.text}")
            messages.append({"role": "user", "content": result.text})

    def _native(self, transcript, tools, workspace, messages, served, deadline) -> None:
        schemas = tool_schemas(workspace.has_engine)
        turn = 0
        for _ in range(self.max_turns):
            if time.monotonic() > deadline:
                transcript.error = _OUT_OF_TIME
                return
            reply = self.complete(messages, tools=schemas, **served)
            choice = (reply.get("choices") or [{}])[0]
            message = choice.get("message") or {}
            transcript.usage = transcript.usage + _usage(reply)
            _keep_reasoning(transcript, message)

            calls = message.get("tool_calls") or []
            messages.append(
                {
                    "role": "assistant",
                    "content": message.get("content") or "",
                    **({"tool_calls": calls} if calls else {}),
                }
            )
            if not calls:
                transcript.final_text = str(message.get("content") or "")
                return

            for call in calls:
                turn += 1
                name = (call.get("function") or {}).get("name", "")
                arguments, malformed = _arguments(call)
                if malformed:
                    # A small model that emits unparseable arguments has not
                    # used the tool, and counting it as a call would put a
                    # tool-calling failure into a reasoning number.
                    transcript.malformed_calls += 1
                    messages.append(_tool_reply(call, f"Malformed arguments: {malformed}"))
                    continue

                self._record(transcript, turn, name, arguments)
                result = tools.run(name, arguments)
                if result.denied:
                    transcript.denials.append(f"turn {turn}: {name} — {result.text}")
                messages.append(_tool_reply(call, result.text))


def _keep_reasoning(transcript: Transcript, message: dict) -> None:
    """Store what the model thought, under either spelling.

    ollama says ``reasoning``, vLLM says ``reasoning_content``. Reading both is
    two lines and is the difference between recording the thing under test and
    silently dropping it when the server changes.
    """
    text = message.get("reasoning") or message.get("reasoning_content") or ""
    if text:
        transcript.reasoning.append(str(text))


def _arguments(call: dict) -> tuple[dict, str | None]:
    raw = (call.get("function") or {}).get("arguments", "")
    if isinstance(raw, dict):
        return raw, None
    try:
        parsed = json.loads(raw or "{}")
    except json.JSONDecodeError as exc:
        return {}, str(exc)
    if not isinstance(parsed, dict):
        return {}, "arguments must be a JSON object"
    return parsed, None


def _tool_reply(call: dict, text: str) -> dict:
    return {
        "role": "tool",
        "tool_call_id": call.get("id", ""),
        "name": (call.get("function") or {}).get("name", ""),
        "content": text,
    }


def _usage(reply: dict) -> Usage:
    usage = reply.get("usage") or {}
    return Usage(
        input_tokens=int(usage.get("prompt_tokens", 0) or 0),
        output_tokens=int(usage.get("completion_tokens", 0) or 0),
    )
