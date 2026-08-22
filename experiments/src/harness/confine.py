"""Keeping a cell inside its workspace.

Two different things want this, and only one of them is safety:

**Validity.** The answer key is on disk. Every domain's ``truth.py`` holds the
expected answers, and the engine's source, spec and skill sit in the same
checkout. A subject that wanders up the tree can read the answers, and a *prose
arm* subject that finds the binary can run it — which erases the one difference
the whole experiment is built on. Scrubbing `PATH` does nothing against
``/abs/path/to/datalog``.

**Safety.** A full grid is ~112 unattended agent sessions with `bash`. They
should not be able to touch anything but their own scratch directory.

The gate is a **PreToolUse hook**, because that is the only gate that fires:
under ``permission_mode="bypassPermissions"`` the SDK auto-approves every call
before ``can_use_tool`` is consulted, and says so — *"To gate every tool call,
use a PreToolUse hook instead."* Layered under it, the OS bash sandbox is what
actually confines a shell; this module is the part that can also explain itself
in a transcript.
"""

from __future__ import annotations

import re
from pathlib import Path

#: Tool inputs that name a path, by tool.
_PATH_FIELDS = ("file_path", "path", "notebook_path")

#: An absolute path, or a traversal that climbs out of the workspace.
_ABSOLUTE = re.compile(r"(?:^|[\s\"'=(])(/[^\s\"';|&)]*)")
_CLIMBS_OUT = re.compile(r"\.\./")


def _inside(candidate: Path, workspace: Path) -> bool:
    try:
        candidate.resolve().relative_to(workspace.resolve())
    except (ValueError, OSError):
        return False
    return True


def path_violation(tool_input: dict, workspace: Path) -> str | None:
    for field in _PATH_FIELDS:
        raw = tool_input.get(field)
        if not raw:
            continue
        target = Path(str(raw))
        if not target.is_absolute():
            target = workspace / target
        if not _inside(target, workspace):
            return f"{raw} is outside the cell's workspace"
    return None


def command_violation(command: str, workspace: Path) -> str | None:
    """A cheap textual check over a shell command.

    Deliberately not a shell parser — the OS sandbox is what actually confines
    bash. This catches the legible escapes so they show up as a denial in the
    transcript rather than as a silently better score.
    """
    if _CLIMBS_OUT.search(command):
        return "the command climbs out of the workspace with `..`"
    for match in _ABSOLUTE.finditer(command):
        candidate = Path(match.group(1))
        # Reading system binaries and libraries is ordinary; leaving the
        # workspace for *data* is not. Only flag paths that exist and are not
        # part of a normal toolchain.
        if not candidate.exists():
            continue
        if _inside(candidate, workspace):
            continue
        if str(candidate).startswith(("/usr", "/bin", "/sbin", "/lib", "/etc", "/proc", "/dev")):
            continue
        return f"{candidate} is outside the cell's workspace"
    return None


def violation(tool_name: str, tool_input: dict, workspace: Path) -> str | None:
    """Why this call should be denied, or ``None`` to let it through."""
    if tool_name == "Bash":
        return command_violation(str(tool_input.get("command", "")), workspace)
    return path_violation(tool_input, workspace)
