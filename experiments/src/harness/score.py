"""Run one Datalog program against one task's fixture, and grade what it derived.

Grading a *program* rather than an agent's answer file. The grid measures a
subject; this measures a program, which is what the reference corpus needs and
what any later verifiable-reward use would need — one command, no model in the
loop, an oracle that never came from the engine (control 1).

**The timeout is ours.** The engine has no fuel, cap or wall-clock budget, by
decision and not omission (`../datalog/spec.md` non-goals, rejected twice): a
non-terminating program is *named before it runs* rather than killed during it.
That is the right call for a person at a prompt and the wrong one for anything
running programs unattended, so the bound lives here, at the call site, where it
belongs.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path

from harness.arms import require_engine
from harness.reference import parse_facts
from harness.task import Answer, Task

#: Seconds one program gets. Generous against the measured baseline — a 5,248-edge
#: closure yielding 173k tuples took 11.6s — and short enough that a value-creating
#: recursion costs a minute rather than a session.
DEFAULT_TIMEOUT = 120

#: What the engine calls a synthesized answer relation (§14).
SYNTHESIZED = "answer"


@dataclass(frozen=True)
class Score:
    """What a program derived, and how close it was.

    ``exit_code`` and ``stderr`` are kept rather than collapsed into a boolean:
    a program the engine *rejected* and one that ran and derived the wrong rows
    are different failures, and the diagnostic is the whole of what S3 claims is
    repairable.
    """

    accepted: bool
    exit_code: int
    stderr: str
    #: ``None`` when the program was rejected or timed out — no rows, as distinct
    #: from an empty relation, which is a legitimate answer.
    derived: Answer | None
    correct: bool
    missing: int
    extra: int
    timed_out: bool = False


def score(
    task: Task,
    program: str,
    workdir: Path,
    *,
    relation: str | None = None,
    timeout: int = DEFAULT_TIMEOUT,
) -> Score:
    """Run ``program`` over ``task``'s fixture in ``workdir`` and grade its output.

    ``relation`` names which derived relation is the answer. Left unset, the
    engine's synthesized ``answer/N`` is used if present, and otherwise the sole
    relation printed — but only if there is exactly one. Guessing between several
    would silently pick the flattering one.
    """
    binary = require_engine()
    for filename, contents in task.fixture.files.items():
        destination = workdir / filename
        destination.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(contents, bytes):
            destination.write_bytes(contents)
        else:
            destination.write_text(contents, encoding="utf-8")
    source = workdir / "program.dl"
    source.write_text(program, encoding="utf-8")

    try:
        completed = subprocess.run(
            [str(binary), source.name],
            cwd=workdir,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return Score(
            accepted=False,
            exit_code=-1,
            stderr=f"timed out after {timeout}s",
            derived=None,
            correct=False,
            missing=len(task.truth.rows),
            extra=0,
            timed_out=True,
        )

    # Exit 0 is rows found, 1 is every query ran and none answered, 2 and above is
    # "did not answer" (§14). So 0 and 1 are both *accepted*: a program that
    # correctly derives nothing exits 1, and that is the right answer to a whole
    # question class.
    accepted = completed.returncode in (0, 1)
    if not accepted:
        return Score(
            accepted=False,
            exit_code=completed.returncode,
            stderr=completed.stderr,
            derived=None,
            correct=False,
            missing=len(task.truth.rows),
            extra=0,
        )

    relations = parse_facts(completed.stdout)
    rows = _pick(relations, relation)
    if rows is None:
        return Score(
            accepted=True,
            exit_code=completed.returncode,
            stderr=(
                f"cannot tell which relation is the answer: derived "
                f"{sorted(relations) or '(nothing)'}. Name one with `relation=`, "
                f"or publish it with `?- name: body.`"
            ),
            derived=None,
            correct=False,
            missing=len(task.truth.rows),
            extra=0,
        )

    derived = Answer(frozenset(rows))
    return Score(
        accepted=True,
        exit_code=completed.returncode,
        stderr=completed.stderr,
        derived=derived,
        correct=derived.rows == task.truth.rows,
        missing=len(task.truth.rows - derived.rows),
        extra=len(derived.rows - task.truth.rows),
    )


def _pick(
    relations: dict[str, set[tuple[str, ...]]], wanted: str | None
) -> set[tuple[str, ...]] | None:
    if wanted is not None:
        return relations.get(wanted, set())
    if SYNTHESIZED in relations:
        return relations[SYNTHESIZED]
    if len(relations) == 1:
        return next(iter(relations.values()))
    if not relations:
        # Nothing printed is an empty answer, which is a real one.
        return set()
    return None
