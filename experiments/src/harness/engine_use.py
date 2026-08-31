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

#: The interpreters a subject reaches for when it writes code instead of a
#: Datalog program. Matched on the **command name**, through the same splitter
#: `invokes_engine` uses, so `cat x.py` and `Read`ing a `.py` file are not
#: running one.
#:
#: `awk` and `sed` are deliberately absent: `signals._SEARCH_COMMANDS` already
#: counts them as search, and a command that scored as both would be double-
#: counted in two tables that are meant to partition the same transcript.
_INTERPRETERS = frozenset(
    {"python", "python3", "python3.13", "perl", "ruby", "node", "bash", "sh", "zsh"}
)

#: Files that are a script rather than data or a Datalog program.
_SCRIPT_SUFFIXES = (".py", ".sh", ".pl", ".rb", ".js")

#: An explanation goal. Both sigils, and `\b` so `?why` does not swallow
#: `?whynot` — they are separate answers to separate questions and a count that
#: merged them would be reporting neither.
#:
#: This lives here, beside `_DATALOG_SYNTAX`, and not in `signals`. That is the
#: whole point of this module: there were two substring tests for the engine once
#: and they disagreed, which is what the docstring above is about. A second
#: parser for provenance would be the same defect with a new name.
_PROVENANCE_GOAL = re.compile(r"\?why(?:not)?\b")


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


def provenance_in_command(command: str) -> bool:
    """Did a `datalog` **process** run an explanation goal?

    Narrow, and it has to be: `?whynot` typed into a `.dl` file that is never
    run explains nothing, and the run this signal exists for is precisely about
    whether the subject *asks* rather than whether it writes the word down.
    """
    return invokes_engine(command) and bool(_PROVENANCE_GOAL.search(command))


def asks_provenance(tool_name: str, tool_input: dict) -> bool:
    """Did this call reach for an explanation, however the subject spelled it?

    The wide predicate — the counterpart to `uses_engine`, not to `_ran_engine`.
    It counts a goal written into a file or handed to the skill, which
    `provenance_in_command` deliberately does not.
    """
    if tool_name == "Bash":
        return bool(_PROVENANCE_GOAL.search(str(tool_input.get("command", ""))))
    if tool_name in ("Write", "Edit"):
        content = str(tool_input.get("content") or tool_input.get("new_string") or "")
        return bool(_PROVENANCE_GOAL.search(content))
    if tool_name == "Skill":
        return bool(_PROVENANCE_GOAL.search(str(tool_input.get("command", tool_input))))
    return False


def runs_script(command: str) -> bool:
    """Did this command **execute** an interpreter?

    The counterpart to `invokes_engine`, and it lives beside it for the reason
    that module docstring gives: these are the same question — *what did this
    command actually run?* — and answering it in two places is how the two
    substring tests came to disagree. (The module's name has outgrown its
    contents; the predicates have not.)
    """
    return any(_command_name(segment) in _INTERPRETERS for segment in _segments(command))


def script_in_write(tool_input: dict) -> bool:
    """Did this write put source in a script file?"""
    path = str(tool_input.get("file_path", ""))
    return path.endswith(_SCRIPT_SUFFIXES)


def writes_script(tool_name: str, tool_input: dict) -> bool:
    """Did this call author a script, in a file or straight into a heredoc?"""
    if tool_name in ("Write", "Edit"):
        return script_in_write(tool_input)
    if tool_name == "Bash":
        command = str(tool_input.get("command", ""))
        # A heredoc into an interpreter is authoring and running in one call —
        # the shape a subject uses when it is already in a pipeline, and the
        # one a `.py`-suffix test misses entirely.
        return runs_script(command) and "<<" in command
    return False
