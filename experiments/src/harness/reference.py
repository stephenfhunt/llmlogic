"""The reference corpus: programs whose output is pinned byte-exact.

Two halves, and the second is the point. **Correct** programs answer a domain's
four questions and pin what the engine prints. **Malformed** ones are wrong in a
named way and pin the diagnostic — because a corpus of correct programs cannot
say what the tool does when a run goes wrong, which is most of what a subject
sees while it is still getting the program right.

What this is *for*: the harness measures an agent against an engine that moves
under it. A grid run in August and one run in October are the same measurement
only if the engine answered the same way, and nothing else in `experiments/`
checks that. These pins are the tripwire. A diff here is not necessarily a
regression — it is a change in the instrument, which has to be looked at before
the next run's numbers are compared to the last one's.

The programs are not written by this project's authors as an exercise. They are
the ones actually verified row-for-row against each domain's ``truth.py`` when
that domain was built, kept rather than thrown away.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path

from harness.arms import require_engine
from harness.task import Fixture

#: `experiments/reference/`, resolved from this file so cwd does not matter.
REFERENCE_ROOT = Path(__file__).resolve().parents[2] / "reference"
CORRECT_DIR = REFERENCE_ROOT / "correct"
MALFORMED_DIR = REFERENCE_ROOT / "malformed"

#: The file a program is written to inside the scratch directory. Fixed, so a
#: diagnostic that names the source file is stable across runs — several do.
PROGRAM_NAME = "program.dl"


@dataclass(frozen=True)
class Entry:
    """One pinned program.

    ``domain`` is set on a correct entry and names the pack whose fixture the
    program is written against; a malformed entry carries its own facts and has
    none. ``extractor`` is set only for ``static_analysis``, whose fixture ships
    no fact tables because inventing them is the task.
    """

    name: str
    program: Path
    expected_stdout: Path
    expected_stderr: Path
    domain: str | None = None
    extractor: Path | None = None

    @property
    def source(self) -> str:
        return self.program.read_text(encoding="utf-8")

    @property
    def stdout(self) -> str:
        return self.expected_stdout.read_text(encoding="utf-8")

    @property
    def stderr(self) -> str:
        return self.expected_stderr.read_text(encoding="utf-8")


@dataclass(frozen=True)
class Run:
    exit_code: int
    stdout: str
    stderr: str


def repin(entry: Entry, result: Run) -> None:
    """Overwrite an entry's pins with what it just printed.

    Deliberate by design — there is a command for it (`harness reference
    --repin`) and no code path that reaches it from a failing test. A pin that
    regenerates itself when it goes red is not a pin.
    """
    if entry.domain is not None:
        entry.expected_stdout.write_text(result.stdout, encoding="utf-8")
    entry.expected_stderr.write_text(result.stderr, encoding="utf-8")


def _entries(directory: Path, *, with_domain: bool) -> list[Entry]:
    found = []
    for program in sorted(directory.glob("*.dl")):
        name = program.stem
        extractor = program.with_suffix(".extract.py")
        found.append(
            Entry(
                name=name,
                program=program,
                expected_stdout=program.with_suffix(".out"),
                expected_stderr=program.with_suffix(".err"),
                domain=name if with_domain else None,
                extractor=extractor if extractor.exists() else None,
            )
        )
    return found


def correct() -> list[Entry]:
    """One entry per domain, answering that domain's four questions."""
    return _entries(CORRECT_DIR, with_domain=True)


def malformed() -> list[Entry]:
    """Programs that are wrong in a named way, pinned to their diagnostic."""
    return _entries(MALFORMED_DIR, with_domain=False)


def materialize(entry: Entry, fixture: Fixture | None, workdir: Path) -> None:
    """Lay out everything the entry needs to run, in ``workdir``.

    The fixture is written the same way ``arms.build`` writes it, so a reference
    program runs against the bytes a subject would have been given rather than
    against a second copy that can drift from them.
    """
    if fixture is not None:
        for filename, contents in fixture.files.items():
            destination = workdir / filename
            destination.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(contents, bytes):
                destination.write_bytes(contents)
            else:
                destination.write_text(contents, encoding="utf-8")
    (workdir / PROGRAM_NAME).write_text(entry.source, encoding="utf-8")


def run(entry: Entry, workdir: Path) -> Run:
    """Run the extractor if there is one, then the engine. Never grades."""
    binary = require_engine()
    if entry.extractor is not None:
        extraction = subprocess.run(
            ["python3", str(entry.extractor)],
            cwd=workdir,
            capture_output=True,
            text=True,
        )
        if extraction.returncode != 0:
            return Run(extraction.returncode, extraction.stdout, extraction.stderr)
    completed = subprocess.run(
        [str(binary), PROGRAM_NAME],
        cwd=workdir,
        capture_output=True,
        text=True,
    )
    return Run(completed.returncode, completed.stdout, completed.stderr)


def _split_arguments(inner: str) -> list[str]:
    """Split a fact's argument list on top-level commas.

    Quote-aware, because a string value may contain a comma. Nothing here has to
    handle nesting: §14's canonical output has no compound terms, so an argument
    is a quoted string or a bare literal and never another fact.
    """
    fields: list[str] = []
    current: list[str] = []
    quoted = False
    escaped = False
    for char in inner:
        if escaped:
            current.append(char)
            escaped = False
        elif char == "\\":
            current.append(char)
            escaped = True
        elif char == '"':
            quoted = not quoted
            current.append(char)
        elif char == "," and not quoted:
            fields.append("".join(current))
            current = []
        else:
            current.append(char)
    fields.append("".join(current))
    return fields


def _unwrap(field: str) -> str:
    """One printed argument down to its value."""
    field = field.strip()
    if field.startswith('"') and field.endswith('"') and len(field) >= 2:
        return field[1:-1].replace('\\"', '"')
    # `@2026-01-01`, `@2026-01-01T09:00:00`, `@1d` — §14 prints a temporal
    # literal with the sigil it is written with.
    return field.removeprefix("@")


def parse_facts(stdout: str) -> dict[str, set[tuple[str, ...]]]:
    """Canonical output back into ``relation -> {row}``.

    §14 notation is unwrapped down to the value: quotes come off a string, and
    the `@` comes off a date, timestamp or duration literal. The oracles hold
    bare values, the engine prints §14, and the difference is spelling rather
    than an answer.

    **This is not what the grader does, and must not become it.** A subject is
    told the format its answer file wants — `imports/busiest-month-per-region`
    says *as YYYY-MM-DD* to both arms — so an engine-arm answer of `@2026-01-01`
    is a subject that pasted its tool's output without reading the question, and
    grading it correct would hide that. Here the engine's own stdout is being
    read on purpose, so here the sigil is noise.
    """
    relations: dict[str, set[tuple[str, ...]]] = {}
    for line in stdout.splitlines():
        line = line.strip()
        if not line or line.startswith("%") or not line.endswith(")."):
            continue
        head, _, rest = line[:-2].partition("(")
        row = tuple(_unwrap(field) for field in _split_arguments(rest))
        relations.setdefault(head.strip(), set()).add(row)
    return relations
