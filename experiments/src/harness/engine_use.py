"""Recognising that the subject used the engine — and what it asked it.

One home for this, because it was two, and the two disagreed. Both were a
substring test for ``"datalog"``, which the first real cell immediately fooled:
the subject ran ``ls .../.claude/skills/datalog/`` and the harness recorded that
as its **first program**. Control 4 measures the program written before any
feedback, so a false positive there does not just add noise — it replaces the
measurement with a directory listing.

The other half of that cell: the subject wrote no ``.dl`` file at all. It passed
programs to the binary inline, so a ``.dl``-suffix test reads "never wrote a
program" for a subject that wrote several.
"""

from __future__ import annotations

import re
import shlex

#: Where one shell command ends and the next begins.
_SEPARATORS = re.compile(r"\|\||&&|[|;\n]")

#: `NAME=value` prefixes, which sit before the command name.
_ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")

#: A rule, or a fact with an argument list — enough to tell Datalog source from a
#: sentence mentioning it.
_DATALOG_SYNTAX = re.compile(r":-|\?why|\?whynot|\w+\s*\([^)]*\)")


def _segments(command: str) -> list[str]:
    return [segment.strip() for segment in _SEPARATORS.split(command) if segment.strip()]


def _command_name(segment: str) -> str | None:
    try:
        tokens = shlex.split(segment)
    except ValueError:  # unbalanced quotes, e.g. a heredoc body
        tokens = segment.split()
    for token in tokens:
        if _ASSIGNMENT.match(token):
            continue
        return token.rsplit("/", maxsplit=1)[-1]
    return None


def invokes_engine(command: str) -> bool:
    """Is ``datalog`` the command being *run*, rather than a path being listed?"""
    return any(_command_name(segment) == "datalog" for segment in _segments(command))


def program_in_command(command: str) -> str | None:
    """The Datalog source a command carries, if any.

    Covers the two spellings a subject actually uses: ``-q 'goal(X)'`` and a
    heredoc piped into the binary. A bare ``datalog facts.dl`` carries no source
    of its own — the program is in the file, which the file-write path catches.
    """
    if not invokes_engine(command):
        return None
    try:
        tokens = shlex.split(command)
    except ValueError:
        tokens = command.split()
    for index, token in enumerate(tokens):
        if token in ("-q", "--query") and index + 1 < len(tokens):
            return tokens[index + 1]
    if _DATALOG_SYNTAX.search(command):
        return command
    return None


def program_in_write(tool_input: dict) -> str | None:
    path = str(tool_input.get("file_path", ""))
    if not path.endswith(".dl"):
        return None
    return str(tool_input.get("content") or tool_input.get("new_string") or "")


def program_from_call(tool_name: str, tool_input: dict) -> str | None:
    """The Datalog program in one tool call, however the subject spelled it."""
    if tool_name in ("Write", "Edit"):
        return program_in_write(tool_input)
    if tool_name == "Bash":
        return program_in_command(str(tool_input.get("command", "")))
    return None


def uses_engine(tool_name: str, tool_input: dict) -> bool:
    """Did this call use the engine at all — including invoking its skill?"""
    if tool_name == "Bash":
        return invokes_engine(str(tool_input.get("command", "")))
    if tool_name == "Skill":
        # The subject explicitly reaching for the skill is the strongest possible
        # form of "reached for it", and the first real cell did exactly this.
        return "datalog" in str(tool_input.get("command", tool_input)).lower()
    return False
