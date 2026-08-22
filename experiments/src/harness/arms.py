"""Building the two workspaces. The engine is the only difference.

Both arms get the same directory, the same fixture files, the same prompt and the
same tools — including `bash`, so the prose arm is free to write a Python script.
That is the honest counterfactual: an agent's real alternative to a logic engine
is not careful prose, it is ad-hoc code (``decisions.md``, 2026-08-21).

The engine arm additionally gets the ``datalog`` binary on `PATH` and the skill
under ``.claude/skills/``. Neither is mentioned in the prompt — availability is a
fact about the workspace, which is what leaves *"did it reach for the engine?"*
free to be measured.
"""

from __future__ import annotations

import hashlib
import os
import shutil
from dataclasses import dataclass
from pathlib import Path

from harness.catalogue import assemble, verify
from harness.cell import Cell

#: Repo-relative, resolved from this file so the harness works from any cwd.
REPO_ROOT = Path(__file__).resolve().parents[3]
DATALOG_BIN_DIR = REPO_ROOT / "datalog" / "target" / "release"
DATALOG_SKILL_DIR = REPO_ROOT / "datalog" / "skill"

#: Workspaces live **outside the checkout**, and that is a control rather than
#: tidiness. Every domain's ``truth.py`` is the answer key, and the engine's
#: binary, source and spec sit in the same tree — from a workspace inside the
#: repo, both were two directories up. See ``confine.py``.
WORKSPACE_ROOT = Path(
    os.environ.get("HARNESS_WORKSPACE_ROOT", Path.home() / ".cache" / "llmlogic-experiments")
)


class EngineMissing(Exception):
    """The engine arm was asked for and the binary is not built."""


@dataclass(frozen=True)
class Workspace:
    path: Path
    #: The cell this workspace belongs to. Kept here rather than in the directory
    #: name, which is a hash so the subject cannot read its own arm off `pwd`.
    cell_id: str
    prompt: str
    #: The subject's full environment. Both arms get a **scrubbed** `PATH`; only
    #: the engine arm gets the engine prepended to it.
    env: dict[str, str]
    has_engine: bool


def scrubbed_path() -> str:
    """`PATH` with any directory that already contains a ``datalog`` removed.

    Without this the prose arm inherits whatever the operator has installed, and a
    globally-installed engine would quietly give both arms the engine — leaving a
    run that measures nothing and says so nowhere. The same reasoning keeps
    ``setting_sources`` off ``"user"`` in ``subject.py``: the operator's own
    machine must not be part of the experiment.
    """
    keep = []
    for entry in os.environ.get("PATH", "").split(os.pathsep):
        if not entry:
            continue
        candidate = Path(entry) / "datalog"
        if candidate.exists():
            continue
        keep.append(entry)
    return os.pathsep.join(keep)


def _link_binary(destination: Path) -> Path:
    """Hardlink the engine into the workspace, so nothing has to reach outside it.

    A hardlink because the binary is 50 MB and a full grid materializes dozens of
    engine-arm workspaces; a copy would be gigabytes. Falls back to copying when
    the workspace root is on another filesystem.
    """
    bin_dir = destination / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    target = bin_dir / "datalog"
    if target.exists():
        return bin_dir
    source = DATALOG_BIN_DIR / "datalog"
    try:
        os.link(source, target)
    except OSError:
        shutil.copy2(source, target)
    return bin_dir


def _copy_skill(destination: Path) -> None:
    skill_root = destination / ".claude" / "skills" / "datalog"
    skill_root.mkdir(parents=True, exist_ok=True)
    shutil.copy2(DATALOG_SKILL_DIR / "SKILL.md", skill_root / "SKILL.md")
    for subdir in ("examples", "recipes"):
        source = DATALOG_SKILL_DIR / subdir
        if source.is_dir():
            shutil.copytree(source, skill_root / subdir, dirs_exist_ok=True)


def build(cell: Cell, root: Path) -> Workspace:
    """Materialize a cell's workspace. Raises if the engine arm has no engine."""
    verify(cell.task.fixture)

    # The directory name is a hash, not the cell id. Named after the cell it
    # would read `...who-can-read-r03.engine.haiku-4.5`, so a subject that runs
    # `pwd` learns it is the engine arm of an experiment — and the whole design
    # rests on it not knowing that (control 3).
    path = root / hashlib.sha256(cell.id.encode()).hexdigest()[:16]
    path.mkdir(parents=True, exist_ok=True)
    for filename, contents in cell.task.fixture.files.items():
        (path / filename).write_text(contents, encoding="utf-8")

    search_path = scrubbed_path()
    has_engine = cell.arm == "engine"
    if has_engine:
        if not (DATALOG_BIN_DIR / "datalog").exists():
            raise EngineMissing(
                f"no datalog binary at {DATALOG_BIN_DIR / 'datalog'} — "
                "build it with `cargo build --release --offline` in datalog/"
            )
        _copy_skill(path)
        search_path = os.pathsep.join([str(_link_binary(path)), search_path])

    return Workspace(
        path=path,
        cell_id=cell.id,
        prompt=assemble(cell.task),
        env={"PATH": search_path},
        has_engine=has_engine,
    )
