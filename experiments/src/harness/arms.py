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
import re
import shutil
from dataclasses import dataclass
from pathlib import Path

from harness.ablate import apply as apply_ablation
from harness.catalogue import assemble, verify
from harness.cell import ENGINE_ARMS, Cell

#: Repo-relative, resolved from this file so the harness works from any cwd.
REPO_ROOT = Path(__file__).resolve().parents[3]
DATALOG_ROOT = REPO_ROOT / "datalog"
DATALOG_BIN_DIR = DATALOG_ROOT / "target" / "release"
DATALOG_SKILL_DIR = DATALOG_ROOT / "skill"

#: What the engine is built from. Anything here newer than the binary means the
#: binary is not this checkout's engine — see `require_engine`.
ENGINE_SOURCES = ("src", "Cargo.toml", "Cargo.lock")

#: Workspaces live **outside the checkout**, and that is a control rather than
#: tidiness. Every domain's ``truth.py`` is the answer key, and the engine's
#: binary, source and spec sit in the same tree — from a workspace inside the
#: repo, both were two directories up. See ``confine.py``.
WORKSPACE_ROOT = Path(
    os.environ.get("HARNESS_WORKSPACE_ROOT", Path.home() / ".cache" / "llmlogic-experiments")
)


#: A cell directory is the first 16 hex characters of a sha256. Nothing else in
#: the workspace root is one, and `_clear` deletes nothing that is not.
_WORKSPACE_NAME = re.compile(r"[0-9a-f]{16}")


class EngineMissing(Exception):
    """The engine arm was asked for and the binary is not built."""


class EngineStale(EngineMissing):
    """The binary exists but predates the engine source it is supposed to be.

    A separate class from `EngineMissing` because the two are different
    situations for a caller to explain, and a subclass of it because every
    existing handler means *"there is no usable engine"* and both are that.
    """


def require_engine(
    bin_dir: Path = DATALOG_BIN_DIR,
    source_root: Path = DATALOG_ROOT,
) -> Path:
    """Return the engine binary, refusing one that is not this checkout's.

    Every measurement the harness makes is a measurement of whatever was last
    compiled. Checking only that the file *exists* is what let the reference
    corpus report 12/12 green against a two-day-old engine, and — the same
    coupling seen from the other side — turns a checkout of an earlier commit
    into four red tests until someone remembers to rebuild. The pins are
    versioned; the binary they are pinned against is not, so the harness has to
    be the one that notices.

    Refusing is the whole behaviour: building here would hide the coupling
    rather than surface it, and a release build is a minute the caller should
    spend deliberately.
    """
    binary = bin_dir / "datalog"
    if not binary.exists():
        raise EngineMissing(
            f"no datalog binary at {binary} — "
            "build it with `cargo build --release --offline` in datalog/"
        )

    built = binary.stat().st_mtime
    newest, newest_at = None, built
    for entry in ENGINE_SOURCES:
        path = source_root / entry
        candidates = [path, *path.rglob("*")] if path.is_dir() else [path]
        for candidate in candidates:
            if not candidate.is_file():
                continue
            modified = candidate.stat().st_mtime
            if modified > newest_at:
                newest, newest_at = candidate, modified

    if newest is not None:
        raise EngineStale(
            f"the datalog binary at {binary} is older than "
            f"{newest.relative_to(source_root)} — it is not this checkout's "
            "engine, so anything measured against it is measured against "
            "whatever was last compiled. Rebuild it with "
            "`cargo build --release --offline` in datalog/"
        )
    return binary


@dataclass(frozen=True)
class Workspace:
    path: Path
    #: The cell this workspace belongs to. Kept here rather than in the directory
    #: name, which is a hash so the subject cannot read its own arm off `pwd`.
    cell_id: str
    prompt: str
    #: The documentation block cut from this workspace, if any.
    ablated: str | None
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


def _copy_skill(destination: Path, ablate: str | None = None) -> None:
    skill_root = destination / ".claude" / "skills" / "datalog"
    skill_root.mkdir(parents=True, exist_ok=True)
    shutil.copy2(DATALOG_SKILL_DIR / "SKILL.md", skill_root / "SKILL.md")
    for subdir in ("examples", "recipes"):
        source = DATALOG_SKILL_DIR / subdir
        if source.is_dir():
            shutil.copytree(source, skill_root / subdir, dirs_exist_ok=True)
    # Always: the block markers come out of every copy, so an ablated cell and
    # its control differ by the cut and not by a comment the model can read.
    apply_ablation(skill_root, ablate)


def _clear(path: Path, root: Path) -> None:
    """Delete a cell's directory, refusing anything that is not one.

    The guard is not ceremony: this deletes a tree, and the only thing standing
    between it and someone's home directory is that ``path`` was derived from a
    hash a moment ago. Both conditions are cheap and neither can hold by accident.
    """
    if not path.exists():
        return
    resolved, base = path.resolve(), root.resolve()
    if not resolved.is_relative_to(base) or resolved == base:
        raise ValueError(f"refusing to clear {resolved}, which is not inside {base}")
    if not _WORKSPACE_NAME.fullmatch(path.name):
        raise ValueError(f"refusing to clear {resolved}: not a cell workspace name")
    shutil.rmtree(resolved)


def build(cell: Cell, root: Path) -> Workspace:
    """Materialize a cell's workspace. Raises if the engine arm has no engine."""
    verify(cell.task.fixture)

    # The directory name is a hash, not the cell id. Named after the cell it
    # would read `...who-can-read-r03.engine.haiku-4.5`, so a subject that runs
    # `pwd` learns it is the engine arm of an experiment — and the whole design
    # rests on it not knowing that (control 3).
    path = root / hashlib.sha256(cell.id.encode()).hexdigest()[:16]

    # **A cell starts from an empty directory.** Reusing one is not a tidiness
    # problem, it is a validity one: the previous occupant's `answer.txt` is
    # still there, and a subject that fails to write its own is graded on it.
    # Measured on the first pilot (2026-08-23) — `--dry-run` and a paid run
    # derive the same directory from the same cell id, so two engine-arm cells
    # were graded on the *stub's* answer: one scored wrong on its deliberately
    # truncated one, one scored correct without doing the work. Contamination
    # shows up exactly when the subject did not do the work, which is when the
    # verdict matters most.
    _clear(path, root)
    path.mkdir(parents=True)
    for filename, contents in cell.task.fixture.files.items():
        # Nested keys are how a fixture carries a source tree, and `bytes` is how
        # it carries a Parquet copy; both arms get the same files either way.
        destination = path / filename
        destination.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(contents, bytes):
            destination.write_bytes(contents)
        else:
            destination.write_text(contents, encoding="utf-8")

    search_path = scrubbed_path()
    has_engine = cell.arm in ENGINE_ARMS
    if has_engine:
        require_engine()
        _copy_skill(path, cell.ablate)
        search_path = os.pathsep.join([str(_link_binary(path)), search_path])

    return Workspace(
        path=path,
        cell_id=cell.id,
        prompt=assemble(cell.task, cell.arm),
        env={"PATH": search_path},
        has_engine=has_engine,
        ablated=cell.ablate if has_engine else None,
    )
